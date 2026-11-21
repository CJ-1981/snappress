use std::path::{Path, PathBuf};

use image::RgbaImage;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureItem {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub created_at: String,
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibraryFile {
    #[serde(default)]
    pub items: Vec<CaptureItem>,
}

impl LibraryFile {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn find(&self, id: &str) -> Option<CaptureItem> {
        self.items.iter().find(|i| i.id == id).cloned()
    }
}

pub fn timestamp_slug() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

pub fn short_uuid() -> String {
    let s = uuid::Uuid::new_v4().simple().to_string();
    s[..8].to_string()
}

pub fn captures_dir(app: &AppHandle) -> PathBuf {
    app.state::<AppState>().data_dir.join("captures")
}

pub fn register_file(
    app: &AppHandle,
    path: &Path,
    kind: &str,
    width: u32,
    height: u32,
    duration_ms: Option<u64>,
    origin: Option<&str>,
) -> Result<CaptureItem, String> {
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("failed to read metadata of {}: {e}", path.display()))?;
    let item = CaptureItem {
        id: uuid::Uuid::new_v4().to_string(),
        kind: kind.to_string(),
        path: path.to_string_lossy().to_string(),
        created_at: chrono::Local::now().to_rfc3339(),
        width,
        height,
        size_bytes: meta.len(),
        duration_ms,
        origin: origin.map(|s| s.to_string()),
    };
    {
        let state = app.state::<AppState>();
        let mut lib = state.library.lock().unwrap();
        lib.items.insert(0, item.clone());
        lib.save(&state.index_path())?;
    }
    app.emit("library:changed", ()).ok();
    Ok(item)
}

pub fn save_image_to_library(
    app: &AppHandle,
    img: &RgbaImage,
    origin: &str,
) -> Result<CaptureItem, String> {
    let dir = captures_dir(app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}-{}.png", timestamp_slug(), short_uuid()));
    img.save(&path).map_err(|e| format!("failed to save image: {e}"))?;
    let (w, h) = (img.width(), img.height());
    let item = register_file(app, &path, "image", w, h, None, Some(origin))?;
    if origin == "capture" || origin == "scroll" {
        app.emit("capture:saved", &item).ok();
    }
    Ok(item)
}

pub fn delete_item(app: &AppHandle, id: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let removed = {
        let mut lib = state.library.lock().unwrap();
        let before = lib.items.len();
        let removed: Vec<CaptureItem> = lib.items.iter().filter(|i| i.id == id).cloned().collect();
        lib.items.retain(|i| i.id != id);
        if lib.items.len() == before {
            return Err("Item not found".into());
        }
        lib.save(&state.index_path())?;
        removed
    };
    for r in removed {
        let _ = std::fs::remove_file(&r.path);
    }
    app.emit("library:changed", ()).ok();
    Ok(())
}
