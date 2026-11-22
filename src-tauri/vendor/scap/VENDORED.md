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

The only modification is in `src/capturer/engine/win/mod.rs`: the two
`Settings::new` call sites now pass `SecondaryWindowSettings::Default`,
`MinimumUpdateIntervalSettings::Default` and `DirtyRegionSettings::Default`
(new arguments introduced in windows-capture 1.5.0), and the import was
extended accordingly. The workspace pins `windows-capture = "=1.5.0"`.

Everything else matches crates.io scap 0.0.8 (non-essential files removed:
banner assets, changelog, CI config). Snappress wires it in via
`[patch.crates-io] scap = { path = "vendor/scap" }` in `../Cargo.toml`.
