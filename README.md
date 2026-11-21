# SnapPress

[![Build SnapPress](https://github.com/CJ-1981/snappress/actions/workflows/build.yml/badge.svg)](https://github.com/CJ-1981/snappress/actions/workflows/build.yml)

A Screenpresso-style screen capture, scrolling capture and recording studio,
rebuilt in Rust with Tauri 2 for **Windows** and **macOS**.

- Region and fullscreen capture through a transparent, always-on-top overlay
- Scrolling capture that auto-scrolls a page and smart-stitches it into one tall image
- Screen recording to MP4 (H.264) with GIF export from the library
- Built-in annotation editor (arrows, shapes, pen, text, blur/pixelate, numbered
  steps, highlight, crop, undo/redo)
- Capture library with thumbnails, clipboard copy, reveal-in-folder and delete
- Global hotkeys plus a system tray menu; the main window hides to tray on close

## Downloads

Every push to `main` runs CI on GitHub Actions and produces installers for
macOS (Apple Silicon + Intel) and Windows x64 — grab them from the workflow
run's **Artifacts** section. Official releases are published automatically when
a `v*` tag (e.g. `v0.1.0`) is pushed.

## Repository layout

```
snappress/
  index.html / overlay.html   Two webview entries: main window + capture overlay
  src/                        React UI (Vite + TypeScript + Tailwind 4)
    lib/                      Typed invoke wrappers + shared types
    views/                    Capture view, library view, canvas editor
    components/               Recorder bar, scroll progress card, settings dialog
    RegionOverlay.tsx         Region selection overlay (separate window)
  src-tauri/
    src/capture/              scap-backed display grab + scrolling stitcher
    src/encode/               ffmpeg pipe recorder (MP4) + GIF converter
    src/input/                enigo cursor move + wheel scroll injection
    src/commands/             Tauri command handlers
    src/state.rs              App state: settings, library index, job handles
    capabilities/             Tauri 2 permission set
  scripts/gen_icons.py        Regenerates the icon set from app-icon.png
```

## Prerequisites

| Platform | Requirements |
| --- | --- |
| Both | Rust 1.77+ (`rustup`), Node.js 18+ (npm), first recording downloads ffmpeg (~80 MB) automatically |
| Windows | Windows 10 1903+ (DirectX Graphics Capture), WebView2 runtime (auto-installed by the Tauri bundler), MSVC Build Tools |
| macOS | macOS 12.3+ (ScreenCaptureKit), Xcode Command Line Tools |

macOS permissions (granted on first use, System Settings > Privacy & Security):

- **Screen Recording** - required for any capture/recording
- **Accessibility** - required for scrolling capture (simulated wheel scroll)

## Run and build

```bash
cd snappress
npm install
npm run tauri dev     # development build with hot reload
npm run tauri build   # production bundles (dmg/app on macOS, msi/nsis on Windows)
```

The UI can also be built standalone (`npm run ui:build`) for typechecking and
Vite output; the Tauri CLI wires it into the desktop app.

## Global hotkeys

| Action | Windows | macOS |
| --- | --- | --- |
| Region capture | `PrintScreen` | `Cmd+Shift+6` |
| Scrolling capture | `Ctrl+Alt+S` | `Cmd+Alt+S` |
| Record screen | `Ctrl+Alt+V` | `Cmd+Alt+V` |
| Stop recording | `Ctrl+Alt+X` | `Cmd+Alt+X` |

Hotkey registration failures (for example a conflicting macOS binding) are
logged to the console and do not prevent startup.

## How the pieces work

### Region capture
`start_region_capture` creates a borderless, transparent, always-on-top window
sized to the chosen monitor. The overlay page measures the dragged rectangle in
CSS pixels, converts to physical pixels with `devicePixelRatio`, and invokes
`capture_region`. The backend closes the overlay, grabs a full frame from
`scap` (DirectX Graphics Capture on Windows, ScreenCaptureKit on macOS) and
crops the saved PNG.

### Scrolling capture
`run_scroll_session` (in `src-tauri/src/capture/stitch.rs`) moves the cursor
into the region, then loops: scroll the wheel down with `enigo`, wait, grab a
new frame, and align it against the stitched tail using per-row luminance
profiles of four vertical bands (coarse scan + local refinement). Only rows
below the detected overlap are appended. The loop stops when the page stops
moving (bottom reached), the max height is hit, or the user cancels. Progress
events drive the live preview card in the main window.

### Recording (MP4 + GIF)
`run_record` spawns ffmpeg through `ffmpeg-sidecar` (auto-download on first
run) and pipes raw RGBA frames cropped from each display frame into it
(`-f rawvideo -pix_fmt rgba` -> libx264 -> faststart MP4). Pause/resume simply
gates the frame pump; stopping drops stdin so ffmpeg finalizes the container.
GIF export runs the two-pass `palettegen`/`paletteuse` filter chain over the
recorded MP4 with configurable FPS and width.

### Editor
Pure canvas implementation in `src/views/EditorView.tsx` + `src/views/editor/
render.ts`. Layers are drawn over the base image in image-pixel space; blur and
pixelate are implemented with double `drawImage` down/up-scaling (no
`ctx.filter`, which macOS WebKit does not support). The base image is loaded
through `read_image_base64` so the canvas never becomes tainted, which keeps
PNG/JPEG export working. Exports are saved back into the library as new items.

## Settings

Stored in the app data directory as `settings.json` (library index:
`index.json`, captures: `captures/`):

- Recording FPS (15/24/30), GIF FPS and GIF width (used on export)
- Scroll delay (ms), wheel ticks per step, max page height

## Known limitations (v1)

- Display-to-monitor mapping matches physical sizes, so two identical monitors
  may be swapped in multi-monitor setups.
- Fixed headers/sidebars inside scrollable pages can produce ghosting in
  stitched captures (typical for this class of tool).
- Recording is MP4 only; GIF is produced by converting a recording.
- No per-window capture mode yet (regions cover that use case).

## Troubleshooting

- **Black frames or "capture failed" on macOS** - grant Screen Recording and
  restart the app.
- **"failed to scroll" on macOS** - grant Accessibility permission.
- **"ffmpeg has no stdin" / spawn errors** - make sure the first-run ffmpeg
  download completed; delete the `ffmpeg-exe` folder inside the app data
  directory to force a re-download.
- **Crate API drift** - the Rust ecosystem crates used here are pinned by
  minor version (`scap 0.16`, `enigo 0.2`, `ffmpeg-sidecar 1.1`,
  `image 0.25`, `tauri 2`). If a dependency publishes breaking changes, pin
  the exact patch version from `Cargo.lock` or adjust the small surface area
  in `capture/mod.rs` (frame access), `input/mod.rs` (scroll sign/units) and
  `encode/mod.rs` (ffmpeg arguments).
