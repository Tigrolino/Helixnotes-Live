use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::ZipArchive;

use crate::types::BackupEntry;

/// Returns the default backup directory (~/.config/helixnotes/backups/)
pub fn default_backup_dir() -> Result<PathBuf, String> {
    let config_dir = dirs::config_dir().ok_or("Cannot find config directory")?;
    let dir = config_dir.join("helixnotes").join("backups");
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

/// Returns the effective backup directory (custom or default)
pub fn get_backup_dir(custom: &Option<String>) -> Result<PathBuf, String> {
    match custom {
        Some(p) if !p.is_empty() => {
            let dir = PathBuf::from(p);
            if !dir.exists() {
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            }
            Ok(dir)
        }
        _ => default_backup_dir(),
    }
}

/// Create a backup zip of the vault
pub fn create_backup(
    vault_path: &str,
    backup_dir: &Path,
    include_attachments: bool,
) -> Result<BackupEntry, String> {
    let vault = Path::new(vault_path);
    if !vault.is_dir() {
        return Err("Vault directory does not exist".to_string());
    }

    if !backup_dir.exists() {
        fs::create_dir_all(backup_dir).map_err(|e| e.to_string())?;
    }

    let now = Utc::now();
    let timestamp = now.format("%Y-%m-%dT%H-%M-%S").to_string();
    let filename = format!("helixnotes-backup-{}.zip", timestamp);
    let backup_path = backup_dir.join(&filename);

    let file = fs::File::create(&backup_path)
        .map_err(|e| format!("Failed to create backup file: {}", e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6));

    let helixnotes_dir = vault.join(".helixnotes");
    let attachments_dir = helixnotes_dir.join("attachments");

    for entry in WalkDir::new(vault).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        // Handle .helixnotes/ directory
        if path.starts_with(&helixnotes_dir) {
            if include_attachments {
                // Include attachments/ but skip everything else
                if !path.starts_with(&attachments_dir) && path != helixnotes_dir {
                    continue;
                }
                if path == helixnotes_dir {
                    continue;
                }
            } else {
                // Skip .helixnotes entirely
                continue;
            }
        }

        // Skip hidden files/directories (except .helixnotes which we handle above)
        if path != vault {
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') && name_str != ".helixnotes" {
                    continue;
                }
            }
        }

        let relative = path.strip_prefix(vault).map_err(|e| e.to_string())?;

        if path.is_dir() {
            let dir_name = format!("{}/", relative.to_string_lossy());
            if dir_name != "/" {
                zip.add_directory(&dir_name, options)
                    .map_err(|e| e.to_string())?;
            }
        } else {
            let name = relative.to_string_lossy().to_string();
            zip.start_file(&name, options).map_err(|e| e.to_string())?;
            let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
            let mut buffer = Vec::new();
            f.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
            zip.write_all(&buffer).map_err(|e| e.to_string())?;
        }
    }

    zip.finish().map_err(|e| e.to_string())?;

    let meta = fs::metadata(&backup_path).map_err(|e| e.to_string())?;

    Ok(BackupEntry {
        filename: filename.clone(),
        path: backup_path.to_string_lossy().to_string(),
        size: meta.len(),
        created: now.to_rfc3339(),
    })
}

/// List all backups in the backup directory
pub fn list_backups(backup_dir: &Path) -> Result<Vec<BackupEntry>, String> {
    if !backup_dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<BackupEntry> = Vec::new();

    for entry in fs::read_dir(backup_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();

        if path.extension().map_or(false, |ext| ext == "zip") {
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let meta = fs::metadata(&path).map_err(|e| e.to_string())?;

            // Parse timestamp from filename: helixnotes-backup-YYYY-MM-DDTHH-MM-SS.zip
            let created = if let Some(ts) = filename
                .strip_prefix("helixnotes-backup-")
                .and_then(|s| s.strip_suffix(".zip"))
            {
                // ts = "2026-02-08T18-09-15" → "2026-02-08T18:09:15Z"
                if let Some(t_pos) = ts.find('T') {
                    let date_part = &ts[..t_pos];
                    let time_part = &ts[t_pos + 1..];
                    let time_colons = time_part.replace('-', ":");
                    format!("{}T{}Z", date_part, time_colons)
                } else {
                    meta.modified()
                        .map(|t| {
                            let dt: chrono::DateTime<Utc> = t.into();
                            dt.to_rfc3339()
                        })
                        .unwrap_or_default()
                }
            } else {
                meta.modified()
                    .map(|t| {
                        let dt: chrono::DateTime<Utc> = t.into();
                        dt.to_rfc3339()
                    })
                    .unwrap_or_default()
            };

            entries.push(BackupEntry {
                filename,
                path: path.to_string_lossy().to_string(),
                size: meta.len(),
                created,
            });
        }
    }

    // Sort newest first
    entries.sort_by(|a, b| b.created.cmp(&a.created));
    Ok(entries)
}

fn validated_backup_file(backup_dir: &Path, backup_path: &str) -> Result<PathBuf, String> {
    let backup_dir = fs::canonicalize(backup_dir).map_err(|error| error.to_string())?;
    let backup = fs::canonicalize(backup_path).map_err(|error| error.to_string())?;
    if backup.parent() != Some(backup_dir.as_path())
        || backup.extension().and_then(|extension| extension.to_str()) != Some("zip")
        || !backup.is_file()
    {
        return Err(
            "Backup path must point to a ZIP file in the configured backup directory".to_string(),
        );
    }
    Ok(backup)
}

/// Restore a backup by extracting the zip over the vault directory
pub fn restore_backup(
    vault_path: &str,
    backup_dir: &Path,
    backup_path: &str,
) -> Result<(), String> {
    let vault = Path::new(vault_path);
    let backup = validated_backup_file(backup_dir, backup_path)?;

    let file = fs::File::open(backup).map_err(|e| format!("Failed to open backup: {}", e))?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| format!("Invalid backup archive: {}", e))?;

    // Clear existing notes (but keep .helixnotes/ internals except attachments)
    for entry in fs::read_dir(vault).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Keep hidden dirs (.helixnotes) - we'll handle attachments separately
        if name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }

    // Clear existing attachments so backup's attachments replace them
    let attachments_dir = vault.join(".helixnotes").join("attachments");
    if attachments_dir.exists() {
        fs::remove_dir_all(&attachments_dir).map_err(|e| e.to_string())?;
    }

    // Extract archive
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = vault.join(file.mangled_name());

        if file.is_dir() {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// Delete a single backup file
pub fn delete_backup(backup_dir: &Path, backup_path: &str) -> Result<(), String> {
    let path = validated_backup_file(backup_dir, backup_path)?;
    fs::remove_file(path).map_err(|error| error.to_string())
}

/// Remove old backups keeping only the newest `max_count`
pub fn cleanup_old_backups(backup_dir: &Path, max_count: u32) -> Result<(), String> {
    let mut backups = list_backups(backup_dir)?;

    // backups are already sorted newest-first
    if backups.len() as u32 > max_count {
        let to_remove = backups.split_off(max_count as usize);
        for entry in to_remove {
            delete_backup(backup_dir, &entry.path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::delete_backup;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn delete_backup_rejects_files_outside_backup_directory() {
        let root = std::env::temp_dir().join(format!("helixnotes-backup-test-{}", Uuid::new_v4()));
        let backup_dir = root.join("backups");
        let outside = root.join("outside.zip");
        fs::create_dir_all(&backup_dir).unwrap();
        fs::write(&outside, "must survive").unwrap();

        let result = delete_backup(&backup_dir, &outside.to_string_lossy());

        assert!(result.is_err());
        assert!(outside.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
