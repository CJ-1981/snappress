mod capture;
mod commands;
mod encode;
mod input;
mod library;
mod settings;
mod state;

use state::AppState;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

fn hotkeys() -> [(&'static str, &'static str); 4] {
    #[cfg(target_os = "macos")]
    {
        [
            ("Cmd+Shift+6", "image"),
            ("Cmd+Alt+S", "scroll"),
            ("Cmd+Alt+V", "record"),
            ("Cmd+Alt+X", "stop"),
        ]
    }
    #[cfg(not(target_os = "macos"))]
    {
        [
            ("PrintScreen", "image"),
            ("CmdOrCtrl+Alt+S", "scroll"),
            ("CmdOrCtrl+Alt+V", "record"),
            ("CmdOrCtrl+Alt+X", "stop"),
        ]
    }
}

fn setup_shortcuts(app: &tauri::App) {
    let gs = app.global_shortcut();
    for (key, action) in hotkeys() {
        let action = action;
        let result = gs.on_shortcut(key, move |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                match action {
                    "image" => {
                        let _ = commands::overlay::open_overlay(app, "image", None);
                    }
                    "scroll" => {
                        let _ = commands::overlay::open_overlay(app, "scroll", None);
                    }
                    "record" => {
                        let _ = commands::overlay::open_overlay(app, "record", None);
                    }
                    _ => {
                        let _ = commands::record::stop_recording_impl(app);
                    }
                }
            }
        });
        if let Err(e) = result {
            eprintln!("failed to register global shortcut '{key}': {e}");
        }
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open SnapPress", true, None::<&str>)?;
    let capture = MenuItem::with_id(app, "capture", "Capture region", true, None::<&str>)?;
    let scroll = MenuItem::with_id(app, "scroll", "Scrolling capture", true, None::<&str>)?;
    let record = MenuItem::with_id(app, "record", "Record screen", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit SnapPress", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &capture, &scroll, &record, &quit])?;

    let mut tray = TrayIconBuilder::with_id("snappress-tray")
        .tooltip("SnapPress")
        .menu(&menu)
        .show_menu_on_left_click(true);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.on_menu_event(|app, event| match event.id().as_ref() {
        "open" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        "capture" => {
            let _ = commands::overlay::open_overlay(app, "image", None);
        }
        "scroll" => {
            let _ = commands::overlay::open_overlay(app, "scroll", None);
        }
        "record" => {
            let _ = commands::overlay::open_overlay(app, "record", None);
        }
        "quit" => app.exit(0),
        _ => {}
    })
    .build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // Keep the app alive in the tray when the main window is closed.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            let _ = std::fs::create_dir_all(data_dir.join("captures"));
            // ffmpeg-sidecar downloads its binary relative to the current
            // directory, so point the process at a writable location.
            let _ = std::env::set_current_dir(&data_dir);
            app.manage(AppState::new(data_dir));

            // Non-blocking ffmpeg availability probe; emits ffmpeg:status.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let ready = encode::check_ffmpeg();
                let _ = handle.emit("ffmpeg:status", serde_json::json!({ "ready": ready }));
            });

            setup_tray(app)?;
            setup_shortcuts(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::overlay::start_region_capture,
            commands::overlay::overlay_ready,
            commands::overlay::cancel_capture,
            commands::overlay::capture_full_screen,
            commands::capture::capture_region,
            commands::capture::start_scroll_capture,
            commands::capture::cancel_scroll_capture,
            commands::record::start_recording,
            commands::record::stop_recording,
            commands::record::pause_recording,
            commands::system::list_displays,
            commands::system::get_settings,
            commands::system::set_settings,
            commands::system::list_library,
            commands::system::delete_item,
            commands::system::open_path,
            commands::system::get_library_dir,
            commands::system::copy_image_to_clipboard,
            commands::system::export_gif,
            commands::system::save_edited,
            commands::system::read_image_base64,
        ])
        .run(tauri::generate_context!())
        .expect("error while running snappress");
}
