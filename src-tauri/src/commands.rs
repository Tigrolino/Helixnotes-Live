use crate::search::SearchIndex;
use crate::state::AppState;
use crate::types::*;
use crate::vault::{operations, watcher};
use std::path::Path;
use tauri::{AppHandle, Manager, State};

// ── Vault Management ──

#[tauri::command]
pub fn open_vault(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    operations::ensure_vault_structure(&path)?;

    // Initialize search index - rebuild in background on mobile (sandboxed FS is slow)
    let search = std::sync::Arc::new(SearchIndex::new(&path)?);
    #[cfg(mobile)]
    {
        let search_bg = search.clone();
        let vault = path.clone();
        std::thread::spawn(move || {
            let _ = search_bg.rebuild(&vault);
            log::info!("mobile: search index rebuild complete");
        });
    }
    #[cfg(desktop)]
    {
        search.rebuild(&path)?;
    }
    *state.search_index.lock().map_err(|e| e.to_string())? = Some(search);

    // Start file watcher
    let w = watcher::start_watcher(app.clone(), path.clone())?;
    *state.watcher.lock().map_err(|e| e.to_string())? = Some(w);

    // Update config
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    if !config.vaults.iter().any(|v| v.path == path) {
        let name = std::path::Path::new(&path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        config.vaults.push(VaultConfig {
            path: path.clone(),
            name,
        });
    }
    config.active_vault = Some(path);
    save_app_config(&config)?;

    Ok(())
}

#[tauri::command]
pub fn remove_vault(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.vaults.retain(|v| v.path != path);
    if config.active_vault.as_deref() == Some(path.as_str()) {
        config.active_vault = None;
    }
    save_app_config(&config)?;
    Ok(())
}

#[tauri::command]
pub fn get_app_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(config.clone())
}

#[tauri::command]
pub fn get_pending_open_file(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let mut pending = state.pending_open_file.lock().map_err(|e| e.to_string())?;
    Ok(pending.take())
}

#[tauri::command]
pub fn set_theme(state: State<'_, AppState>, theme: String) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.theme = theme;
    save_app_config(&config)?;
    Ok(())
}

#[tauri::command]
pub fn set_accent_color(state: State<'_, AppState>, color: String) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.accent_color = Some(color);
    save_app_config(&config)?;
    Ok(())
}

#[tauri::command]
pub fn set_font_size(state: State<'_, AppState>, size: u32) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.font_size = Some(size);
    save_app_config(&config)?;
    Ok(())
}

#[tauri::command]
pub fn set_font_family(state: State<'_, AppState>, family: String) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.font_family = Some(family);
    save_app_config(&config)?;
    Ok(())
}

#[tauri::command]
pub fn set_line_height(state: State<'_, AppState>, height: f64) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.line_height = Some(height);
    save_app_config(&config)?;
    Ok(())
}

#[tauri::command]
pub fn set_ui_scale(state: State<'_, AppState>, scale: f64) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.ui_scale = Some(scale);
    save_app_config(&config)?;
    Ok(())
}

#[tauri::command]
pub fn set_content_width(state: State<'_, AppState>, width: Option<u32>) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.content_width = width;
    save_app_config(&config)?;
    Ok(())
}

// ── Notebooks ──

#[tauri::command]
pub fn get_notebooks(state: State<'_, AppState>) -> Result<Vec<NotebookEntry>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::scan_notebooks(vault_path)
}

#[tauri::command]
pub fn count_root_notes(state: State<'_, AppState>) -> Result<usize, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::count_root_notes(vault_path)
}

#[tauri::command]
pub fn create_notebook(
    state: State<'_, AppState>,
    parent_relative: Option<String>,
    name: String,
) -> Result<NotebookEntry, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::create_notebook(vault_path, parent_relative.as_deref(), &name)
}

#[tauri::command]
pub fn rename_notebook(path: String, new_name: String) -> Result<String, String> {
    operations::rename_notebook(&path, &new_name)
}

#[tauri::command]
pub fn move_notebook(
    state: State<'_, AppState>,
    notebook_path: String,
    dest_parent: String,
) -> Result<String, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;

    let old_relative = Path::new(&notebook_path)
        .strip_prefix(vault_path.as_str())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let new_full_path = operations::move_notebook(&notebook_path, &dest_parent)?;

    let new_relative = Path::new(&new_full_path)
        .strip_prefix(vault_path.as_str())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // Update Quick Access paths for all notes inside the moved notebook
    if let Ok(mut qa) = operations::load_quick_access(vault_path) {
        let old_prefix = format!("{}/", old_relative);
        let mut changed = false;
        for path in qa.iter_mut() {
            if path.starts_with(&old_prefix) {
                *path = format!("{}/{}", new_relative, &path[old_prefix.len()..]);
                changed = true;
            }
        }
        if changed {
            let _ = operations::save_quick_access(vault_path, &qa);
        }
    }

    // Update notebook icon mappings
    if let Ok(icons) = operations::load_notebook_icons(vault_path) {
        let old_prefix = format!("{}/", old_relative);
        let mut new_icons = std::collections::HashMap::new();
        let mut changed = false;
        for (key, value) in &icons {
            if *key == old_relative {
                new_icons.insert(new_relative.clone(), value.clone());
                changed = true;
            } else if key.starts_with(&old_prefix) {
                let new_key = format!("{}/{}", new_relative, &key[old_prefix.len()..]);
                new_icons.insert(new_key, value.clone());
                changed = true;
            } else {
                new_icons.insert(key.clone(), value.clone());
            }
        }
        if changed {
            let icons_path = operations::helixnotes_dir(vault_path).join("notebook_icons.json");
            if let Ok(data) = serde_json::to_string_pretty(&new_icons) {
                let _ = std::fs::write(&icons_path, data);
            }
        }
    }

    // Rebuild search index (paths changed for all contained notes)
    if let Ok(search_guard) = state.search_index.lock() {
        if let Some(ref search) = *search_guard {
            let _ = search.rebuild(vault_path);
        }
    }

    Ok(new_full_path)
}

#[tauri::command]
pub fn delete_notebook(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let vault_path = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        config.active_vault.as_ref().ok_or("No active vault")?.clone()
    };
    operations::delete_notebook(&vault_path, &path)
}

// ── Notes ──

#[tauri::command]
pub fn get_notes(
    state: State<'_, AppState>,
    notebook_path: Option<String>,
) -> Result<Vec<NoteEntry>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::scan_notes(vault_path, notebook_path.as_deref())
}

#[tauri::command]
pub fn read_note(path: String) -> Result<NoteContent, String> {
    operations::read_note(&path)
}

#[tauri::command]
pub fn save_note(
    state: State<'_, AppState>,
    path: String,
    meta: NoteMeta,
    body: String,
) -> Result<(), String> {
    // Snapshot current content before overwriting (if file exists)
    let config = state.config.lock().map_err(|e| e.to_string())?;
    if let Some(vault_path) = &config.active_vault {
        if std::path::Path::new(&path).exists() {
            if let Ok(old_raw) = std::fs::read_to_string(&path) {
                let max_versions = config.max_versions_per_note;
                let note_id = meta.id.clone();
                let vp = vault_path.clone();
                // Snapshot in background so save isn't slowed down
                std::thread::spawn(move || {
                    crate::history::maybe_snapshot(&vp, &note_id, &old_raw, max_versions);
                });
            }
        }
    }
    drop(config);

    operations::save_note(&path, &meta, &body)?;

    // Re-index note so search picks up changes immediately
    if let Ok(search_guard) = state.search_index.lock() {
        if let Some(ref search) = *search_guard {
            let _ = search.index_note(&path);
        }
    }

    Ok(())
}

#[tauri::command]
pub fn create_note(
    state: State<'_, AppState>,
    notebook_relative: Option<String>,
    title: String,
) -> Result<NoteEntry, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    let entry = operations::create_note(vault_path, notebook_relative.as_deref(), &title)?;

    // Index new note
    if let Ok(search_guard) = state.search_index.lock() {
        if let Some(ref search) = *search_guard {
            let _ = search.index_note(&entry.path);
        }
    }

    Ok(entry)
}

#[tauri::command]
pub fn create_daily_note(state: State<'_, AppState>, date: Option<String>) -> Result<NoteEntry, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    let entry = operations::create_daily_note(vault_path, date.as_deref(), &config.daily_title_format)?;

    if let Ok(search_guard) = state.search_index.lock() {
        if let Some(ref search) = *search_guard {
            let _ = search.index_note(&entry.path);
        }
    }

    Ok(entry)
}

#[tauri::command]
pub fn rename_note(state: State<'_, AppState>, path: String, new_title: String) -> Result<String, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?.clone();
    drop(config);
    operations::rename_note(&path, &new_title, &vault_path)
}

#[tauri::command]
pub fn delete_note(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::delete_note(vault_path, &path)?;

    // Remove from index
    if let Ok(search_guard) = state.search_index.lock() {
        if let Some(ref search) = *search_guard {
            let _ = search.remove_note(&path);
        }
    }

    Ok(())
}

#[tauri::command]
pub fn move_note(
    state: State<'_, AppState>,
    note_path: String,
    dest_notebook: String,
) -> Result<String, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;

    // Compute old relative path before move
    let old_relative = Path::new(&note_path)
        .strip_prefix(vault_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let new_full_path = operations::move_note(&note_path, &dest_notebook)?;

    // Update quick access if the moved note was in it
    if !old_relative.is_empty() {
        if let Ok(mut qa) = operations::load_quick_access(vault_path) {
            if let Some(pos) = qa.iter().position(|p| *p == old_relative) {
                let new_relative = Path::new(&new_full_path)
                    .strip_prefix(vault_path)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                if !new_relative.is_empty() {
                    qa[pos] = new_relative;
                    let _ = operations::save_quick_access(vault_path, &qa);
                }
            }
        }
    }

    Ok(new_full_path)
}

// ── Tags ──

#[tauri::command]
pub fn get_all_tags(state: State<'_, AppState>) -> Result<Vec<(String, usize)>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::get_all_tags(vault_path)
}

// ── Wiki-links ──

#[tauri::command]
pub fn get_all_note_titles(state: State<'_, AppState>) -> Result<Vec<NoteTitleEntry>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    let vault = std::path::Path::new(vault_path);
    let hn_dir = operations::helixnotes_dir(vault_path);
    let mut entries = Vec::new();

    for entry in walkdir::WalkDir::new(vault)
        .into_iter()
        // Cross-platform exclusion (string "/.helixnotes/" checks miss Windows
        // backslash paths, which leaked .helixnotes/history snapshots in as dupes).
        .filter_entry(|e| !operations::is_hidden(e.path()) && !e.path().starts_with(&hn_dir))
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let path_str = path.to_string_lossy();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        // Read frontmatter title, fallback to filename
        let title = if let Ok(raw) = std::fs::read_to_string(path) {
            crate::vault::frontmatter::extract_title(&raw).unwrap_or_else(|| {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            })
        } else {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        };

        entries.push(NoteTitleEntry {
            title,
            path: path_str.to_string(),
        });
    }

    Ok(entries)
}

// ── Graph ──

#[tauri::command]
pub fn get_graph_data(state: State<'_, AppState>) -> Result<crate::types::GraphData, String> {
    use std::collections::{HashMap, HashSet};

    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    let vault = std::path::Path::new(vault_path);

    // Pass 1: collect ALL notes as nodes (no title deduplication - every note must appear)
    let mut graph_nodes = Vec::new();
    // title → list of node indices (one title can map to multiple notes in different folders)
    let mut title_to_idxs: HashMap<String, Vec<usize>> = HashMap::new();
    // vault-relative path without extension → idx (for [[subfolder/note]] style links)
    let mut relpath_to_idx: HashMap<String, usize> = HashMap::new();
    // absolute path → idx (for fast active-note lookup)
    let mut path_to_idx: HashMap<String, usize> = HashMap::new();
    let mut seen_paths: HashSet<String> = HashSet::new();
    let mut contents: Vec<String> = Vec::new();

    let hn_dir = operations::helixnotes_dir(vault_path);
    for entry in walkdir::WalkDir::new(vault)
        .into_iter()
        // Cross-platform exclusion (string "/.helixnotes/" checks miss Windows
        // backslash paths; is_hidden also covers .stversions/.stfolder/.trash/.git).
        .filter_entry(|e| !operations::is_hidden(e.path()) && !e.path().starts_with(&hn_dir))
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() { continue; }
        let path_str = path.to_string_lossy().to_string();
        if path.extension().and_then(|e| e.to_str()) != Some("md") { continue; }
        // Skip Syncthing conflict files
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.contains(".sync-conflict-") { continue; }
        }
        // Deduplicate by canonical path (handles symlinks)
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let canonical_str = canonical.to_string_lossy().to_string();
        if !seen_paths.insert(canonical_str) { continue; }

        let raw = std::fs::read_to_string(path).unwrap_or_default();

        // Fast title extraction: scan for "title: " line in frontmatter without full YAML parse
        let title = extract_title_fast(&raw).unwrap_or_else(|| {
            path.file_stem().unwrap_or_default().to_string_lossy().to_string()
        });

        let idx = graph_nodes.len();
        let title_lower = title.to_lowercase();
        title_to_idxs.entry(title_lower).or_default().push(idx);
        path_to_idx.insert(path_str.clone(), idx);

        // Also index by vault-relative path without extension (e.g. "subfolder/note name")
        if let Ok(rel) = path.strip_prefix(vault) {
            let rel_no_ext = rel.with_extension("");
            // Normalize Windows backslashes so [[folder/note]] links resolve cross-platform.
            let rel_lower = rel_no_ext.to_string_lossy().replace('\\', "/").to_lowercase();
            relpath_to_idx.entry(rel_lower).or_insert(idx);
        }

        graph_nodes.push(crate::types::GraphNode {
            title,
            path: path_str,
        });
        contents.push(raw);
    }

    // Pass 2: extract edges from wiki-links (inline scan, no regex)
    // edge_map: (src, tgt) → index in edges vec, used to detect reverse links
    let mut edges: Vec<crate::types::GraphEdge> = Vec::new();
    let mut edge_map: HashMap<(usize, usize), usize> = HashMap::new();

    let add_edge = |edges: &mut Vec<crate::types::GraphEdge>, edge_map: &mut HashMap<(usize, usize), usize>, src: usize, tgt: usize| {
        if src == tgt { return; }
        if edge_map.contains_key(&(src, tgt)) { return; } // exact duplicate
        if let Some(&rev_idx) = edge_map.get(&(tgt, src)) {
            // Reverse direction already exists - mark it as bidirectional
            edges[rev_idx].bidirectional = true;
        } else {
            let idx = edges.len();
            edge_map.insert((src, tgt), idx);
            edges.push(crate::types::GraphEdge { source: src, target: tgt, bidirectional: false });
        }
    };

    for (source_idx, body) in contents.iter().enumerate() {
        let bytes = body.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i + 1 < len {
            if bytes[i] == b'[' && bytes[i + 1] == b'[' {
                i += 2;
                let start = i;
                while i + 1 < len && !(bytes[i] == b']' && bytes[i + 1] == b']') {
                    i += 1;
                }
                if i + 1 < len {
                    let link_raw = &body[start..i];
                    // Strip |alias, #heading, ^block
                    let link = link_raw.split('|').next().unwrap_or(link_raw);
                    let link = link.split('#').next().unwrap_or(link);
                    let link = link.split('^').next().unwrap_or(link);
                    let link = link.trim().to_lowercase();

                    // 1. Try vault-relative path match (e.g. "arkhost/note name")
                    if let Some(&target_idx) = relpath_to_idx.get(&link) {
                        add_edge(&mut edges, &mut edge_map, source_idx, target_idx);
                    } else if let Some(targets) = title_to_idxs.get(&link) {
                        // 2. Title match - connect to all notes with this title
                        for &target_idx in targets {
                            add_edge(&mut edges, &mut edge_map, source_idx, target_idx);
                        }
                    } else if link.contains('/') {
                        // 3. Path-based ref: try last segment as title fallback
                        if let Some(seg) = link.rsplit('/').next() {
                            if let Some(targets) = title_to_idxs.get(seg) {
                                for &target_idx in targets {
                                    add_edge(&mut edges, &mut edge_map, source_idx, target_idx);
                                }
                            }
                        }
                    }
                    i += 2;
                }
            } else {
                i += 1;
            }
        }
    }

    Ok(crate::types::GraphData { nodes: graph_nodes, edges })
}

/// Fast title extraction from frontmatter without full YAML parsing.
/// Scans for `title: ...` line within `---` fences.
fn extract_title_fast(raw: &str) -> Option<String> {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") { return None; }
    // Find the closing ---
    let after_open = &trimmed[3..];
    let end = after_open.find("\n---")?;
    let frontmatter = &after_open[..end];
    for line in frontmatter.lines() {
        let line = line.trim();
        if line.starts_with("title:") {
            let val = line[6..].trim();
            // Strip surrounding quotes
            if (val.starts_with('"') && val.ends_with('"')) || (val.starts_with('\'') && val.ends_with('\'')) {
                return Some(val[1..val.len()-1].to_string());
            }
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

// ── Tasks ──

#[tauri::command]
pub fn get_tasks(state: State<'_, AppState>) -> Result<Vec<crate::types::TaskItem>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    let vault = std::path::Path::new(vault_path);
    let hn_dir = operations::helixnotes_dir(vault_path);

    let task_re = regex::Regex::new(r"^\s*[-*]\s\[([ xX])\]\s+(.+?)\s*$").unwrap();
    let due_re = regex::Regex::new(r"\bdue:(\d{4}-\d{2}-\d{2})\b").unwrap();
    let prio_re = regex::Regex::new(r"(?i)(?:^|\s)!(high|medium|med|low)\b").unwrap();

    let mut tasks = Vec::new();
    for entry in walkdir::WalkDir::new(vault)
        .into_iter()
        .filter_entry(|e| !operations::is_hidden(e.path()) && !e.path().starts_with(&hn_dir))
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let raw = match std::fs::read_to_string(path) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let (meta, body) = crate::vault::frontmatter::parse_note(&raw, &filename);
        let note_path = path.to_string_lossy().to_string();

        for (i, line) in body.lines().enumerate() {
            let caps = match task_re.captures(line) {
                Some(c) => c,
                None => continue,
            };
            let completed = !caps[1].trim().is_empty(); // 'x'/'X' completed, ' ' open
            let content = caps[2].to_string();

            let due = due_re.captures(&content).map(|c| c[1].to_string());
            let priority = prio_re.captures(&content).map(|c| {
                let p = c[1].to_lowercase();
                if p == "medium" { "med".to_string() } else { p }
            });
            // Strip metadata tokens for the display text, then collapse whitespace.
            let mut text = due_re.replace_all(&content, "").to_string();
            text = prio_re.replace_all(&text, " ").to_string();
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");

            tasks.push(crate::types::TaskItem {
                note_path: note_path.clone(),
                note_title: meta.title.clone(),
                line: i,
                raw_line: line.to_string(),
                text,
                completed,
                due,
                priority,
            });
        }
    }
    Ok(tasks)
}

fn toggle_checkbox_line(line: &str, done: bool) -> String {
    let mut s = line.to_string();
    if done {
        if let Some(pos) = s.find("[ ]") {
            s.replace_range(pos..pos + 3, "[x]");
        }
    } else {
        for marker in ["[x]", "[X]"] {
            if let Some(pos) = s.find(marker) {
                s.replace_range(pos..pos + 3, "[ ]");
                break;
            }
        }
    }
    s
}

#[tauri::command]
pub fn set_task_done(
    state: State<'_, AppState>,
    note_path: String,
    line: usize,
    raw_line: String,
    done: bool,
) -> Result<(), String> {
    let p = std::path::Path::new(&note_path);
    let raw = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
    let filename = p.file_name().unwrap_or_default().to_string_lossy().to_string();
    let (meta, body) = crate::vault::frontmatter::parse_note(&raw, &filename);

    let mut lines: Vec<String> = body.lines().map(|l| l.to_string()).collect();
    // Verify the expected line; if the note drifted, fall back to the first exact match.
    let idx = if lines.get(line).map(|l| *l == raw_line).unwrap_or(false) {
        line
    } else {
        lines
            .iter()
            .position(|l| *l == raw_line)
            .ok_or("Task line not found (note changed)")?
    };

    let toggled = toggle_checkbox_line(&lines[idx], done);
    if toggled == lines[idx] {
        return Ok(()); // already in the desired state
    }
    lines[idx] = toggled;
    let mut new_body = lines.join("\n");
    if body.ends_with('\n') && !new_body.ends_with('\n') {
        new_body.push('\n');
    }
    operations::save_note(&note_path, &meta, &new_body)?;

    if let Ok(guard) = state.search_index.lock() {
        if let Some(search) = guard.as_ref() {
            let _ = search.index_note(&note_path);
        }
    }
    Ok(())
}

fn set_priority_on_line(line: &str, priority: Option<&str>) -> String {
    let prio_re = regex::Regex::new(r"(?i)(?:^|\s)!(?:high|medium|med|low)\b").unwrap();
    let stripped = prio_re.replace_all(line, "").to_string();
    let stripped = stripped.trim_end();
    match priority {
        Some(p) => format!("{} !{}", stripped, p),
        None => stripped.to_string(),
    }
}

#[tauri::command]
pub fn set_task_priority(
    state: State<'_, AppState>,
    note_path: String,
    line: usize,
    raw_line: String,
    priority: Option<String>,
) -> Result<(), String> {
    // Normalize/validate priority ("medium" -> "med"); None clears it.
    let prio = match priority.as_deref() {
        None | Some("") | Some("none") => None,
        Some("high") => Some("high"),
        Some("med") | Some("medium") => Some("med"),
        Some("low") => Some("low"),
        Some(_) => return Err("Invalid priority".to_string()),
    };

    let p = std::path::Path::new(&note_path);
    let raw = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
    let filename = p.file_name().unwrap_or_default().to_string_lossy().to_string();
    let (meta, body) = crate::vault::frontmatter::parse_note(&raw, &filename);

    let mut lines: Vec<String> = body.lines().map(|l| l.to_string()).collect();
    let idx = if lines.get(line).map(|l| *l == raw_line).unwrap_or(false) {
        line
    } else {
        lines
            .iter()
            .position(|l| *l == raw_line)
            .ok_or("Task line not found (note changed)")?
    };

    let updated = set_priority_on_line(&lines[idx], prio);
    if updated == lines[idx] {
        return Ok(());
    }
    lines[idx] = updated;
    let mut new_body = lines.join("\n");
    if body.ends_with('\n') && !new_body.ends_with('\n') {
        new_body.push('\n');
    }
    operations::save_note(&note_path, &meta, &new_body)?;

    if let Ok(guard) = state.search_index.lock() {
        if let Some(search) = guard.as_ref() {
            let _ = search.index_note(&note_path);
        }
    }
    Ok(())
}

fn set_due_on_line(line: &str, due: Option<&str>) -> String {
    let due_re = regex::Regex::new(r"(?:^|\s)due:\d{4}-\d{2}-\d{2}\b").unwrap();
    let stripped = due_re.replace_all(line, "").to_string();
    let stripped = stripped.trim_end();
    match due {
        Some(d) => format!("{} due:{}", stripped, d),
        None => stripped.to_string(),
    }
}

#[tauri::command]
pub fn set_task_due(
    state: State<'_, AppState>,
    note_path: String,
    line: usize,
    raw_line: String,
    due: Option<String>,
) -> Result<(), String> {
    // Validate format (YYYY-MM-DD); None/empty clears the due date.
    let date_re = regex::Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();
    let due_val: Option<&str> = match due.as_deref() {
        None | Some("") => None,
        Some(d) if date_re.is_match(d) => Some(d),
        Some(_) => return Err("Invalid due date".to_string()),
    };

    let p = std::path::Path::new(&note_path);
    let raw = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
    let filename = p.file_name().unwrap_or_default().to_string_lossy().to_string();
    let (meta, body) = crate::vault::frontmatter::parse_note(&raw, &filename);

    let mut lines: Vec<String> = body.lines().map(|l| l.to_string()).collect();
    let idx = if lines.get(line).map(|l| *l == raw_line).unwrap_or(false) {
        line
    } else {
        lines
            .iter()
            .position(|l| *l == raw_line)
            .ok_or("Task line not found (note changed)")?
    };

    let updated = set_due_on_line(&lines[idx], due_val);
    if updated == lines[idx] {
        return Ok(());
    }
    lines[idx] = updated;
    let mut new_body = lines.join("\n");
    if body.ends_with('\n') && !new_body.ends_with('\n') {
        new_body.push('\n');
    }
    operations::save_note(&note_path, &meta, &new_body)?;

    if let Ok(guard) = state.search_index.lock() {
        if let Some(search) = guard.as_ref() {
            let _ = search.index_note(&note_path);
        }
    }
    Ok(())
}

// ── Search ──

#[tauri::command]
pub fn search_notes(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, String> {
    let search_guard = state.search_index.lock().map_err(|e| e.to_string())?;
    let search = search_guard
        .as_ref()
        .ok_or("Search index not initialized")?;
    search.search(&query, limit.unwrap_or(20))
}

#[tauri::command]
pub fn reindex(state: State<'_, AppState>) -> Result<(), String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    let search_guard = state.search_index.lock().map_err(|e| e.to_string())?;
    let search = search_guard
        .as_ref()
        .ok_or("Search index not initialized")?;
    search.rebuild(vault_path)
}

// ── Trash ──

#[tauri::command]
pub fn get_trash(state: State<'_, AppState>) -> Result<TrashContents, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::get_trash_contents(vault_path)
}

#[tauri::command]
pub fn restore_note(
    state: State<'_, AppState>,
    trash_path: String,
    dest_notebook: Option<String>,
) -> Result<String, String> {
    let vault_path = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        config.active_vault.as_ref().ok_or("No active vault")?.clone()
    };
    operations::restore_note(&vault_path, &trash_path, dest_notebook.as_deref())
}

#[tauri::command]
pub fn restore_notebook(state: State<'_, AppState>, trash_path: String) -> Result<String, String> {
    let vault_path = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        config.active_vault.as_ref().ok_or("No active vault")?.clone()
    };
    operations::restore_notebook(&vault_path, &trash_path)
}

#[tauri::command]
pub fn permanent_delete(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let vault_path = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        config.active_vault.as_ref().ok_or("No active vault")?.clone()
    };
    operations::permanent_delete(&vault_path, &path)
}

#[tauri::command]
pub fn empty_trash(state: State<'_, AppState>) -> Result<(), String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::empty_trash(vault_path)
}

// ── Vault State ──

#[tauri::command]
pub fn load_vault_state(state: State<'_, AppState>) -> Result<VaultState, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::load_vault_state(vault_path)
}

#[tauri::command]
pub fn save_vault_state(state: State<'_, AppState>, vault_state: VaultState) -> Result<(), String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::save_vault_state(vault_path, &vault_state)
}

// ── Clipboard ──

/// Read image from system clipboard (bypasses WebKitGTK clipboard bug).
/// Returns PNG bytes as Vec<u8>, or error if no image on clipboard.
#[cfg(desktop)]
#[tauri::command]
pub fn read_clipboard_image() -> Result<Vec<u8>, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Clipboard init failed: {}", e))?;
    let img = clipboard
        .get_image()
        .map_err(|_| "No image on clipboard".to_string())?;
    // Encode RGBA data to PNG
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut encoder =
            png::Encoder::new(std::io::Cursor::new(&mut buf), img.width as u32, img.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("PNG header failed: {}", e))?;
        writer
            .write_image_data(&img.bytes)
            .map_err(|e| format!("PNG encode failed: {}", e))?;
    }
    Ok(buf)
}

#[cfg(mobile)]
#[tauri::command]
pub fn read_clipboard_image() -> Result<Vec<u8>, String> {
    Err("Clipboard image reading not supported on Android".to_string())
}

/// Copy an image file to the system clipboard.
#[cfg(desktop)]
#[tauri::command]
pub fn copy_image_to_clipboard(path: String) -> Result<(), String> {
    let data = std::fs::read(&path).map_err(|e| format!("Failed to read image: {}", e))?;
    let img = image::load_from_memory(&data)
        .map_err(|e| format!("Failed to decode image: {}", e))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let img_data = arboard::ImageData {
        width: w as usize,
        height: h as usize,
        bytes: std::borrow::Cow::Owned(rgba.into_raw()),
    };
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| format!("Clipboard init failed: {}", e))?;
    clipboard.set_image(img_data)
        .map_err(|e| format!("Failed to set clipboard image: {}", e))?;
    Ok(())
}

#[cfg(mobile)]
#[tauri::command]
pub fn copy_image_to_clipboard(_path: String) -> Result<(), String> {
    Err("Clipboard image copy not supported on Android".to_string())
}

/// Copy PNG bytes directly to the system clipboard.
#[cfg(desktop)]
#[tauri::command]
pub fn copy_png_to_clipboard(data: Vec<u8>) -> Result<(), String> {
    let img = image::load_from_memory(&data)
        .map_err(|e| format!("Failed to decode image: {}", e))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let img_data = arboard::ImageData {
        width: w as usize,
        height: h as usize,
        bytes: std::borrow::Cow::Owned(rgba.into_raw()),
    };
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| format!("Clipboard init failed: {}", e))?;
    clipboard.set_image(img_data)
        .map_err(|e| format!("Failed to set clipboard image: {}", e))?;
    Ok(())
}

#[cfg(mobile)]
#[tauri::command]
pub fn copy_png_to_clipboard(_data: Vec<u8>) -> Result<(), String> {
    Err("Clipboard image copy not supported on Android".to_string())
}

// ── Attachments ──

#[tauri::command]
pub fn save_image(
    state: State<'_, AppState>,
    name: String,
    data: Vec<u8>,
) -> Result<String, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::save_image(vault_path, &name, &data)
}

#[tauri::command]
pub fn save_attachment(
    state: State<'_, AppState>,
    name: String,
    data: Vec<u8>,
) -> Result<String, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::save_attachment(vault_path, &name, &data)
}

// ── Notebook Icons ──

#[tauri::command]
pub fn get_notebook_icons(
    state: State<'_, AppState>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::load_notebook_icons(vault_path)
}

#[tauri::command]
pub fn set_notebook_icon(
    state: State<'_, AppState>,
    notebook_relative: String,
    icon_relative: Option<String>,
) -> Result<(), String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::set_notebook_icon(vault_path, &notebook_relative, icon_relative.as_deref())
}

// ── General Settings ──

#[tauri::command]
pub fn set_general_settings(
    state: State<'_, AppState>,
    compact_notes: bool,
    time_format: String,
    week_start: String,
    daily_title_format: String,
    gpu_acceleration: bool,
    autostart: bool,
    pdf_preview: bool,
    pdf_height: u32,
    title_mode: String,
    hide_title_in_body: bool,
    show_line_numbers: bool,
    show_link_arrows: bool,
    default_view_mode: bool,
    show_tray_icon: bool,
    close_to_tray: bool,
    enable_wiki_links: bool,
    show_note_dates: bool,
    restore_last_session: bool,
    show_all_notes: bool,
    show_quick_access: bool,
    show_tasks: bool,
    show_daily_notes: bool,
    show_trash: bool,
) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.compact_notes = compact_notes;
    config.show_note_dates = show_note_dates;
    config.restore_last_session = restore_last_session;
    config.show_all_notes = show_all_notes;
    config.show_quick_access = show_quick_access;
    config.show_tasks = show_tasks;
    config.show_daily_notes = show_daily_notes;
    config.show_trash = show_trash;
    config.time_format = time_format;
    config.week_start = week_start;
    config.daily_title_format = daily_title_format;
    config.gpu_acceleration = gpu_acceleration;
    config.autostart = autostart;
    config.pdf_preview = pdf_preview;
    config.pdf_height = pdf_height;
    config.title_mode = title_mode;
    config.hide_title_in_body = hide_title_in_body;
    config.show_line_numbers = show_line_numbers;
    config.show_link_arrows = show_link_arrows;
    config.default_view_mode = default_view_mode;
    config.show_tray_icon = show_tray_icon;
    config.close_to_tray = close_to_tray;
    config.enable_wiki_links = enable_wiki_links;
    save_app_config(&config)?;
    Ok(())
}

// ── Quick Access ──

#[tauri::command]
pub fn get_quick_access(state: State<'_, AppState>) -> Result<Vec<NoteEntry>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::get_quick_access_notes(vault_path)
}

#[tauri::command]
pub fn add_quick_access(state: State<'_, AppState>, note_relative: String) -> Result<(), String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::add_quick_access(vault_path, &note_relative)
}

#[tauri::command]
pub fn remove_quick_access(
    state: State<'_, AppState>,
    note_relative: String,
) -> Result<(), String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::remove_quick_access(vault_path, &note_relative)
}

#[tauri::command]
pub fn reorder_quick_access(state: State<'_, AppState>, paths: Vec<String>) -> Result<(), String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    operations::save_quick_access(vault_path, &paths)
}

#[tauri::command]
pub fn get_vault_stats(state: State<'_, AppState>) -> Result<VaultStats, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;

    let mut total_notes: u64 = 0;
    let mut total_attachments: u64 = 0;
    let mut notes_size: u64 = 0;
    let mut attachments_size: u64 = 0;

    for entry in walkdir::WalkDir::new(vault_path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let path_str = path.to_string_lossy();
        // Include .helixnotes/attachments, skip everything else in .helixnotes
        if path_str.contains("/.helixnotes/") && !path_str.contains("/.helixnotes/attachments/") {
            continue;
        }
        // Skip trash
        if path_str.contains("/.trash/") {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext == "md" {
            total_notes += 1;
            notes_size += size;
        } else {
            total_attachments += 1;
            attachments_size += size;
        }
    }

    Ok(VaultStats {
        total_notes,
        total_attachments,
        notes_size,
        attachments_size,
        total_size: notes_size + attachments_size,
    })
}

#[tauri::command]
pub fn import_obsidian(app: AppHandle) -> Result<(), String> {
    let vault_path = {
        let state = app.state::<AppState>();
        let config = state.config.lock().map_err(|e| e.to_string())?;
        config
            .active_vault
            .clone()
            .ok_or("No active vault".to_string())?
    };
    // Fire-and-forget: return immediately, do work in background thread
    std::thread::spawn(move || {
        use tauri::Emitter;
        match do_import_obsidian(app.clone(), &vault_path) {
            Ok(result) => {
                let _ = app.emit(
                    "import-done",
                    serde_json::json!({
                        "success": true,
                        "files_converted": result.files_converted,
                        "links_converted": result.links_converted,
                    }),
                );
            }
            Err(e) => {
                let _ = app.emit(
                    "import-done",
                    serde_json::json!({
                        "success": false,
                        "error": e,
                    }),
                );
            }
        }
    });
    Ok(())
}

fn do_import_obsidian(app: AppHandle, vault_path: &str) -> Result<ImportResult, String> {
    // Pause file watcher during import
    let state = app.state::<AppState>();
    state
        .importing
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let result = do_import_obsidian_inner(vault_path);

    // Resume file watcher
    state
        .importing
        .store(false, std::sync::atomic::Ordering::Relaxed);

    result
}

fn do_import_obsidian_inner(vault_path: &str) -> Result<ImportResult, String> {
    let vault = std::path::Path::new(vault_path);

    // Build file index: filename -> relative path from vault root
    let mut file_index: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
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
                file_index.insert(name.to_string(), rel.to_string_lossy().to_string());
            }
        }
    }

    let wiki_embed =
        regex::Regex::new(r"!\[\[([^\]|]+?)(?:\|([^\]]*))?\]\]").map_err(|e| e.to_string())?;
    let wiki_link =
        regex::Regex::new(r"\[\[([^\]|]+?)(?:\|([^\]]*))?\]\]").map_err(|e| e.to_string())?;
    let md_img = regex::Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").map_err(|e| e.to_string())?;
    let md_link = regex::Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").map_err(|e| e.to_string())?;

    let mut files_converted: u64 = 0;
    let mut links_converted: u64 = 0;

    // Walk all markdown files
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
        .collect();

    for entry in &md_files {
        let path = entry.path();
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let note_dir = path.parent().unwrap_or(std::path::Path::new(""));

        let mut changed = false;
        // Convert embeds first: ![[file.ext]] or ![[file.ext|alt]]
        let after_embeds = wiki_embed
            .replace_all(&content, |caps: &regex::Captures| {
                changed = true;
                links_converted += 1;
                let file_ref = &caps[1];
                let alt = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                let resolved = resolve_wiki_ref(&file_index, file_ref);
                // Convert vault-root-relative path to note-relative
                let abs_target = vault.join(&resolved);
                let rel = pathdiff(
                    abs_target.to_str().unwrap_or(""),
                    note_dir.to_str().unwrap_or(""),
                )
                .unwrap_or(resolved);
                let encoded = rel.replace(' ', "%20");
                // Use image syntax only for images/PDFs; regular link for other files
                let ext = std::path::Path::new(file_ref)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let is_embeddable = matches!(
                    ext.as_str(),
                    "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" | "ico" | "pdf"
                );
                if is_embeddable {
                    format!("![{}]({})", alt, encoded)
                } else {
                    let display = if alt.is_empty() {
                        file_ref.rsplit('/').next().unwrap_or(file_ref)
                    } else {
                        alt
                    };
                    format!("[{}]({})", display, encoded)
                }
            })
            .to_string();

        // Convert wiki links: [[note]] or [[note|display]]
        let after_links = wiki_link
            .replace_all(&after_embeds, |caps: &regex::Captures| {
                changed = true;
                links_converted += 1;
                let note_ref = &caps[1];
                let display = caps.get(2).map(|m| m.as_str()).unwrap_or(note_ref);
                let resolved = resolve_wiki_ref(&file_index, note_ref);
                // Convert vault-root-relative path to note-relative
                let abs_target = vault.join(&resolved);
                let rel = pathdiff(
                    abs_target.to_str().unwrap_or(""),
                    note_dir.to_str().unwrap_or(""),
                )
                .unwrap_or(resolved);
                let encoded = rel.replace(' ', "%20");
                format!("[{}]({})", display, encoded)
            })
            .to_string();

        // Fix bare-filename standard markdown references: ![alt](filename.ext) where filename has no path separator

        let after_img_fix = md_img
            .replace_all(&after_links, |caps: &regex::Captures| {
                let alt = &caps[1];
                let src = &caps[2];
                let decoded = percent_decode(src);
                // Skip URLs and absolute paths
                if decoded.starts_with("http") || decoded.starts_with('/') {
                    return format!("![{}]({})", alt, src);
                }
                // Try to resolve bare filename via file index
                if !decoded.contains('/') {
                    if let Some(rel) = file_index.get(&decoded) {
                        changed = true;
                        links_converted += 1;
                        let abs_target = vault.join(rel);
                        let note_rel = pathdiff(
                            abs_target.to_str().unwrap_or(""),
                            note_dir.to_str().unwrap_or(""),
                        )
                        .unwrap_or(rel.clone());
                        let encoded = note_rel.replace(' ', "%20");
                        return format!("![{}]({})", alt, encoded);
                    }
                }
                // Fix vault-root-relative paths: if the path resolves from vault root, make it note-relative
                if decoded.contains('/') {
                    let vault_abs = vault.join(&decoded);
                    if vault_abs.exists() {
                        let note_rel = pathdiff(
                            vault_abs.to_str().unwrap_or(""),
                            note_dir.to_str().unwrap_or(""),
                        )
                        .unwrap_or(decoded.clone());
                        if note_rel != decoded {
                            changed = true;
                            links_converted += 1;
                            let encoded = note_rel.replace(' ', "%20");
                            return format!("![{}]({})", alt, encoded);
                        }
                    }
                }
                format!("![{}]({})", alt, src)
            })
            .to_string();

        let after_link_fix = md_link
            .replace_all(&after_img_fix, |caps: &regex::Captures| {
                let display = &caps[1];
                let href = &caps[2];
                let decoded = percent_decode(href);
                // Skip URLs and absolute paths
                if decoded.starts_with("http") || decoded.starts_with('/') {
                    return format!("[{}]({})", display, href);
                }
                // Try to resolve bare filename via file index
                if !decoded.contains('/') {
                    if let Some(rel) = file_index.get(&decoded) {
                        changed = true;
                        links_converted += 1;
                        let abs_target = vault.join(rel);
                        let note_rel = pathdiff(
                            abs_target.to_str().unwrap_or(""),
                            note_dir.to_str().unwrap_or(""),
                        )
                        .unwrap_or(rel.clone());
                        let encoded = note_rel.replace(' ', "%20");
                        return format!("[{}]({})", display, encoded);
                    }
                }
                // Fix vault-root-relative paths
                if decoded.contains('/') {
                    let vault_abs = vault.join(&decoded);
                    if vault_abs.exists() {
                        let note_rel = pathdiff(
                            vault_abs.to_str().unwrap_or(""),
                            note_dir.to_str().unwrap_or(""),
                        )
                        .unwrap_or(decoded.clone());
                        if note_rel != decoded {
                            changed = true;
                            links_converted += 1;
                            let encoded = note_rel.replace(' ', "%20");
                            return format!("[{}]({})", display, encoded);
                        }
                    }
                }
                format!("[{}]({})", display, href)
            })
            .to_string();

        if changed {
            let _ = std::fs::write(path, after_link_fix);
            files_converted += 1;
        }
    }

    // ── Phase 2: Move all non-.md files into .helixnotes/attachments/ preserving structure ──
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
        .collect();

    // Map: old vault-relative path -> new vault-relative path (preserving structure)
    let mut moved_files: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for entry in &attachment_files {
        let src_path = entry.path();
        let old_rel = match src_path.strip_prefix(vault) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        // Preserve directory structure: ArkHost/Files/img.png -> .helixnotes/attachments/ArkHost/Files/img.png
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
            moved_files.insert(old_rel, new_rel);
        }
    }

    // ── Phase 3: Rewrite references in markdown files to point to new locations ──
    // Use regex to only replace inside ![...](...) and [...](...) patterns
    if !moved_files.is_empty() {
        let md_ref = regex::Regex::new(r"(!?\[[^\]]*\])\(([^)]+)\)").map_err(|e| e.to_string())?;

        let md_files_pass2: Vec<_> = walkdir::WalkDir::new(vault)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let p = e.path();
                p.is_file()
                    && p.extension().and_then(|x| x.to_str()) == Some("md")
                    && !p.to_string_lossy().contains("/.helixnotes/")
                    && !p.to_string_lossy().contains("/.trash/")
            })
            .collect();

        for entry in &md_files_pass2 {
            let path = entry.path();
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let note_dir = path.parent().unwrap_or(std::path::Path::new(""));
            let mut changed_p3 = false;

            let new_content = md_ref
                .replace_all(&content, |caps: &regex::Captures| {
                    let prefix = &caps[1]; // ![alt] or [text]
                    let href = &caps[2];
                    let decoded_href = percent_decode(href);

                    // Skip URLs
                    if decoded_href.starts_with("http") || decoded_href.starts_with("data:") {
                        return format!("{}({})", prefix, href);
                    }

                    // Resolve the href to a vault-relative path (files already moved, can't use .exists())
                    // Try note-dir-relative first, then vault-root-relative
                    let abs_from_note = note_dir.join(&decoded_href);
                    let rel_from_note = abs_from_note
                        .strip_prefix(vault)
                        .ok()
                        .map(|r| r.to_string_lossy().to_string());
                    let rel_from_vault = decoded_href.clone();

                    // Check note-relative resolution first, then vault-root
                    let old_vault_rel = rel_from_note
                        .as_ref()
                        .filter(|r| moved_files.contains_key(r.as_str()))
                        .cloned()
                        .or_else(|| {
                            if moved_files.contains_key(&rel_from_vault) {
                                Some(rel_from_vault)
                            } else {
                                None
                            }
                        });

                    if let Some(old_rel) = old_vault_rel {
                        if let Some(new_rel) = moved_files.get(&old_rel) {
                            changed_p3 = true;
                            let new_abs = vault.join(new_rel);
                            let new_note_rel = pathdiff(
                                new_abs.to_str().unwrap_or(""),
                                note_dir.to_str().unwrap_or(""),
                            )
                            .unwrap_or(new_rel.clone());
                            let encoded = new_note_rel.replace(' ', "%20");
                            return format!("{}({})", prefix, encoded);
                        }
                    }

                    format!("{}({})", prefix, href)
                })
                .to_string();

            if changed_p3 {
                let _ = std::fs::write(path, new_content);
            }
        }
    }

    // Clean up empty directories left behind after moving files
    cleanup_empty_dirs(vault);

    Ok(ImportResult {
        files_converted,
        links_converted,
    })
}

/// Remove empty directories recursively (bottom-up), skipping .helixnotes and .obsidian
fn cleanup_empty_dirs(root: &std::path::Path) {
    let mut dirs: Vec<_> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.path().to_path_buf())
        .collect();

    // Sort by depth (deepest first) so we clean bottom-up
    dirs.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

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

/// Compute a relative path from `base` to `target`
fn pathdiff(target: &str, base: &str) -> Result<String, ()> {
    let target = std::path::Path::new(target);
    let base = std::path::Path::new(base);

    let target_components: Vec<_> = target.components().collect();
    let base_components: Vec<_> = base.components().collect();

    // Find common prefix length
    let common = target_components
        .iter()
        .zip(base_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut result = std::path::PathBuf::new();
    // Go up from base to common ancestor
    for _ in common..base_components.len() {
        result.push("..");
    }
    // Go down from common ancestor to target
    for component in &target_components[common..] {
        result.push(component);
    }

    Ok(result.to_string_lossy().to_string())
}

// ── Orphaned attachment cleanup ──

#[derive(serde::Serialize)]
pub struct OrphanAttachment {
    pub name: String,
    pub size: u64,
}

// Conservatively find files in .helixnotes/attachments not referenced by ANY note. Scans every
// .md in the vault (including .helixnotes/trash, so a restorable trashed note keeps its files)
// plus notebook_icons.json, and matches each filename against both the raw text and a
// percent-decoded copy (so a URL-encoded path like `my%20file.png` still counts as a reference).
// When in doubt a file is KEPT: a leftover orphan is harmless, a wrong deletion is not.
fn scan_orphaned_attachments(vault: &str) -> Result<Vec<(String, u64)>, String> {
    let attachments_dir = operations::helixnotes_dir(vault).join("attachments");
    if !attachments_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files: Vec<(String, u64)> = Vec::new();
    for entry in std::fs::read_dir(&attachments_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.is_file() {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            files.push((name, size));
        }
    }
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut haystack = String::new();
    for entry in walkdir::WalkDir::new(vault).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("md") {
            if let Ok(content) = std::fs::read_to_string(p) {
                haystack.push_str(&content);
                haystack.push('\n');
            }
        }
    }
    // Folder icons live in attachments but are referenced here, not in notes. (#157)
    let icons_path = operations::helixnotes_dir(vault).join("notebook_icons.json");
    if let Ok(content) = std::fs::read_to_string(&icons_path) {
        haystack.push_str(&content);
        haystack.push('\n');
    }
    let decoded = percent_decode(&haystack);
    let orphans = files
        .into_iter()
        .filter(|(name, _)| !haystack.contains(name.as_str()) && !decoded.contains(name.as_str()))
        .collect();
    Ok(orphans)
}

#[tauri::command]
pub fn find_orphaned_attachments(
    state: State<'_, AppState>,
) -> Result<Vec<OrphanAttachment>, String> {
    let vault = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        config.active_vault.clone().ok_or("No active vault")?
    };
    let orphans = scan_orphaned_attachments(&vault)?;
    Ok(orphans
        .into_iter()
        .map(|(name, size)| OrphanAttachment { name, size })
        .collect())
}

#[tauri::command]
pub fn trash_orphaned_attachments(
    state: State<'_, AppState>,
    names: Vec<String>,
) -> Result<u32, String> {
    let vault = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        config.active_vault.clone().ok_or("No active vault")?
    };
    // Re-scan now and only move files that are STILL orphaned (guards against a reference added
    // between scan and confirm), intersected with the caller's selection.
    let current: std::collections::HashSet<String> = scan_orphaned_attachments(&vault)?
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let attachments_dir = operations::helixnotes_dir(&vault).join("attachments");
    let trash_dir = operations::helixnotes_dir(&vault).join("trash");
    std::fs::create_dir_all(&trash_dir).map_err(|e| e.to_string())?;
    let stamp = chrono::Utc::now().format("%Y%m%d%H%M%S%3f").to_string();
    let mut moved = 0u32;
    for (i, name) in names.iter().enumerate() {
        if !current.contains(name) {
            continue;
        }
        // Path-traversal guard: only act on a bare filename inside the attachments dir.
        if std::path::Path::new(name)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .as_deref()
            != Some(name.as_str())
        {
            continue;
        }
        let src = attachments_dir.join(name);
        if !src.is_file() {
            continue;
        }
        let dest = trash_dir.join(format!("{}_{}_attachment_{}", stamp, i, name));
        if std::fs::rename(&src, &dest).is_ok() {
            moved += 1;
        }
    }
    Ok(moved)
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

fn resolve_wiki_ref(
    file_index: &std::collections::HashMap<String, String>,
    reference: &str,
) -> String {
    let reference = reference.trim();
    // Strip Obsidian heading anchors (#) and block references (^) before lookup
    let reference = reference.split('#').next().unwrap_or(reference).trim();
    let reference = reference.split('^').next().unwrap_or(reference).trim();
    // Direct match by filename
    if let Some(rel) = file_index.get(reference) {
        return rel.clone();
    }
    // Try with .md extension
    let with_md = format!("{}.md", reference);
    if let Some(rel) = file_index.get(&with_md) {
        return rel.clone();
    }
    // Try matching just the filename part (e.g. "folder/note" -> "note.md")
    if let Some(name) = reference.rsplit('/').next() {
        if let Some(rel) = file_index.get(name) {
            return rel.clone();
        }
        let name_md = format!("{}.md", name);
        if let Some(rel) = file_index.get(&name_md) {
            return rel.clone();
        }
    }
    // Fallback: return as-is
    reference.to_string()
}

// ── Open file/URL with system default handler ──

fn xdg_open(arg: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let mut cmd = std::process::Command::new("xdg-open");
        cmd.arg(arg);
        // Clear AppImage environment so child processes find host binaries
        // (e.g. gio-launch-desktop on GNOME)
        if std::env::var("APPIMAGE").is_ok() {
            cmd.env_remove("LD_LIBRARY_PATH")
                .env_remove("LD_PRELOAD")
                .env_remove("GIO_LAUNCHED_DESKTOP_FILE")
                .env_remove("GIO_LAUNCHED_DESKTOP_FILE_PID");
            if let Ok(original_path) = std::env::var("PATH_ORIG") {
                cmd.env("PATH", original_path);
            }
        }
        cmd.spawn()
            .map_err(|e| format!("Failed to open {}: {}", arg, e))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(arg)
            .spawn()
            .map_err(|e| format!("Failed to open {}: {}", arg, e))?;
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: open the URL/path without flashing a console window.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", arg])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open {}: {}", arg, e))?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    xdg_open(&path)
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    xdg_open(&url)
}

#[tauri::command]
pub fn copy_file_to(source: String, destination: String) -> Result<(), String> {
    std::fs::copy(&source, &destination).map_err(|e| format!("Failed to copy file: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn write_bytes_to(destination: String, data: Vec<u8>) -> Result<(), String> {
    std::fs::write(&destination, &data).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(())
}


// ── Backup ──

#[tauri::command]
pub fn create_backup(app: AppHandle) -> Result<(), String> {
    let (vault_path, backup_dir, include_attachments, max_count) = {
        let state = app.state::<AppState>();
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let vault_path = config.active_vault.clone().ok_or("No active vault")?;
        let backup_dir = crate::backup::get_backup_dir(&config.backup_location)?;
        (
            vault_path,
            backup_dir,
            config.backup_include_attachments,
            config.backup_max_count,
        )
    };

    std::thread::spawn(move || {
        use tauri::Emitter;
        match crate::backup::create_backup(&vault_path, &backup_dir, include_attachments) {
            Ok(entry) => {
                // Update last backup time
                let state = app.state::<AppState>();
                if let Ok(mut config) = state.config.lock() {
                    config.last_backup_time = Some(entry.created.clone());
                    let _ = save_app_config(&config);
                }
                // Cleanup old backups
                let _ = crate::backup::cleanup_old_backups(&backup_dir, max_count);
                let _ = app.emit(
                    "backup-done",
                    serde_json::json!({
                        "success": true,
                        "entry": entry,
                    }),
                );
            }
            Err(e) => {
                let _ = app.emit(
                    "backup-done",
                    serde_json::json!({
                        "success": false,
                        "error": e,
                    }),
                );
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn list_backups(state: State<'_, AppState>) -> Result<Vec<BackupEntry>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let backup_dir = crate::backup::get_backup_dir(&config.backup_location)?;
    crate::backup::list_backups(&backup_dir)
}

#[tauri::command]
pub fn restore_backup(app: AppHandle, backup_path: String) -> Result<(), String> {
    let vault_path = {
        let state = app.state::<AppState>();
        let config = state.config.lock().map_err(|e| e.to_string())?;
        config.active_vault.clone().ok_or("No active vault")?
    };

    std::thread::spawn(move || {
        use tauri::Emitter;
        match crate::backup::restore_backup(&vault_path, &backup_path) {
            Ok(()) => {
                let _ = app.emit(
                    "restore-done",
                    serde_json::json!({
                        "success": true,
                    }),
                );
            }
            Err(e) => {
                let _ = app.emit(
                    "restore-done",
                    serde_json::json!({
                        "success": false,
                        "error": e,
                    }),
                );
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn delete_backup(backup_path: String) -> Result<(), String> {
    crate::backup::delete_backup(&backup_path)
}

#[tauri::command]
pub fn set_backup_settings(
    state: State<'_, AppState>,
    enabled: bool,
    frequency: String,
    max_count: u32,
    location: Option<String>,
    include_attachments: bool,
) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.backup_enabled = enabled;
    config.backup_frequency = frequency;
    config.backup_max_count = max_count;
    config.backup_location = location;
    config.backup_include_attachments = include_attachments;
    save_app_config(&config)?;
    Ok(())
}

// ── Version History ──

#[tauri::command]
pub fn get_note_versions(
    state: State<'_, AppState>,
    note_id: String,
) -> Result<Vec<crate::types::VersionEntry>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    crate::history::list_versions(vault_path, &note_id)
}

#[tauri::command]
pub fn create_version(
    state: State<'_, AppState>,
    path: String,
    note_id: String,
) -> Result<(), String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    let max_versions = config.max_versions_per_note;
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    crate::history::force_snapshot(vault_path, &note_id, &raw, max_versions);
    Ok(())
}

#[tauri::command]
pub fn get_note_version_content(
    state: State<'_, AppState>,
    note_id: String,
    timestamp: String,
) -> Result<String, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let vault_path = config.active_vault.as_ref().ok_or("No active vault")?;
    crate::history::get_version(vault_path, &note_id, &timestamp)
}

// ── AI ──

#[tauri::command]
pub fn set_ai_settings(
    state: State<'_, AppState>,
    provider: Option<String>,
    api_key: Option<String>,
    model: String,
    writing_style: Option<String>,
    base_url: Option<String>,
    ollama_api_key: Option<String>,
    openai_compatible_base_url: Option<String>,
    openai_compatible_api_key: Option<String>,
) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    let key = api_key.filter(|k| !k.is_empty());
    match provider.as_deref() {
        Some("openai") => config.openai_api_key = key,
        Some("ollama") => {
            config.ollama_base_url = base_url.filter(|u| !u.trim().is_empty());
            config.ollama_api_key = ollama_api_key.filter(|k| !k.is_empty());
        }
        Some("openai_compatible") => {
            config.openai_compatible_base_url = openai_compatible_base_url.filter(|u| !u.trim().is_empty());
            config.openai_compatible_api_key = openai_compatible_api_key.filter(|k| !k.is_empty());
        }
        _ => config.ai_api_key = key,
    }
    config.ai_provider = provider;
    config.ai_model = model;
    config.ai_writing_style = writing_style.filter(|s| !s.trim().is_empty());
    save_app_config(&config)?;
    Ok(())
}

#[tauri::command]
pub fn test_ai_connection(app: AppHandle) -> Result<(), String> {
    let (provider, api_key, model, base_url) = {
        let state = app.state::<AppState>();
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let provider = config
            .ai_provider
            .clone()
            .unwrap_or_else(|| "anthropic".to_string());
        let key = match provider.as_str() {
            "ollama" => Some(config.ollama_api_key.clone().unwrap_or_default()),
            "openai_compatible" => Some(config.openai_compatible_api_key.clone().unwrap_or_default()),
            "openai" => config.openai_api_key.clone(),
            _ => config.ai_api_key.clone(),
        }
        .ok_or("No API key configured")?;
        let model = config.ai_model.clone();
        let base_url = match provider.as_str() {
            "openai_compatible" => config.openai_compatible_base_url.clone(),
            _ => config.ollama_base_url.clone(),
        };
        (provider, key, model, base_url)
    };

    std::thread::spawn(move || {
        use tauri::Emitter;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(crate::ai::test_connection(
            &provider,
            &api_key,
            &model,
            base_url.as_deref(),
        ));
        match result {
            Ok(msg) => {
                let _ = app.emit(
                    "ai-test-result",
                    serde_json::json!({ "success": true, "message": msg }),
                );
            }
            Err(e) => {
                let _ = app.emit(
                    "ai-test-result",
                    serde_json::json!({ "success": false, "error": e }),
                );
            }
        }
    });
    Ok(())
}

// ── Sync (WebDAV) ──

fn sync_config_from(config: &AppConfig) -> Result<crate::sync::WebdavConfig, String> {
    if config.sync_provider.as_deref() != Some("webdav") {
        return Err("Sync is not configured".to_string());
    }
    let url = config
        .webdav_url
        .clone()
        .filter(|u| !u.trim().is_empty())
        .ok_or("WebDAV URL is not set")?;
    Ok(crate::sync::WebdavConfig {
        url,
        username: config.webdav_username.clone().unwrap_or_default(),
        password: config.webdav_password.clone().unwrap_or_default(),
    })
}

#[tauri::command]
pub fn set_sync_settings(
    state: State<'_, AppState>,
    provider: Option<String>,
    url: Option<String>,
    username: Option<String>,
    password: Option<String>,
    sync_on_open: bool,
    sync_on_change: bool,
    sync_interval_minutes: u32,
) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.sync_provider = provider.filter(|p| !p.is_empty());
    config.webdav_url = url.filter(|u| !u.trim().is_empty());
    config.webdav_username = username.filter(|u| !u.is_empty());
    config.webdav_password = password.filter(|p| !p.is_empty());
    config.sync_on_open = sync_on_open;
    config.sync_on_change = sync_on_change;
    config.sync_interval_minutes = sync_interval_minutes;
    save_app_config(&config)?;
    Ok(())
}

#[tauri::command]
pub fn test_sync_connection(app: AppHandle) -> Result<(), String> {
    let cfg = {
        let state = app.state::<AppState>();
        let config = state.config.lock().map_err(|e| e.to_string())?;
        sync_config_from(&config)?
    };
    std::thread::spawn(move || {
        use tauri::Emitter;
        match crate::sync::test_connection(cfg) {
            Ok(msg) => {
                let _ = app.emit(
                    "sync-test-result",
                    serde_json::json!({ "success": true, "message": msg }),
                );
            }
            Err(e) => {
                let _ = app.emit(
                    "sync-test-result",
                    serde_json::json!({ "success": false, "error": e }),
                );
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn sync_now(app: AppHandle) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    // Guard against overlapping syncs (manual button + interval + on-change can collide).
    if app.state::<AppState>().syncing.swap(true, Ordering::SeqCst) {
        return Ok(()); // a sync is already running
    }
    let (vault, cfg) = {
        let state = app.state::<AppState>();
        let config = match state.config.lock() {
            Ok(c) => c,
            Err(e) => {
                state.syncing.store(false, Ordering::SeqCst);
                return Err(e.to_string());
            }
        };
        let gathered = config
            .active_vault
            .clone()
            .ok_or_else(|| "No active vault".to_string())
            .and_then(|v| sync_config_from(&config).map(|c| (v, c)));
        match gathered {
            Ok(vc) => vc,
            Err(e) => {
                drop(config);
                state.syncing.store(false, Ordering::SeqCst);
                return Err(e);
            }
        }
    };
    std::thread::spawn(move || {
        use tauri::Emitter;
        let result = crate::sync::run_sync(app.clone(), vault, cfg);
        app.state::<AppState>().syncing.store(false, Ordering::SeqCst);
        match result {
            Ok(summary) => {
                let ts = chrono::Utc::now().to_rfc3339();
                if let Ok(mut config) = app.state::<AppState>().config.lock() {
                    config.last_sync_time = Some(ts.clone());
                    let _ = save_app_config(&config);
                }
                let _ = app.emit(
                    "sync-done",
                    serde_json::json!({ "success": true, "summary": summary, "last_sync_time": ts }),
                );
            }
            Err(e) => {
                let _ = app.emit("sync-error", serde_json::json!({ "success": false, "error": e }));
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn ai_ask(
    app: AppHandle,
    action: String,
    text: String,
    custom_prompt: Option<String>,
    request_id: String,
) -> Result<(), String> {
    let (provider, api_key, model, writing_style, base_url) = {
        let state = app.state::<AppState>();
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let provider = config
            .ai_provider
            .clone()
            .unwrap_or_else(|| "anthropic".to_string());
        let key = match provider.as_str() {
            "ollama" => Some(config.ollama_api_key.clone().unwrap_or_default()),
            "openai_compatible" => Some(config.openai_compatible_api_key.clone().unwrap_or_default()),
            "openai" => config.openai_api_key.clone(),
            _ => config.ai_api_key.clone(),
        }
        .ok_or("No API key configured. Go to Settings > AI to set up your API key.")?;
        let model = config.ai_model.clone();
        let style = config.ai_writing_style.clone();
        let base_url = match provider.as_str() {
            "openai_compatible" => config.openai_compatible_base_url.clone(),
            _ => config.ollama_base_url.clone(),
        };
        (provider, key, model, style, base_url)
    };

    let mut system_prompt = "You are a helpful writing assistant inside a note-taking app called HelixNotes. \
        You help users improve, rewrite, summarize, and transform their text. \
        Return ONLY the resulting text - no explanations, no markdown code fences, no preamble. \
        Preserve the original language of the text unless specifically asked to translate. \
        Preserve any markdown formatting (bold, italic, links, images, tables, etc.) unless the user asks to change it. \
        If the text contains placeholders like __MEDIA_0__, __MEDIA_1__, etc., keep them exactly as they are in their original positions - they represent images and embedded files.".to_string();

    if let Some(ref style) = writing_style {
        system_prompt.push_str(&format!(
            "\n\nThe user's preferred writing style: {}",
            style
        ));
    }

    let user_message = match action.as_str() {
        "improve" => format!("Improve the writing quality of this text while keeping the same meaning and tone:\n\n{}", text),
        "fix_grammar" => format!("Fix all grammar, spelling, and punctuation errors in this text. Keep the original style:\n\n{}", text),
        "shorter" => format!("Make this text more concise while keeping the key points:\n\n{}", text),
        "longer" => format!("Expand this text with more detail while keeping the same style and tone:\n\n{}", text),
        "professional" => format!("Rewrite this text in a professional, formal tone:\n\n{}", text),
        "friendly" => format!("Rewrite this text in a casual, friendly tone:\n\n{}", text),
        "summarize" => format!("Write a brief summary of this text:\n\n{}", text),
        "explain" => format!("Explain this text in simpler terms:\n\n{}", text),
        "translate_en" => format!("Translate this text to English:\n\n{}", text),
        "translate_nl" => format!("Translate this text to Dutch:\n\n{}", text),
        "translate_de" => format!("Translate this text to German:\n\n{}", text),
        "translate_fr" => format!("Translate this text to French:\n\n{}", text),
        "translate_es" => format!("Translate this text to Spanish:\n\n{}", text),
        "custom" => {
            let prompt = custom_prompt.unwrap_or_else(|| "Improve this text".to_string());
            format!("{}\n\n{}", prompt, text)
        }
        _ => format!("Improve this text:\n\n{}", text),
    };

    crate::ai::ai_request(
        app,
        provider,
        api_key,
        model,
        system_prompt,
        user_message,
        request_id,
        base_url,
    );
    Ok(())
}

// ── Helpers ──

// On mobile (Android + iOS) the OS config dir is injected at startup via the Tauri
// path resolver, since dirs::config_dir() is not reliable in the app sandbox.
static MOBILE_CONFIG_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

pub fn set_mobile_config_dir(path: std::path::PathBuf) {
    let _ = MOBILE_CONFIG_DIR.set(path);
}

fn app_config_path() -> Result<std::path::PathBuf, String> {
    // Prefer the injected mobile dir when present (set only on mobile); fall back to
    // the platform config dir on desktop.
    let app_dir = if let Some(mobile_dir) = MOBILE_CONFIG_DIR.get() {
        mobile_dir.join("helixnotes")
    } else if let Some(config_dir) = dirs::config_dir() {
        config_dir.join("helixnotes")
    } else {
        return Err("Config directory not available yet".to_string());
    };
    std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
    Ok(app_dir.join("config.json"))
}

pub fn load_app_config() -> AppConfig {
    app_config_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_app_config(config: &AppConfig) -> Result<(), String> {
    let path = app_config_path()?;
    let data = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(path, data).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Install Type Detection ──

#[tauri::command]
pub fn is_mobile_platform() -> bool {
    // Compile-time platform: true only for the Android/iOS builds. Authoritative, unlike the
    // webview user-agent, which some desktop WebKitGTK builds report mobile-looking (issue #63).
    cfg!(mobile)
}

#[tauri::command]
pub fn get_install_type() -> String {
    // Build-time override for distro packagers (e.g. Solus): build with
    // HELIXNOTES_INSTALL_TYPE=solus to report that type and suppress the in-app updater.
    if let Some(forced) = option_env!("HELIXNOTES_INSTALL_TYPE") {
        if !forced.is_empty() {
            return forced.to_string();
        }
    }
    if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if std::path::Path::new("/var/lib/dpkg/info/helix-notes.list").exists() {
        "deb".to_string()
    } else if std::path::Path::new("/var/lib/pacman/local").exists()
        && ["helixnotes", "helixnotes-bin", "helixnotes-appimage-bin"].iter().any(|pkg| {
            std::process::Command::new("pacman")
                .args(["-Q", pkg])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        })
    {
        "aur".to_string()
    } else if std::env::var("APPIMAGE").is_ok() {
        "appimage".to_string()
    } else {
        "native".to_string()
    }
}
