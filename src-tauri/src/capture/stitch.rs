use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use image::RgbaImage;
use serde::Serialize;
use tauri::Emitter;

use crate::capture;
use crate::input;
use crate::library::{self, CaptureItem};

pub struct ScrollConfig {
    pub delay_ms: u64,
    pub step_ticks: i32,
    pub max_height: u32,
    pub max_frames: u32,
}

pub struct ScrollRegion {
    /// Global screen coordinates (physical px) of the region's top-left corner.
    pub cursor: (i32, i32),
    /// Region top-left relative to the display frame (physical px).
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScrollProgress {
    pub frames: u32,
    pub stitched_height: u32,
}

const BANDS: usize = 4;
const MIN_NEW_ROWS: usize = 12;
const MAX_COST: f32 = 18.0;
const EQUAL_COST: f32 = 12.0;

/// Per-row mean luminance of vertical bands, used as a cheap alignment signal.
pub struct RowProfiles {
    pub rows: usize,
    pub bands: Vec<Vec<f32>>,
}

pub fn row_profiles(img: &RgbaImage) -> RowProfiles {
    let w = img.width() as usize;
    let h = img.height() as usize;
    let mut bands = vec![vec![0.0f32; h]; BANDS];
    let band_w = (w / BANDS).max(1);
    for (y, row) in img.rows().enumerate() {
        for b in 0..BANDS {
            let x0 = b * band_w;
            let x1 = if b == BANDS - 1 { w } else { (b + 1) * band_w };
            let mut acc = 0.0f32;
            let mut n = 0.0f32;
            for x in (x0..x1).step_by(2) {
                let p = row[x];
                acc += 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
                n += 1.0;
            }
            bands[b][y] = if n > 0.0 { acc / n } else { 0.0 };
        }
    }
    RowProfiles { rows: h, bands }
}

pub fn tail_profiles(img: &RgbaImage, tail_h: u32) -> RowProfiles {
    let h = img.height();
    let t = tail_h.min(h);
    let sub = image::imageops::crop_imm(img, 0, h - t, img.width(), t).to_image();
    row_profiles(&sub)
}

pub enum Overlap {
    /// The new shot adds nothing new: the bottom of the page was reached.
    End,
    /// Rows of the new shot that are already present at the stitched tail.
    Offset(usize),
    /// No reliable alignment found (the page probably did not scroll).
    NoMatch,
}

fn cost_at(bottom: &RowProfiles, next: &RowProfiles, o: usize) -> f32 {
    if o == 0 || o > bottom.rows || o > next.rows {
        return f32::MAX;
    }
    let mut acc = 0.0f32;
    let mut cnt = 0.0f32;
    for band in 0..BANDS {
        let bv = &bottom.bands[band];
        let nv = &next.bands[band];
        let mut i = 0usize;
        while i < o {
            acc += (bv[bottom.rows - o + i] - nv[i]).abs();
            cnt += 1.0;
            i += 2;
        }
    }
    if cnt == 0.0 {
        f32::MAX
    } else {
        acc / cnt
    }
}

pub fn find_overlap(bottom: &RowProfiles, next: &RowProfiles) -> Overlap {
    let bh = bottom.rows;
    let nh = next.rows;
    if bh < MIN_NEW_ROWS + 4 || nh < MIN_NEW_ROWS + 4 {
        return Overlap::NoMatch;
    }
    // Identical shot means the content no longer moves: bottom reached.
    if bh >= nh {
        let full = cost_at(bottom, next, nh);
        if full < EQUAL_COST {
            return Overlap::End;
        }
    }
    let max_o = bh.min(nh) - MIN_NEW_ROWS;
    if max_o < MIN_NEW_ROWS {
        return Overlap::NoMatch;
    }
    // Coarse scan, then a small local refinement pass.
    let mut best_o = MIN_NEW_ROWS;
    let mut best_cost = f32::MAX;
    let mut o = MIN_NEW_ROWS;
    while o <= max_o {
        let c = cost_at(bottom, next, o);
        if c < best_cost {
            best_cost = c;
            best_o = o;
        }
        o += 4;
    }
    let lo = best_o.saturating_sub(3).max(MIN_NEW_ROWS);
    let hi = (best_o + 3).min(max_o);
    for o in lo..=hi {
        let c = cost_at(bottom, next, o);
        if c < best_cost {
            best_cost = c;
            best_o = o;
        }
    }
    if best_cost > MAX_COST {
        Overlap::NoMatch
    } else {
        Overlap::Offset(best_o)
    }
}

pub fn append_bottom(stitched: &RgbaImage, next: &RgbaImage, overlap: usize) -> RgbaImage {
    let new_rows = next.height().saturating_sub(overlap as u32);
    let mut out = RgbaImage::new(stitched.width(), stitched.height() + new_rows);
    let _ = out.copy_from(stitched, 0, 0);
    if new_rows > 0 {
        let sub =
            image::imageops::crop_imm(next, 0, overlap as u32, next.width(), new_rows).to_image();
        let _ = out.copy_from(&sub, 0, stitched.height());
    }
    out
}

/// Drive the scrolling capture: capture, scroll, align, append - until the
/// bottom is reached, the max height is hit or the user cancels.
pub fn run_scroll_session(
    app: &tauri::AppHandle,
    display_id: u32,
    region: ScrollRegion,
    cfg: ScrollConfig,
    cancel: Arc<AtomicBool>,
    preview_path: PathBuf,
) -> Result<Option<CaptureItem>, String> {
    // Put the cursor inside the region so wheel events land there.
    input::move_cursor(
        region.cursor.0 + (region.width / 2) as i32,
        region.cursor.1 + (region.height / 2) as i32,
    )?;

    let mut capturer = capture::create_display_capturer(display_id, 30)?;
    let first = capture::grab_from(&mut capturer)?;
    let mut stitched =
        capture::crop_rgba(&first, region.x, region.y, region.width, region.height)?;
    let mut bottom = row_profiles(&stitched);
    let mut frames = 1u32;
    let mut stalls = 0u32;
    let mut last_preview = 0u32;

    loop {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if stitched.height() >= cfg.max_height {
            break;
        }
        if frames >= cfg.max_frames {
            break;
        }

        input::scroll_down(cfg.step_ticks)?;
        std::thread::sleep(Duration::from_millis(cfg.delay_ms));

        let full = capture::grab_from(&mut capturer)?;
        let shot = capture::crop_rgba(&full, region.x, region.y, region.width, region.height)?;
        frames += 1;

        let profiles = row_profiles(&shot);
        match find_overlap(&bottom, &profiles) {
            Overlap::End => break,
            Overlap::NoMatch => {
                stalls += 1;
                if stalls >= 2 {
                    break;
                }
                continue;
            }
            Overlap::Offset(o) => {
                stalls = 0;
                stitched = append_bottom(&stitched, &shot, o);
                bottom = tail_profiles(&stitched, shot.height());
            }
        }

        app.emit(
            "scroll:progress",
            ScrollProgress {
                frames,
                stitched_height: stitched.height(),
            },
        )
        .ok();
        if frames - last_preview >= 3 {
            let _ = stitched.save(&preview_path);
            last_preview = frames;
        }
    }

    let _ = stitched.save(&preview_path);
    if frames < 1 {
        return Ok(None);
    }
    let item = library::save_image_to_library(app, &stitched, "scroll")?;
    Ok(Some(item))
}
