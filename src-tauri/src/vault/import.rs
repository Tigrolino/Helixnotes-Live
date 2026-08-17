use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::Utc;
use gray_matter::engine::YAML;
use gray_matter::Matter;
use regex::Regex;
use uuid::Uuid;

use crate::types::{ImportResult, NoteMeta};
use crate::vault::frontmatter;

pub fn import(vault_path: &str) -> Result<ImportResult, String> {
    let vault = Path::new(vault_path);

    let file_index = build_file_index(vault);

    let md_files: Vec<_> = walkdir::WalkDir::new(vault)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            p.is_file()
                && p.extension().and_then(|x| x.to_str()) == Some("md")
                && !p.to_string_lossy().contains("/.helixnotes/")
                && !p.to_string_lossy().contains("/.trash/")
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    let wiki_embed_re =
        Regex::new(r"!\[\[([^\]|]+?)(?:\|([^\]]*))?\]\]").map_err(|e| e.to_string())?;
    let wiki_link_re =
        Regex::new(r"\[\[([^\]|]+?)(?:\|([^\]]*))?\]\]").map_err(|e| e.to_string())?;
    let highlight_re = Regex::new(r"==([^\n=]+?)==").map_err(|e| e.to_string())?;
    let comment_re = Regex::new(r"(?s)%%([\s\S]*?)%%").map_err(|e| e.to_string())?;
    let md_img_re = Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").map_err(|e| e.to_string())?;
    let md_link_re = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").map_err(|e| e.to_string())?;

    let mut result = ImportResult {
        files_converted: 0,
        links_converted: 0,
        frontmatter_normalized: 0,
        syntax_converted: 0,
        attachments_moved: 0,
    };

    for path in &md_files {
        let raw = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let note_dir = path.parent().unwrap_or(Path::new(""));

        let mut content = raw.clone();
        let mut changed = false;

        let renamed = rename_deprecated_properties(&content);
        if renamed != content {
            content = renamed;
            changed = true;
        }

        let (meta, body) = normalize_frontmatter(&content, path);
        let normalized = frontmatter::merge_frontmatter(&content, &meta, &body);
        if normalized != content {
            content = normalized;
            changed = true;
            result.frontmatter_normalized += 1;
        }

        let after_syntax = convert_syntax(&content, &highlight_re, &comment_re);
        if after_syntax != content {
            content = after_syntax;
            changed = true;
            result.syntax_converted += 1;
        }

        let links_before = result.links_converted;

        let after_embeds = wiki_embed_re
            .replace_all(&content, |caps: &regex::Captures| {
                let file_ref = &caps[1];
                let alt_param = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                let (file_part, anchor) = split_anchor(file_ref);
                if file_part.is_empty() {
                    if let Some(a) = anchor {
                        let display = if alt_param.is_empty() { a } else { alt_param };
                        result.links_converted += 1;
                        return format!("[{}](#{})", display, a);
                    }
                }
                let resolved = resolve_wiki_ref(&file_index, file_part);
                let abs_target = vault.join(&resolved);
                let rel = pathdiff(
                    abs_target.to_str().unwrap_or(""),
                    note_dir.to_str().unwrap_or(""),
                )
                .unwrap_or(resolved);
                let encoded = rel.replace(' ', "%20");
                let link_target = match anchor {
                    Some(a) => format!("{}#{}", encoded, a),
                    None => encoded,
                };
                let ext = Path::new(file_part)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let is_embeddable = matches!(
                    ext.as_str(),
                    "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" | "ico" | "pdf"
                );
                if is_embeddable {
                    let alt = if is_dimension_spec(alt_param) { "" } else { alt_param };
                    result.links_converted += 1;
                    format!("![{}]({})", alt, link_target)
                } else {
                    let display = if alt_param.is_empty() || is_dimension_spec(alt_param) {
                        file_part.rsplit('/').next().unwrap_or(file_part)
                    } else {
                        alt_param
                    };
                    result.links_converted += 1;
                    format!("[{}]({})", display, link_target)
                }
            })
            .to_string();
        content = after_embeds;

        let after_links = wiki_link_re
            .replace_all(&content, |caps: &regex::Captures| {
                let note_ref = &caps[1];
                let display_param = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                let (file_part, anchor) = split_anchor(note_ref);
                if file_part.is_empty() {
                    if let Some(a) = anchor {
                        let display = if display_param.is_empty() { a } else { display_param };
                        result.links_converted += 1;
                        return format!("[{}](#{})", display, a);
                    }
                }
                let resolved = resolve_wiki_ref(&file_index, file_part);
                let abs_target = vault.join(&resolved);
                let rel = pathdiff(
                    abs_target.to_str().unwrap_or(""),
                    note_dir.to_str().unwrap_or(""),
                )
                .unwrap_or(resolved);
                let encoded = rel.replace(' ', "%20");
                let link_target = match anchor {
                    Some(a) => format!("{}#{}", encoded, a),
                    None => encoded,
                };
                let display = if display_param.is_empty() {
                    note_ref
                } else {
                    display_param
                };
                result.links_converted += 1;
                format!("[{}]({})", display, link_target)
            })
            .to_string();
        content = after_links;

        content = fix_md_image_refs(&content, &md_img_re, vault, note_dir, &file_index, &mut result.links_converted);
        content = fix_md_link_refs(&content, &md_link_re, vault, note_dir, &file_index, &mut result.links_converted);

        if result.links_converted > links_before {
            changed = true;
        }

        if changed {
            let _ = std::fs::write(path, &content);
            result.files_converted += 1;
        }
    }

    let moved = move_attachments(vault)?;
    result.attachments_moved = moved.len() as u64;

    if !moved.is_empty() {
        rewrite_attachment_refs(vault, &moved)?;
    }

    cleanup_empty_dirs(vault);

    Ok(result)
}

fn build_file_index(vault: &Path) -> HashMap<String, String> {
    let mut index = HashMap::new();
    for entry in walkdir::WalkDir::new(vault)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Ok(rel) = path.strip_prefix(vault) {
                index.insert(name.to_string(), rel.to_string_lossy().to_string());
            }
        }
    }
    index
}

fn rename_deprecated_properties(raw: &str) -> String {
    let matter = Matter::<YAML>::new();
    let result = matter.parse(raw);

    if result.matter.trim().is_empty() {
        return raw.to_string();
    }

    let mut mapping: serde_yaml::Mapping = match serde_yaml::from_str(&result.matter) {
        Ok(m) => m,
        Err(_) => return raw.to_string(),
    };

    let mut renamed = false;
    for (old, new) in &[("alias", "aliases"), ("cssclass", "cssclasses")] {
        let old_key = serde_yaml::Value::String((*old).into());
        let new_key = serde_yaml::Value::String((*new).into());
        if !mapping.contains_key(&new_key) {
            if let Some(val) = mapping.remove(&old_key) {
                mapping.insert(new_key, val);
                renamed = true;
            }
        } else {
            if mapping.remove(&old_key).is_some() {
                renamed = true;
            }
        }
    }

    if !renamed {
        return raw.to_string();
    }

    let yaml_str = match serde_yaml::to_string(&serde_yaml::Value::Mapping(mapping)) {
        Ok(s) => s,
        Err(_) => return raw.to_string(),
    };

    format!("---\n{}---\n{}", yaml_str, result.content)
}

fn normalize_frontmatter(raw: &str, path: &Path) -> (NoteMeta, String) {
    let matter = Matter::<YAML>::new();
    let result = matter.parse(raw);
    let yaml_str = result.matter.trim();

    let mapping: serde_yaml::Mapping = if yaml_str.is_empty() {
        serde_yaml::Mapping::new()
    } else {
        match serde_yaml::from_str::<serde_yaml::Mapping>(yaml_str) {
            Ok(m) => m,
            Err(_) => serde_yaml::Mapping::new(),
        }
    };

    let tags = normalize_tags(&mapping);

    let title = mapping
        .get(serde_yaml::Value::String("title".into()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            if let Some(h) = extract_heading_title(&result.content) {
                h
            } else {
                let fname = path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Untitled");
                frontmatter::filename_to_title(fname)
            }
        });

    let id = mapping
        .get(serde_yaml::Value::String("id".into()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let created = ["created", "date", "ctime"]
        .iter()
        .find_map(|key| {
            mapping
                .get(serde_yaml::Value::String((*key).into()))
                .and_then(|v| v.as_str())
                .and_then(frontmatter::parse_date_flexible)
        })
        .or_else(|| file_created(path))
        .unwrap_or_else(Utc::now);

    let modified = ["modified", "mtime", "lastmod"]
        .iter()
        .find_map(|key| {
            mapping
                .get(serde_yaml::Value::String((*key).into()))
                .and_then(|v| v.as_str())
                .and_then(frontmatter::parse_date_flexible)
        })
        .or_else(|| file_modified(path))
        .unwrap_or_else(Utc::now);

    let pinned = mapping
        .get(serde_yaml::Value::String("pinned".into()))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let meta = NoteMeta {
        id,
        title,
        tags,
        pinned,
        created,
        modified,
    };
    (meta, result.content)
}

fn normalize_tags(mapping: &serde_yaml::Mapping) -> Vec<String> {
    let mut tags = Vec::new();
    let mut seen = HashSet::new();

    for key in &["tags", "tag"] {
        if let Some(val) = mapping.get(serde_yaml::Value::String((*key).into())) {
            for raw in yaml_value_to_strings(val) {
                let cleaned = raw.trim().trim_start_matches('#').trim().to_string();
                if !cleaned.is_empty() && seen.insert(cleaned.to_lowercase()) {
                    tags.push(cleaned);
                }
            }
        }
    }
    tags
}

fn yaml_value_to_strings(val: &serde_yaml::Value) -> Vec<String> {
    match val {
        serde_yaml::Value::String(s) => {
            if s.contains(',') {
                s.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect()
            } else {
                vec![s.clone()]
            }
        }
        serde_yaml::Value::Sequence(seq) => seq
            .iter()
            .filter_map(|v| match v {
                serde_yaml::Value::String(s) => Some(s.clone()),
                serde_yaml::Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .collect(),
        serde_yaml::Value::Number(n) => vec![n.to_string()],
        serde_yaml::Value::Bool(b) => vec![b.to_string()],
        _ => Vec::new(),
    }
}

fn extract_heading_title(body: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            return Some(rest.trim().to_string());
        }
        return None;
    }
    None
}

fn file_created(path: &Path) -> Option<chrono::DateTime<Utc>> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.created().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| {
            chrono::DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos())
        })
}

fn file_modified(path: &Path) -> Option<chrono::DateTime<Utc>> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| {
            chrono::DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos())
        })
}

fn convert_syntax(content: &str, highlight_re: &Regex, comment_re: &Regex) -> String {
    let segments = split_code_segments(content);
    let mut result = String::with_capacity(content.len());

    for (is_code, text) in segments {
        if is_code {
            result.push_str(&text);
        } else {
            let with_marks = highlight_re.replace_all(&text, "<mark>$1</mark>");
            let with_comments = comment_re.replace_all(&with_marks, "<!-- $1 -->");
            result.push_str(&with_comments);
        }
    }
    result
}

fn split_code_segments(content: &str) -> Vec<(bool, String)> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_code = false;
    let mut fence_char = '`';

    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_start();

        let is_fence = if in_code {
            trimmed.starts_with(&fence_char.to_string().repeat(3))
        } else if trimmed.starts_with("```") {
            fence_char = '`';
            true
        } else if trimmed.starts_with("~~~") {
            fence_char = '~';
            true
        } else {
            false
        };

        if is_fence {
            current.push_str(line);
            segments.push((in_code, std::mem::take(&mut current)));
            in_code = !in_code;
        } else {
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        segments.push((in_code, current));
    }
    segments
}

fn fix_md_image_refs(
    content: &str,
    re: &Regex,
    vault: &Path,
    note_dir: &Path,
    file_index: &HashMap<String, String>,
    links_converted: &mut u64,
) -> String {
    re.replace_all(content, |caps: &regex::Captures| {
        let alt = &caps[1];
        let src = &caps[2];
        let decoded = percent_decode(src);
        if decoded.starts_with("http") || decoded.starts_with('/') || decoded.starts_with("data:") {
            return format!("![{}]({})", alt, src);
        }
        if !decoded.contains('/') {
            if let Some(rel) = file_index.get(&decoded) {
                *links_converted += 1;
                let abs_target = vault.join(rel);
                let note_rel = pathdiff(
                    abs_target.to_str().unwrap_or(""),
                    note_dir.to_str().unwrap_or(""),
                )
                .unwrap_or(rel.clone());
                return format!("![{}]({})", alt, note_rel.replace(' ', "%20"));
            }
        }
        if decoded.contains('/') {
            let vault_abs = vault.join(&decoded);
            if vault_abs.exists() {
                let note_rel = pathdiff(
                    vault_abs.to_str().unwrap_or(""),
                    note_dir.to_str().unwrap_or(""),
                )
                .unwrap_or(decoded.clone());
                if note_rel != decoded {
                    *links_converted += 1;
                    return format!("![{}]({})", alt, note_rel.replace(' ', "%20"));
                }
            }
        }
        format!("![{}]({})", alt, src)
    })
    .to_string()
}

fn fix_md_link_refs(
    content: &str,
    re: &Regex,
    vault: &Path,
    note_dir: &Path,
    file_index: &HashMap<String, String>,
    links_converted: &mut u64,
) -> String {
    re.replace_all(content, |caps: &regex::Captures| {
        let display = &caps[1];
        let href = &caps[2];
        let decoded = percent_decode(href);
        if decoded.starts_with("http") || decoded.starts_with('/') || decoded.starts_with("data:")
            || decoded.starts_with('#')
        {
            return format!("[{}]({})", display, href);
        }
        if !decoded.contains('/') {
            if let Some(rel) = file_index.get(&decoded) {
                *links_converted += 1;
                let abs_target = vault.join(rel);
                let note_rel = pathdiff(
                    abs_target.to_str().unwrap_or(""),
                    note_dir.to_str().unwrap_or(""),
                )
                .unwrap_or(rel.clone());
                return format!("[{}]({})", display, note_rel.replace(' ', "%20"));
            }
        }
        if decoded.contains('/') {
            let vault_abs = vault.join(&decoded);
            if vault_abs.exists() {
                let note_rel = pathdiff(
                    vault_abs.to_str().unwrap_or(""),
                    note_dir.to_str().unwrap_or(""),
                )
                .unwrap_or(decoded.clone());
                if note_rel != decoded {
                    *links_converted += 1;
                    return format!("[{}]({})", display, note_rel.replace(' ', "%20"));
                }
            }
        }
        format!("[{}]({})", display, href)
    })
    .to_string()
}

fn move_attachments(
    vault: &Path,
) -> Result<HashMap<String, String>, String> {
    let attachments_dir = vault.join(".helixnotes").join("attachments");
    let _ = std::fs::create_dir_all(&attachments_dir);

    let attachment_files: Vec<_> = walkdir::WalkDir::new(vault)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            if !p.is_file() {
                return false;
            }
            if p.extension().and_then(|x| x.to_str()) == Some("md") {
                return false;
            }
            let path_str = p.to_string_lossy();
            !path_str.contains("/.helixnotes/")
                && !path_str.contains("/.obsidian/")
                && !path_str.contains("/.trash/")
                && !path_str.contains("/.stfolder")
                && !path_str.contains("/.claude/")
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    let mut moved: HashMap<String, String> = HashMap::new();

    for src_path in &attachment_files {
        let old_rel = match src_path.strip_prefix(vault) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };
        let new_rel = format!(".helixnotes/attachments/{}", old_rel);
        let dest_path = vault.join(&new_rel);

        if let Some(parent) = dest_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        if std::fs::rename(src_path, &dest_path).is_ok()
            || std::fs::copy(src_path, &dest_path)
                .and_then(|_| std::fs::remove_file(src_path))
                .is_ok()
        {
            moved.insert(old_rel, new_rel);
        }
    }
    Ok(moved)
}

fn rewrite_attachment_refs(
    vault: &Path,
    moved: &HashMap<String, String>,
) -> Result<(), String> {
    let md_ref = Regex::new(r"(!?\[[^\]]*\])\(([^)]+)\)").map_err(|e| e.to_string())?;

    let md_files: Vec<_> = walkdir::WalkDir::new(vault)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            p.is_file()
                && p.extension().and_then(|x| x.to_str()) == Some("md")
                && !p.to_string_lossy().contains("/.helixnotes/")
                && !p.to_string_lossy().contains("/.trash/")
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    for path in &md_files {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let note_dir = path.parent().unwrap_or(Path::new(""));

        let new_content = md_ref
            .replace_all(&content, |caps: &regex::Captures| {
                let prefix = &caps[1];
                let href = &caps[2];
                let decoded_href = percent_decode(href);

                if decoded_href.starts_with("http") || decoded_href.starts_with("data:") {
                    return format!("{}({})", prefix, href);
                }

                let abs_from_note = note_dir.join(&decoded_href);
                let rel_from_note = abs_from_note
                    .strip_prefix(vault)
                    .ok()
                    .map(|r| r.to_string_lossy().to_string());
                let rel_from_vault = decoded_href.clone();

                let old_vault_rel = rel_from_note
                    .as_ref()
                    .filter(|r| moved.contains_key(r.as_str()))
                    .cloned()
                    .or_else(|| {
                        if moved.contains_key(&rel_from_vault) {
                            Some(rel_from_vault)
                        } else {
                            None
                        }
                    });

                if let Some(old_rel) = old_vault_rel {
                    if let Some(new_rel) = moved.get(&old_rel) {
                        let new_abs = vault.join(new_rel);
                        let new_note_rel = pathdiff(
                            new_abs.to_str().unwrap_or(""),
                            note_dir.to_str().unwrap_or(""),
                        )
                        .unwrap_or(new_rel.clone());
                        return format!("{}({})", prefix, new_note_rel.replace(' ', "%20"));
                    }
                }
                format!("{}({})", prefix, href)
            })
            .to_string();

        if new_content != content {
            let _ = std::fs::write(path, new_content);
        }
    }
    Ok(())
}

fn cleanup_empty_dirs(root: &Path) {
    let mut dirs: Vec<_> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.path().to_path_buf())
        .collect();

    dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));

    for dir in dirs {
        let dir_str = dir.to_string_lossy();
        if dir_str.contains("/.helixnotes") || dir_str.contains("/.obsidian") || dir == root {
            continue;
        }
        if let Ok(mut entries) = std::fs::read_dir(&dir) {
            if entries.next().is_none() {
                let _ = std::fs::remove_dir(&dir);
            }
        }
    }
}

fn split_anchor(reference: &str) -> (&str, Option<&str>) {
    if let Some(idx) = reference.find('#') {
        let file_part = &reference[..idx];
        let anchor = &reference[idx + 1..];
        if anchor.is_empty() {
            (reference, None)
        } else {
            (file_part, Some(anchor))
        }
    } else {
        (reference, None)
    }
}

fn is_dimension_spec(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.chars().all(|c| c.is_ascii_digit() || c == 'x' || c == 'X')
}

fn resolve_wiki_ref(file_index: &HashMap<String, String>, reference: &str) -> String {
    let reference = reference.trim();
    let reference = reference.split('#').next().unwrap_or(reference).trim();
    let reference = reference.split('^').next().unwrap_or(reference).trim();

    if let Some(rel) = file_index.get(reference) {
        return rel.clone();
    }
    let with_md = format!("{}.md", reference);
    if let Some(rel) = file_index.get(&with_md) {
        return rel.clone();
    }
    if let Some(name) = reference.rsplit('/').next() {
        if let Some(rel) = file_index.get(name) {
            return rel.clone();
        }
        let name_md = format!("{}.md", name);
        if let Some(rel) = file_index.get(&name_md) {
            return rel.clone();
        }
    }
    reference.to_string()
}

fn pathdiff(target: &str, base: &str) -> Result<String, ()> {
    let target = Path::new(target);
    let base = Path::new(base);

    let target_components: Vec<_> = target.components().collect();
    let base_components: Vec<_> = base.components().collect();

    let common = target_components
        .iter()
        .zip(base_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut result = std::path::PathBuf::new();
    for _ in common..base_components.len() {
        result.push("..");
    }
    for component in &target_components[common..] {
        result.push(component);
    }
    Ok(result.to_string_lossy().to_string())
}

fn percent_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let hi = chars.next();
            let lo = chars.next();
            if let (Some(h), Some(l)) = (hi, lo) {
                if let Ok(byte) = u8::from_str_radix(&format!("{}{}", h as char, l as char), 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            result.push('%');
        } else {
            result.push(b as char);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_tags_comma_string() {
        let mapping: serde_yaml::Mapping = serde_yaml::from_str("tags: \"a, b, c\"").unwrap();
        let tags = normalize_tags(&mapping);
        assert_eq!(tags, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_normalize_tags_single_string() {
        let mapping: serde_yaml::Mapping = serde_yaml::from_str("tags: only-one").unwrap();
        let tags = normalize_tags(&mapping);
        assert_eq!(tags, vec!["only-one"]);
    }

    #[test]
    fn test_normalize_tags_array() {
        let mapping: serde_yaml::Mapping =
            serde_yaml::from_str("tags:\n  - alpha\n  - beta").unwrap();
        let tags = normalize_tags(&mapping);
        assert_eq!(tags, vec!["alpha", "beta"]);
    }

    #[test]
    fn test_normalize_tags_singular_tag_key() {
        let mapping: serde_yaml::Mapping = serde_yaml::from_str("tag: solo").unwrap();
        let tags = normalize_tags(&mapping);
        assert_eq!(tags, vec!["solo"]);
    }

    #[test]
    fn test_normalize_tags_merge_singular_and_plural() {
        let mapping: serde_yaml::Mapping =
            serde_yaml::from_str("tags:\n  - a\n  - b\ntag: c").unwrap();
        let tags = normalize_tags(&mapping);
        assert_eq!(tags, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_normalize_tags_dedup_case_insensitive() {
        let mapping: serde_yaml::Mapping = serde_yaml::from_str("tags: [Foo, foo]").unwrap();
        let tags = normalize_tags(&mapping);
        assert_eq!(tags, vec!["Foo"]);
    }

    #[test]
    fn test_normalize_tags_nested() {
        let mapping: serde_yaml::Mapping =
            serde_yaml::from_str("tags:\n  - project/work\n  - important").unwrap();
        let tags = normalize_tags(&mapping);
        assert_eq!(tags, vec!["project/work", "important"]);
    }

    #[test]
    fn test_split_anchor() {
        assert_eq!(split_anchor("Note"), ("Note", None));
        assert_eq!(split_anchor("Note#Heading"), ("Note", Some("Heading")));
        assert_eq!(split_anchor("Note#^block-id"), ("Note", Some("^block-id")));
        assert_eq!(split_anchor("Note#"), ("Note#", None));
    }

    #[test]
    fn test_is_dimension_spec() {
        assert!(is_dimension_spec("500"));
        assert!(is_dimension_spec("500x300"));
        assert!(is_dimension_spec("100X200"));
        assert!(!is_dimension_spec(""));
        assert!(!is_dimension_spec("alt text"));
        assert!(!is_dimension_spec("50px"));
    }

    #[test]
    fn test_convert_syntax_highlight() {
        let re = Regex::new(r"==([^\n=]+?)==").unwrap();
        let comment_re = Regex::new(r"(?s)%%([\s\S]*?)%%").unwrap();
        let input = "This is ==important== text.";
        let result = convert_syntax(input, &re, &comment_re);
        assert_eq!(result, "This is <mark>important</mark> text.");
    }

    #[test]
    fn test_convert_syntax_comment() {
        let re = Regex::new(r"==([^\n=]+?)==").unwrap();
        let comment_re = Regex::new(r"(?s)%%([\s\S]*?)%%").unwrap();
        let input = "Visible %%hidden note%% text";
        let result = convert_syntax(input, &re, &comment_re);
        assert_eq!(result, "Visible <!-- hidden note --> text");
    }

    #[test]
    fn test_convert_syntax_multiline_comment() {
        let re = Regex::new(r"==([^\n=]+?)==").unwrap();
        let comment_re = Regex::new(r"(?s)%%([\s\S]*?)%%").unwrap();
        let input = "%%line one\nline two%%";
        let result = convert_syntax(input, &re, &comment_re);
        assert_eq!(result, "<!-- line one\nline two -->");
    }

    #[test]
    fn test_convert_syntax_code_block_protection() {
        let re = Regex::new(r"==([^\n=]+?)==").unwrap();
        let comment_re = Regex::new(r"(?s)%%([\s\S]*?)%%").unwrap();
        let input = "```\n==not a highlight==\n```\n==real highlight==";
        let result = convert_syntax(input, &re, &comment_re);
        assert!(result.contains("==not a highlight=="));
        assert!(result.contains("<mark>real highlight</mark>"));
    }

    #[test]
    fn test_extract_heading_title() {
        assert_eq!(
            extract_heading_title("# My Title\n\nBody"),
            Some("My Title".to_string())
        );
        assert_eq!(extract_heading_title("\n\n# Spaced Title"), Some("Spaced Title".to_string()));
        assert_eq!(extract_heading_title("Body without heading"), None);
        assert_eq!(extract_heading_title("## Subheading"), None);
        assert_eq!(extract_heading_title(""), None);
    }

    #[test]
    fn test_normalize_frontmatter_generates_id() {
        let raw = "# Plain Note\n\nNo frontmatter.";
        let (meta, body) = normalize_frontmatter(raw, Path::new("Plain Note.md"));
        assert!(!meta.id.is_empty());
        assert_eq!(meta.title, "Plain Note");
        assert!(meta.tags.is_empty());
        assert!(!meta.pinned);
        assert!(body.contains("# Plain Note"));
    }

    #[test]
    fn test_normalize_frontmatter_extracts_heading_title() {
        let raw = "# My Real Title\n\nContent.";
        let (meta, _) = normalize_frontmatter(raw, Path::new("note1.md"));
        assert_eq!(meta.title, "My Real Title");
    }

    #[test]
    fn test_normalize_frontmatter_preserves_frontmatter_title() {
        let raw = "---\ntitle: Frontmatter Title\n---\n# Different Heading\n\nBody";
        let (meta, _) = normalize_frontmatter(raw, Path::new("file.md"));
        assert_eq!(meta.title, "Frontmatter Title");
    }

    #[test]
    fn test_normalize_frontmatter_tags_from_comma_string() {
        let raw = "---\ntags: a, b, c\n---\n# Note\n\nBody";
        let (meta, _) = normalize_frontmatter(raw, Path::new("note.md"));
        assert_eq!(meta.tags, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_normalize_tags_strips_hash() {
        let mapping: serde_yaml::Mapping =
            serde_yaml::from_str("tags:\n  - \"#hashed\"").unwrap();
        let tags = normalize_tags(&mapping);
        assert_eq!(tags, vec!["hashed"]);
    }

    #[test]
    fn test_pathdiff_same_dir() {
        assert_eq!(pathdiff("/a/b/c.md", "/a/b").unwrap(), "c.md");
    }

    #[test]
    fn test_pathdiff_subdir() {
        assert_eq!(pathdiff("/a/b/c.md", "/a").unwrap(), "b/c.md");
    }

    #[test]
    fn test_pathdiff_parent_dir() {
        assert_eq!(pathdiff("/a/c.md", "/a/b").unwrap(), "../c.md");
    }

    #[test]
    fn test_rename_deprecated_alias() {
        let raw = "---\nalias:\n  - Name1\n---\n# Note\n\nBody";
        let result = rename_deprecated_properties(raw);
        assert!(result.contains("aliases:"));
        assert!(!result.contains("\nalias:"));
    }

    #[test]
    fn test_rename_deprecated_cssclass() {
        let raw = "---\ncssclass: custom\n---\n# Note";
        let result = rename_deprecated_properties(raw);
        assert!(result.contains("cssclasses:"));
        assert!(!result.contains("\ncssclass:"));
    }

    #[test]
    fn test_rename_deprecated_no_override() {
        let raw = "---\naliases:\n  - A\nalias:\n  - B\n---\n# Note";
        let result = rename_deprecated_properties(raw);
        assert!(result.contains("aliases:"));
        assert!(!result.contains("\nalias:"));
    }

    #[test]
    fn test_rename_deprecated_no_change_needed() {
        let raw = "---\naliases: [A]\ncssclasses: [c]\n---\n# Note";
        let result = rename_deprecated_properties(raw);
        assert_eq!(result, raw);
    }

    #[test]
    fn test_split_anchor_same_note() {
        let (file, anchor) = split_anchor("#Heading");
        assert_eq!(file, "");
        assert_eq!(anchor, Some("Heading"));
    }
}
