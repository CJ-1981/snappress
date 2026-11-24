use std::time::Duration;

use image::RgbaImage;
use scap::capturer::{Capturer, CaptureError};
use scap::frame::Frame;
use scap::Target;

#[cfg(target_os = "windows")]
mod gdi;
pub mod stitch;

// --- Screen Recording permission (macOS TCC) --------------------------------
// scap→screencapturekit unwraps SCShareableContent, so without Screen
// Recording access it panics — and panicking inside the WKWebView IPC
// callback aborts the app on launch. Preflight the permission first so
// every capture path degrades to a plain error instead.

#[cfg(target_os = "macos")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// Hint surfaced to the UI whenever a capture fails due to missing TCC access.
pub const SCREEN_PERMISSION_HINT: &str = "SnapPress needs Screen Recording permission: open System Settings → Privacy & Security → Screen Recording, allow SnapPress, then restart the app.";

/// True when macOS has already granted Screen Recording access.
pub fn has_screen_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: stateless CoreGraphics/TCC query, no UI side effects.
        unsafe { CGPreflightScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    true
}

/// Fire the macOS Screen Recording prompt (no-op once decided).
/// Returns the current grant state.
pub fn request_screen_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: CoreGraphics call; shows the system TCC prompt at most once.
        unsafe { CGRequestScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    true
}

/// How long the first frame of a capture session may take to arrive. Short
/// on Windows, where a session that stays silent is replaced by GDI.
#[cfg(target_os = "windows")]
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(3);
#[cfg(not(target_os = "windows"))]
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(8);

/// Whether the OS reports its capture API as available.
pub fn capture_supported() -> bool {
    scap::is_supported()
}

/// Enumerate scap display targets (scap 0.0.8 exposes displays via targets).
pub fn display_targets() -> Vec<Target> {
    if !has_screen_permission() {
        return Vec::new();
    }
    scap::get_all_targets()
        .into_iter()
        .filter(|t| matches!(t, Target::Display(_)))
        .collect()
}

/// scap displays paired with the tauri monitors that show them.
pub struct DisplayMap {
    monitors: Vec<tauri::Monitor>,
    displays: Vec<Target>,
    monitor_of_display: Vec<Option<usize>>,
    display_of_monitor: Vec<Option<usize>>,
}

impl DisplayMap {
    /// Enumerate the app's monitors and pair them with `displays`.
    pub fn build(app: &tauri::AppHandle, displays: Vec<Target>) -> Self {
        let monitors = app.available_monitors().unwrap_or_default();
        let pairing = pair(&displays, &monitors);
        Self {
            monitors,
            displays,
            monitor_of_display: pairing.monitor_of_display,
            display_of_monitor: pairing.display_of_monitor,
        }
    }

    /// The monitor showing scap display `display_id`.
    pub fn monitor_of(&self, display_id: u32) -> Option<&tauri::Monitor> {
        let i = self.display_index(display_id)?;
        match self.monitor_of_display.get(i) {
            Some(Some(m)) => self.monitors.get(*m),
            _ => None,
        }
    }

    /// The scap display shown on `monitor`.
    pub fn display_on(&self, monitor: &tauri::Monitor) -> Option<&Target> {
        let m = self
            .monitors
            .iter()
            .position(|known| same_monitor(known, monitor))?;
        match self.display_of_monitor.get(m) {
            Some(Some(d)) => self.displays.get(*d),
            _ => None,
        }
    }

    pub fn displays(&self) -> &[Target] {
        &self.displays
    }

    pub fn monitors(&self) -> &[tauri::Monitor] {
        &self.monitors
    }

    fn display_index(&self, display_id: u32) -> Option<usize> {
        self.displays
            .iter()
            .position(|t| matches!(t, Target::Display(d) if d.id == display_id))
    }
}

struct Pairing {
    monitor_of_display: Vec<Option<usize>>,
    display_of_monitor: Vec<Option<usize>>,
}

/// Pair every display with the monitor that shows it.
///
/// scap and tao enumerate monitors independently, so index order only holds
/// while both happen to agree. Both also expose the monitor's device name,
/// which pairs them exactly — unless a name is missing or shared by two
/// monitors, in which case index order is all there is.
fn pair(displays: &[Target], monitors: &[tauri::Monitor]) -> Pairing {
    let by_name = unique_names(displays, monitors).then(|| {
        displays
            .iter()
            .map(|t| match t {
                Target::Display(d) => monitors
                    .iter()
                    .position(|m| m.name().map(String::as_str) == Some(d.title.as_str())),
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    let monitor_of_display: Vec<Option<usize>> = (0..displays.len())
        .map(|i| {
            by_name
                .as_ref()
                .and_then(|p| p[i])
                .or_else(|| (i < monitors.len()).then_some(i))
        })
        .collect();
    let display_of_monitor = (0..monitors.len())
        .map(|m| monitor_of_display.iter().position(|paired| *paired == Some(m)))
        .collect();
    Pairing {
        monitor_of_display,
        display_of_monitor,
    }
}

/// True when every monitor and display has a name and no name is repeated.
fn unique_names(displays: &[Target], monitors: &[tauri::Monitor]) -> bool {
    let names: Vec<&str> = monitors.iter().filter_map(|m| m.name()).map(String::as_str).collect();
    let titles: Vec<&str> = displays
        .iter()
        .filter_map(|t| match t {
            Target::Display(d) => Some(d.title.as_str()),
            _ => None,
        })
        .collect();
    names.len() == monitors.len() && unique(&names) && unique(&titles)
}

fn unique(items: &[&str]) -> bool {
    (0..items.len()).all(|i| !items[..i].contains(&items[i]))
}

fn same_monitor(a: &tauri::Monitor, b: &tauri::Monitor) -> bool {
    a.position() == b.position() && a.size() == b.size()
}

/// Resolve a display id coming from the UI into a scap display id, falling
/// back to the primary display, then to the first one.
pub fn resolve_scap_display_id(
    app: &tauri::AppHandle,
    display_id: Option<u32>,
) -> Result<u32, String> {
    if !has_screen_permission() {
        return Err(SCREEN_PERMISSION_HINT.to_string());
    }
    let displays = display_targets();
    if displays.is_empty() {
        log::warn!("no scap display targets found");
        return Err("No displays detected".to_string());
    }
    if let Some(id) = display_id {
        if displays
            .iter()
            .any(|t| matches!(t, Target::Display(d) if d.id == id))
        {
            return Ok(id);
        }
        log::warn!("display id {display_id:?} is no longer known; using the primary display");
    }
    let map = DisplayMap::build(app, displays);
    if let Ok(Some(primary)) = app.primary_monitor() {
        if let Some(Target::Display(d)) = map.display_on(&primary) {
            return Ok(d.id);
        }
    }
    match map.displays.first() {
        Some(Target::Display(d)) => Ok(d.id),
        _ => Err("No displays detected".to_string()),
    }
}

/// Start a capture session on `display_id`. Fails when the session does not
/// come up, so callers never wait on a session that cannot deliver frames.
pub fn create_display_capturer(display_id: u32, fps: u32) -> Result<Capturer, String> {
    if !has_screen_permission() {
        return Err(SCREEN_PERMISSION_HINT.to_string());
    }
    let target = display_targets()
        .into_iter()
        .find(|t| matches!(t, Target::Display(d) if d.id == display_id))
        .ok_or_else(|| format!("display {display_id} not found"))?;
    let options = scap::capturer::Options {
        fps: fps.max(1),
        output_type: scap::frame::FrameType::BGRAFrame,
        target: Some(target),
        show_cursor: false,
        ..Default::default()
    };
    log::info!("starting capture session for display {display_id} @ {}fps", fps.max(1));
    let mut capturer = Capturer::build(options).map_err(|e| {
        log::warn!("screen capture is unavailable on this system: {e}");
        format!("screen capture is unavailable on this system: {e}")
    })?;
    capturer.start_capture();
    if let Some(e) = capturer.take_start_error() {
        log::warn!("capture session failed to start: {e}");
        return Err(describe(&CaptureError::NotStarted(e)));
    }
    Ok(capturer)
}

#[cfg(target_os = "windows")]
const CAPTURE_HINT: &str = " Windows.Graphics.Capture needs Windows 10 1903+ and a GPU-accelerated desktop; remote desktop and VM sessions often cannot use it.";
#[cfg(not(target_os = "windows"))]
const CAPTURE_HINT: &str = "";

/// A capture failure as shown to the user.
fn describe(error: &CaptureError) -> String {
    format!("{error}.{CAPTURE_HINT}")
}

/// Outcome of a grab.
pub enum Grab {
    /// A frame newer than the previous grab.
    Frame(RgbaImage),
    /// Nothing changed since the previous grab.
    Unchanged,
}

/// Repeated grabs of one display.
///
/// The OS only delivers frames while the screen is changing, so a grab that
/// sees nothing new reports [`Grab::Unchanged`] instead of waiting or failing.
pub struct DisplayGrabber {
    backend: Backend,
}

enum Backend {
    /// Windows.Graphics.Capture on Windows, ScreenCaptureKit on macOS.
    Scap(Capturer),
    #[cfg(target_os = "windows")]
    Gdi(gdi::GdiDisplay),
}

impl DisplayGrabber {
    /// Start grabbing `display_id`; returns the grabber with its first frame.
    pub fn start(display_id: u32, fps: u32) -> Result<(Self, RgbaImage), String> {
        let (capturer, first) = match Self::start_scap(display_id, fps) {
            Ok(started) => started,
            Err(e) => {
                #[cfg(target_os = "windows")]
                return Self::start_gdi(display_id, e);
                #[cfg(not(target_os = "windows"))]
                return Err(e);
            }
        };
        Ok((
            Self {
                backend: Backend::Scap(capturer),
            },
            first,
        ))
    }

    #[cfg(all(test, target_os = "windows"))]
    fn backend_name(&self) -> &'static str {
        match self.backend {
            Backend::Scap(_) => "Windows.Graphics.Capture",
            Backend::Gdi(_) => "GDI",
        }
    }

    /// Wait up to `wait` for a frame newer than the previous grab.
    pub fn grab(&mut self, wait: Duration) -> Result<Grab, String> {
        match &mut self.backend {
            Backend::Scap(capturer) => {
                // Keep only the newest frame: anything queued while the
                // caller was busy is already out of date.
                let mut latest: Option<RgbaImage> = None;
                loop {
                    let wait = if latest.is_some() { Duration::ZERO } else { wait };
                    match next_image(capturer, wait)? {
                        Some(frame) => latest = Some(frame),
                        None => break,
                    }
                }
                Ok(latest.map_or(Grab::Unchanged, Grab::Frame))
            }
            #[cfg(target_os = "windows")]
            Backend::Gdi(display) => display.grab().map(Grab::Frame),
        }
    }

    fn start_scap(display_id: u32, fps: u32) -> Result<(Capturer, RgbaImage), String> {
        let capturer = create_display_capturer(display_id, fps)?;
        match next_image(&capturer, FIRST_FRAME_TIMEOUT) {
            Ok(Some(first)) => Ok((capturer, first)),
            Ok(None) => Err(describe(&CaptureError::Timeout)),
            Err(e) => Err(e),
        }
    }

    #[cfg(target_os = "windows")]
    fn start_gdi(display_id: u32, scap_error: String) -> Result<(Self, RgbaImage), String> {
        log::warn!("grabbing display {display_id} through GDI instead: {scap_error}");
        crate::state::record_gdi_fallback(&scap_error);
        let display = gdi::GdiDisplay::open(display_id)
            .map_err(|e| format!("{scap_error} The GDI fallback failed too: {e}"))?;
        let first = display.grab()?;
        Ok((
            Self {
                backend: Backend::Gdi(display),
            },
            first,
        ))
    }
}

/// Wait up to `timeout` for the next frame; `None` when none arrived.
fn next_image(capturer: &Capturer, timeout: Duration) -> Result<Option<RgbaImage>, String> {
    match capturer.get_next_frame_with_timeout(timeout) {
        Ok(Frame::BGRA(f)) => {
            let (w, h) = (f.width.max(0) as u32, f.height.max(0) as u32);
            Ok(Some(bgra_to_rgba(&f.data, w, h)))
        }
        Ok(_) => Err("Unsupported frame type".to_string()),
        Err(CaptureError::Timeout) => Ok(None),
        Err(e) => {
            log::warn!("capture session produced no frame: {e}");
            Err(describe(&e))
        }
    }
}

/// Single-shot full-display grab (physical pixels).
pub fn grab_display(display_id: u32) -> Result<RgbaImage, String> {
    DisplayGrabber::start(display_id, 30).map(|(_, first)| first)
}

/// Convert a raw BGRA buffer into an RGBA image.
pub fn bgra_to_rgba(data: &[u8], width: u32, height: u32) -> RgbaImage {
    let px = (width as usize).saturating_mul(height as usize);
    let mut rgba = vec![0u8; px * 4];
    let n = px.min(data.len() / 4);
    for i in 0..n {
        let s = i * 4;
        rgba[s] = data[s + 2]; // R
        rgba[s + 1] = data[s + 1]; // G
        rgba[s + 2] = data[s]; // B
        rgba[s + 3] = data[s + 3]; // A
    }
    RgbaImage::from_raw(width, height, rgba).expect("rgba buffer size mismatch")
}

/// Crop a region out of a full-frame BGRA buffer into an RGBA destination.
/// `rx`/`ry` are relative to the display's top-left corner, in physical px.
pub fn crop_bgra_into(
    src: &[u8],
    fw: u32,
    fh: u32,
    rx: i32,
    ry: i32,
    rw: u32,
    rh: u32,
    dst: &mut [u8],
) {
    let rx = rx.clamp(0, fw as i32 - 1) as u32;
    let ry = ry.clamp(0, fh as i32 - 1) as u32;
    let rw = rw.min(fw - rx);
    let rh = rh.min(fh - ry);
    for row in 0..rh {
        let src_row = ((ry + row) as usize) * fw as usize;
        let dst_row = (row as usize) * rw as usize;
        for col in 0..rw {
            let i = (src_row + (rx as usize + col as usize)) * 4;
            let o = (dst_row + col as usize) * 4;
            if i + 3 < src.len() && o + 3 < dst.len() {
                dst[o] = src[i + 2];
                dst[o + 1] = src[i + 1];
                dst[o + 2] = src[i];
                dst[o + 3] = src[i + 3];
            }
        }
    }
}

/// Clamp a rect into image bounds and crop it from an RGBA image.
pub fn crop_rgba(img: &RgbaImage, x: i32, y: i32, w: u32, h: u32) -> Result<RgbaImage, String> {
    let iw = img.width() as i32;
    let ih = img.height() as i32;
    let x0 = x.clamp(0, iw - 1);
    let y0 = y.clamp(0, ih - 1);
    let cw = w.min((iw - x0) as u32).max(1);
    let ch = h.min((ih - y0) as u32).max(1);
    Ok(image::imageops::crop_imm(img, x0 as u32, y0 as u32, cw, ch).to_image())
}

/// Exercises the real capture APIs on a Windows desktop session. They print
/// what each path did, and append it to the file named by
/// `SNAPPRESS_TEST_REPORT` when a workflow sets one.
#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::*;
    use std::time::Instant;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, MB_OK, MessageBoxW, PostMessageW, WM_CLOSE,
    };
    use windows::core::{PCWSTR, w};

    /// Print `line`, and append it to the report file when a workflow set one.
    fn report(line: impl std::fmt::Display) {
        let line = line.to_string();
        eprintln!("{line}");
        if let Ok(path) = std::env::var("SNAPPRESS_TEST_REPORT") {
            use std::io::Write;
            let file = std::fs::OpenOptions::new().create(true).append(true).open(path);
            if let Ok(mut file) = file {
                let _ = writeln!(file, "{line}");
            }
        }
    }

    fn display_ids() -> Vec<u32> {
        report(format!(
            "capture API available: {}",
            scap::is_supported()
        ));
        let ids: Vec<u32> = display_targets()
            .into_iter()
            .filter_map(|t| match t {
                Target::Display(d) => {
                    report(format!("display id {} = \"{}\"", d.id, d.title));
                    Some(d.id)
                }
                _ => None,
            })
            .collect();
        assert!(!ids.is_empty(), "no displays found");
        ids
    }

    /// Show a message box for a moment. It changes the screen, so every live
    /// capture session delivers a frame.
    fn change_screen() {
        const TITLE: PCWSTR = w!("SnapPress capture test");
        std::thread::spawn(|| {
            // SAFETY: plain message box on a desktop session.
            unsafe {
                MessageBoxW(
                    HWND::default(),
                    w!("A capture test is changing the screen."),
                    TITLE,
                    MB_OK,
                );
            }
        });
        std::thread::sleep(Duration::from_millis(1500));
        // SAFETY: posts a close to the message box created above, if any.
        unsafe {
            if let Ok(window) = FindWindowW(PCWSTR::null(), TITLE) {
                if !window.is_invalid() {
                    let _ = PostMessageW(window, WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    #[test]
    fn gdi_grabs_every_display() {
        for id in display_ids() {
            let display = gdi::GdiDisplay::open(id).unwrap_or_else(|e| panic!("GDI open {id}: {e}"));
            let image = display.grab().unwrap_or_else(|e| panic!("GDI grab {id}: {e}"));
            let painted = image
                .pixels()
                .filter(|p| p.0[0] > 0 || p.0[1] > 0 || p.0[2] > 0)
                .count();
            report(format!(
                "GDI display {id}: {}x{}, {} non-black pixels",
                image.width(),
                image.height(),
                painted
            ));
            assert!(image.width() > 0 && image.height() > 0, "empty frame for {id}");
            assert!(image.pixels().all(|p| p.0[3] == 255), "frame must be opaque");
        }
    }

    #[test]
    fn grabber_starts_and_stops_cleanly_on_every_display() {
        for id in display_ids() {
            let started = Instant::now();
            let (mut grabber, first) =
                DisplayGrabber::start(id, 30).unwrap_or_else(|e| panic!("start {id}: {e}"));
            report(format!(
                "grabber display {id}: {}x{} via {} in {} ms",
                first.width(),
                first.height(),
                grabber.backend_name(),
                started.elapsed().as_millis()
            ));
            assert!(first.width() > 0 && first.height() > 0, "empty frame for {id}");
            let _ = grabber
                .grab(Duration::from_millis(500))
                .unwrap_or_else(|e| panic!("grab {id}: {e}"));
            drop(grabber);
        }
        std::thread::sleep(Duration::from_secs(1));
    }

    #[test]
    fn capture_sessions_report_why_they_fail() {
        for id in display_ids() {
            let started = Instant::now();
            match create_display_capturer(id, 30) {
                Ok(capturer) => {
                    report(format!(
                        "Windows.Graphics.Capture display {id}: started in {} ms",
                        started.elapsed().as_millis()
                    ));
                    let frame = capturer.get_next_frame_with_timeout(Duration::from_secs(3));
                    report(format!(
                        "Windows.Graphics.Capture display {id}: first frame {frame:?}"
                    ));
                    drop(capturer);
                }
                Err(e) => report(format!(
                    "Windows.Graphics.Capture display {id}: no session after {} ms: {e}",
                    started.elapsed().as_millis()
                )),
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }

    /// Upstream left the capture session running when the capturer was
    /// dropped; the next frame then hit `tx.send(..).expect(..)` inside the
    /// WinRT callback and aborted the process. Reaching the end of this test
    /// with a live session means that no longer happens.
    #[test]
    fn dropping_a_live_session_does_not_abort_the_process() {
        for id in display_ids() {
            let Ok(capturer) = create_display_capturer(id, 30) else {
                report(format!("display {id}: no session, nothing to drop"));
                continue;
            };
            let frame = capturer.get_next_frame_with_timeout(Duration::from_secs(3));
            drop(capturer);
            std::thread::sleep(Duration::from_millis(300));
            change_screen();
            std::thread::sleep(Duration::from_secs(1));
            report(format!(
                "display {id}: alive after a dropped session ({}) and a screen change",
                match &frame {
                    Ok(_) => "it was delivering frames",
                    Err(_) => "it never delivered a frame",
                }
            ));
        }
    }
}
