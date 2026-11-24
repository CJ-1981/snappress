use crate::{
    capturer::{Area, Options, Point, Resolution, Size},
    frame::{BGRAFrame, Frame, FrameType},
    targets::{self, get_scale_factor, Target},
};
use std::cmp;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows_capture::{
    capture::{CaptureControl, GraphicsCaptureApiHandler},
    frame::Frame as WCFrame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor as WCMonitor,
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings as WCSettings,
    },
    window::Window as WCWindow,
};
use windows_capture::capture::Context;

type HandlerError = Box<dyn std::error::Error + Send + Sync>;

/// Why the frame handler stopped, shared with the `Capturer` that waits on
/// its frames so it can report the reason.
type SharedError = Arc<Mutex<Option<String>>>;

/// Upper bound on bringing a capture session up. `start_free_threaded` only
/// returns once the capture thread has done so, and has no timeout of its own.
const START_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct Capturer {
    /// Held while the session is live and dropped once it ends, so the frame
    /// receiver fails fast instead of waiting for frames that never come.
    tx: Option<mpsc::Sender<Frame>>,
    pub crop: Option<Area>,
    error: SharedError,
}

#[derive(Clone)]
enum Settings {
    Window(WCSettings<FlagStruct, WCWindow>),
    Display(WCSettings<FlagStruct, WCMonitor>),
}

pub struct WCStream {
    /// Taken by `start_capture`; dropped on a failed start so the frame
    /// channel disconnects instead of waiting forever.
    settings: Option<Settings>,
    capture_control: Option<CaptureControl<Capturer, HandlerError>>,
    /// Set when the capture session failed to start (e.g. Windows.Graphics.
    /// Capture unavailable: VM/RDP session, pre-1903 Windows, GPU problem).
    pub(crate) start_error: Option<String>,
    pub(crate) handler_error: SharedError,
}

/// Copy one frame out as BGRA, cropped to `crop` when set.
fn capture_bgra(frame: &mut WCFrame, crop: Option<&Area>) -> Result<BGRAFrame, HandlerError> {
    let display_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);

    if let Some(cropped_area) = crop {
        // get the cropped area
        let start_x = cropped_area.origin.x as u32;
        let start_y = cropped_area.origin.y as u32;
        let end_x = (cropped_area.origin.x + cropped_area.size.width) as u32;
        let end_y = (cropped_area.origin.y + cropped_area.size.height) as u32;

        // crop the frame
        let mut cropped_buffer = frame
            .buffer_crop(start_x, start_y, end_x, end_y)
            .map_err(|e| format!("Failed to crop buffer: {e}"))?;

        // get raw frame buffer
        let raw_frame_buffer = cropped_buffer
            .as_nopadding_buffer()
            .map_err(|e| format!("Failed to get raw buffer: {e}"))?;

        Ok(BGRAFrame {
            display_time,
            width: cropped_area.size.width as i32,
            height: cropped_area.size.height as i32,
            data: raw_frame_buffer.to_vec(),
        })
    } else {
        // get raw frame buffer. Scoped: the buffer borrows the frame mutably,
        // and its size is read below.
        let frame_data = {
            let mut frame_buffer = frame
                .buffer()
                .map_err(|e| format!("Failed to get frame buffer: {e}"))?;
            frame_buffer.as_raw_buffer().to_vec()
        };

        Ok(BGRAFrame {
            display_time,
            width: frame.width() as i32,
            height: frame.height() as i32,
            data: frame_data,
        })
    }
}

// Runs inside the WinRT `FrameArrived` callback: a panic there unwinds
// through the FFI boundary and aborts the whole process, so every failure is
// returned or ends the session instead.
impl GraphicsCaptureApiHandler for Capturer {
    type Flags = FlagStruct;
    type Error = HandlerError;

    fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            tx: Some(context.flags.tx),
            crop: context.flags.crop,
            error: context.flags.error,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut WCFrame,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if self.tx.is_none() {
            // Already ended; the session is torn down asynchronously.
            capture_control.stop();
            return Ok(());
        }

        match capture_bgra(frame, self.crop.as_ref()) {
            Ok(bgr_frame) => {
                let delivered = self
                    .tx
                    .as_ref()
                    .is_some_and(|tx| tx.send(Frame::BGRA(bgr_frame)).is_ok());
                if !delivered {
                    // The receiver is gone: the caller dropped its capturer.
                    self.tx = None;
                    capture_control.stop();
                }
                Ok(())
            }
            Err(e) => {
                if let Ok(mut error) = self.error.lock() {
                    *error = Some(e.to_string());
                }
                self.tx = None;
                capture_control.stop();
                Err(e)
            }
        }
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        println!("Closed");
        Ok(())
    }
}

impl WCStream {
    pub fn start_capture(&mut self) {
        let settings = self.settings.take().expect("capture already started");

        // Run on a helper thread so a session that never comes up cannot
        // wedge the caller (see START_TIMEOUT).
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let started = match settings {
                Settings::Display(st) => Capturer::start_free_threaded(st),
                Settings::Window(st) => Capturer::start_free_threaded(st),
            };
            let _ = tx.send(started.map_err(|e| e.to_string()));
        });

        match rx.recv_timeout(START_TIMEOUT) {
            Ok(Ok(capture_control)) => self.capture_control = Some(capture_control),
            Ok(Err(e)) => self.start_error = Some(e),
            Err(_) => {
                self.start_error = Some(format!(
                    "timed out after {}s waiting for the capture session to start",
                    START_TIMEOUT.as_secs()
                ));
            }
        }
    }

    pub fn stop_capture(&mut self) {
        if let Some(capture_control) = self.capture_control.take() {
            let _ = capture_control.stop();
        }
    }
}

impl Drop for WCStream {
    fn drop(&mut self) {
        if let Some(capture_control) = self.capture_control.take() {
            // Nothing else ends the session: without this the capture thread
            // outlives the capturer and keeps the session (and its capture
            // border) alive. Halt frames right away, then stop on a helper
            // thread — `stop` joins the capture thread, and a session that
            // will not shut down must not wedge whoever drops the capturer.
            capture_control.halt_handle().store(true, Ordering::Relaxed);
            std::thread::spawn(move || {
                let _ = capture_control.stop();
            });
        }
    }
}

#[derive(Clone, Debug)]
struct FlagStruct {
    pub tx: mpsc::Sender<Frame>,
    pub crop: Option<Area>,
    error: SharedError,
}

pub fn create_capturer(options: &Options, tx: mpsc::Sender<Frame>) -> WCStream {
    let target = options
        .target
        .clone()
        .unwrap_or_else(|| Target::Display(targets::get_main_display()));

    let color_format = match options.output_type {
        FrameType::BGRAFrame => ColorFormat::Bgra8,
        _ => ColorFormat::Rgba8,
    };

    let show_cursor = match options.show_cursor {
        true => CursorCaptureSettings::WithCursor,
        false => CursorCaptureSettings::WithoutCursor,
    };

    let handler_error: SharedError = Arc::new(Mutex::new(None));

    let settings = match target {
        Target::Display(display) => Settings::Display(WCSettings::new(
            WCMonitor::from_raw_hmonitor(display.raw_handle.0),
            show_cursor,
            DrawBorderSettings::Default,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            color_format,
            FlagStruct {
                tx,
                crop: Some(get_crop_area(options)),
                error: handler_error.clone(),
            },
        )),
        Target::Window(window) => Settings::Window(WCSettings::new(
            WCWindow::from_raw_hwnd(window.raw_handle.0),
            show_cursor,
            DrawBorderSettings::Default,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            color_format,
            FlagStruct {
                tx,
                crop: Some(get_crop_area(options)),
                error: handler_error.clone(),
            },
        )),
    };

    WCStream {
        settings: Some(settings),
        capture_control: None,
        start_error: None,
        handler_error,
    }
}

pub fn get_output_frame_size(options: &Options) -> [u32; 2] {
    let target = options
        .target
        .clone()
        .unwrap_or_else(|| Target::Display(targets::get_main_display()));

    let crop_area = get_crop_area(options);

    let mut output_width = (crop_area.size.width) as u32;
    let mut output_height = (crop_area.size.height) as u32;

    match options.output_resolution {
        Resolution::Captured => {}
        _ => {
            let [resolved_width, resolved_height] = options
                .output_resolution
                .value((crop_area.size.width as f32) / (crop_area.size.height as f32));
            // 1280 x 853
            output_width = cmp::min(output_width, resolved_width);
            output_height = cmp::min(output_height, resolved_height);
        }
    }

    output_width -= output_width % 2;
    output_height -= output_height % 2;

    [output_width, output_height]
}

fn get_absolute_value(value: f64, scale_factor: f64) -> f64 {
    let value = (value * scale_factor).floor();
    value + value % 2.0
}

pub fn get_crop_area(options: &Options) -> Area {
    let target = options
        .target
        .clone()
        .unwrap_or_else(|| Target::Display(targets::get_main_display()));

    let (width, height) = targets::get_target_dimensions(&target);

    let scale_factor = targets::get_scale_factor(&target);
    options
        .crop_area
        .as_ref()
        .map(|val| {
            // WINDOWS: limit values [input-width, input-height] = [146, 50]
            Area {
                origin: Point {
                    x: get_absolute_value(val.origin.x, scale_factor),
                    y: get_absolute_value(val.origin.y, scale_factor),
                },
                size: Size {
                    width: get_absolute_value(val.size.width, scale_factor),
                    height: get_absolute_value(val.size.height, scale_factor),
                },
            }
        })
        .unwrap_or_else(|| Area {
            origin: Point { x: 0.0, y: 0.0 },
            size: Size {
                width: width as f64,
                height: height as f64,
            },
        })
}
