use crate::types::{
    NoteContent, NoteEntry, NoteMeta, NoteTitleEntry, NotebookEntry, TrashContents,
    TrashNotebookEntry, VaultState,
};
use crate::vault::frontmatter;
use chrono::{DateTime, Local, Locale, Utc};
use rayon::prelude::*;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;
use walkdir::WalkDir;

pub fn helixnotes_dir(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join(".helixnotes")
}

fn canonicalize_path(path: &Path, label: &str) -> Result<PathBuf, String> {
    fs::canonicalize(path).map_err(|error| format!("Invalid {label}: {error}"))
}

fn ensure_vault_content_path(
    vault_path: &str,
    requested_path: &Path,
    allow_root: bool,
) -> Result<PathBuf, String> {
    let vault = canonicalize_path(Path::new(vault_path), "vault path")?;
    let requested = canonicalize_path(requested_path, "vault item path")?;
    let metadata = vault.join(".helixnotes");

    if !requested.starts_with(&vault)
        || requested.starts_with(&metadata)
        || (!allow_root && requested == vault)
    {
        return Err("Path must stay inside the active vault".to_string());
    }

    Ok(requested_path.to_path_buf())
}

fn ensure_vault_content_dir(vault_path: &str, requested_path: &Path) -> Result<PathBuf, String> {
    let requested = ensure_vault_content_path(vault_path, requested_path, true)?;
    if !requested.is_dir() {
        return Err("Vault destination is not a directory".to_string());
    }
    Ok(requested)
}

fn ensure_note_path(vault_path: &str, requested_path: &Path) -> Result<PathBuf, String> {
    let requested = ensure_vault_content_path(vault_path, requested_path, false)?;
    if !requested.is_file()
        || requested.extension().and_then(|extension| extension.to_str()) != Some("md")
    {
        return Err("Note path must point to a Markdown file".to_string());
    }
    Ok(requested)
}

fn ensure_readable_note_path(vault_path: &str, requested_path: &Path) -> Result<PathBuf, String> {
    if let Ok(note) = ensure_note_path(vault_path, requested_path) {
        return Ok(note);
    }

    let trashed_note = ensure_trash_entry(vault_path, requested_path)?;
    if !trashed_note.is_file()
        || trashed_note.extension().and_then(|extension| extension.to_str()) != Some("md")
    {
        return Err("Note path must point to a Markdown file".to_string());
    }
    Ok(trashed_note)
}

fn ensure_notebook_path(vault_path: &str, requested_path: &Path) -> Result<PathBuf, String> {
    let requested = ensure_vault_content_path(vault_path, requested_path, false)?;
    if !requested.is_dir() {
        return Err("Notebook path must point to a directory".to_string());
    }
    Ok(requested)
}

fn ensure_trash_entry(vault_path: &str, requested_path: &Path) -> Result<PathBuf, String> {
    let trash = canonicalize_path(&helixnotes_dir(vault_path).join("trash"), "trash path")?;
    let requested = canonicalize_path(requested_path, "trash item path")?;
    if requested == trash || !requested.starts_with(&trash) {
        return Err("Path must be an item inside the active vault trash".to_string());
    }
    Ok(requested_path.to_path_buf())
}

fn safe_relative_path(path: &str) -> Result<&Path, String> {
    let relative = Path::new(path);
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err("Path must be a safe vault-relative path".to_string());
    }
    Ok(relative)
}

fn safe_child_name(name: &str) -> Result<&str, String> {
    let mut components = Path::new(name).components();
    if name.trim().is_empty()
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err("Name must not contain path separators".to_string());
    }
    Ok(name)
}

pub fn ensure_vault_structure(vault_path: &str) -> Result<(), String> {
    let hn_dir = helixnotes_dir(vault_path);
    fs::create_dir_all(hn_dir.join("trash")).map_err(|e| e.to_string())?;
    fs::create_dir_all(hn_dir.join("attachments")).map_err(|e| e.to_string())?;

    let config_path = hn_dir.join("config.json");
    if !config_path.exists() {
        let config = serde_json::json!({
            "version": "0.1.0",
            "created": Utc::now().to_rfc3339()
        });
        fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
            .map_err(|e| e.to_string())?;
    }

    let state_path = hn_dir.join("state.json");
    if !state_path.exists() {
        let state = VaultState::default();
        fs::write(&state_path, serde_json::to_string_pretty(&state).unwrap())
            .map_err(|e| e.to_string())?;
    }

    let gitignore_path = hn_dir.join(".gitignore");
    if !gitignore_path.exists() {
        fs::write(&gitignore_path, "trash/\nindex.json\nstate.json\n")
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn digit_run(value: &[u8], start: usize) -> (usize, &[u8]) {
    let mut end = start;
    while end < value.len() && value[end].is_ascii_digit() {
        end += 1;
    }
    let mut significant = start;
    while significant + 1 < end && value[significant] == b'0' {
        significant += 1;
    }
    (end, &value[significant..end])
}

fn compare_natural_names(left: &str, right: &str) -> Ordering {
    let left_lower = left.to_lowercase();
    let right_lower = right.to_lowercase();
    let left = left_lower.as_bytes();
    let right = right_lower.as_bytes();
    let (mut left_index, mut right_index) = (0, 0);
    let mut leading_zero_order = Ordering::Equal;

    while left_index < left.len() && right_index < right.len() {
        if left[left_index].is_ascii_digit() && right[right_index].is_ascii_digit() {
            let (left_end, left_number) = digit_run(left, left_index);
            let (right_end, right_number) = digit_run(right, right_index);
            let number_order = left_number
                .len()
                .cmp(&right_number.len())
                .then_with(|| left_number.cmp(right_number));
            if number_order != Ordering::Equal {
                return number_order;
            }
            if leading_zero_order == Ordering::Equal {
                leading_zero_order = (left_end - left_index).cmp(&(right_end - right_index));
            }
            left_index = left_end;
            right_index = right_end;
        } else {
            let character_order = left[left_index].cmp(&right[right_index]);
            if character_order != Ordering::Equal {
                return character_order;
            }
            left_index += 1;
            right_index += 1;
        }
    }

    match (left_index == left.len(), right_index == right.len()) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => leading_zero_order,
    }
}

pub fn scan_notebooks(vault_path: &str) -> Result<Vec<NotebookEntry>, String> {
    let root = Path::new(vault_path);
    if !root.exists() {
        return Err("Vault path does not exist".to_string());
    }
    Ok(scan_dir_recursive(root, vault_path))
}

fn scan_dir_recursive(dir: &Path, vault_root: &str) -> Vec<NotebookEntry> {
    let root = Path::new(vault_root);

    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };

    // Collect subdirs (root level only lists directories, note counts come from scan_dir_with_count)
    let mut dirs: Vec<_> = read_dir
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().map(|ft| ft.is_dir()).unwrap_or(false)
                && !is_hidden(&e.path())
                && e.file_name().to_string_lossy() != "Daily"
        })
        .collect();

    dirs.sort_by(|a, b| {
        compare_natural_names(
            &a.file_name().to_string_lossy(),
            &b.file_name().to_string_lossy(),
        )
    });

    // Scan sibling notebooks in parallel: each subtree is independent, so
    // overlapping the per-directory read_dir calls hides FUSE latency on Android.
    // par_iter().collect() preserves the (already-sorted) order.
    let paths: Vec<PathBuf> = dirs.iter().map(|e| e.path()).collect();
    paths
        .par_iter()
        .map(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let relative = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            let (children, child_note_count) = scan_dir_with_count(path, vault_root);
            NotebookEntry {
                name,
                path: path.to_string_lossy().to_string(),
                relative_path: relative,
                children,
                note_count: child_note_count,
            }
        })
        .collect()
}

/// Combined scan: returns (children, note_count) in a single read_dir pass.
fn scan_dir_with_count(dir: &Path, vault_root: &str) -> (Vec<NotebookEntry>, usize) {
    let root = Path::new(vault_root);

    let Ok(read_dir) = fs::read_dir(dir) else {
        return (Vec::new(), 0);
    };

    let mut subdirs = Vec::new();
    let mut note_count = 0usize;
    for entry in read_dir.filter_map(|e| e.ok()) {
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        if ft.is_dir() && !is_hidden(&entry.path()) {
            subdirs.push(entry);
        } else if ft.is_file() && entry.path().extension().and_then(|x| x.to_str()) == Some("md") {
            note_count += 1;
        }
    }

    subdirs.sort_by(|a, b| {
        compare_natural_names(
            &a.file_name().to_string_lossy(),
            &b.file_name().to_string_lossy(),
        )
    });

    let paths: Vec<PathBuf> = subdirs.iter().map(|e| e.path()).collect();
    let entries: Vec<NotebookEntry> = paths
        .par_iter()
        .map(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let relative = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            let (children, child_note_count) = scan_dir_with_count(path, vault_root);
            NotebookEntry {
                name,
                path: path.to_string_lossy().to_string(),
                relative_path: relative,
                children,
                note_count: child_note_count,
            }
        })
        .collect();

    (entries, note_count)
}

pub(crate) fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| {
            n.starts_with('.')
                || matches!(
                    n,
                    "_res" | "_resources" | "_attachments" | "_assets" | "assets" | "node_modules"
                )
        })
        .unwrap_or(false)
}

pub fn count_root_notes(vault_path: &str) -> Result<usize, String> {
    let root = Path::new(vault_path);
    if !root.exists() {
        return Err("Vault path does not exist".to_string());
    }
    let count = fs::read_dir(root)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().map(|ft| ft.is_file()).unwrap_or(false)
                && e.path().extension().and_then(|x| x.to_str()) == Some("md")
        })
        .count();
    Ok(count)
}

pub fn scan_notes(vault_path: &str, notebook_path: Option<&str>) -> Result<Vec<NoteEntry>, String> {
    let scan_path = notebook_path.unwrap_or(vault_path);
    let root = Path::new(scan_path);
    ensure_vault_content_dir(vault_path, root)?;
    let vault_root = Path::new(vault_path);

    log::info!("scan_notes: vault={}, scan={}, exists={}", vault_path, scan_path, root.exists());

    if !root.exists() {
        return Err("Path does not exist".to_string());
    }

    // On mobile, use metadata-only scan (no file reads) for fast listing on sandboxed FS
    #[cfg(mobile)]
    {
        // Collect candidate .md paths first, then read their metadata in parallel.
        // Overlapping the per-file reads hides FUSE latency on Android (matches the
        // desktop path below). The previous version also did an extra read_dir purely
        // for debug logging, which doubled the directory I/O on the sandboxed FS.
        let paths: Vec<PathBuf> = if notebook_path.is_some() {
            match fs::read_dir(root) {
                Ok(rd) => rd
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("md"))
                    .collect(),
                Err(_) => Vec::new(),
            }
        } else {
            let hn_dir = helixnotes_dir(vault_path);
            WalkDir::new(root)
                .into_iter()
                // filter_entry skips descending into hidden dirs (unlike filter)
                .filter_entry(|e| !is_hidden(e.path()) && !e.path().starts_with(&hn_dir))
                .filter_map(|e| e.ok())
                .map(|e| e.path().to_path_buf())
                .filter(|p| p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("md"))
                .collect()
        };

        let mut notes: Vec<NoteEntry> = paths
            .par_iter()
            .filter_map(|path| read_note_entry_metadata_only(path, vault_root).ok())
            .collect();

        log::info!("scan_notes: mobile scan found {} notes", notes.len());
        notes.sort_by_key(|note| std::cmp::Reverse(note.meta.modified));
        return Ok(notes);
    }

    #[cfg(desktop)]
    {
        let md_files: Vec<PathBuf> = if notebook_path.is_some() {
            fs::read_dir(root)
                .map_err(|e| e.to_string())?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("md"))
                .collect()
        } else {
            WalkDir::new(root)
                .into_iter()
                .filter_entry(|e| !is_hidden(e.path()) && !e.path().starts_with(helixnotes_dir(vault_path)))
                .filter_map(|e| e.ok())
                .map(|e| e.path().to_path_buf())
                .filter(|p| p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("md"))
                .collect()
        };

        let mut notes: Vec<NoteEntry> = md_files
            .par_iter()
            .filter_map(|path| read_note_entry_fast(path, vault_root).ok())
            .collect();

        notes.sort_by_key(|note| std::cmp::Reverse(note.meta.modified));
        Ok(notes)
    }
}

fn read_note_entry(path: &Path, vault_root: &Path) -> Result<NoteEntry, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    read_note_entry_from_str(&raw, path, vault_root)
}

/// Android-only: reads just the frontmatter (first 2KB) for tags/title/pinned,
/// uses filesystem timestamps for dates. No preview text.
#[cfg(mobile)]
fn read_note_entry_metadata_only(path: &Path, vault_root: &Path) -> Result<NoteEntry, String> {
    // Read first 2KB - enough for frontmatter with tags, title, pinned
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 2048];
    let bytes_read = file.read(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(bytes_read);
    let raw = String::from_utf8_lossy(&buf);

    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let (mut meta, content) = frontmatter::parse_note(&raw, &filename);

    // Always use filesystem timestamps on Android (faster than parsing date strings)
    if let Ok(fs_meta) = fs::metadata(path) {
        if let Ok(m) = fs_meta.modified() {
            meta.modified = m.into();
        }
        if let Ok(c) = fs_meta.created() {
            meta.created = c.into();
        }
    }

    let relative = path
        .strip_prefix(vault_root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let preview = frontmatter::extract_preview(&content, 120);

    Ok(NoteEntry {
        path: path.to_string_lossy().to_string(),
        relative_path: relative,
        meta,
        preview,
    })
}

/// Fast version: reads only the first ~2KB of the file (enough for frontmatter + preview).
fn read_note_entry_fast(path: &Path, vault_root: &Path) -> Result<NoteEntry, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 2048];
    let bytes_read = file.read(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(bytes_read);
    let raw = String::from_utf8_lossy(&buf);
    read_note_entry_from_str(&raw, path, vault_root)
}

fn read_note_entry_from_str(
    raw: &str,
    path: &Path,
    vault_root: &Path,
) -> Result<NoteEntry, String> {
    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let (mut meta, content) = frontmatter::parse_note(raw, &filename);

    // Use filesystem times only if frontmatter didn't have created/modified fields
    let has_created = raw.contains("\ncreated:") || raw.starts_with("created:");
    let has_modified = raw.contains("\nmodified:") || raw.starts_with("modified:");
    if let Ok(fs_meta) = fs::metadata(path) {
        if !has_modified {
            if let Ok(modified) = fs_meta.modified() {
                meta.modified = modified.into();
            }
        }
        if !has_created {
            if let Ok(created) = fs_meta.created() {
                meta.created = created.into();
            }
        }
    }

    let relative = path
        .strip_prefix(vault_root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let preview = frontmatter::extract_preview(&content, 120);

    Ok(NoteEntry {
        path: path.to_string_lossy().to_string(),
        relative_path: relative,
        meta,
        preview,
    })
}

pub fn read_note(vault_path: &str, path: &str) -> Result<NoteContent, String> {
    let validated = ensure_readable_note_path(vault_path, Path::new(path))?;
    read_note_content(&validated, path)
}

pub fn read_vault_note(vault_path: &str, path: &str) -> Result<NoteContent, String> {
    let validated = ensure_note_path(vault_path, Path::new(path))?;
    read_note_content(&validated, path)
}

fn read_note_content(validated: &Path, reported_path: &str) -> Result<NoteContent, String> {
    let p = validated;
    let raw = fs::read_to_string(p).map_err(|e| e.to_string())?;
    let filename = p
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let (mut meta, content) = frontmatter::parse_note(&raw, &filename);

    // Fall back to filesystem times when the frontmatter has no created/modified, matching the
    // note-list scan (read_note_entry_from_str). Without this, a missing `created:` defaults to
    // Utc::now() here and then gets written to disk on the next save (e.g. a title change),
    // resetting the create date of imported/frontmatter-less notes. (issue #139)
    let has_created = raw.contains("\ncreated:") || raw.starts_with("created:");
    let has_modified = raw.contains("\nmodified:") || raw.starts_with("modified:");
    if let Ok(fs_meta) = fs::metadata(p) {
        if !has_modified {
            if let Ok(modified) = fs_meta.modified() {
                meta.modified = modified.into();
            }
        }
        if !has_created {
            if let Ok(created) = fs_meta.created() {
                meta.created = created.into();
            }
        }
    }

    Ok(NoteContent {
        path: reported_path.to_string(),
        meta,
        content,
        raw,
    })
}

pub fn save_note(vault_path: &str, path: &str, meta: &NoteMeta, body: &str) -> Result<(), String> {
    let path = ensure_note_path(vault_path, Path::new(path))?;
    let mut updated_meta = meta.clone();
    updated_meta.modified = Utc::now();

    // Generate UUID on first save if note didn't have one
    if updated_meta.id.is_empty() {
        updated_meta.id = Uuid::new_v4().to_string();
    }

    // Read existing file to preserve unknown frontmatter fields
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let raw = if existing.is_empty() {
        frontmatter::update_note_raw(&updated_meta, body)
    } else {
        frontmatter::merge_frontmatter(&existing, &updated_meta, body)
    };

    fs::write(path, raw).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn create_note(
    vault_path: &str,
    notebook_relative: Option<&str>,
    title: &str,
) -> Result<NoteEntry, String> {
    let requested_dir = match notebook_relative {
        Some(rel) => Path::new(vault_path).join(safe_relative_path(rel)?),
        None => PathBuf::from(vault_path),
    };
    let dir = ensure_vault_content_dir(vault_path, &requested_dir)?;

    let filename = sanitize_filename(title);
    let mut file_path = dir.join(format!("{}.md", filename));

    // Deduplicate filename
    let mut counter = 1;
    while file_path.exists() {
        file_path = dir.join(format!("{} {}.md", filename, counter));
        counter += 1;
    }

    let now = Utc::now();
    let meta = NoteMeta {
        id: Uuid::new_v4().to_string(),
        title: title.to_string(),
        tags: Vec::new(),
        pinned: false,
        created: now,
        modified: now,
    };

    let raw = frontmatter::update_note_raw(&meta, "\n");
    fs::write(&file_path, raw).map_err(|e| e.to_string())?;

    let vault_root = Path::new(vault_path);
    let relative = file_path
        .strip_prefix(vault_root)
        .unwrap_or(&file_path)
        .to_string_lossy()
        .to_string();

    Ok(NoteEntry {
        path: file_path.to_string_lossy().to_string(),
        relative_path: relative,
        meta,
        preview: String::new(),
    })
}

pub fn duplicate_note(path: &str, vault_path: &str) -> Result<NoteEntry, String> {
    let validated = ensure_note_path(vault_path, Path::new(path))?;
    let src = validated.as_path();

    let parent = src
        .parent()
        .ok_or_else(|| "Note has no parent directory".to_string())?;
    let raw = fs::read_to_string(src).map_err(|e| e.to_string())?;
    let filename = src
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let (mut meta, body) = frontmatter::parse_note(&raw, &filename);
    let source_title = meta.title.clone();
    let base_title = format!("{} copy", source_title.trim_end());
    let mut copy_title = base_title.clone();
    let mut copy_number = 2;
    let mut copy_path = parent.join(format!("{}.md", sanitize_filename(&copy_title)));

    while copy_path.exists() {
        copy_title = format!("{} {}", base_title, copy_number);
        copy_number += 1;
        copy_path = parent.join(format!("{}.md", sanitize_filename(&copy_title)));
    }

    let now = Utc::now();
    meta.id = Uuid::new_v4().to_string();
    meta.title = copy_title;
    meta.pinned = false;
    meta.created = now;
    meta.modified = now;

    let body = retitle_leading_heading(&body, &source_title, &meta.title);
    let copy_raw = frontmatter::merge_frontmatter(&raw, &meta, &body);
    fs::write(&copy_path, copy_raw).map_err(|e| e.to_string())?;

    read_note_entry(&copy_path, Path::new(vault_path))
}

fn retitle_leading_heading(body: &str, old_title: &str, new_title: &str) -> String {
    let trimmed = body.trim_start();
    let leading_len = body.len() - trimmed.len();
    let first_line_end = trimmed.find('\n').unwrap_or(trimmed.len());
    let first_line = &trimmed[..first_line_end];
    let heading_len = first_line.chars().take_while(|&c| c == '#').count();

    if !(1..=6).contains(&heading_len) {
        return body.to_string();
    }

    let Some(heading_title) = first_line[heading_len..].strip_prefix(' ') else {
        return body.to_string();
    };
    if !heading_title.trim().eq_ignore_ascii_case(old_title.trim()) {
        return body.to_string();
    }

    let mut retitled = String::with_capacity(body.len() + new_title.len());
    retitled.push_str(&body[..leading_len]);
    retitled.push_str(&"#".repeat(heading_len));
    retitled.push(' ');
    retitled.push_str(new_title);
    retitled.push_str(&trimmed[first_line_end..]);
    retitled
}

fn get_system_locale() -> Locale {
    let sys = sys_locale::get_locale().unwrap_or_else(|| "en-US".to_string());
    let lang = sys.split(&['-', '_', '.'][..]).next().unwrap_or("en");
    match lang {
        "af" => Locale::af_ZA,
        "ar" => Locale::ar_SA,
        "be" => Locale::be_BY,
        "bg" => Locale::bg_BG,
        "ca" => Locale::ca_ES,
        "cs" => Locale::cs_CZ,
        "da" => Locale::da_DK,
        "de" => Locale::de_DE,
        "el" => Locale::el_GR,
        "es" => Locale::es_ES,
        "et" => Locale::et_EE,
        "fi" => Locale::fi_FI,
        "fr" => Locale::fr_FR,
        "he" => Locale::he_IL,
        "hi" => Locale::hi_IN,
        "hr" => Locale::hr_HR,
        "hu" => Locale::hu_HU,
        "id" => Locale::id_ID,
        "is" => Locale::is_IS,
        "it" => Locale::it_IT,
        "ja" => Locale::ja_JP,
        "ka" => Locale::ka_GE,
        "ko" => Locale::ko_KR,
        "lt" => Locale::lt_LT,
        "lv" => Locale::lv_LV,
        "mk" => Locale::mk_MK,
        "nb" | "no" => Locale::nb_NO,
        "nl" => Locale::nl_NL,
        "nn" => Locale::nn_NO,
        "pl" => Locale::pl_PL,
        "pt" => Locale::pt_BR,
        "ro" => Locale::ro_RO,
        "ru" => Locale::ru_RU,
        "sk" => Locale::sk_SK,
        "sl" => Locale::sl_SI,
        "sq" => Locale::sq_AL,
        "sr" => Locale::sr_RS,
        "sv" => Locale::sv_SE,
        "th" => Locale::th_TH,
        "tr" => Locale::tr_TR,
        "uk" => Locale::uk_UA,
        "vi" => Locale::vi_VN,
        "zh" => Locale::zh_CN,
        _ => Locale::en_US,
    }
}

pub fn create_daily_note(
    vault_path: &str,
    date: Option<&str>,
    format: &str,
) -> Result<NoteEntry, String> {
    let target_date = match date {
        Some(d) => chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .map_err(|e| format!("Invalid date: {}", e))?,
        None => Local::now().date_naive(),
    };
    let date_str = target_date.format("%Y-%m-%d").to_string();
    let title = match format {
        "iso" => target_date.format("%Y-%m-%d").to_string(),
        "long" => target_date.format("%B %-d, %Y").to_string(),
        "us" => target_date.format("%m/%d/%Y").to_string(),
        "eu" => target_date.format("%d/%m/%Y").to_string(),
        _ => {
            let locale = get_system_locale();
            target_date.format_localized("%B %d, %Y", locale).to_string()
        }
    };

    let dir = Path::new(vault_path).join("Daily");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let file_path = dir.join(format!("{}.md", date_str));
    let vault_root = Path::new(vault_path);

    // If today's note already exists, return it
    if file_path.exists() {
        return read_note_entry(&file_path, vault_root);
    }

    let now = Utc::now();
    let meta = NoteMeta {
        id: Uuid::new_v4().to_string(),
        title,
        tags: vec!["daily".to_string()],
        pinned: false,
        created: now,
        modified: now,
    };

    let raw = frontmatter::update_note_raw(&meta, "\n");
    fs::write(&file_path, raw).map_err(|e| e.to_string())?;

    let relative = file_path
        .strip_prefix(vault_root)
        .unwrap_or(&file_path)
        .to_string_lossy()
        .to_string();

    Ok(NoteEntry {
        path: file_path.to_string_lossy().to_string(),
        relative_path: relative,
        meta,
        preview: String::new(),
    })
}

pub fn create_notebook(
    vault_path: &str,
    parent_relative: Option<&str>,
    name: &str,
) -> Result<NotebookEntry, String> {
    let requested_parent = match parent_relative {
        Some(rel) => Path::new(vault_path).join(safe_relative_path(rel)?),
        None => PathBuf::from(vault_path),
    };
    let parent = ensure_vault_content_dir(vault_path, &requested_parent)?;
    let dir_path = parent.join(safe_child_name(name)?);
    if dir_path.exists() {
        return Err("Notebook already exists".to_string());
    }

    fs::create_dir_all(&dir_path).map_err(|e| e.to_string())?;

    let vault_root = Path::new(vault_path);
    let relative = dir_path
        .strip_prefix(vault_root)
        .unwrap_or(&dir_path)
        .to_string_lossy()
        .to_string();

    Ok(NotebookEntry {
        name: name.to_string(),
        path: dir_path.to_string_lossy().to_string(),
        relative_path: relative,
        children: Vec::new(),
        note_count: 0,
    })
}

pub fn delete_note(vault_path: &str, note_path: &str) -> Result<(), String> {
    let validated = ensure_note_path(vault_path, Path::new(note_path))?;
    let src = validated.as_path();

    let trash_dir = helixnotes_dir(vault_path).join("trash");
    fs::create_dir_all(&trash_dir).map_err(|e| e.to_string())?;

    let filename = src
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let timestamp = Utc::now().format("%Y%m%d%H%M%S%3f");
    let trash_name = format!("{}_{}", timestamp, filename);
    let dest = trash_dir.join(&trash_name);

    fs::rename(src, dest).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_notebook(vault_path: &str, notebook_path: &str) -> Result<(), String> {
    let validated = ensure_notebook_path(vault_path, Path::new(notebook_path))?;
    let src = validated.as_path();

    let trash_dir = helixnotes_dir(vault_path).join("trash");
    fs::create_dir_all(&trash_dir).map_err(|e| e.to_string())?;

    let dirname = src
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let timestamp = Utc::now().format("%Y%m%d%H%M%S%3f");
    let trash_name = format!("{}_{}", timestamp, dirname);
    let dest = trash_dir.join(&trash_name);

    // Save original relative path in a sidecar .meta file for restore
    let relative = src
        .strip_prefix(vault_path)
        .unwrap_or(src)
        .to_string_lossy()
        .to_string();
    let meta_path = trash_dir.join(format!("{}.meta", trash_name));
    let _ = fs::write(&meta_path, &relative);

    fs::rename(src, dest).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn rename_note(path: &str, new_title: &str, vault_path: &str) -> Result<String, String> {
    let validated = ensure_note_path(vault_path, Path::new(path))?;
    let src = validated.as_path();

    // Read old title before renaming
    let raw = fs::read_to_string(src).map_err(|e| e.to_string())?;
    let filename = src
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let (mut meta, content) = frontmatter::parse_note(&raw, &filename);
    let old_title = meta.title.clone();
    let old_path_str = src.to_string_lossy().to_string();

    // Update frontmatter
    meta.title = new_title.to_string();
    meta.modified = Utc::now();
    if meta.id.is_empty() {
        meta.id = Uuid::new_v4().to_string();
    }
    let updated = frontmatter::merge_frontmatter(&raw, &meta, &content);

    // Rename file
    let new_filename = sanitize_filename(new_title);
    let new_path = src.parent().unwrap().join(format!("{}.md", new_filename));
    fs::write(src, &updated).map_err(|e| e.to_string())?;

    if new_path != src {
        fs::rename(src, &new_path).map_err(|e| e.to_string())?;
    }

    let new_path_str = new_path.to_string_lossy().to_string();

    // Update wikilinks in other notes that reference this note
    update_wikilinks_after_rename(vault_path, &old_path_str, &new_path_str, &old_title, new_title);

    Ok(new_path_str)
}

/// Walk all .md files in the vault and update wikilink references after a note rename.
/// Updates both HTML data-attributes (data-path, data-title) and raw [[old_title]] references.
fn update_wikilinks_after_rename(
    vault_path: &str,
    old_path: &str,
    new_path: &str,
    old_title: &str,
    new_title: &str,
) {
    let vault = Path::new(vault_path);
    let hn_dir = helixnotes_dir(vault_path);

    // Vault-relative path ref (without .md), normalized to forward slashes so it
    // matches [[folder/note]] wikilinks on Windows (OS paths use backslashes there).
    let rel_ref = |p: &str| -> String {
        let rel = Path::new(p)
            .strip_prefix(vault)
            .map(|r| r.to_string_lossy().into_owned())
            .unwrap_or_else(|_| p.to_string())
            .replace('\\', "/");
        rel.strip_suffix(".md").unwrap_or(&rel).to_string()
    };
    let old_rel_ref = rel_ref(old_path);
    let new_rel_ref = rel_ref(new_path);

    // Check if another note with the same old_title exists in the vault.
    // If so, title-based replacements (rules 1-3) are ambiguous and must be skipped,
    // because we can't tell which [[Old Title]] ref points to the renamed note
    // vs. the other note with the same title.
    let title_is_unique = !WalkDir::new(vault)
        .into_iter()
        .filter_entry(|e| !is_hidden(e.path()) && !e.path().starts_with(&hn_dir))
        .filter_map(|e| e.ok())
        .any(|e| {
            let p = e.path();
            if !p.is_file() || p.to_string_lossy().as_ref() == new_path {
                return false;
            }
            if p.extension().and_then(|ext| ext.to_str()) != Some("md") {
                return false;
            }
            // Check if this note's filename (without .md) matches the old title
            p.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case(old_title))
                .unwrap_or(false)
        });

    for entry in WalkDir::new(vault)
        .into_iter()
        .filter_entry(|e| !is_hidden(e.path()) && !e.path().starts_with(&hn_dir))
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() { continue; }
        let path_str = path.to_string_lossy();
        if path.extension().and_then(|e| e.to_str()) != Some("md") { continue; }
        // Skip the renamed note itself
        if *path_str == *new_path { continue; }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut result = content.clone();

        // Notes are saved as markdown with [[ref]] or [[ref|display]] syntax.
        // Update all wikilink forms that reference the renamed note:

        // Title-based rules only apply when the old title is unique in the vault.
        // If another note shares the same title, these would be ambiguous.
        if old_title != new_title && title_is_unique {
            // 1. Short title ref: [[Old Title]] → [[New Title]]
            result = result.replace(
                &format!("[[{}]]", old_title),
                &format!("[[{}]]", new_title),
            );

            // 2. Short title with alias: [[Old Title|display]] → [[New Title|display]]
            result = result.replace(
                &format!("[[{}|", old_title),
                &format!("[[{}|", new_title),
            );

            // 3. Short title as alias display: [[ref|Old Title]] → [[ref|New Title]]
            result = result.replace(
                &format!("|{}]]", old_title),
                &format!("|{}]]", new_title),
            );
        }

        // Path-based rules are always safe (paths are unique).
        if old_rel_ref != new_rel_ref {
            // 4. Path-based ref: [[folder/Old Name]] → [[folder/New Name]]
            result = result.replace(
                &format!("[[{}]]", old_rel_ref),
                &format!("[[{}]]", new_rel_ref),
            );

            // 5. Path-based ref with alias: [[folder/Old Name|display]] → [[folder/New Name|display]]
            //    Also update the alias if it matches the old title.
            //    e.g. [[folder/Old Name|Old Title]] → [[folder/New Name|New Title]]
            if old_title != new_title {
                result = result.replace(
                    &format!("[[{}|{}]]", old_rel_ref, old_title),
                    &format!("[[{}|{}]]", new_rel_ref, new_title),
                );
            }
            // For aliases that DON'T match the old title, just update the ref part
            result = result.replace(
                &format!("[[{}|", old_rel_ref),
                &format!("[[{}|", new_rel_ref),
            );
        }

        if result != content {
            let _ = fs::write(path, &result);
        }
    }
}

pub fn rename_notebook(vault_path: &str, path: &str, new_name: &str) -> Result<String, String> {
    let validated = ensure_notebook_path(vault_path, Path::new(path))?;
    let src = validated.as_path();

    let new_path = src.parent().unwrap().join(safe_child_name(new_name)?);
    if new_path.exists() {
        return Err("A notebook with that name already exists".to_string());
    }

    fs::rename(src, &new_path).map_err(|e| e.to_string())?;
    Ok(new_path.to_string_lossy().to_string())
}

pub fn move_note(vault_path: &str, note_path: &str, dest_notebook: &str) -> Result<String, String> {
    let validated = ensure_note_path(vault_path, Path::new(note_path))?;
    let src = validated.as_path();

    let validated_dest = ensure_vault_content_dir(vault_path, Path::new(dest_notebook))?;
    let dest_dir = validated_dest.as_path();
    if !dest_dir.is_dir() {
        return Err("Destination notebook does not exist".to_string());
    }

    let filename = src.file_name().unwrap_or_default();
    let dest = dest_dir.join(filename);

    fs::rename(src, &dest).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

pub fn move_notebook(
    vault_path: &str,
    notebook_path: &str,
    dest_parent: &str,
) -> Result<String, String> {
    let validated = ensure_notebook_path(vault_path, Path::new(notebook_path))?;
    let src = validated.as_path();

    let validated_dest = ensure_vault_content_dir(vault_path, Path::new(dest_parent))?;
    let dest_parent_path = validated_dest.as_path();
    if !dest_parent_path.is_dir() {
        return Err("Destination does not exist".to_string());
    }

    // No-op if already in that parent
    if src.parent() == Some(dest_parent_path) {
        return Err("Notebook is already in that location".to_string());
    }

    let dir_name = src.file_name().unwrap_or_default();
    let dest = dest_parent_path.join(dir_name);

    // Prevent moving into itself or a descendant
    if dest.starts_with(src) {
        return Err("Cannot move a notebook into itself or its descendants".to_string());
    }

    if dest.exists() {
        return Err("A notebook with that name already exists in the destination".to_string());
    }

    fs::rename(src, &dest).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

/// Remove a directory inside trash if it's empty (after restoring/deleting its last note).
fn cleanup_empty_trash_dir(vault_path: &str, dir: Option<&Path>) {
    let trash_dir = helixnotes_dir(vault_path).join("trash");
    if let Some(d) = dir {
        if d != trash_dir && d.starts_with(&trash_dir) {
            if let Ok(mut entries) = fs::read_dir(d) {
                if entries.next().is_none() {
                    let _ = fs::remove_dir(d);
                }
            }
        }
    }
}

pub fn get_trash_contents(vault_path: &str) -> Result<TrashContents, String> {
    let trash_dir = helixnotes_dir(vault_path).join("trash");
    if !trash_dir.exists() {
        return Ok(TrashContents { notes: Vec::new(), notebooks: Vec::new() });
    }

    let vault_root = Path::new(vault_path);
    let mut notes = Vec::new();
    let mut notebooks = Vec::new();

    for entry in fs::read_dir(&trash_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|x| x.to_str()) == Some("md") {
            if let Ok(note) = read_note_entry(&path, vault_root) {
                notes.push(note);
            }
        } else if path.is_dir() {
            let note_count = WalkDir::new(&path)
                .min_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file() && e.path().extension().and_then(|x| x.to_str()) == Some("md"))
                .count();
            let dirname = path.file_name().unwrap_or_default().to_string_lossy();
            // Strip timestamp prefix to get original notebook name
            let name = if dirname.len() > 18 && dirname.chars().nth(17) == Some('_') {
                dirname[18..].to_string()
            } else if dirname.len() > 15 && dirname.chars().nth(14) == Some('_') {
                dirname[15..].to_string()
            } else {
                dirname.to_string()
            };
            let modified = fs::metadata(&path)
                .and_then(|m| m.modified())
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now());
            notebooks.push(TrashNotebookEntry {
                name,
                path: path.to_string_lossy().to_string(),
                note_count,
                modified,
            });
        }
    }

    notes.sort_by_key(|note| std::cmp::Reverse(note.meta.modified));
    notebooks.sort_by_key(|notebook| std::cmp::Reverse(notebook.modified));
    Ok(TrashContents { notes, notebooks })
}

pub fn restore_note(
    vault_path: &str,
    trash_path: &str,
    dest_notebook: Option<&str>,
) -> Result<String, String> {
    let validated = ensure_trash_entry(vault_path, Path::new(trash_path))?;
    let src = validated.as_path();
    if !src.is_file() {
        return Err("Trashed note does not exist".to_string());
    }

    let requested_dest = match dest_notebook {
        Some(nb) => PathBuf::from(nb),
        None => PathBuf::from(vault_path),
    };
    let dest_dir = ensure_vault_content_dir(vault_path, &requested_dest)?;

    // Strip timestamp prefix from trash filename (17-char with millis or 14-char legacy)
    let filename = src.file_name().unwrap_or_default().to_string_lossy();
    let original_name = if filename.len() > 18 && filename.chars().nth(17) == Some('_') {
        &filename[18..]
    } else if filename.len() > 15 && filename.chars().nth(14) == Some('_') {
        &filename[15..]
    } else {
        &filename
    };

    let parent = src.parent().map(|p| p.to_path_buf());
    let dest = dest_dir.join(original_name);
    fs::rename(src, &dest).map_err(|e| e.to_string())?;

    // Clean up empty parent directory (deleted notebook folder) in trash
    cleanup_empty_trash_dir(vault_path, parent.as_deref());

    Ok(dest.to_string_lossy().to_string())
}

pub fn restore_notebook(vault_path: &str, trash_path: &str) -> Result<String, String> {
    let validated = ensure_trash_entry(vault_path, Path::new(trash_path))?;
    let src = validated.as_path();
    if !src.is_dir() {
        return Err("Trashed notebook does not exist".to_string());
    }

    let dirname = src.file_name().unwrap_or_default().to_string_lossy();

    // Try to read original path from sidecar .meta file
    let meta_path = src.with_extension("").with_file_name(format!("{}.meta", dirname));
    let relative = if let Ok(original) = fs::read_to_string(&meta_path) {
        original
    } else {
        // Fallback for notebooks trashed before .meta files existed:
        // strip timestamp prefix to get notebook name, restore to root
        let name = if dirname.len() > 18 && dirname.chars().nth(17) == Some('_') {
            &dirname[18..]
        } else if dirname.len() > 15 && dirname.chars().nth(14) == Some('_') {
            &dirname[15..]
        } else {
            &dirname
        };
        name.to_string()
    };

    let dest = Path::new(vault_path).join(safe_relative_path(&relative)?);

    // Recreate parent directories if needed
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    fs::rename(src, &dest).map_err(|e| e.to_string())?;
    // Clean up .meta file
    let _ = fs::remove_file(&meta_path);
    Ok(dest.to_string_lossy().to_string())
}

pub fn permanent_delete(vault_path: &str, path: &str) -> Result<(), String> {
    let validated = ensure_trash_entry(vault_path, Path::new(path))?;
    let p = validated.as_path();
    let parent = p.parent().map(|pp| pp.to_path_buf());
    if p.is_dir() {
        fs::remove_dir_all(p).map_err(|e| e.to_string())?;
    } else {
        fs::remove_file(p).map_err(|e| e.to_string())?;
    }

    // Clean up empty parent directory (deleted notebook folder) in trash
    cleanup_empty_trash_dir(vault_path, parent.as_deref());

    Ok(())
}

pub fn empty_trash(vault_path: &str) -> Result<(), String> {
    let trash_dir = helixnotes_dir(vault_path).join("trash");
    if trash_dir.exists() {
        fs::remove_dir_all(&trash_dir).map_err(|e| e.to_string())?;
        fs::create_dir_all(&trash_dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn load_vault_state(vault_path: &str) -> Result<VaultState, String> {
    let state_path = helixnotes_dir(vault_path).join("state.json");
    if state_path.exists() {
        let data = fs::read_to_string(&state_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&data).map_err(|e| e.to_string())
    } else {
        Ok(VaultState::default())
    }
}

pub fn save_vault_state(vault_path: &str, state: &VaultState) -> Result<(), String> {
    let state_path = helixnotes_dir(vault_path).join("state.json");
    let data = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(&state_path, data).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_all_tags(vault_path: &str) -> Result<Vec<(String, usize)>, String> {
    let hn_dir = helixnotes_dir(vault_path);
    let md_files: Vec<PathBuf> = WalkDir::new(vault_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            p.is_file()
                && p.extension().and_then(|x| x.to_str()) == Some("md")
                && !is_hidden(p)
                && !p.starts_with(&hn_dir)
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    // Read frontmatter in parallel (only need tags, so partial read is fine)
    let all_tags: Vec<Vec<String>> = md_files
        .par_iter()
        .filter_map(|path| {
            let mut file = fs::File::open(path).ok()?;
            let mut buf = vec![0u8; 1024]; // Tags are in frontmatter, 1KB is plenty
            let n = file.read(&mut buf).ok()?;
            buf.truncate(n);
            let raw = String::from_utf8_lossy(&buf);
            let filename = path.file_name()?.to_string_lossy().to_string();
            let (meta, _) = frontmatter::parse_note(&raw, &filename);
            Some(meta.tags)
        })
        .collect();

    let mut tag_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for tags in all_tags {
        for tag in tags {
            *tag_counts.entry(tag).or_insert(0) += 1;
        }
    }

    let mut tags: Vec<(String, usize)> = tag_counts.into_iter().collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(tags)
}

pub fn save_image(vault_path: &str, name: &str, data: &[u8]) -> Result<String, String> {
    save_attachment(vault_path, name, data)
}

pub fn save_attachment(vault_path: &str, name: &str, data: &[u8]) -> Result<String, String> {
    let attachments_dir = helixnotes_dir(vault_path).join("attachments");
    fs::create_dir_all(&attachments_dir).map_err(|e| e.to_string())?;

    let timestamp = Utc::now().format("%Y%m%d%H%M%S");
    let safe_name = sanitize_filename(name);
    let filename = format!("{}_{}", timestamp, safe_name);
    let dest = attachments_dir.join(&filename);

    fs::write(&dest, data).map_err(|e| e.to_string())?;

    // Return relative path from vault root
    let relative = format!(".helixnotes/attachments/{}", filename);
    Ok(relative)
}

pub(crate) fn normalize_notebook_icon_key(path: &str) -> String {
    path.replace('\\', "/")
}

pub fn load_notebook_icons(
    vault_path: &str,
) -> Result<std::collections::HashMap<String, String>, String> {
    let icons_path = helixnotes_dir(vault_path).join("notebook_icons.json");
    if !icons_path.exists() {
        return Ok(std::collections::HashMap::new());
    }

    let data = fs::read_to_string(&icons_path).map_err(|e| e.to_string())?;
    let icons: std::collections::HashMap<String, String> =
        serde_json::from_str(&data).map_err(|e| e.to_string())?;
    Ok(icons
        .into_iter()
        .map(|(path, icon)| (normalize_notebook_icon_key(&path), icon))
        .collect())
}

pub fn set_notebook_icon(
    vault_path: &str,
    notebook_relative: &str,
    icon_relative: Option<&str>,
) -> Result<(), String> {
    let mut icons = load_notebook_icons(vault_path)?;
    let notebook_key = normalize_notebook_icon_key(notebook_relative);
    match icon_relative {
        Some(icon) => {
            icons.insert(notebook_key, icon.to_string());
        }
        None => {
            icons.remove(&notebook_key);
        }
    }
    let icons_path = helixnotes_dir(vault_path).join("notebook_icons.json");
    let data = serde_json::to_string_pretty(&icons).map_err(|e| e.to_string())?;
    fs::write(&icons_path, data).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_quick_access(vault_path: &str) -> Result<Vec<String>, String> {
    let qa_path = helixnotes_dir(vault_path).join("quick_access.json");
    if qa_path.exists() {
        let data = fs::read_to_string(&qa_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&data).map_err(|e| e.to_string())
    } else {
        Ok(Vec::new())
    }
}

pub fn save_quick_access(vault_path: &str, paths: &[String]) -> Result<(), String> {
    let qa_path = helixnotes_dir(vault_path).join("quick_access.json");
    let data = serde_json::to_string_pretty(paths).map_err(|e| e.to_string())?;
    fs::write(&qa_path, data).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn add_quick_access(vault_path: &str, note_relative: &str) -> Result<(), String> {
    let mut list = load_quick_access(vault_path)?;
    if !list.contains(&note_relative.to_string()) {
        list.push(note_relative.to_string());
        save_quick_access(vault_path, &list)?;
    }
    Ok(())
}

pub fn remove_quick_access(vault_path: &str, note_relative: &str) -> Result<(), String> {
    let mut list = load_quick_access(vault_path)?;
    list.retain(|p| p != note_relative);
    save_quick_access(vault_path, &list)?;
    Ok(())
}

const NOTE_SWITCHER_RECENT_LIMIT: usize = 6;

pub fn get_note_switcher_titles(
    vault_path: &str,
    recent_paths: &[String],
) -> Result<Vec<NoteTitleEntry>, String> {
    let vault_root = Path::new(vault_path);
    if !vault_root.is_dir() {
        return Err("Vault path does not exist".to_string());
    }

    let mut seen = HashSet::new();
    let mut titles = Vec::with_capacity(NOTE_SWITCHER_RECENT_LIMIT);

    for requested_path in recent_paths {
        if titles.len() >= NOTE_SWITCHER_RECENT_LIMIT {
            break;
        }

        let path = Path::new(requested_path);
        let Ok(relative) = path.strip_prefix(vault_root) else {
            continue;
        };
        let safe_relative = !relative.as_os_str().is_empty()
            && relative.components().all(|component| match component {
                Component::Normal(name) => !is_hidden(Path::new(name)),
                Component::CurDir => true,
                _ => false,
            });
        if !safe_relative
            || path.extension().and_then(|extension| extension.to_str()) != Some("md")
            || !path.is_file()
        {
            continue;
        }

        let relative_path = relative.to_path_buf();
        if !seen.insert(relative_path.clone()) {
            continue;
        }
        let Ok(entry) = read_note_entry_fast(path, vault_root) else {
            continue;
        };

        titles.push(NoteTitleEntry {
            title: entry.meta.title,
            path: relative_path.to_string_lossy().replace('\\', "/"),
        });
    }

    Ok(titles)
}

pub fn get_quick_access_notes(vault_path: &str) -> Result<Vec<NoteEntry>, String> {
    let list = load_quick_access(vault_path)?;
    let vault_root = Path::new(vault_path);
    let mut notes = Vec::new();

    for relative in &list {
        let full_path = vault_root.join(relative);
        if full_path.exists() {
            if let Ok(entry) = read_note_entry_fast(&full_path, vault_root) {
                notes.push(entry);
            }
        }
    }

    Ok(notes)
}

pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        compare_natural_names, create_notebook, duplicate_note, get_note_switcher_titles,
        helixnotes_dir, load_notebook_icons, permanent_delete, read_note, restore_notebook,
        scan_notebooks, set_notebook_icon,
    };
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn compares_numeric_segments_anywhere_in_names() {
        let mut names = ["Class 10b", "Class 2b", "Class 10a", "Class 2a"];
        names.sort_by(|left, right| compare_natural_names(left, right));
        assert_eq!(
            names,
            ["Class 2a", "Class 2b", "Class 10a", "Class 10b"]
        );
        assert_eq!(
            compare_natural_names("Class 02", "Class 2b"),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn sorts_notebooks_with_numeric_names_naturally() {
        let vault =
            std::env::temp_dir().join(format!("helixnotes-natural-sort-test-{}", Uuid::new_v4()));
        let nested = vault.join("Classes");
        fs::create_dir_all(&nested).unwrap();
        let expected = ["5g", "6g", "7g", "8g", "9g", "10g", "11g", "12g"];
        for name in ["10g", "11g", "12g", "5g", "6g", "7g", "8g", "9g"] {
            fs::create_dir(vault.join(name)).unwrap();
            fs::create_dir(nested.join(name)).unwrap();
        }

        let notebooks = scan_notebooks(&vault.to_string_lossy()).unwrap();
        let root_names: Vec<_> = notebooks
            .iter()
            .filter(|notebook| notebook.name != "Classes")
            .map(|notebook| notebook.name.as_str())
            .collect();
        assert_eq!(root_names, expected);
        let nested_names: Vec<_> = notebooks
            .iter()
            .find(|notebook| notebook.name == "Classes")
            .unwrap()
            .children
            .iter()
            .map(|notebook| notebook.name.as_str())
            .collect();
        assert_eq!(nested_names, expected);

        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn persists_and_removes_builtin_notebook_icons() {
        let vault =
            std::env::temp_dir().join(format!("helixnotes-notebook-icon-test-{}", Uuid::new_v4()));
        let vault_path = vault.to_string_lossy();
        fs::create_dir_all(helixnotes_dir(&vault_path)).unwrap();

        set_notebook_icon(&vault_path, r"Projects\Client", Some("builtin:briefcase")).unwrap();
        let stored: std::collections::HashMap<String, String> = serde_json::from_str(
            &fs::read_to_string(helixnotes_dir(&vault_path).join("notebook_icons.json")).unwrap(),
        )
        .unwrap();
        assert!(stored.contains_key("Projects/Client"));
        assert!(!stored.contains_key(r"Projects\Client"));

        let icons = load_notebook_icons(&vault_path).unwrap();
        assert_eq!(
            icons.get("Projects/Client").map(String::as_str),
            Some("builtin:briefcase")
        );

        set_notebook_icon(&vault_path, "Projects/Client", None).unwrap();
        assert!(load_notebook_icons(&vault_path).unwrap().is_empty());

        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn normalizes_legacy_notebook_icon_keys_when_loading() {
        let vault =
            std::env::temp_dir().join(format!("helixnotes-notebook-icon-test-{}", Uuid::new_v4()));
        let vault_path = vault.to_string_lossy();
        let icons_path = helixnotes_dir(&vault_path).join("notebook_icons.json");
        fs::create_dir_all(helixnotes_dir(&vault_path)).unwrap();
        fs::write(&icons_path, r#"{"Projects\\Client":"builtin:folder"}"#).unwrap();

        let icons = load_notebook_icons(&vault_path).unwrap();
        assert_eq!(
            icons.get("Projects/Client").map(String::as_str),
            Some("builtin:folder")
        );

        fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn loads_only_requested_note_switcher_titles() {
        let vault =
            std::env::temp_dir().join(format!("helixnotes-note-switcher-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&vault).unwrap();

        let mut note_paths = Vec::new();
        for index in 0..8 {
            let path = vault.join(format!("Note {index}.md"));
            fs::write(
                &path,
                format!("---\ntitle: Note {index}\n---\n\nNote {index} body.\n"),
            )
            .unwrap();
            note_paths.push(path);
        }
        fs::write(
            vault.join("Unrelated.md"),
            "---\ntitle: Unrelated\n---\n\nMust not be loaded.\n",
        )
        .unwrap();

        let outside =
            std::env::temp_dir().join(format!("helixnotes-outside-note-{}.md", Uuid::new_v4()));
        fs::write(&outside, "---\ntitle: Outside\n---\n").unwrap();

        let requested = vec![
            note_paths[0].to_string_lossy().into_owned(),
            note_paths[0].to_string_lossy().into_owned(),
            vault.join("Missing.md").to_string_lossy().into_owned(),
            outside.to_string_lossy().into_owned(),
            note_paths[1].to_string_lossy().into_owned(),
            note_paths[2].to_string_lossy().into_owned(),
            note_paths[3].to_string_lossy().into_owned(),
            note_paths[4].to_string_lossy().into_owned(),
            note_paths[5].to_string_lossy().into_owned(),
            note_paths[6].to_string_lossy().into_owned(),
            note_paths[7].to_string_lossy().into_owned(),
        ];

        let vault_path = vault.to_string_lossy();
        let titles = get_note_switcher_titles(&vault_path, &requested).unwrap();
        assert_eq!(titles.len(), 6);
        assert_eq!(
            titles
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Note 0.md",
                "Note 1.md",
                "Note 2.md",
                "Note 3.md",
                "Note 4.md",
                "Note 5.md",
            ]
        );
        assert!(titles.iter().all(|entry| entry.title != "Unrelated"));
        assert!(titles.iter().all(|entry| entry.title != "Outside"));

        fs::remove_dir_all(vault).unwrap();
        fs::remove_file(outside).unwrap();
    }

    #[test]
    fn reads_markdown_notes_from_trash_without_allowing_external_files() {
        let test_root =
            std::env::temp_dir().join(format!("helixnotes-path-security-test-{}", Uuid::new_v4()));
        let vault = test_root.join("vault");
        let trash = helixnotes_dir(&vault.to_string_lossy()).join("trash");
        let trashed_note = trash.join("20240101000000000_Note.md");
        let outside = test_root.join("outside.md");
        fs::create_dir_all(&trash).unwrap();
        fs::write(&trashed_note, "---\ntitle: Note\n---\n\ntrashed").unwrap();
        fs::write(&outside, "outside").unwrap();

        assert!(read_note(&vault.to_string_lossy(), &trashed_note.to_string_lossy()).is_ok());
        assert!(read_note(&vault.to_string_lossy(), &outside.to_string_lossy()).is_err());

        fs::remove_dir_all(test_root).unwrap();
    }

    #[test]
    fn rejects_permanent_deletion_outside_trash() {
        let test_root =
            std::env::temp_dir().join(format!("helixnotes-path-security-test-{}", Uuid::new_v4()));
        let vault = test_root.join("vault");
        let outside = test_root.join("outside.md");
        fs::create_dir_all(helixnotes_dir(&vault.to_string_lossy()).join("trash")).unwrap();
        fs::write(&outside, "must survive").unwrap();

        let result = permanent_delete(&vault.to_string_lossy(), &outside.to_string_lossy());

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&outside).unwrap(), "must survive");
        fs::remove_dir_all(test_root).unwrap();
    }

    #[test]
    fn rejects_notebook_creation_outside_vault() {
        let test_root =
            std::env::temp_dir().join(format!("helixnotes-path-security-test-{}", Uuid::new_v4()));
        let vault = test_root.join("vault");
        fs::create_dir_all(&vault).unwrap();

        let result = create_notebook(&vault.to_string_lossy(), Some(".."), "escaped");

        assert!(result.is_err());
        assert!(!test_root.join("escaped").exists());
        fs::remove_dir_all(test_root).unwrap();
    }

    #[test]
    fn rejects_traversal_in_restored_notebook_metadata() {
        let test_root =
            std::env::temp_dir().join(format!("helixnotes-path-security-test-{}", Uuid::new_v4()));
        let vault = test_root.join("vault");
        let trash = helixnotes_dir(&vault.to_string_lossy()).join("trash");
        let trashed_notebook = trash.join("20240101000000000_Notebook");
        fs::create_dir_all(&trashed_notebook).unwrap();
        fs::write(trash.join("20240101000000000_Notebook.meta"), "../escaped").unwrap();

        let result = restore_notebook(
            &vault.to_string_lossy(),
            &trashed_notebook.to_string_lossy(),
        );

        assert!(result.is_err());
        assert!(trashed_notebook.exists());
        assert!(!test_root.join("escaped").exists());
        fs::remove_dir_all(test_root).unwrap();
    }

    #[test]
    fn duplicates_note_content_and_assigns_unique_identity_and_name() {
        let vault =
            std::env::temp_dir().join(format!("helixnotes-duplicate-note-test-{}", Uuid::new_v4()));
        let notebook = vault.join("Projects");
        fs::create_dir_all(&notebook).unwrap();
        let source_path = notebook.join("Project.md");
        let source_raw = "---\nid: source-id\ntitle: Project\ntags:\n  - work\npinned: true\ncreated: 2020-01-01T00:00:00Z\nmodified: 2020-01-02T00:00:00Z\naliases:\n  - Plan\n---\n# Project\n\nOriginal body.\n";
        fs::write(&source_path, source_raw).unwrap();

        let vault_path = vault.to_string_lossy();
        let first = duplicate_note(&source_path.to_string_lossy(), &vault_path).unwrap();
        let second = duplicate_note(&source_path.to_string_lossy(), &vault_path).unwrap();

        assert_eq!(first.meta.title, "Project copy");
        assert_eq!(second.meta.title, "Project copy 2");
        assert_eq!(first.meta.tags, vec!["work"]);
        assert!(!first.meta.pinned);
        assert_ne!(first.meta.id, "source-id");
        assert_ne!(first.meta.id, second.meta.id);
        assert_eq!(first.relative_path, "Projects/Project copy.md");
        assert_eq!(second.relative_path, "Projects/Project copy 2.md");

        let first_raw = fs::read_to_string(&first.path).unwrap();
        assert!(first_raw.contains("aliases:\n- Plan"));
        assert!(first_raw.contains("# Project copy\n\nOriginal body."));
        assert!(!first_raw.contains("\n# Project\n"));
        assert_eq!(fs::read_to_string(&source_path).unwrap(), source_raw);

        fs::remove_dir_all(vault).unwrap();
    }
}