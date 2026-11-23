pub mod engine;

use std::{error::Error, sync::mpsc};

use engine::ChannelItem;

use crate::{
    frame::{Frame, FrameType},
    has_permission, is_supported,
    targets::Target,
};

pub use engine::get_output_frame_size;

#[derive(Debug, Clone, Copy, Default)]
pub enum Resolution {
    _480p,
    _720p,
    _1080p,
    _1440p,
    _2160p,
    _4320p,

    #[default]
    Captured,
}

impl Resolution {
    fn value(&self, aspect_ratio: f32) -> [u32; 2] {
        match *self {
            Resolution::_480p => [640, (640_f32 / aspect_ratio).floor() as u32],
            Resolution::_720p => [1280, (1280_f32 / aspect_ratio).floor() as u32],
            Resolution::_1080p => [1920, (1920_f32 / aspect_ratio).floor() as u32],
            Resolution::_1440p => [2560, (2560_f32 / aspect_ratio).floor() as u32],
            Resolution::_2160p => [3840, (3840_f32 / aspect_ratio).floor() as u32],
            Resolution::_4320p => [7680, (7680_f32 / aspect_ratio).floor() as u32],
            Resolution::Captured => {
                panic!(".value should not be called when Resolution type is Captured")
            }
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Default, Clone)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}
#[derive(Debug, Default, Clone)]
pub struct Area {
    pub origin: Point,
    pub size: Size,
}

/// Options passed to the screen capturer
#[derive(Debug, Default, Clone)]
pub struct Options {
    pub fps: u32,
    pub show_cursor: bool,
    pub show_highlight: bool,
    pub target: Option<Target>,
    pub crop_area: Option<Area>,
    pub output_type: FrameType,
    pub output_resolution: Resolution,
    // excluded targets will only work on macOS
    pub excluded_targets: Option<Vec<Target>>,
}

/// Screen capturer class
pub struct Capturer {
    engine: engine::Engine,
    rx: mpsc::Receiver<ChannelItem>,
    start_error: Option<String>,
}

#[derive(Debug)]
pub enum CapturerBuildError {
    NotSupported,
    PermissionNotGranted,
}

impl std::fmt::Display for CapturerBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapturerBuildError::NotSupported => write!(f, "Screen capturing is not supported"),
            CapturerBuildError::PermissionNotGranted => {
                write!(f, "Permission to capture the screen is not granted")
            }
        }
    }
}

impl Error for CapturerBuildError {}

impl Capturer {
    /// Create a new capturer instance with the provided options
    #[deprecated(
        since = "0.0.6",
        note = "Use `build` instead of `new` to create a new capturer instance."
    )]
    pub fn new(options: Options) -> Capturer {
        let (tx, rx) = mpsc::channel();
        let engine = engine::Engine::new(&options, tx);

        Capturer { engine, rx, start_error: None }
    }

    /// Build a new [Capturer] instance with the provided options
    pub fn build(options: Options) -> Result<Capturer, CapturerBuildError> {
        if !is_supported() {
            return Err(CapturerBuildError::NotSupported);
        }

        if !has_permission() {
            return Err(CapturerBuildError::PermissionNotGranted);
        }

        let (tx, rx) = mpsc::channel();
        let engine = engine::Engine::new(&options, tx);

        Ok(Capturer { engine, rx, start_error: None })
    }

    // TODO
    // Prevent starting capture if already started
    /// Start capturing the frames
    pub fn start_capture(&mut self) {
        self.engine.start();
        if let Some(e) = self.engine.take_start_error() {
            self.start_error = Some(e);
        }
    }

    /// Stop the capturer
    pub fn stop_capture(&mut self) {
        self.engine.stop();
    }

    /// Returns and clears the startup failure recorded by the engine, if any.
    pub fn take_start_error(&mut self) -> Option<String> {
        self.start_error.take()
    }

    /// Get the next captured frame.
    ///
    /// Waits at most 15 seconds per frame; a session that produces no frames
    /// (e.g. a failed Windows.Graphics.Capture start, or a GPU driver issue)
    /// returns an error instead of blocking the caller forever.
    pub fn get_next_frame(&self) -> Result<Frame, String> {
        if let Some(e) = &self.start_error {
            return Err(format!("capture session failed to start: {e}"));
        }
        loop {
            match self.rx.recv_timeout(std::time::Duration::from_secs(15)) {
                Ok(item) => {
                    if let Some(frame) = self.engine.process_channel_item(item) {
                        return Ok(frame);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(
                        "timed out after 15s waiting for capture frames — the capture session never produced a frame".into(),
                    );
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("capture session ended without delivering any frames".into());
                }
            }
        }
    }

    /// Get the dimensions the frames will be captured in
    pub fn get_output_frame_size(&mut self) -> [u32; 2] {
        self.engine.get_output_frame_size()
    }

    pub fn raw(&self) -> RawCapturer {
        RawCapturer { capturer: self }
    }
}

pub struct RawCapturer<'a> {
    capturer: &'a Capturer,
}
