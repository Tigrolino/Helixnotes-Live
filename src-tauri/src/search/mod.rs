use crate::types::SearchResult;
use crate::vault::frontmatter;
use crate::vault::operations::helixnotes_dir;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use tantivy::collector::TopDocs;
#[cfg(desktop)]
use tantivy::directory::MmapDirectory;
#[cfg(mobile)]
use tantivy::directory::RamDirectory;
use tantivy::query::{BooleanQuery, FuzzyTermQuery, Occur, PhrasePrefixQuery, Query, TermQuery};
use tantivy::schema::*;
use tantivy::tokenizer::{LowerCaser, TextAnalyzer, Token, TokenStream, Tokenizer};
use tantivy::{Index, IndexWriter, TantivyDocument, Term};
use walkdir::WalkDir;

/// Bumped whenever the index schema or tokenizer changes, so the on-disk index is
/// wiped and rebuilt once on the next vault open (the index is derived from the
/// notes, so this never loses data).
const INDEX_SCHEMA_VERSION: &str = "2-cjk-bigram";

/// Per-vault search index dir in local app-data, kept out of the (possibly synced) vault.
#[cfg(desktop)]
fn vault_index_base(vault_path: &str) -> Option<std::path::PathBuf> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(vault_path.as_bytes());
    let key: String = hasher.finalize()[..8].iter().map(|b| format!("{:02x}", b)).collect();
    dirs::data_local_dir().map(|d| d.join("helixnotes").join("search").join(key))
}

/// True for characters in the CJK / Japanese / Korean blocks, which are written
/// without spaces between words. These get uni/bi-gram tokenized so substring
/// search works; everything else keeps the default word-splitting behaviour.
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x4DBF   // CJK Unified Ideographs Extension A
        | 0x4E00..=0x9FFF // CJK Unified Ideographs (Han)
        | 0x3040..=0x309F // Hiragana
        | 0x30A0..=0x30FF // Katakana
        | 0xAC00..=0xD7AF // Hangul Syllables
        | 0xF900..=0xFAFF // CJK Compatibility Ideographs
        | 0xFF00..=0xFFEF // Halfwidth and Fullwidth Forms
    )
}

/// Tokenize `text` so that CJK runs become overlapping unigrams + bigrams (recall
/// over precision: a query for any substring of a CJK run will match), while runs
/// of other alphanumeric characters become a single word token (same as tantivy's
/// SimpleTokenizer, so Latin/English indexing is unchanged). Lowercasing is applied
/// by a LowerCaser filter in the analyzer, not here.
fn cjk_tokens(text: &str) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    let mut position: usize = 0;
    let mut cjk_run: Vec<(usize, char)> = Vec::new();
    let mut word = String::new();
    let mut word_start: usize = 0;

    macro_rules! flush_word {
        () => {{
            if !word.is_empty() {
                let len = word.len();
                tokens.push(Token {
                    offset_from: word_start,
                    offset_to: word_start + len,
                    position,
                    text: std::mem::take(&mut word),
                    position_length: 1,
                });
                position += 1;
            }
        }};
    }
    macro_rules! flush_cjk {
        () => {{
            let n = cjk_run.len();
            for i in 0..n {
                let (off, ch) = cjk_run[i];
                tokens.push(Token {
                    offset_from: off,
                    offset_to: off + ch.len_utf8(),
                    position,
                    text: ch.to_string(),
                    position_length: 1,
                });
                position += 1;
            }
            for i in 0..n.saturating_sub(1) {
                let (off1, c1) = cjk_run[i];
                let (off2, c2) = cjk_run[i + 1];
                let mut s = String::with_capacity(c1.len_utf8() + c2.len_utf8());
                s.push(c1);
                s.push(c2);
                tokens.push(Token {
                    offset_from: off1,
                    offset_to: off2 + c2.len_utf8(),
                    position,
                    text: s,
                    position_length: 1,
                });
                position += 1;
            }
            cjk_run.clear();
        }};
    }

    for (offset, c) in text.char_indices() {
        if is_cjk(c) {
            flush_word!();
            cjk_run.push((offset, c));
        } else if c.is_alphanumeric() {
            flush_cjk!();
            if word.is_empty() {
                word_start = offset;
            }
            word.push(c);
        } else {
            flush_cjk!();
            flush_word!();
        }
    }
    flush_cjk!();
    flush_word!();
    let _ = position; // the final flush increments position but nothing reads it after
    tokens
}

/// A pre-computed token stream (all tokens collected up front by `cjk_tokens`).
struct PreTokenizedStream {
    tokens: Vec<Token>,
    idx: usize,
}

impl TokenStream for PreTokenizedStream {
    fn advance(&mut self) -> bool {
        if self.idx < self.tokens.len() {
            self.idx += 1;
            true
        } else {
            false
        }
    }
    fn token(&self) -> &Token {
        &self.tokens[self.idx - 1]
    }
    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.idx - 1]
    }
}

#[derive(Clone)]
struct CjkTokenizer;

impl Tokenizer for CjkTokenizer {
    type TokenStream<'a> = PreTokenizedStream;
    fn token_stream<'a>(&'a mut self, text: &'a str) -> PreTokenizedStream {
        PreTokenizedStream {
            tokens: cjk_tokens(text),
            idx: 0,
        }
    }
}

pub struct SearchIndex {
    index: Index,
    writer: Mutex<Option<IndexWriter>>,
    #[allow(dead_code)]
    schema: Schema,
    path_field: Field,
    title_field: Field,
    body_field: Field,
    tags_field: Field,
}

impl SearchIndex {
    pub fn new(vault_path: &str) -> Result<Self, String> {
        let mut schema_builder = Schema::builder();
        let path_field = schema_builder.add_text_field("path", STRING | STORED);
        // CJK-aware tokenizer for the indexed text fields (keeps Latin/English
        // behaviour identical; adds substring matching for Chinese/Japanese/Korean).
        let cjk_indexing = TextFieldIndexing::default()
            .set_tokenizer("cjk")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions);
        let cjk_text = TextOptions::default().set_indexing_options(cjk_indexing.clone());
        let cjk_text_stored = cjk_text.clone().set_stored();
        let title_field = schema_builder.add_text_field("title", cjk_text_stored.clone());
        let body_field = schema_builder.add_text_field("body", cjk_text);
        let tags_field = schema_builder.add_text_field("tags", cjk_text_stored);
        let schema = schema_builder.build();

        // Mobile: use in-memory index (flock is unreliable on the sandboxed/FUSE filesystem)
        // Desktop: use mmap directory for persistent index on disk
        #[cfg(mobile)]
        let index = {
            let dir = RamDirectory::create();
            Index::open_or_create(dir, schema.clone()).map_err(|e| e.to_string())?
        };
        #[cfg(desktop)]
        let index = {
            let base = vault_index_base(vault_path)
                .unwrap_or_else(|| helixnotes_dir(vault_path).join("search_index"));
            let index_dir = base.join("index");
            let version_path = base.join("version");

            // Remove the old in-vault index so it stops syncing.
            let hn = helixnotes_dir(vault_path);
            let _ = fs::remove_dir_all(hn.join("search_index"));
            let _ = fs::remove_file(hn.join("search_index.version"));

            // One-time wipe when the schema/tokenizer version changes; rebuild() repopulates.
            let version_ok = fs::read_to_string(&version_path)
                .map(|v| v.trim() == INDEX_SCHEMA_VERSION)
                .unwrap_or(false);
            if !version_ok {
                let _ = fs::remove_dir_all(&index_dir);
            }
            fs::create_dir_all(&index_dir).map_err(|e| e.to_string())?;
            let dir = MmapDirectory::open(&index_dir).map_err(|e| e.to_string())?;
            let idx = match Index::open_or_create(dir, schema.clone()) {
                Ok(idx) => idx,
                Err(_) => {
                    // Schema mismatch (older index) or corruption: wipe and recreate.
                    let _ = fs::remove_dir_all(&index_dir);
                    fs::create_dir_all(&index_dir).map_err(|e| e.to_string())?;
                    let dir = MmapDirectory::open(&index_dir).map_err(|e| e.to_string())?;
                    Index::open_or_create(dir, schema.clone()).map_err(|e| e.to_string())?
                }
            };
            let _ = fs::write(&version_path, INDEX_SCHEMA_VERSION);
            idx
        };

        // Register the CJK-aware tokenizer (in-memory; must be done on every open,
        // before the writer is created, so both indexing and querying use it).
        index.tokenizers().register(
            "cjk",
            TextAnalyzer::builder(CjkTokenizer).filter(LowerCaser).build(),
        );

        #[cfg(mobile)]
        let heap_size = 15_000_000;
        #[cfg(desktop)]
        let heap_size = 50_000_000;

        let writer = index.writer(heap_size).map_err(|e| e.to_string())?;

        Ok(Self {
            index,
            writer: Mutex::new(Some(writer)),
            schema,
            path_field,
            title_field,
            body_field,
            tags_field,
        })
    }

    pub fn rebuild(&self, vault_path: &str) -> Result<(), String> {
        let mut writer_guard = self.writer.lock().map_err(|e| e.to_string())?;
        let writer = writer_guard.as_mut().ok_or("Writer not available")?;

        writer.delete_all_documents().map_err(|e| e.to_string())?;

        let hn_dir = helixnotes_dir(vault_path);

        for entry in WalkDir::new(vault_path)
            .into_iter()
            .filter_entry(|e| {
                let p = e.path();
                !p.starts_with(&hn_dir)
                    && !p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.starts_with('.'))
                        .unwrap_or(false)
            })
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file()
                    && e.path().extension().and_then(|x| x.to_str()) == Some("md")
            })
        {
            if let Ok(raw) = fs::read_to_string(entry.path()) {
                let filename = entry.file_name().to_string_lossy().to_string();
                let (meta, content) = frontmatter::parse_note(&raw, &filename);
                let path_str = entry.path().to_string_lossy().to_string();

                let mut doc = TantivyDocument::new();
                doc.add_text(self.path_field, &path_str);
                doc.add_text(self.title_field, &meta.title);
                doc.add_text(self.body_field, &content);
                doc.add_text(self.tags_field, &meta.tags.join(" "));
                let _ = writer.add_document(doc);
            }
        }

        writer.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn index_note(&self, path: &str) -> Result<(), String> {
        let p = Path::new(path);
        let raw = fs::read_to_string(p).map_err(|e| e.to_string())?;
        let filename = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let (meta, content) = frontmatter::parse_note(&raw, &filename);

        let mut writer_guard = self.writer.lock().map_err(|e| e.to_string())?;
        let writer = writer_guard.as_mut().ok_or("Writer not available")?;

        // Delete old entry
        let term = tantivy::Term::from_field_text(self.path_field, path);
        writer.delete_term(term);

        // Add updated
        let mut doc = TantivyDocument::new();
        doc.add_text(self.path_field, path);
        doc.add_text(self.title_field, &meta.title);
        doc.add_text(self.body_field, &content);
        doc.add_text(self.tags_field, &meta.tags.join(" "));
        let _ = writer.add_document(doc);

        writer.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn remove_note(&self, path: &str) -> Result<(), String> {
        let mut writer_guard = self.writer.lock().map_err(|e| e.to_string())?;
        let writer = writer_guard.as_mut().ok_or("Writer not available")?;
        let term = tantivy::Term::from_field_text(self.path_field, path);
        writer.delete_term(term);
        writer.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
        let reader = self.index.reader().map_err(|e| e.to_string())?;
        let searcher = reader.searcher();

        let fields = vec![self.title_field, self.body_field, self.tags_field];
        // Tokenize the query with the SAME CJK-aware analyzer used for indexing, so a
        // Chinese/Japanese/Korean query becomes the same uni/bigram tokens as the docs.
        // (For pure-ASCII queries this yields the same lowercased word tokens as before.)
        let mut analyzer = self
            .index
            .tokenizers()
            .get("cjk")
            .ok_or("cjk tokenizer not registered")?;
        let terms: Vec<String> = {
            let mut out = Vec::new();
            let mut stream = analyzer.token_stream(query_str);
            while stream.advance() {
                out.push(stream.token().text.clone());
            }
            out
        };

        // For each term, OR queries across all fields, then AND all terms together.
        // CJK tokens (uni/bigrams) already encode segmentation -> exact term match.
        // Latin tokens keep prefix + fuzzy matching (unchanged English behaviour).
        let term_queries: Vec<(Occur, Box<dyn Query>)> = terms
            .iter()
            .map(|term| {
                let is_cjk_term = term.chars().any(is_cjk);
                let field_queries: Vec<(Occur, Box<dyn Query>)> = fields
                    .iter()
                    .flat_map(|&field| {
                        if is_cjk_term {
                            let exact: Box<dyn Query> = Box::new(TermQuery::new(
                                Term::from_field_text(field, term),
                                IndexRecordOption::WithFreqs,
                            ));
                            vec![(Occur::Should, exact)]
                        } else {
                            let prefix: Box<dyn Query> = Box::new(PhrasePrefixQuery::new(
                                vec![Term::from_field_text(field, term)],
                            ));
                            let fuzzy: Box<dyn Query> = Box::new(FuzzyTermQuery::new(
                                Term::from_field_text(field, term),
                                1,
                                true,
                            ));
                            vec![(Occur::Should, prefix), (Occur::Should, fuzzy)]
                        }
                    })
                    .collect();
                let combined: Box<dyn Query> = Box::new(BooleanQuery::new(field_queries));
                (Occur::Must, combined)
            })
            .collect();

        let query = BooleanQuery::new(term_queries);

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit))
            .map_err(|e| e.to_string())?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address).map_err(|e| e.to_string())?;

            let path = doc
                .get_first(self.path_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let title = doc
                .get_first(self.title_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            results.push(SearchResult {
                path,
                title,
                snippet: String::new(),
                score,
            });
        }

        Ok(results)
    }
}
