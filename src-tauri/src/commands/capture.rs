use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::capture;
use crate::capture::stitch::{self, ScrollConfig, ScrollRegion};
use crate::commands::overlay;
use crate::library::{self, CaptureItem};
use crate::state::{AppState, ScrollHandle};

#[tauri::command]
pub async fn capture_region(
    app: AppHandle,
    display_id: Option<u32>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<CaptureItem, String> {
    tauri::async_runtime::spawn_blocking(move || {
        overlay::run_capture(&app, || {
            // The overlay has to be gone before the grab so it is not in the shot.
            overlay::close_overlay(&app);
            std::thread::sleep(Duration::from_millis(300));
            let sid = capture::resolve_scap_display_id(&app, display_id)?;
            let full = capture::grab_display(sid)?;
            let img = capture::crop_rgba(&full, x, y, width, height)?;
            library::save_image_to_library(&app, &img, "capture")
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn capture_full_screen(
    app: AppHandle,
    display_id: Option<u32>,
) -> Result<CaptureItem, String> {
    tauri::async_runtime::spawn_blocking(move || {
        overlay::run_capture(&app, || {
            overlay::hide_main(&app);
            std::thread::sleep(Duration::from_millis(300));
            let sid = capture::resolve_scap_display_id(&app, display_id)?;
            let img = capture::grab_display(sid)?;
            library::save_image_to_library(&app, &img, "capture")
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn start_scroll_capture(
    app: AppHandle,
    display_id: Option<u32>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    // Resolving the display talks to the main thread, so this cannot run there.
    tauri::async_runtime::spawn_blocking(move || {
        let started = start_scroll_session(&app, display_id, x, y, width, height);
        if let Err(e) = &started {
            overlay::fail_capture(&app, e);
        }
        started
    })
    .await
    .map_err(|e| e.to_string())?
}

fn start_scroll_session(
    app: &AppHandle,
    display_id: Option<u32>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    overlay::close_overlay(app);
    overlay::show_main(app);

    let st = app.state::<AppState>();
    if st.scrolling.lock().unwrap().is_some() {
        return Err("A scrolling capture is already running".into());
    }
    let settings = st.settings.lock().unwrap().clone();
    let monitor = overlay::pick_tauri_monitor(app, display_id)?;
    let sid = capture::resolve_scap_display_id(app, display_id)?;
    let cancel = Arc::new(AtomicBool::new(false));
    let preview_path = library::captures_dir(app).join(".scroll-preview.png");
    {
        let st = app.state::<AppState>();
        *st.scrolling.lock().unwrap() = Some(ScrollHandle {
            cancel: cancel.clone(),
        });
    }
    app.emit(
        "scroll:started",
        json!({ "previewPath": preview_path.to_string_lossy() }),
    )
    .ok();

    let app2 = app.clone();
    let cfg = ScrollConfig {
        delay_ms: settings.scroll_delay_ms,
        step_ticks: settings.scroll_step_ticks,
        max_height: settings.max_scroll_height,
        max_frames: 400,
    };
    let region = ScrollRegion {
        cursor: (monitor.position().x + x, monitor.position().y + y),
        x: x.max(0),
        y: y.max(0),
        width: width.max(4),
        height: height.max(4),
    };
    std::thread::spawn(move || {
        let res = stitch::run_scroll_session(&app2, sid, region, cfg, cancel, preview_path);
        {
            let st = app2.state::<AppState>();
            st.scrolling.lock().unwrap().take();
        }
        match res {
            Ok(Some(item)) => {
                app2.emit("scroll:done", item).ok();
            }
            Ok(None) => {
                app2
                    .emit(
                        "scroll:error",
                        json!({ "message": "Scrolling capture produced no content" }),
                    )
                    .ok();
            }
            Err(e) => {
                app2.emit("scroll:error", json!({ "message": e })).ok();
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_scroll_capture(app: AppHandle) -> Result<(), String> {
    let st = app.state::<AppState>();
    let handle = st
        .scrolling
        .lock()
        .unwrap()
        .take()
        .ok_or("No scrolling capture is running")?;
    handle.cancel.store(true, Ordering::Relaxed);
    Ok(())
}
