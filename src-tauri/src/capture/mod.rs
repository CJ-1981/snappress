use image::RgbaImage;
use scap::capturer::Capturer;
use scap::frame::Frame;
use scap::Target;
use tauri::Manager;

pub mod stitch;

/// Enumerate scap display targets (scap 0.0.8 exposes displays via targets).
pub fn display_targets() -> Vec<Target> {
    scap::get_all_targets()
        .into_iter()
        .filter(|t| matches!(t, Target::Display(_)))
        .collect()
}

/// Resolve a display id coming from the UI into a scap display id.
/// Falls back by pairing scap displays with tauri monitors in enumeration
/// order (exact on single-monitor machines), then to the first display.
pub fn resolve_scap_display_id(
    app: &tauri::AppHandle,
    display_id: Option<u32>,
) -> Result<u32, String> {
    let displays = display_targets();
    if displays.is_empty() {
        return Err("No displays detected".to_string());
    }
    if let Some(id) = display_id {
        if displays.iter().any(|t| match t {
            Target::Display(d) => d.id == id,
            _ => false,
        }) {
            return Ok(id);
        }
    }
    if let Ok(monitors) = app.available_monitors() {
        if let Ok(Some(primary)) = app.primary_monitor() {
            let idx = monitors.iter().position(|m| {
                m.position() == primary.position() && m.size() == primary.size()
            });
            if let Some(idx) = idx {
                if let Some(Target::Display(d)) = displays.get(idx) {
                    return Ok(d.id);
                }
            }
        }
    }
    if let Some(Target::Display(d)) = displays.first() {
        return Ok(d.id);
    }
    Err("No displays detected".to_string())
}

pub fn create_display_capturer(display_id: u32, fps: u32) -> Result<Capturer, String> {
    let target = display_targets()
        .into_iter()
        .find(|t| match t {
            Target::Display(d) => d.id == display_id,
            _ => false,
        })
        .ok_or_else(|| format!("display {display_id} not found"))?;
    let options = scap::capturer::Options {
        fps: fps.max(1),
        output_type: scap::frame::FrameType::BGRAFrame,
        target: Some(target),
        show_cursor: false,
        ..Default::default()
    };
    let mut capturer = Capturer::build(options)
        .map_err(|e| format!("capturer init failed (screen permission missing?): {e}"))?;
    capturer.start_capture();
    Ok(capturer)
}

pub fn grab_from(capturer: &Capturer) -> Result<RgbaImage, String> {
    match capturer.get_next_frame() {
        Ok(Frame::BGRA(f)) => {
            let (w, h) = (f.width.max(0) as u32, f.height.max(0) as u32);
            Ok(bgra_to_rgba(&f.data, w, h))
        }
        Ok(_) => Err("Unsupported frame type".to_string()),
        Err(e) => Err(format!("capture failed: {e:?}")),
    }
}

/// Single-shot full-display grab (physical pixels).
pub fn grab_display(display_id: u32) -> Result<RgbaImage, String> {
    let mut capturer = create_display_capturer(display_id, 30)?;
    grab_from(&mut capturer)
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
