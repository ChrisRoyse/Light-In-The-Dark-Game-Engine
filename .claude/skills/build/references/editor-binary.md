# Editor UI is binary code, not the lib

`crates/litd-editor/src/main.rs` is the editor BINARY. `EditorShellRuntime` and every
`main_editor_*.rs`, `main_egui_renderer.rs`, `main_*.rs` module are `mod`'d from `main.rs`, so
they are compiled ONLY when the binary is compiled.

`crates/litd-editor/Cargo.toml` declares two bins:
- `litd-editor` (path `src/main.rs`) -- the editor
- `litd-editor-mcp-stdio` (path `src/bin/litd-editor-mcp-stdio.rs`)

`lib.rs` contains only the lib modules (object_manager.rs, calyx_probe_cache.rs,
offline_degradation.rs, the `*_cache` structs, etc.).

## Consequence

`cargo build -p litd-editor --lib` and `cargo test -p litd-editor --lib` do NOT compile the
editor UI. A cache/helper struct in a lib module can pass `--lib` tests while its wiring into
`EditorShellRuntime` is broken and the binary does not link.

This burned #1461: new cache fields were wired into `EditorShellRuntime` and panel draw loops
(all binary-only), every check was `--lib` (green), but the binary failed: an index-cache struct
derived `Default` but not `Clone`, and `EditorShellRuntime` derives `Clone`.

## Rules

- After ANY change to `EditorShellRuntime`, a `main_editor_*.rs`, or `main_egui_renderer.rs`,
  verify with `cargo build -p litd-editor --bin litd-editor` (and
  `cargo test -p litd-editor --bin litd-editor <filter>` for bin `#[cfg(test)]` tests).
- `EditorShellRuntime` is `#[derive(Debug, Clone)]`. Any new field type MUST derive
  `Debug + Clone`. For non-Clone async state use `Arc<Mutex<_>>` (see the calyx_probe_cache
  pattern).

## FSV example binaries

`autoexamples = false`; FSV examples are gated behind the `fsv` feature and declared explicitly,
so a routine `cargo test` never compiles/links them (#1497). Run one with
`cargo run -p litd-editor --features fsv --example <name>`.
