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
2. Session lifecycle. Upstream never stops a session unless the caller does:
   the capture thread then outlives the `Capturer` with a dead receiver, and
   the next frame hits `tx.send(..).expect(..)` inside the WinRT frame
   callback, which aborts the whole process. Now:
   - dropping the engine stops the session (frames are halted at once; the
     join that tears the session down runs on a helper thread, so a session
     that will not shut up cannot wedge whoever drops the capturer);
   - the frame callback never panics: failures are returned, and a dropped
     receiver ends the session;
   - a failed start is recorded (`start_error`), and bringing a session up is
     bounded by a timeout instead of blocking on the capture thread.
3. Errors. `Capturer::get_next_frame` returns `CaptureError`
   (`NotStarted` / `Timeout` / `Ended`) and takes an explicit timeout via
   `get_next_frame_with_timeout`, instead of blocking forever; the reason the
   frame handler stopped is passed back to the caller.

Everything else matches crates.io scap 0.0.8 (non-essential files removed:
banner assets, changelog, CI config). Snappress wires it in via
`[patch.crates-io] scap = { path = "vendor/scap" }` in `../Cargo.toml`.
