use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::library::LibraryFile;
use crate::settings::Settings;

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
        }
    }

    pub fn settings_path(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }

    pub fn index_path(&self) -> PathBuf {
        self.data_dir.join("index.json")
    }
}
