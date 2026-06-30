use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;
use crate::types::FileEvent;
use crate::vault::operations::helixnotes_dir;

pub fn start_watcher(app: AppHandle, vault_path: String) -> Result<RecommendedWatcher, String> {
    let (tx, rx) = mpsc::channel();

    let mut watcher = RecommendedWatcher::new(
        tx,
        Config::default().with_poll_interval(Duration::from_secs(1)),
    )
    .map_err(|e| e.to_string())?;

    watcher
        .watch(Path::new(&vault_path), RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    let hn_dir = helixnotes_dir(&vault_path);

    std::thread::spawn(move || {
        while let Ok(result) = rx.recv() {
            match result {
                Ok(event) => {
                    // Skip all file events while importing
                    let state = app.state::<AppState>();
                    if state.importing.load(std::sync::atomic::Ordering::Relaxed) {
                        continue;
                    }
                    // Skip .helixnotes directory events
                    let dominated_by_hn = event.paths.iter().all(|p| p.starts_with(&hn_dir));
                    if dominated_by_hn {
                        continue;
                    }

                    // Only care about .md file events
                    let has_md = event.paths.iter().any(|p| {
                        p.extension().and_then(|x| x.to_str()) == Some("md") || p.is_dir()
                    });

                    if !has_md {
                        continue;
                    }

                    let event_type = match event.kind {
                        EventKind::Create(_) => "create",
                        EventKind::Modify(_) => "modify",
                        EventKind::Remove(_) => "remove",
                        _ => continue,
                    };

                    for path in &event.paths {
                        let fe = FileEvent {
                            event_type: event_type.to_string(),
                            path: path.to_string_lossy().to_string(),
                        };
                        let _ = app.emit("file-changed", &fe);
                    }

                    // On mobile, throttle event emission to prevent IPC flooding
                    // on FUSE filesystems where Syncthing/other apps generate
                    // constant file activity that blocks the Tauri command channel.
                    #[cfg(mobile)]
                    {
                        std::thread::sleep(Duration::from_secs(2));
                        while rx.try_recv().is_ok() {}
                    }
                }
                Err(e) => {
                    log::error!("File watcher error: {}", e);
                }
            }
        }
    });

    Ok(watcher)
}
