use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ffmpeg_sidecar::command::FfmpegCommand;
use serde::Serialize;
use tauri::Emitter;

use crate::capture;
use crate::library::{self, CaptureItem};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecordTick {
    pub elapsed_ms: u64,
    pub frames: u64,
}

/// Probe whether an ffmpeg binary is reachable (PATH or sidecar cache).
pub fn check_ffmpeg() -> bool {
    match FfmpegCommand::new().arg("-version").spawn() {
        Ok(mut child) => {
            let _ = child.kill();
            let _ = child.wait();
            log::info!("ffmpeg available");
            true
        }
        Err(_) => {
            log::info!("ffmpeg not found yet; recording will download it on first use");
            false
        }
    }
}

/// Make sure ffmpeg is usable, downloading it on demand if necessary.
pub fn ensure_ffmpeg_available() -> Result<(), String> {
    if check_ffmpeg() {
        return Ok(());
    }
    ffmpeg_sidecar::download::auto_download()
        .map_err(|e| format!("automatic ffmpeg download failed: {e}"))?;
    if !check_ffmpeg() {
        return Err("ffmpeg is still unavailable after the automatic download".to_string());
    }
    Ok(())
}

pub struct RecordJob {
    pub display_id: u32,
    /// (x, y, w, h) relative to the display frame, even dimensions.
    pub rect: (u32, u32, u32, u32),
    pub fps: u32,
    pub out: PathBuf,
}

/// Record raw screen frames to an H.264 MP4 by piping RGBA frames into ffmpeg.
pub fn run_record(
    app: tauri::AppHandle,
    job: RecordJob,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
) -> Result<Option<CaptureItem>, String> {
    ensure_ffmpeg_available()?;
    let (rx, ry, rw, rh) = job.rect;

    let args: Vec<String> = vec![
        "-y".into(),
        "-loglevel".into(),
        "error".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgba".into(),
        "-video_size".into(),
        format!("{rw}x{rh}"),
        "-framerate".into(),
        job.fps.max(1).to_string(),
        "-thread_queue_size".into(),
        "1024".into(),
        "-i".into(),
        "pipe:0".into(),
        "-an".into(),
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "veryfast".into(),
        "-tune".into(),
        "zerolatency".into(),
        "-crf".into(),
        "23".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-movflags".into(),
        "+faststart".into(),
        job.out.to_string_lossy().to_string(),
    ];
    let mut child = FfmpegCommand::new()
        .args(&args)
        .spawn()
        .map_err(|e| format!("failed to start ffmpeg: {e}"))?;
    let mut stdin = child.take_stdin().ok_or("ffmpeg has no stdin")?;
    let mut capturer = capture::create_display_capturer(job.display_id, job.fps)?;

    let start = Instant::now();
    let mut last_tick = Instant::now();
    let mut frames: u64 = 0;
    let mut buf = vec![0u8; rw as usize * rh as usize * 4];

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if paused.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(60));
            continue;
        }
        match capturer.get_next_frame() {
            Ok(scap::frame::Frame::BGRA(f)) => {
                let (fw, fh) = (f.width.max(0) as u32, f.height.max(0) as u32);
                capture::crop_bgra_into(
                    &f.data,
                    fw,
                    fh,
                    rx as i32,
                    ry as i32,
                    rw,
                    rh,
                    &mut buf,
                );
                if stdin.write_all(&buf).is_err() {
                    break;
                }
                frames += 1;
            }
            Ok(_) => {}
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
        if last_tick.elapsed() >= Duration::from_secs(1) {
            app.emit(
                "recording:tick",
                RecordTick {
                    elapsed_ms: start.elapsed().as_millis() as u64,
                    frames,
                },
            )
            .ok();
            last_tick = Instant::now();
        }
    }

    // Dropping stdin signals EOF so ffmpeg finalizes the MP4 container.
    drop(stdin);
    let _ = child.wait();

    if frames == 0 || !job.out.exists() {
        let _ = std::fs::remove_file(&job.out);
        return Ok(None);
    }
    let duration_ms = start.elapsed().as_millis() as u64;
    let item = library::register_file(
        &app,
        &job.out,
        "video",
        rw,
        rh,
        Some(duration_ms),
        Some("record"),
    )?;
    Ok(Some(item))
}

/// Convert an MP4 into an optimized GIF (palette diff mode, bayer dither).
pub fn convert_to_gif(input: &Path, out: &Path, gif_fps: u32, gif_width: u32) -> Result<(), String> {
    ensure_ffmpeg_available()?;
    let vf = format!(
        "fps={gif_fps},scale={gif_width}:-2:flags=lanczos,split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=4"
    );
    let mut child = FfmpegCommand::new()
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(input)
        .args(["-vf", &vf, "-loop", "0"])
        .arg(out)
        .spawn()
        .map_err(|e| format!("failed to start ffmpeg: {e}"))?;
    let status = child.wait().map_err(|e| format!("ffmpeg wait failed: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(out);
        return Err("ffmpeg failed to convert the video to GIF".to_string());
    }
    Ok(())
}
