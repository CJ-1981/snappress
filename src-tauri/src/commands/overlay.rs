use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::capture;
use crate::library::{self, CaptureItem};
use crate::state::{AppState, PendingOverlay};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingOverlayDto {
    pub mode: String,
    pub display_id: Option<u32>,
}

#[tauri::command]
pub fn start_region_capture(
    app: AppHandle,
    mode: String,
    display_id: Option<u32>,
) -> Result<(), String> {
    let mode = match mode.as_str() {
        "image" | "scroll" | "record" => mode,
        _ => "image".to_string(),
    };
    open_overlay(&app, &mode, display_id)
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

#[tauri::command]
pub async fn capture_full_screen(
    app: AppHandle,
    display_id: Option<u32>,
) -> Result<CaptureItem, String> {
    tauri::async_runtime::spawn_blocking(move || {
        hide_main(&app);
        std::thread::sleep(Duration::from_millis(300));
        let sid = capture::resolve_scap_display_id(&app, display_id)?;
        let img = capture::grab_display(sid)?;
        let item = library::save_image_to_library(&app, &img, "capture")?;
        show_main(&app);
        Ok(item)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Open the borderless transparent overlay window covering the target monitor.
/// Window creation must happen on the main thread, so the builder is dispatched
/// there and its result is collected through a shared slot.
pub fn open_overlay(
    app: &AppHandle,
    mode: &str,
    display_id: Option<u32>,
) -> Result<(), String> {
    {
        let st = app.state::<AppState>();
        *st.pending_overlay.lock().unwrap() = Some(PendingOverlay {
            mode: mode.to_string(),
            display_id,
        });
    }
    hide_main(app);
    close_overlay(app);
    std::thread::sleep(Duration::from_millis(150));

    let app2 = app.clone();
    let slot: Arc<Mutex<Option<Result<(), String>>>> = Arc::new(Mutex::new(None));
    let slot2 = slot.clone();
    app.run_on_main_thread(move || {
        let result = build_overlay_window(&app2, display_id);
        *slot2.lock().unwrap() = Some(result);
    })
    .map_err(|e| e.to_string())?;
    slot.lock()
        .unwrap()
        .take()
        .unwrap_or_else(|| Err("overlay window build was skipped".to_string()))?;
    Ok(())
}

fn build_overlay_window(app: &AppHandle, display_id: Option<u32>) -> Result<(), String> {
    let monitor = pick_tauri_monitor(app, display_id)?;
    let win = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay.html".into()))
        .title("SnapPress Capture Overlay")
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())?;
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

/// Map a scap display id onto a tauri Monitor by matching physical sizes,
/// falling back to the primary monitor.
pub fn pick_tauri_monitor(
    app: &AppHandle,
    display_id: Option<u32>,
) -> Result<tauri::Monitor, String> {
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    if let Some(id) = display_id {
        let scap_monitors = scap::monitor::get_all();
        if let Some(sm) = scap_monitors.iter().find(|m| m.id == id) {
            if let Some(t) = monitors
                .iter()
                .find(|m| m.size().width as u32 == sm.width && m.size().height as u32 == sm.height)
            {
                return Ok(t.clone());
            }
        }
    }
    if let Ok(Some(primary)) = app.primary_monitor() {
        return Ok(primary);
    }
    monitors
        .into_iter()
        .next()
        .ok_or_else(|| "No monitor available".to_string())
}
