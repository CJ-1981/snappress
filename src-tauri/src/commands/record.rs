use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::capture;
use crate::encode::{self, RecordJob};
use crate::library;
use crate::state::{AppState, RecordingHandle};

/// Clamp a region into monitor bounds and force even dimensions (needed for
/// yuv420p encoding).
fn clamp_rect(
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    mw: u32,
    mh: u32,
) -> (u32, u32, u32, u32) {
    let x0 = x.clamp(0, mw.saturating_sub(2) as i32);
    let y0 = y.clamp(0, mh.saturating_sub(2) as i32);
    let w = width.min(mw.saturating_sub(x0 as u32)).max(2) & !1;
    let h = height.min(mh.saturating_sub(y0 as u32)).max(2) & !1;
    (x0 as u32, y0 as u32, w, h)
}

#[tauri::command]
pub fn start_recording(
    app: AppHandle,
    display_id: Option<u32>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    crate::commands::overlay::close_overlay(&app);
    crate::commands::overlay::show_main(&app);

    if app.state::<AppState>().recording.lock().unwrap().is_some() {
        return Err("A recording is already in progress".into());
    }
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let monitor = crate::commands::overlay::pick_tauri_monitor(&app, display_id)?;
    let sid = capture::resolve_scap_display_id(&app, display_id)?;
    let (rx, ry, rw, rh) = clamp_rect(
        x,
        y,
        width,
        height,
        monitor.size().width,
        monitor.size().height,
    );
    let out = library::captures_dir(&app).join(format!(
        "rec-{}.mp4",
        library::timestamp_slug()
    ));
    let stop = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));

    let job = RecordJob {
        display_id: sid,
        rect: (rx, ry, rw, rh),
        fps: settings.recording_fps,
        out: out.clone(),
    };

    {
        let st = app.state::<AppState>();
        *st.recording.lock().unwrap() = Some(RecordingHandle {
            stop: stop.clone(),
            paused: paused.clone(),
            out_path: out,
        });
    }

    app.emit("recording:started", json!({ "format": "mp4" })).ok();

    let app2 = app.clone();
    std::thread::spawn(move || {
        let res = encode::run_record(app2.clone(), job, stop, paused);
        {
            let st = app2.state::<AppState>();
            st.recording.lock().unwrap().take();
        }
        match res {
            Ok(Some(item)) => {
                app2.emit("recording:done", item).ok();
            }
            Ok(None) => {
                app2
                    .emit(
                        "recording:error",
                        json!({ "message": "No frames were captured" }),
                    )
                    .ok();
            }
            Err(e) => {
                app2.emit("recording:error", json!({ "message": e })).ok();
            }
        }
    });
    Ok(())
}

pub fn stop_recording_impl(app: &AppHandle) -> Result<(), String> {
    let st = app.state::<AppState>();
    let handle = st
        .recording
        .lock()
        .unwrap()
        .take()
        .ok_or("Not recording")?;
    handle.stop.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn stop_recording(app: AppHandle) -> Result<(), String> {
    stop_recording_impl(&app)
}

#[tauri::command]
pub fn pause_recording(app: AppHandle, paused: bool) -> Result<(), String> {
    let st = app.state::<AppState>();
    let flag = {
        let rec = st.recording.lock().unwrap();
        rec.as_ref().ok_or("Not recording")?.paused.clone()
    };
    flag.store(paused, Ordering::Relaxed);
    app.emit("recording:paused", json!({ "paused": paused })).ok();
    Ok(())
}
