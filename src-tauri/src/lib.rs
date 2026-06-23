mod ai;
mod backup;
mod commands;
mod history;
mod search;
mod state;
mod sync;
mod types;
mod vault;

use state::AppState;
#[allow(unused_imports)]
use tauri::{Emitter, Manager};

#[cfg(desktop)]
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Work around blank window from WebKitGTK DMABUF/GBM allocation failures on some Linux GPUs.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let config = commands::load_app_config();
    #[cfg(desktop)]
    let show_tray = config.show_tray_icon;
    #[cfg(desktop)]
    let close_to_tray = config.close_to_tray && show_tray;
    let app_state = AppState::new(config);

    // Inject the compile-time platform so the frontend never sniffs the (sometimes
    // mobile-looking) WebKitGTK user-agent. (#63)
    let platform_init = format!(
        "window.__HELIX_PLATFORM__={{mobile:{},android:{},ios:{}}};",
        cfg!(mobile),
        cfg!(target_os = "android"),
        cfg!(target_os = "ios"),
    );

    let mut builder = tauri::Builder::default()
        .plugin(
            tauri::plugin::Builder::<tauri::Wry>::new("helix-platform")
                .js_init_script(platform_init)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // On mobile, set config dir from Tauri's path resolver, then reload config
            #[cfg(mobile)]
            {
                if let Ok(config_dir) = app.path().config_dir() {
                    commands::set_mobile_config_dir(config_dir);
                } else if let Ok(data_dir) = app.path().data_dir() {
                    commands::set_mobile_config_dir(data_dir);
                }
                // Reload config now that the mobile config dir is available
                let reloaded = commands::load_app_config();
                let _ = app.state::<AppState>().config.lock().map(|mut cfg| {
                    *cfg = reloaded;
                });
            }

            #[cfg(desktop)]
            if show_tray {
                setup_tray(app)?;
            }

            // Check CLI args for a .md file path on initial launch
            #[cfg(desktop)]
            {
                let file_arg = std::env::args().skip(1).find(|a| {
                    let a = a.trim();
                    !a.starts_with('-') && a.ends_with(".md")
                });
                if let Some(path) = file_arg {
                    let resolved = if std::path::Path::new(&path).is_absolute() {
                        std::path::PathBuf::from(&path)
                    } else {
                        std::env::current_dir().unwrap_or_default().join(&path)
                    };
                    if let Some(resolved_str) = resolved.to_str() {
                        let app_state = app.state::<AppState>();
                        let _ = app_state.pending_open_file.lock().map(|mut p| {
                            *p = Some(resolved_str.to_string());
                        });
                    }
                }
            }

            Ok(())
        })
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::open_vault,
            commands::remove_vault,
            commands::get_app_config,
            commands::set_theme,
            commands::set_accent_color,
            commands::set_font_size,
            commands::set_font_family,
            commands::set_line_height,
            commands::set_ui_scale,
            commands::set_content_width,
            commands::get_notebooks,
            commands::count_root_notes,
            commands::create_notebook,
            commands::rename_notebook,
            commands::delete_notebook,
            commands::move_notebook,
            commands::get_notes,
            commands::read_note,
            commands::save_note,
            commands::create_note,
            commands::create_daily_note,
            commands::rename_note,
            commands::delete_note,
            commands::move_note,
            commands::get_all_tags,
            commands::get_all_note_titles,
            commands::get_graph_data,
            commands::get_tasks,
            commands::set_task_done,
            commands::set_task_priority,
            commands::set_task_due,
            commands::search_notes,
            commands::reindex,
            commands::get_trash,
            commands::restore_note,
            commands::restore_notebook,
            commands::permanent_delete,
            commands::empty_trash,
            commands::load_vault_state,
            commands::save_vault_state,
            commands::read_clipboard_image,
            commands::copy_image_to_clipboard,
            commands::save_image,
            commands::save_attachment,
            commands::get_notebook_icons,
            commands::set_notebook_icon,
            commands::set_general_settings,
            commands::get_quick_access,
            commands::add_quick_access,
            commands::remove_quick_access,
            commands::reorder_quick_access,
            commands::get_vault_stats,
            commands::find_orphaned_attachments,
            commands::trash_orphaned_attachments,
            commands::import_obsidian,
            commands::open_file,
            commands::open_url,
            commands::copy_file_to,
            commands::write_bytes_to,
            commands::copy_png_to_clipboard,
            commands::create_backup,
            commands::list_backups,
            commands::restore_backup,
            commands::delete_backup,
            commands::set_backup_settings,
            commands::get_note_versions,
            commands::get_note_version_content,
            commands::create_version,
            commands::set_ai_settings,
            commands::test_ai_connection,
            commands::ai_ask,
            commands::set_sync_settings,
            commands::test_sync_connection,
            commands::sync_now,
            commands::get_install_type,
            commands::is_mobile_platform,
            commands::get_pending_open_file,
        ])
        .register_asynchronous_uri_scheme_protocol("imgproxy", |_ctx, request, responder| {
            let path = request.uri().path().to_string();
            std::thread::spawn(move || {
                let encoded = path.strip_prefix('/').unwrap_or(&path);
                let external_url = percent_decode(encoded);

                if !external_url.starts_with("http://") && !external_url.starts_with("https://") {
                    let _ = responder.respond(
                        tauri::http::Response::builder()
                            .status(400)
                            .body(Vec::new())
                            .unwrap(),
                    );
                    return;
                }

                let client = match reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()
                {
                    Ok(c) => c,
                    Err(_) => {
                        let _ = responder.respond(
                            tauri::http::Response::builder()
                                .status(502)
                                .body(Vec::new())
                                .unwrap(),
                        );
                        return;
                    }
                };

                match client.get(&external_url).send() {
                    Ok(resp) => {
                        let content_type = resp
                            .headers()
                            .get("content-type")
                            .and_then(|h| h.to_str().ok())
                            .unwrap_or("application/octet-stream")
                            .to_string();
                        let status = resp.status().as_u16();
                        match resp.bytes() {
                            Ok(bytes) => {
                                let _ = responder.respond(
                                    tauri::http::Response::builder()
                                        .status(status)
                                        .header("Content-Type", &content_type)
                                        .header("Access-Control-Allow-Origin", "*")
                                        .body(bytes.to_vec())
                                        .unwrap(),
                                );
                            }
                            Err(_) => {
                                let _ = responder.respond(
                                    tauri::http::Response::builder()
                                        .status(502)
                                        .body(Vec::new())
                                        .unwrap(),
                                );
                            }
                        }
                    }
                    Err(_) => {
                        let _ = responder.respond(
                            tauri::http::Response::builder()
                                .status(502)
                                .body(Vec::new())
                                .unwrap(),
                        );
                    }
                }
            });
        });

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            // Always show/focus the main window
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }

            // Extract .md file path from args (args[0] is the binary)
            let file_path = args.iter().skip(1).find(|arg| {
                let a = arg.trim();
                !a.starts_with('-') && a.ends_with(".md")
            });

            if let Some(path) = file_path {
                let resolved = if std::path::Path::new(path.as_str()).is_absolute() {
                    std::path::PathBuf::from(path)
                } else {
                    std::path::Path::new(&cwd).join(path)
                };
                if let Some(resolved_str) = resolved.to_str() {
                    let _ = app.emit("open-file", resolved_str.to_string());
                }
            }
        }));

        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
        builder = builder.plugin(tauri_plugin_window_state::Builder::default().build());

        builder = builder.on_window_event(move |window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    // Only hide to tray for the main window
                    if close_to_tray && window.label() == "main" {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
                tauri::WindowEvent::Destroyed => {
                    // When main window is destroyed, close all note windows
                    if window.label() == "main" {
                        let app = window.app_handle();
                        for (label, win) in app.webview_windows() {
                            if label.starts_with("note-") {
                                let _ = win.close();
                            }
                        }
                    }
                }
                _ => {}
            }
        });
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(desktop)]
fn setup_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItemBuilder::with_id("show", "Show HelixNotes").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;

    let _tray = TrayIconBuilder::new()
        .icon(
            Image::from_path("icons/32x32.png")
                .unwrap_or_else(|_| app.default_window_icon().cloned().unwrap()),
        )
        .menu(&menu)
        .tooltip("HelixNotes")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click { .. } = event {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

fn percent_decode(input: &str) -> String {
    let mut output = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(decoded) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                output.push(decoded);
                i += 3;
                continue;
            }
        }
        output.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&output).to_string()
}
