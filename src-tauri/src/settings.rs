use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub recording_fps: u32,
    pub gif_fps: u32,
    pub gif_width: u32,
    pub scroll_delay_ms: u64,
    pub scroll_step_ticks: i32,
    pub max_scroll_height: u32,
    /// Automatically remove library entries older than this many days (0 = off).
    pub auto_delete_days: u32,
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            recording_fps: 30,
            gif_fps: 12,
            gif_width: 800,
            scroll_delay_ms: 350,
            scroll_step_ticks: 5,
            max_scroll_height: 20_000,
            auto_delete_days: 0,
            theme: "dark".to_string(),
        }
    }
}

impl Settings {
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

    /// Clamp user-provided values into safe ranges.
    pub fn sanitize(&mut self) {
        self.recording_fps = self.recording_fps.clamp(1, 60);
        self.gif_fps = self.gif_fps.clamp(1, 30);
        self.gif_width = self.gif_width.clamp(160, 1920);
        self.scroll_delay_ms = self.scroll_delay_ms.clamp(80, 5000);
        self.scroll_step_ticks = self.scroll_step_ticks.clamp(1, 15);
        self.max_scroll_height = self.max_scroll_height.clamp(1_000, 100_000);
        self.auto_delete_days = self.auto_delete_days.min(365);
    }
}
