use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::types::VersionEntry;

fn safe_path_component<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    let mut components = Path::new(value).components();
    if value.is_empty()
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(format!("Invalid {label}"));
    }
    Ok(value)
}

/// Directory: .helixnotes/history/<note-id>/
fn history_dir(vault_path: &str, note_id: &str) -> Result<PathBuf, String> {
    Ok(Path::new(vault_path)
        .join(".helixnotes")
        .join("history")
        .join(safe_path_component(note_id, "note ID")?))
}

/// Save a version snapshot if enough time has passed since the last one.
/// Minimum interval: 5 minutes between snapshots.
pub fn maybe_snapshot(vault_path: &str, note_id: &str, raw_content: &str, max_versions: u32) {
    let Ok(dir) = history_dir(vault_path, note_id) else {
        log::warn!("Skipping history snapshot with an invalid note ID");
        return;
    };

    // Check if we should create a snapshot (5 min cooldown)
    if let Ok(entries) = fs::read_dir(&dir) {
        let mut files: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
            .filter(|n| n.ends_with(".md"))
            .collect();
        files.sort();

        if let Some(last) = files.last() {
            if let Some(ts) = last.strip_suffix(".md") {
                // Parse timestamp: 2026-02-08T18-30-00.md
                if let Some(t_pos) = ts.find('T') {
                    let date_part = &ts[..t_pos];
                    let time_part = &ts[t_pos + 1..];
                    let time_colons = time_part.replace('-', ":");
                    let iso = format!("{}T{}Z", date_part, time_colons);
                    if let Ok(last_time) = iso.parse::<DateTime<Utc>>() {
                        let elapsed = Utc::now() - last_time;
                        if elapsed.num_minutes() < 5 {
                            return; // Too soon, skip
                        }
                    }
                }
            }
        }
    }

    // Create snapshot
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("Failed to create history dir: {}", e);
        return;
    }

    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%S").to_string();
    let filename = format!("{}.md", timestamp);
    let path = dir.join(&filename);

    if let Err(e) = fs::write(&path, raw_content) {
        eprintln!("Failed to write version snapshot: {}", e);
        return;
    }

    // Prune old versions
    if max_versions > 0 {
        let _ = prune_versions(&dir, max_versions);
    }
}

/// Force-create a version snapshot, bypassing the cooldown.
pub fn force_snapshot(
    vault_path: &str,
    note_id: &str,
    raw_content: &str,
    max_versions: u32,
) -> Result<(), String> {
    let dir = history_dir(vault_path, note_id)?;
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;

    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%S").to_string();
    let filename = format!("{}.md", timestamp);
    let path = dir.join(&filename);

    fs::write(&path, raw_content).map_err(|error| error.to_string())?;

    if max_versions > 0 {
        prune_versions(&dir, max_versions)?;
    }
    Ok(())
}

/// List all version snapshots for a note, newest first.
pub fn list_versions(vault_path: &str, note_id: &str) -> Result<Vec<VersionEntry>, String> {
    let dir = history_dir(vault_path, note_id)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<VersionEntry> = Vec::new();

    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "md") {
            let filename = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let meta = fs::metadata(&path).map_err(|e| e.to_string())?;

            // Parse timestamp from filename
            let timestamp = if let Some(t_pos) = filename.find('T') {
                let date_part = &filename[..t_pos];
                let time_part = &filename[t_pos + 1..];
                let time_colons = time_part.replace('-', ":");
                format!("{}T{}Z", date_part, time_colons)
            } else {
                filename.clone()
            };

            entries.push(VersionEntry {
                timestamp,
                size: meta.len(),
            });
        }
    }

    // Newest first
    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(entries)
}

/// Get the raw content of a specific version.
pub fn get_version(vault_path: &str, note_id: &str, timestamp: &str) -> Result<String, String> {
    safe_path_component(timestamp, "version timestamp")?;
    // Convert ISO timestamp back to filename: 2026-02-08T18:30:00Z → 2026-02-08T18-30-00.md
    let filename = if let Some(t_pos) = timestamp.find('T') {
        let date_part = &timestamp[..t_pos];
        let time_part = timestamp[t_pos + 1..].trim_end_matches('Z');
        let time_dashes = time_part.replace(':', "-");
        format!("{}.md", format!("{}T{}", date_part, time_dashes))
    } else {
        format!("{}.md", timestamp)
    };

    let path = history_dir(vault_path, note_id)?.join(&filename);
    fs::read_to_string(&path).map_err(|e| format!("Version not found: {}", e))
}

/// Remove old versions keeping only the newest `max` entries.
fn prune_versions(dir: &Path, max: u32) -> Result<(), String> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |ext| ext == "md"))
        .collect();

    // Sort by name (timestamps sort lexicographically) - newest last
    files.sort();

    if files.len() as u32 > max {
        let to_remove = files.len() as u32 - max;
        for path in files.iter().take(to_remove as usize) {
            let _ = fs::remove_file(path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{get_version, list_versions};
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn rejects_history_path_traversal() {
        let vault =
            std::env::temp_dir().join(format!("helixnotes-history-test-{}", Uuid::new_v4()));
        let metadata = vault.join(".helixnotes");
        let escaped_history = metadata.join("escaped");
        fs::create_dir_all(&escaped_history).unwrap();
        fs::write(escaped_history.join("2026-01-01T00-00-00.md"), "escaped").unwrap();
        fs::create_dir_all(metadata.join("history").join("safe")).unwrap();
        fs::write(metadata.join("secret.md"), "secret").unwrap();

        assert!(list_versions(&vault.to_string_lossy(), "../escaped").is_err());
        assert!(get_version(&vault.to_string_lossy(), "safe", "../../secret").is_err());

        fs::remove_dir_all(vault).unwrap();
    }
}
