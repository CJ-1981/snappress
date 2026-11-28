use std::fmt::Write;

use tauri::{AppHandle, Manager};

use crate::capture::{self, DisplayMap};
use crate::commands::system;

/// Log lines included in a copied report.
const LOG_TAIL: usize = 60;

/// The capture environment: whether capture is available, every display with
/// the monitor that shows it, and any fallback in use.
fn describe_environment(app: &AppHandle) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "SnapPress {}", app.package_info().version);
    let _ = writeln!(
        out,
        "platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let _ = writeln!(
        out,
        "screen capture API available: {}",
        capture::capture_supported()
    );
    let _ = writeln!(
        out,
        "screen recording permission: {}",
        capture::has_screen_permission()
    );

    let map = DisplayMap::build(app, capture::display_targets());
    let _ = writeln!(
        out,
        "displays: {}, monitors: {}",
        map.displays().len(),
        map.monitors().len()
    );
    for (i, target) in map.displays().iter().enumerate() {
        let scap::Target::Display(display) = target else {
            continue;
        };
        match map.monitor_of(display.id) {
            Some(monitor) => {
                let name = monitor.name().map(String::as_str).unwrap_or("(unnamed)");
                let position = monitor.position();
                let size = monitor.size();
                let _ = writeln!(
                    out,
                    "  display {} (id {}): \"{}\" shown on \"{}\" at {},{}, {}x{}, scale {}",
                    i + 1,
                    display.id,
                    display.title,
                    name,
                    position.x,
                    position.y,
                    size.width,
                    size.height,
                    monitor.scale_factor()
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "  display {} (id {}): \"{}\" has no matching monitor",
                    i + 1,
                    display.id,
                    display.title
                );
            }
        }
    }
    if let Some(reason) = crate::state::gdi_fallback() {
        let _ = writeln!(
            out,
            "the last capture used the GDI fallback because: {reason}"
        );
    }
    let state = app.state::<crate::state::AppState>();
    let hotkeys = state.registered_hotkeys.lock().unwrap();
    if hotkeys.is_empty() {
        let _ = writeln!(out, "global shortcuts: none registered");
    } else {
        let shown: Vec<String> = hotkeys.iter().map(|(a, s)| format!("{a}={s}")).collect();
        let _ = writeln!(out, "global shortcuts: {}", shown.join(", "));
    }
    out
}

/// Write the environment to the log once at startup.
pub fn log_environment(app: &AppHandle) {
    for line in describe_environment(app).lines() {
        log::info!("{line}");
    }
}

/// Copy a diagnostics report to the clipboard and return it.
#[tauri::command]
pub fn copy_diagnostics(app: AppHandle) -> Result<String, String> {
    let mut report = describe_environment(&app);
    let _ = writeln!(report, "--- last {LOG_TAIL} log lines ---");
    match app.path().app_log_dir() {
        Ok(dir) => match std::fs::read_to_string(dir.join("snappress.log")) {
            Ok(log) => {
                let lines: Vec<&str> = log.lines().collect();
                for line in &lines[lines.len().saturating_sub(LOG_TAIL)..] {
                    let _ = writeln!(report, "{line}");
                }
            }
            Err(e) => {
                let _ = writeln!(report, "(no log yet: {e})");
            }
        },
        Err(e) => {
            let _ = writeln!(report, "(log folder unavailable: {e})");
        }
    }

    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("clipboard unavailable: {e}"))?;
    clipboard
        .set_text(report.clone())
        .map_err(|e| format!("failed to copy: {e}"))?;
    log::info!("diagnostics copied to the clipboard");
    Ok(report)
}

#[tauri::command]
pub fn open_log_folder(app: AppHandle) -> Result<(), String> {
    let dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    system::open_path(app, dir.to_string_lossy().to_string(), Some(false))
}
