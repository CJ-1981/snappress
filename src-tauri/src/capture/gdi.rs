//! GDI screen grab for Windows, used when Windows.Graphics.Capture cannot run
//! (remote desktop and VM sessions, missing or broken GPU drivers).
//!
//! Slower than Windows.Graphics.Capture and without the cursor, but it works
//! wherever there is a desktop, and per monitor regardless of arrangement.

use std::ffi::c_void;
use std::ptr::null_mut;

use image::RgbaImage;
use windows::core::{PCWSTR, w};
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CAPTUREBLT, BitBlt, CreateCompatibleDC, CreateDCW,
    CreateDIBSection, DeleteDC, DeleteObject, DESKTOPHORZRES, DESKTOPVERTRES, DIB_RGB_COLORS,
    GetDC, GetDeviceCaps, GetMonitorInfoW, HBITMAP, HDC, HGDIOBJ, HMONITOR, MONITORINFOEXW,
    ReleaseDC, SRCCOPY, SelectObject,
};

/// One monitor, read through a GDI device context.
pub struct GdiDisplay {
    source: Source,
    /// Memory DC holding `bitmap`, which stays selected into it for `grab`.
    memory: HDC,
    /// What was selected into `memory` before `bitmap`; selected back on drop
    /// because a selected bitmap cannot be deleted.
    previous: HGDIOBJ,
    bitmap: HBITMAP,
    /// Pixels of `bitmap`: top-down, four bytes per pixel, blue first.
    bits: *mut c_void,
    width: i32,
    height: i32,
    /// How `source` was obtained, for the capture tests' report.
    #[cfg(test)]
    provenance: &'static str,
}

enum Source {
    /// A DC for this monitor alone; its origin is the monitor's top-left.
    Monitor(HDC),
    /// The whole virtual screen; the monitor starts at its offset inside it.
    Desktop { dc: HDC, x: i32, y: i32 },
}

impl Source {
    fn dc(&self) -> HDC {
        match self {
            Source::Monitor(dc) => *dc,
            Source::Desktop { dc, .. } => *dc,
        }
    }

    /// Where the monitor starts inside this source.
    fn origin(&self) -> (i32, i32) {
        match self {
            Source::Monitor(_) => (0, 0),
            Source::Desktop { x, y, .. } => (*x, *y),
        }
    }

    /// # SAFETY
    ///
    /// Must be called once, when the source is no longer used.
    unsafe fn release(&self) {
        match self {
            Source::Monitor(dc) => {
                let _ = DeleteDC(*dc);
            }
            Source::Desktop { dc, .. } => {
                ReleaseDC(HWND::default(), *dc);
            }
        }
    }
}

impl GdiDisplay {
    pub fn open(display_id: u32) -> Result<Self, String> {
        let display = crate::capture::display_targets()
            .into_iter()
            .find_map(|t| match t {
                scap::Target::Display(d) if d.id == display_id => Some(d),
                _ => None,
            })
            .ok_or_else(|| format!("display {display_id} not found"))?;
        // SAFETY: the handle comes from the enumeration that just ran.
        unsafe { Self::open_monitor(display.raw_handle) }
    }

    /// Grab the whole monitor as it is right now.
    pub fn grab(&self) -> Result<RgbaImage, String> {
        let (x, y) = self.source.origin();
        // SAFETY: every handle is valid and `bitmap` is exactly the blitted
        // area, so `bits` holds `width * height` pixels afterwards.
        unsafe {
            BitBlt(
                self.memory,
                0,
                0,
                self.width,
                self.height,
                self.source.dc(),
                x,
                y,
                SRCCOPY | CAPTUREBLT,
            )
            .map_err(|e| format!("BitBlt failed: {e}"))?;

            let len = (self.width as usize) * (self.height as usize) * 4;
            let bgra = std::slice::from_raw_parts(self.bits as *const u8, len);
            let mut rgba = vec![0u8; len];
            for (s, d) in bgra.chunks_exact(4).zip(rgba.chunks_exact_mut(4)) {
                d[0] = s[2];
                d[1] = s[1];
                d[2] = s[0];
                // GDI leaves the alpha byte unset; the screen is opaque.
                d[3] = 255;
            }
            RgbaImage::from_raw(self.width as u32, self.height as u32, rgba)
                .ok_or_else(|| "GDI returned a frame with an invalid size".to_string())
        }
    }

    /// # SAFETY
    ///
    /// `monitor` must be a valid monitor handle.
    unsafe fn open_monitor(monitor: HMONITOR) -> Result<Self, String> {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if !GetMonitorInfoW(monitor, std::ptr::addr_of_mut!(info).cast()).as_bool() {
            return Err("GetMonitorInfoW failed".into());
        }
        let rect = info.monitorInfo.rcMonitor;
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        if width <= 0 || height <= 0 {
            return Err("the monitor reports no usable size".into());
        }

        #[cfg_attr(not(test), allow(unused_variables))]
        let (source, provenance) = match Self::monitor_dc(&info, width, height) {
            Some(found) => found,
            None => {
                // No DC for the monitor alone: read it out of the whole
                // virtual screen, where it starts at its monitor rectangle.
                let dc = GetDC(HWND::default());
                (
                    Source::Desktop {
                        dc,
                        x: rect.left,
                        y: rect.top,
                    },
                    "the virtual screen",
                )
            }
        };
        let dc = source.dc();
        if dc.is_invalid() {
            return Err("could not get a device context for the screen".into());
        }

        // A top-down 32-bpp DIB: one blue-green-red-unused quad per pixel.
        let header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = null_mut();
        let bitmap = CreateDIBSection(
            dc,
            &header,
            DIB_RGB_COLORS,
            &mut bits,
            HANDLE::default(),
            0,
        )
        .map_err(|e| {
            source.release();
            format!("CreateDIBSection failed: {e}")
        })?;

        let memory = CreateCompatibleDC(dc);
        if memory.is_invalid() {
            let _ = DeleteObject(HGDIOBJ::from(bitmap));
            source.release();
            return Err("CreateCompatibleDC failed".into());
        }

        let previous = SelectObject(memory, HGDIOBJ::from(bitmap));
        if previous.is_invalid() || bits.is_null() {
            let _ = DeleteDC(memory);
            let _ = DeleteObject(HGDIOBJ::from(bitmap));
            source.release();
            return Err("SelectObject failed".into());
        }

        Ok(Self {
            source,
            memory,
            previous,
            bitmap,
            bits,
            width,
            height,
            #[cfg(test)]
            provenance,
        })
    }

    #[cfg(test)]
    pub(super) fn provenance(&self) -> &'static str {
        self.provenance
    }

    /// A DC that spans exactly this monitor, when Windows will make one.
    ///
    /// The device name is accepted in different argument positions depending
    /// on the Windows version, so each documented form is tried; a DC counts
    /// only when it has exactly the monitor's pixel size.
    fn monitor_dc(info: &MONITORINFOEXW, width: i32, height: i32) -> Option<(Source, &'static str)> {
        let device = PCWSTR::from_raw(info.szDevice.as_ptr());
        let empty = PCWSTR::null();
        let forms = [
            ((device, empty), "a monitor DC (device as driver)"),
            ((empty, device), "a monitor DC (device as device)"),
            ((w!("DISPLAY"), device), "a monitor DC (DISPLAY driver and device)"),
        ];
        // SAFETY: the device string belongs to a monitor Windows just
        // described, and every DC that is rejected is deleted again.
        unsafe {
            for ((driver, name), provenance) in forms {
                let dc = CreateDCW(driver, name, empty, None);
                if dc.is_invalid() {
                    continue;
                }
                let matches = (GetDeviceCaps(dc, DESKTOPHORZRES), GetDeviceCaps(dc, DESKTOPVERTRES))
                    == (width, height);
                if matches {
                    return Some((Source::Monitor(dc), provenance));
                }
                let _ = DeleteDC(dc);
            }
        }
        None
    }
}

impl Drop for GdiDisplay {
    fn drop(&mut self) {
        // SAFETY: each handle is live and is released exactly once, here.
        unsafe {
            let _ = SelectObject(self.memory, self.previous);
            let _ = DeleteDC(self.memory);
            let _ = DeleteObject(self.bitmap);
            self.source.release();
        }
    }
}
