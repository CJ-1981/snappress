# Vendored scap

Vendored copy of [scap 0.0.8](https://crates.io/crates/scap/0.0.8) (MIT license, see LICENSE).

## Why vendored

scap 0.0.8's Windows engine is written against windows-capture 1.3.6, but its
`windows-capture = "1.3.6"` requirement floats to newer 1.x releases per semver.
windows-capture 1.5.0 changed `Settings::new` from 5 to 8 arguments, so scap
0.0.8 as published no longer compiles on Windows. Pinning the old version is
not enough either: 1.3.6 lacks `capture::Context` and the
`as_nopadding_buffer` method that scap 0.0.8 also uses.

## Local changes

1. `src/capturer/engine/win/mod.rs`: the two `Settings::new` call sites pass
   `SecondaryWindowSettings::Default`, `MinimumUpdateIntervalSettings::Default`
   and `DirtyRegionSettings::Default` (new arguments introduced in
   windows-capture 1.5.0), with the import extended accordingly. The workspace
   pins `windows-capture = "=1.5.0"`.
2. Failure handling so broken capture sessions surface as errors instead of
   panics or hangs:
   - `start_capture` no longer unwraps `start_free_threaded`; a failed start
     is recorded (`start_error`) and the settings — and with them the frame
     channel sender — are dropped, so frame waits fail promptly.
   - `stop_capture` no longer unwraps when the session never started.
   - `Capturer::get_next_frame` now times out after 15s per frame and returns
     a descriptive error instead of blocking forever.

Everything else matches crates.io scap 0.0.8 (non-essential files removed:
banner assets, changelog, CI config). Snappress wires it in via
`[patch.crates-io] scap = { path = "vendor/scap" }` in `../Cargo.toml`.
