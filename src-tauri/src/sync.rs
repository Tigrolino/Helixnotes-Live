// WebDAV sync provider.
//
// Model: the vault stays local; this mirrors it to/from a user-configured WebDAV
// remote (Nextcloud/ownCloud/NAS). A local manifest (.helixnotes/sync_state.json)
// records what was in sync last time, so we can do a three-way diff (local vs remote
// vs manifest) and resolve each file as upload/download/delete, with keep-both
// conflict copies so nothing is ever lost.
//
// Synced set: every `*.md` in the vault tree, `.helixnotes/attachments/`, and
// `.helixnotes/notebook_icons.json`. Search indexes, trash, history, other metadata,
// and the manifest itself remain local-only.

use crate::state::AppState;
use crate::vault::operations::helixnotes_dir;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{Emitter, Manager};
use walkdir::WalkDir;

// ───────────────────────── config ─────────────────────────

pub struct WebdavConfig {
    pub url: String,
    pub username: String,
    pub password: String,
}

#[derive(Default, Clone, Serialize)]
pub struct SyncSummary {
    pub uploaded: u32,
    pub downloaded: u32,
    pub deleted_local: u32,
    pub deleted_remote: u32,
    pub conflicts: u32,
}

// ───────────────────────── manifest ─────────────────────────

#[derive(Default, Serialize, Deserialize)]
struct SyncManifest {
    files: HashMap<String, ManifestEntry>, // relpath (forward-slash) -> entry
}

#[derive(Clone, Serialize, Deserialize)]
struct ManifestEntry {
    local_hash: String,  // sha256 of local content at last sync
    remote_etag: String, // remote etag at last sync
}

fn manifest_path(vault: &str) -> PathBuf {
    helixnotes_dir(vault).join("sync_state.json")
}

fn load_manifest(vault: &str) -> SyncManifest {
    fs::read_to_string(manifest_path(vault))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_manifest(vault: &str, m: &SyncManifest) {
    if let Ok(s) = serde_json::to_string(m) {
        let _ = fs::write(manifest_path(vault), s);
    }
}

// ───────────────────────── helpers ─────────────────────────

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn normalize_etag(s: &str) -> String {
    s.trim()
        .trim_start_matches("W/")
        .trim_matches('"')
        .to_string()
}

/// Percent-encode each path segment, keeping the `/` separators.
fn encode_path(relpath: &str) -> String {
    relpath
        .split('/')
        .map(|seg| urlencoding::encode(seg).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Build the conflict-copy relpath: `dir/Note (conflict <stamp>).md`.
fn conflict_relpath(relpath: &str, stamp: &str) -> String {
    let (dir, name) = match relpath.rfind('/') {
        Some(i) => (&relpath[..=i], &relpath[i + 1..]),
        None => ("", relpath),
    };
    let (stem, ext) = match name.rfind('.') {
        Some(i) => (&name[..i], &name[i..]),
        None => (name, ""),
    };
    format!("{}{} (conflict {}){}", dir, stem, stamp, ext)
}

fn write_local(vault: &Path, relpath: &str, bytes: &[u8]) -> Result<(), String> {
    let target = vault.join(relpath);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&target, bytes).map_err(|e| e.to_string())
}

struct LocalFile {
    hash: String,
    path: PathBuf,
}

/// The synced set: `*.md` anywhere outside `.helixnotes/`, everything under
/// `.helixnotes/attachments/`, and the notebook icon mapping. Applied to BOTH local
/// and remote so pointing at a folder with unrelated files never imports them.
fn is_synced_relpath(rel: &str) -> bool {
    if rel.starts_with(".helixnotes/") {
        rel.starts_with(".helixnotes/attachments/") || rel == ".helixnotes/notebook_icons.json"
    } else {
        rel.ends_with(".md")
    }
}

fn scan_local(vault: &str) -> Result<HashMap<String, LocalFile>, String> {
    let vault_path = Path::new(vault);
    let mut map = HashMap::new();
    for entry in WalkDir::new(vault_path).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let rel = match p.strip_prefix(vault_path) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        if !is_synced_relpath(&rel) {
            continue;
        }
        let bytes = match fs::read(p) {
            Ok(b) => b,
            Err(_) => continue,
        };
        map.insert(
            rel,
            LocalFile {
                hash: sha256_hex(&bytes),
                path: p.to_path_buf(),
            },
        );
    }
    Ok(map)
}

// ───────────────────────── WebDAV client ─────────────────────────

struct RemoteEntry {
    relpath: String,
    etag: String,
    is_dir: bool,
}

struct WebdavClient {
    base: String,      // normalized, no trailing slash
    base_path: String, // path component of base, no trailing slash (percent-encoded)
    client: reqwest::blocking::Client,
    user: String,
    pass: String,
}

impl WebdavClient {
    fn new(cfg: WebdavConfig) -> Result<Self, String> {
        let base = cfg.url.trim().trim_end_matches('/').to_string();
        if !base.starts_with("http://") && !base.starts_with("https://") {
            return Err("WebDAV URL must start with http:// or https://".to_string());
        }
        let base_url = reqwest::Url::parse(&base).map_err(|e| format!("Invalid URL: {e}"))?;
        let base_path = base_url.path().trim_end_matches('/').to_string();
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            base,
            base_path,
            client,
            user: cfg.username,
            pass: cfg.password,
        })
    }

    fn url_for(&self, relpath: &str) -> String {
        if relpath.is_empty() {
            self.base.clone()
        } else {
            format!("{}/{}", self.base, encode_path(relpath))
        }
    }

    fn propfind(&self, relpath: &str, depth: &str) -> Result<Vec<RemoteEntry>, String> {
        let method = reqwest::Method::from_bytes(b"PROPFIND").unwrap();
        let body = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:prop><d:getetag/><d:resourcetype/></d:prop></d:propfind>"#;
        let resp = self
            .client
            .request(method, self.url_for(relpath))
            .basic_auth(&self.user, Some(&self.pass))
            .header("Depth", depth)
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(body)
            .send()
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        if status.as_u16() == 401 {
            return Err("Authentication failed (check username / app password)".to_string());
        }
        if status.as_u16() == 404 {
            return Err("WebDAV path not found (check the URL)".to_string());
        }
        if !status.is_success() {
            return Err(format!("WebDAV PROPFIND failed: HTTP {}", status.as_u16()));
        }
        let text = resp.text().map_err(|e| e.to_string())?;
        parse_multistatus(&text, &self.base_path)
    }

    /// Recursively list all files (relpath -> etag) by walking collections at Depth 1
    /// (Depth: infinity is disabled on some servers, so we walk instead).
    fn list(&self) -> Result<HashMap<String, String>, String> {
        let mut files = HashMap::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: Vec<String> = vec![String::new()];
        while let Some(dir) = queue.pop() {
            if !visited.insert(dir.clone()) {
                continue;
            }
            for e in self.propfind(&dir, "1")? {
                if e.relpath == dir {
                    continue; // the collection itself
                }
                if e.is_dir {
                    if !visited.contains(&e.relpath) {
                        queue.push(e.relpath);
                    }
                } else {
                    files.insert(e.relpath, e.etag);
                }
            }
        }
        Ok(files)
    }

    fn get(&self, relpath: &str) -> Result<Vec<u8>, String> {
        let resp = self
            .client
            .get(self.url_for(relpath))
            .basic_auth(&self.user, Some(&self.pass))
            .send()
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!(
                "GET {} failed: HTTP {}",
                relpath,
                resp.status().as_u16()
            ));
        }
        Ok(resp.bytes().map_err(|e| e.to_string())?.to_vec())
    }

    fn put(&self, relpath: &str, bytes: &[u8]) -> Result<Option<String>, String> {
        let resp = self
            .client
            .put(self.url_for(relpath))
            .basic_auth(&self.user, Some(&self.pass))
            .body(bytes.to_vec())
            .send()
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!(
                "PUT {} failed: HTTP {}",
                relpath,
                resp.status().as_u16()
            ));
        }
        Ok(resp
            .headers()
            .get("etag")
            .and_then(|h| h.to_str().ok())
            .map(normalize_etag))
    }

    fn delete(&self, relpath: &str) -> Result<(), String> {
        let resp = self
            .client
            .delete(self.url_for(relpath))
            .basic_auth(&self.user, Some(&self.pass))
            .send()
            .map_err(|e| e.to_string())?;
        let s = resp.status();
        if s.is_success() || s.as_u16() == 404 {
            Ok(())
        } else {
            Err(format!("DELETE {} failed: HTTP {}", relpath, s.as_u16()))
        }
    }

    fn mkcol(&self, relpath: &str) -> Result<(), String> {
        let method = reqwest::Method::from_bytes(b"MKCOL").unwrap();
        let resp = self
            .client
            .request(method, self.url_for(relpath))
            .basic_auth(&self.user, Some(&self.pass))
            .send()
            .map_err(|e| e.to_string())?;
        let s = resp.status().as_u16();
        // 201 created; 405 already exists; 301 redirect-to-existing;
        // 403 some servers (openmediavault et al.) return 403 instead of 405.
        if resp.status().is_success() || s == 405 || s == 301 || s == 403 {
            Ok(())
        } else {
            Err(format!("MKCOL {} failed: HTTP {}", relpath, s))
        }
    }

    /// Ensure every ancestor collection of `relpath` exists on the remote.
    fn ensure_dirs(&self, relpath: &str, created: &mut HashSet<String>) -> Result<(), String> {
        let parts: Vec<&str> = relpath.split('/').collect();
        if parts.len() <= 1 {
            return Ok(());
        }
        let mut cur = String::new();
        for part in &parts[..parts.len() - 1] {
            if !cur.is_empty() {
                cur.push('/');
            }
            cur.push_str(part);
            if created.insert(cur.clone()) {
                self.mkcol(&cur)?;
            }
        }
        Ok(())
    }

    /// Read a file's authoritative remote etag via a Depth-0 PROPFIND. The PUT-response
    /// etag is unreliable across WebDAV servers (some omit it, some differ in format from
    /// the getetag PROPFIND returns), so we always source the manifest etag from PROPFIND
    /// to keep it consistent with future listings. (issue #147)
    fn etag_for(&self, relpath: &str) -> String {
        self.propfind(relpath, "0")
            .ok()
            .and_then(|es| es.into_iter().find(|e| !e.is_dir).map(|e| e.etag))
            .unwrap_or_default()
    }

    /// PUT a file (creating parent collections first), then read back its authoritative
    /// etag via PROPFIND. Returns the PROPFIND etag (empty if the server exposes no
    /// getetag), NOT the PUT-response etag, which is unreliable across servers. (issue #147)
    fn upload(
        &self,
        relpath: &str,
        bytes: &[u8],
        created: &mut HashSet<String>,
    ) -> Result<String, String> {
        self.ensure_dirs(relpath, created)?;
        self.put(relpath, bytes)?; // PUT-response etag intentionally discarded
        Ok(self.etag_for(relpath))
    }
}

/// Map a WebDAV `href` to a vault-relative path (forward slash).
/// `Ok(None)` is the base collection itself. `Err` means the href could not be mapped
/// (e.g. it does not sit under the configured base path, as some providers like Infomaniak
/// kDrive return). Callers MUST treat `Err` as a hard failure: silently dropping an
/// unmappable href makes that file look deleted on the remote, and the reconciler then
/// wipes the local copy. (issue #102)
fn href_to_relpath(href: &str, base_path: &str) -> Result<Option<String>, String> {
    // Pull out just the path component (some servers return absolute-URL hrefs).
    let raw = if href.starts_with("http://") || href.starts_with("https://") {
        reqwest::Url::parse(href)
            .map_err(|e| format!("invalid remote href URL '{href}': {e}"))?
            .path()
            .to_string()
    } else {
        href.to_string()
    };
    // Compare DECODED paths, not the raw percent-encoded strings. Providers differ in how they
    // encode hrefs: Infomaniak kDrive lowercases the hex (`%5b`/`%5d`) and encodes spaces, while
    // reqwest emits uppercase hex (`%5B`/`%5D`) for base_path. A raw strip_prefix then spuriously
    // fails, every remote file looks deleted, and the reconciler wipes the local copies. Decoding
    // both sides first makes the prefix match encoding-agnostic. (issue #102)
    let path = urlencoding::decode(raw.trim_end_matches('/'))
        .map_err(|e| format!("bad percent-encoding in remote path '{raw}': {e}"))?
        .into_owned();
    let base = urlencoding::decode(base_path.trim_end_matches('/'))
        .map_err(|e| format!("bad percent-encoding in WebDAV base path '{base_path}': {e}"))?
        .into_owned();
    let rest = match path.strip_prefix(&base) {
        Some(r) => r.trim_start_matches('/'),
        None => {
            return Err(format!(
                "remote path '{path}' is not under the configured WebDAV folder '{base}'. \
                 Aborting before deleting anything locally - please report this URL."
            ))
        }
    };
    if rest.is_empty() {
        return Ok(None); // the base collection itself
    }
    Ok(Some(rest.to_string()))
}

fn parse_multistatus(xml: &str, base_path: &str) -> Result<Vec<RemoteEntry>, String> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    #[derive(PartialEq)]
    enum Cap {
        None,
        Href,
        Etag,
    }

    let mut reader = Reader::from_str(xml);
    let mut entries = Vec::new();
    let mut href: Option<String> = None;
    let mut etag: Option<String> = None;
    let mut is_dir = false;
    let mut cap = Cap::None;
    let mut buf = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"response" => {
                    href = None;
                    etag = None;
                    is_dir = false;
                }
                b"href" => {
                    cap = Cap::Href;
                    buf.clear();
                }
                b"getetag" => {
                    cap = Cap::Etag;
                    buf.clear();
                }
                b"collection" => is_dir = true,
                _ => {}
            },
            Ok(Event::Empty(e)) => {
                if e.local_name().as_ref() == b"collection" {
                    is_dir = true;
                }
            }
            Ok(Event::Text(e)) => {
                if cap != Cap::None {
                    if let Ok(decoded) = e.decode() {
                        if let Ok(text) = quick_xml::escape::unescape(&decoded) {
                            buf.push_str(&text);
                        }
                    }
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"href" => {
                    href = Some(buf.clone());
                    cap = Cap::None;
                }
                b"getetag" => {
                    etag = Some(buf.clone());
                    cap = Cap::None;
                }
                b"response" => {
                    if let Some(h) = href.take() {
                        if let Some(rel) = href_to_relpath(&h, base_path)? {
                            entries.push(RemoteEntry {
                                relpath: rel,
                                etag: etag.take().map(|s| normalize_etag(&s)).unwrap_or_default(),
                                is_dir,
                            });
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {e}")),
            _ => {}
        }
    }
    Ok(entries)
}

// ───────────────────────── sync engine ─────────────────────────

fn apply_changes(
    client: &WebdavClient,
    vault: &str,
    local: &HashMap<String, LocalFile>,
    remote: &HashMap<String, String>,
    manifest: &SyncManifest,
) -> Result<(SyncManifest, SyncSummary), String> {
    let vault_path = Path::new(vault);
    let mut new_m = SyncManifest::default();
    let mut sum = SyncSummary::default();
    let mut created: HashSet<String> = HashSet::new();
    let stamp = chrono::Local::now().format("%Y-%m-%d %H-%M-%S").to_string();

    let mut keys: HashSet<&String> = HashSet::new();
    keys.extend(local.keys());
    keys.extend(remote.keys());
    keys.extend(manifest.files.keys());

    for key in keys {
        let l = local.get(key);
        let r = remote.get(key);
        let m = manifest.files.get(key);

        match (l, r) {
            (Some(lf), Some(re)) => {
                let local_changed = match m {
                    Some(entry) => entry.local_hash != lf.hash,
                    None => true,
                };
                let remote_changed = match m {
                    Some(entry) => &entry.remote_etag != re,
                    None => true,
                };
                if !local_changed && !remote_changed {
                    new_m.files.insert(
                        key.clone(),
                        ManifestEntry {
                            local_hash: lf.hash.clone(),
                            remote_etag: re.clone(),
                        },
                    );
                } else if local_changed && !remote_changed {
                    let bytes = fs::read(&lf.path).map_err(|e| e.to_string())?;
                    let etag = client.upload(key, &bytes, &mut created)?;
                    new_m.files.insert(
                        key.clone(),
                        ManifestEntry {
                            local_hash: lf.hash.clone(),
                            remote_etag: etag,
                        },
                    );
                    sum.uploaded += 1;
                } else if !local_changed && remote_changed {
                    let bytes = client.get(key)?;
                    write_local(vault_path, key, &bytes)?;
                    new_m.files.insert(
                        key.clone(),
                        ManifestEntry {
                            local_hash: sha256_hex(&bytes),
                            remote_etag: re.clone(),
                        },
                    );
                    sum.downloaded += 1;
                } else {
                    // both changed: compare contents; identical => no real conflict.
                    let rbytes = client.get(key)?;
                    let rhash = sha256_hex(&rbytes);
                    if rhash == lf.hash {
                        new_m.files.insert(
                            key.clone(),
                            ManifestEntry {
                                local_hash: lf.hash.clone(),
                                remote_etag: re.clone(),
                            },
                        );
                    } else {
                        // keep-both: remote becomes canonical; local saved as a conflict copy.
                        let lbytes = fs::read(&lf.path).map_err(|e| e.to_string())?;
                        write_local(vault_path, key, &rbytes)?;
                        new_m.files.insert(
                            key.clone(),
                            ManifestEntry {
                                local_hash: rhash,
                                remote_etag: re.clone(),
                            },
                        );
                        let ckey = conflict_relpath(key, &stamp);
                        write_local(vault_path, &ckey, &lbytes)?;
                        let cetag = client.upload(&ckey, &lbytes, &mut created)?;
                        new_m.files.insert(
                            ckey,
                            ManifestEntry {
                                local_hash: sha256_hex(&lbytes),
                                remote_etag: cetag,
                            },
                        );
                        sum.conflicts += 1;
                    }
                }
            }
            (Some(lf), None) => match m {
                None => {
                    // new local -> upload
                    let bytes = fs::read(&lf.path).map_err(|e| e.to_string())?;
                    let etag = client.upload(key, &bytes, &mut created)?;
                    new_m.files.insert(
                        key.clone(),
                        ManifestEntry {
                            local_hash: lf.hash.clone(),
                            remote_etag: etag,
                        },
                    );
                    sum.uploaded += 1;
                }
                Some(me) => {
                    if me.local_hash != lf.hash {
                        // remote deleted but local edited -> resurrect (upload)
                        let bytes = fs::read(&lf.path).map_err(|e| e.to_string())?;
                        let etag = client.upload(key, &bytes, &mut created)?;
                        new_m.files.insert(
                            key.clone(),
                            ManifestEntry {
                                local_hash: lf.hash.clone(),
                                remote_etag: etag,
                            },
                        );
                        sum.uploaded += 1;
                    } else {
                        // remote deleted, local unchanged -> delete local
                        let _ = fs::remove_file(&lf.path);
                        sum.deleted_local += 1;
                    }
                }
            },
            (None, Some(re)) => match m {
                None => {
                    // new remote -> download
                    let bytes = client.get(key)?;
                    write_local(vault_path, key, &bytes)?;
                    new_m.files.insert(
                        key.clone(),
                        ManifestEntry {
                            local_hash: sha256_hex(&bytes),
                            remote_etag: re.clone(),
                        },
                    );
                    sum.downloaded += 1;
                }
                Some(me) => {
                    if &me.remote_etag != re {
                        // local deleted but remote changed -> resurrect (download)
                        let bytes = client.get(key)?;
                        write_local(vault_path, key, &bytes)?;
                        new_m.files.insert(
                            key.clone(),
                            ManifestEntry {
                                local_hash: sha256_hex(&bytes),
                                remote_etag: re.clone(),
                            },
                        );
                        sum.downloaded += 1;
                    } else {
                        // local deleted, remote unchanged -> delete remote
                        client.delete(key)?;
                        sum.deleted_remote += 1;
                    }
                }
            },
            (None, None) => {
                // gone on both sides -> drop from manifest
            }
        }
    }
    Ok((new_m, sum))
}

/// Validate the connection (auth + URL) with a cheap PROPFIND.
pub fn test_connection(cfg: WebdavConfig) -> Result<String, String> {
    let client = WebdavClient::new(cfg)?;
    client.propfind("", "0")?;
    Ok("Connection successful".to_string())
}

/// Run a full sync. Mutes the file watcher while applying local writes, then
/// rebuilds the search index. Returns a summary of what changed.
pub fn run_sync(
    app: tauri::AppHandle,
    vault: String,
    cfg: WebdavConfig,
) -> Result<SyncSummary, String> {
    use std::sync::atomic::Ordering;

    let state = app.state::<AppState>();
    let client = WebdavClient::new(cfg)?;

    let _ = app.emit("sync-progress", serde_json::json!({ "status": "listing" }));
    let remote: HashMap<String, String> = client
        .list()?
        .into_iter()
        .filter(|(k, _)| is_synced_relpath(k))
        .collect();
    let local = scan_local(&vault)?;
    let manifest = load_manifest(&vault);

    // Safety guard (issue #102): never treat an empty remote listing as "everything was
    // deleted remotely". If a prior sync exists (non-empty manifest) but the remote came back
    // empty, it is almost certainly a listing failure (e.g. a provider whose paths we can't
    // map), not a real mass-deletion - so abort instead of wiping local notes.
    if remote.is_empty() && !manifest.files.is_empty() {
        return Err(
            "Sync aborted to protect your notes: the WebDAV server returned an empty file list \
             even though files were synced before, so no local files were deleted. This usually \
             means the server reports paths in a format the app can't read yet - please report \
             your WebDAV provider and folder URL."
                .to_string(),
        );
    }

    let _ = app.emit("sync-progress", serde_json::json!({ "status": "syncing" }));
    // Mute the watcher across local writes so a pull doesn't storm the reindex path.
    state.importing.store(true, Ordering::Relaxed);
    let applied = apply_changes(&client, &vault, &local, &remote, &manifest);
    state.importing.store(false, Ordering::Relaxed);

    let (new_manifest, summary) = applied?;
    save_manifest(&vault, &new_manifest);

    // Re-index after files changed (search is non-critical; ignore failures).
    if let Ok(guard) = state.search_index.lock() {
        if let Some(search) = guard.as_ref() {
            let _ = search.rebuild(&vault);
        }
    }

    // The watcher was muted during apply, so nudge the UI to refresh its lists.
    let _ = app.emit(
        "file-changed",
        crate::types::FileEvent {
            event_type: "modify".to_string(),
            path: vault.clone(),
        },
    );
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::is_synced_relpath;

    #[test]
    fn syncs_notebook_icon_mapping_and_assets_only() {
        for path in [
            "Notes/plan.md",
            ".helixnotes/attachments/notebook-icon.png",
            ".helixnotes/notebook_icons.json",
        ] {
            assert!(is_synced_relpath(path), "expected {path} to be synced");
        }

        for path in [
            "Notes/image.png",
            ".helixnotes/sync_state.json",
            ".helixnotes/notebook_icons.json.bak",
            ".helixnotes/attachments-old/icon.png",
        ] {
            assert!(!is_synced_relpath(path), "expected {path} to stay local");
        }
    }
}
