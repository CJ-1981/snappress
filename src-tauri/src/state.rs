use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use crate::library::LibraryFile;
use crate::settings::Settings;

/// Why the last still capture had to skip Windows.Graphics.Capture.
static GDI_FALLBACK: Mutex<Option<String>> = Mutex::new(None);

/// Note that a capture fell back to GDI because of `reason`.
#[cfg(target_os = "windows")]
pub fn record_gdi_fallback(reason: &str) {
    *GDI_FALLBACK.lock().unwrap() = Some(reason.to_string());
}

/// The reason of the most recent GDI fallback, for diagnostics.
pub fn gdi_fallback() -> Option<String> {
    GDI_FALLBACK.lock().unwrap().clone()
}

pub struct RecordingHandle {
    pub stop: Arc<std::sync::atomic::AtomicBool>,
    pub paused: Arc<std::sync::atomic::AtomicBool>,
    pub out_path: PathBuf,
}

pub struct ScrollHandle {
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct PendingOverlay {
    pub mode: String,
    pub display_id: Option<u32>,
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub settings: Mutex<Settings>,
    pub library: Mutex<LibraryFile>,
    pub recording: Mutex<Option<RecordingHandle>>,
    pub scrolling: Mutex<Option<ScrollHandle>>,
    pub pending_overlay: Mutex<Option<PendingOverlay>>,
    /// Set while the main window is hidden for a grab, so the overlay going
    /// away is not mistaken for a cancelled capture.
    pub capture_in_flight: AtomicBool,
    /// Set while an overlay is being replaced by a new one, so the old one
    /// going away is not mistaken for a cancelled capture.
    pub overlay_opening: AtomicBool,
    /// Bumped whenever a capture starts or ends; lets a watchdog tell whether
    /// the capture it was started for is still the current one.
    pub capture_generation: AtomicU64,
    /// The global shortcuts currently registered, as (action, shortcut
    /// string); the source of truth for replacing them when settings change.
    pub registered_hotkeys: Mutex<Vec<(&'static str, String)>>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let settings_path = data_dir.join("settings.json");
        let index_path = data_dir.join("index.json");
        Self {
            data_dir,
            settings: Mutex::new(Settings::load(&settings_path)),
            library: Mutex::new(LibraryFile::load(&index_path)),
            recording: Mutex::new(None),
            scrolling: Mutex::new(None),
            pending_overlay: Mutex::new(None),
            capture_in_flight: AtomicBool::new(false),
            overlay_opening: AtomicBool::new(false),
            registered_hotkeys: Mutex::new(Vec::new()),
            capture_generation: AtomicU64::new(0),
        }
    }

    pub fn settings_path(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }

    pub fn index_path(&self) -> PathBuf {
        self.data_dir.join("index.json")
    }
}
