use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::capture::{self, DisplayMap};
use crate::state::AppState;

/// How long a capture may keep the main window hidden before it is brought
/// back whatever happened.
const CAPTURE_WATCHDOG: Duration = Duration::from_secs(30);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingOverlayDto {
    pub mode: String,
    pub display_id: Option<u32>,
}

#[tauri::command]
pub async fn start_region_capture(
    app: AppHandle,
    mode: String,
    display_id: Option<u32>,
) -> Result<(), String> {
    let mode = match mode.as_str() {
        "image" | "scroll" | "record" => mode,
        _ => "image".to_string(),
    };
    // Bringing the overlay up waits on the main thread, so it cannot run there.
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = open_overlay(&app, &mode, display_id) {
            fail_capture(&app, &e);
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn overlay_ready(app: AppHandle) -> PendingOverlayDto {
    let st = app.state::<AppState>();
    let pending = st.pending_overlay.lock().unwrap().clone();
    pending
        .map(|p| PendingOverlayDto {
            mode: p.mode,
            display_id: p.display_id,
        })
        .unwrap_or(PendingOverlayDto {
            mode: "image".to_string(),
            display_id: None,
        })
}

#[tauri::command]
pub fn cancel_capture(app: AppHandle) -> Result<(), String> {
    close_overlay(&app);
    show_main(&app);
    Ok(())
}

/// Open the overlay from the main thread (global shortcut, tray menu).
pub fn spawn_overlay(app: &AppHandle, mode: &'static str) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = open_overlay(&app, mode, None) {
            fail_capture(&app, &e);
        }
    });
}

/// Report a failed capture. The main window may have been hidden for it, so
/// bring it back and say what went wrong: an error raised after the overlay
/// closed would otherwise reach a webview that no longer exists.
pub fn fail_capture(app: &AppHandle, message: &str) {
    log::error!("capture failed: {message}");
    show_main(app);
    app.emit("capture:error", json!({ "message": message }))
        .ok();
}

/// Run a grab with the main window hidden for it. A failure is reported to
/// the user and the window is brought back however the grab ends.
pub fn run_capture<T>(
    app: &AppHandle,
    grab: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _flight = CaptureFlight::begin(app);
    let result = grab();
    if let Err(e) = &result {
        fail_capture(app, e);
    }
    result
}

/// A capture that hides the main window while it runs.
struct CaptureFlight<'a> {
    app: &'a AppHandle,
}

impl<'a> CaptureFlight<'a> {
    fn begin(app: &'a AppHandle) -> Self {
        let st = app.state::<AppState>();
        st.capture_in_flight.store(true, Ordering::SeqCst);
        let generation = st.capture_generation.fetch_add(1, Ordering::SeqCst) + 1;

        // Backstop: nothing inside a capture may keep the main window hidden
        // for good.
        let watchdog = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(CAPTURE_WATCHDOG);
            let st = watchdog.state::<AppState>();
            let current = st.capture_generation.load(Ordering::SeqCst) == generation;
            if current && st.capture_in_flight.load(Ordering::SeqCst) {
                fail_capture(
                    &watchdog,
                    "The capture did not finish in time and was abandoned.",
                );
            }
        });
        Self { app }
    }
}

impl Drop for CaptureFlight<'_> {
    fn drop(&mut self) {
        self.app
            .state::<AppState>()
            .capture_in_flight
            .store(false, Ordering::SeqCst);
        show_main(self.app);
    }
}

/// Hide the main window and cover the target monitor with the capture overlay.
///
/// The overlay is created on the main thread and this waits for that to
/// finish, so it must not be called from the main thread itself.
pub fn open_overlay(
    app: &AppHandle,
    mode: &str,
    display_id: Option<u32>,
) -> Result<(), String> {
    {
        let st = app.state::<AppState>();
        *st.pending_overlay.lock().unwrap() = Some(crate::state::PendingOverlay {
            mode: mode.to_string(),
            display_id,
        });
        st.overlay_opening.store(true, Ordering::SeqCst);
    }
    let _opening = OverlayOpening(app);
    hide_main(app);
    close_overlay(app);
    std::thread::sleep(Duration::from_millis(150));

    let app2 = app.clone();
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(build_overlay_window(&app2, display_id));
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(Duration::from_secs(10))
        .map_err(|_| "timed out waiting for the overlay window".to_string())?
}

struct OverlayOpening<'a>(&'a AppHandle);

impl Drop for OverlayOpening<'_> {
    fn drop(&mut self) {
        self.0
            .state::<AppState>()
            .overlay_opening
            .store(false, Ordering::SeqCst);
    }
}

fn build_overlay_window(app: &AppHandle, display_id: Option<u32>) -> Result<(), String> {
    let monitor = pick_tauri_monitor(app, display_id)?;
    let (position, size) = (monitor.position(), monitor.size());
    log::info!(
        "overlay on monitor {:?} at {},{}, {}x{}, scale {}",
        monitor.name(),
        position.x,
        position.y,
        size.width,
        size.height,
        monitor.scale_factor()
    );
    let win = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay.html".into()))
        .title("SnapPress Capture Overlay")
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .build()
        .map_err(|e| {
            log::error!("failed to create the overlay window: {e}");
            e.to_string()
        })?;
    let _ = win.set_size(tauri::PhysicalSize::new(
        monitor.size().width,
        monitor.size().height,
    ));
    let _ = win.set_position(tauri::PhysicalPosition::new(
        monitor.position().x,
        monitor.position().y,
    ));
    let _ = win.show();
    let _ = win.set_focus();
    Ok(())
}

pub fn close_overlay(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.close();
    }
}

pub fn hide_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// The monitor showing scap display `display_id`, or the primary one.
pub fn pick_tauri_monitor(
    app: &AppHandle,
    display_id: Option<u32>,
) -> Result<tauri::Monitor, String> {
    let map = DisplayMap::build(app, capture::display_targets());
    if let Some(id) = display_id {
        if let Some(monitor) = map.monitor_of(id) {
            return Ok(monitor.clone());
        }
    }
    if let Ok(Some(primary)) = app.primary_monitor() {
        return Ok(primary);
    }
    app.available_monitors()
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or_else(|| "No monitor available".to_string())
}
