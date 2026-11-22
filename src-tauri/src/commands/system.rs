use std::path::PathBuf;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::encode;
use crate::library::{self, CaptureItem};
use crate::settings::Settings;
use crate::state::AppState;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub id: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub is_primary: bool,
}

#[tauri::command]
pub fn list_displays(app: AppHandle) -> Vec<DisplayInfo> {
    let primary_id = crate::capture::resolve_scap_display_id(&app, None).ok();
    let monitors = app.available_monitors().unwrap_or_default();
    crate::capture::display_targets()
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let d = match t {
                scap::Target::Display(d) => d,
                _ => return None,
            };
            let m = monitors.get(i);
            let name = m
                .and_then(|mm| mm.name().cloned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("Display {}", i + 1));
            Some(DisplayInfo {
                id: d.id,
                name,
                width: m.map_or(0, |m| m.size().width),
                height: m.map_or(0, |m| m.size().height),
                scale_factor: m.map_or(1.0, |m| m.scale_factor() as f32),
                is_primary: primary_id.map_or(false, |p| p == d.id),
            })
        })
        .collect()
}

#[tauri::command]
pub fn screen_permission_status() -> bool {
    crate::capture::has_screen_permission()
}

/// Shows the system Screen Recording prompt when still undetermined.
#[tauri::command]
pub fn request_screen_permission() -> bool {
    crate::capture::request_screen_permission()
}

#[tauri::command]
pub fn open_screen_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("failed to open System Settings: {e}"))
    }
    #[cfg(not(target_os = "macos"))]
    Err("Not applicable on this platform".into())
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    app.state::<AppState>().settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_settings(app: AppHandle, settings: Settings) -> Result<Settings, String> {
    let mut s = settings;
    s.sanitize();
    s.save(&app.state::<AppState>().settings_path())?;
    *app.state::<AppState>().settings.lock().unwrap() = s.clone();
    Ok(s)
}

#[tauri::command]
pub fn list_library(app: AppHandle) -> Vec<CaptureItem> {
    // Apply the auto-delete window first so stale entries never surface.
    crate::library::prune_old_items(&app);
    app.state::<AppState>().library.lock().unwrap().items.clone()
}

#[tauri::command]
pub fn delete_all_items(app: AppHandle) -> Result<usize, String> {
    crate::library::delete_all_items(&app)
}

#[tauri::command]
pub fn delete_item(app: AppHandle, id: String) -> Result<(), String> {
    library::delete_item(&app, &id)
}

#[tauri::command]
pub fn get_library_dir(app: AppHandle) -> String {
    library::captures_dir(&app).to_string_lossy().to_string()
}

#[tauri::command]
pub fn open_path(app: AppHandle, path: String, reveal: Option<bool>) -> Result<(), String> {
    use tauri_plugin_shell::ShellExt;
    let p = PathBuf::from(&path);
    let target = if reveal.unwrap_or(false) {
        p.parent().map(|x| x.to_path_buf()).unwrap_or(p)
    } else {
        p
    };
    app.shell()
        .open(
            target.to_string_lossy().to_string(),
            None::<tauri_plugin_shell::open::Program>,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_image_to_clipboard(path: String) -> Result<(), String> {
    let img = image::open(&path).map_err(|e| format!("failed to open image: {e}"))?;
    let rgba = img.to_rgba8();
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("clipboard unavailable: {e}"))?;
    let data = arboard::ImageData {
        width: rgba.width() as usize,
        height: rgba.height() as usize,
        bytes: std::borrow::Cow::Borrowed(rgba.as_raw()),
    };
    clipboard.set_image(data).map_err(|e| format!("failed to copy: {e}"))
}

#[tauri::command]
pub fn export_gif(app: AppHandle, id: String) -> Result<(), String> {
    let item = {
        let st = app.state::<AppState>();
        let lib = st.library.lock().unwrap();
        lib.find(&id).ok_or("Item not found")?
    };
    if item.kind != "video" {
        return Err("Only video recordings can be exported as GIF".into());
    }
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    app.emit("export:started", json!({ "id": id })).ok();

    let app2 = app.clone();
    std::thread::spawn(move || {
        let input = PathBuf::from(&item.path);
        let out = input.with_extension("gif");
        match encode::convert_to_gif(&input, &out, settings.gif_fps, settings.gif_width) {
            Ok(()) => {
                let (w, h) = image::open(&out)
                    .map(|i| (i.width(), i.height()))
                    .unwrap_or((0, 0));
                let _ = library::register_file(
                    &app2,
                    &out,
                    "gif",
                    w,
                    h,
                    item.duration_ms,
                    Some("record"),
                );
                app2.emit("export:done", json!({ "id": item.id })).ok();
            }
            Err(e) => {
                app2
                    .emit("export:error", json!({ "id": item.id, "message": e }))
                    .ok();
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn save_edited(app: AppHandle, data: String, format: String) -> Result<CaptureItem, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let b64 = data
            .split("base64,")
            .nth(1)
            .ok_or("Invalid image data URL")?;
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| format!("base64 decode failed: {e}"))?;
        let img =
            image::load_from_memory(&bytes).map_err(|e| format!("invalid image data: {e}"))?;
        let ext = if format == "jpeg" { "jpg" } else { "png" };
        let dir = library::captures_dir(&app);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!(
            "edited-{}-{}.{}",
            library::timestamp_slug(),
            library::short_uuid(),
            ext
        ));
        img.save(&path)
            .map_err(|e| format!("failed to save edited image: {e}"))?;
        library::register_file(
            &app,
            &path,
            "image",
            img.width(),
            img.height(),
            None,
            Some("edit"),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn read_image_base64(app: AppHandle, path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    let dir = library::captures_dir(&app);
    let pc = p.canonicalize().unwrap_or_else(|_| p.clone());
    let dc = dir.canonicalize().unwrap_or_else(|_| dir.clone());
    if !pc.starts_with(&dc) {
        return Err("Path is outside the capture library".into());
    }
    let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
    use base64::Engine as _;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}
