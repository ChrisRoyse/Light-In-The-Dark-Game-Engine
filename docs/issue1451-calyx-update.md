# Issue 1451 Calyx Update Evidence

Date: 2026-07-08

## Source

- Calyx source repository: `ChrisRoyse/Calyx-Dev`
- Imported ref: `origin/main`
- Imported commit: `9167683b02a12cc8492c99547df3b42ca4e8fbb3`
- Imported subject: `Fail closed on propose-lens measurement errors`
- Import method: clean `git archive` from `C:\code\Calyx-Dev` into the vendored `Calyx\` tree.
- Remote readback: `git ls-remote https://github.com/ChrisRoyse/Calyx-Dev.git refs/heads/main` returned `9167683b02a12cc8492c99547df3b42ca4e8fbb3`.

## Integration Notes

- Replaced stale fixed Calyx MCP tool lists with live registry readback from `calyx-mcp`.
- Updated GameEditor structured capability coverage to match the current 31-tool Calyx MCP surface.
- Rebased object/gameplay structured ingest onto current Calyx primitives and durable Aster readback.
- Removed GameEditor uses of Calyx APIs deleted by the upstream refresh.

## Verification

- `cargo check -p litd-editor-mcp --features calyx-live-calls` passed.
- `cargo check -p litd-editor --all-targets` passed. The remaining warnings are support-example dead code from compiling helper files standalone.
- `cargo test -p litd-editor-mcp --features calyx-live-calls calyx_surface -- --nocapture` passed: 10 passed, 0 failed.
- `cargo test -p litd-editor structured_capability_coverage -- --nocapture` passed: 5 passed, 0 failed.
- `cargo test -p litd-editor mcp_object_tools -- --nocapture` passed: 3 passed, 0 failed.
- `cargo run -p litd-editor --features fsv --example issue1390_editor_reactive_notifications_fsv` passed and produced source-of-truth Aster readback.
- `scripts/check-format.ps1` passed: `FORMAT_GATE_OK all_steps=26`.
- `cargo check -p litd-engine --lib` passed.
- `cargo check --manifest-path Calyx\Cargo.toml -p calyx-cli` passed for the final `9167683b02a12cc8492c99547df3b42ca4e8fbb3` import.

## Artifact Readback

- Side-store eradication gate:
  - Path: `C:\code\GameEditor\target\fsv\calyx-side-store-eradication\side-store-eradication-20260707-234805-557.json`
  - SHA256: `8F1DF2E1B1BBEAA4242D481F7041CB06025471F8AB7D5460FB2EBE7633BD623E`
- Issue 1358 object/gameplay Calyx FSV:
  - Path: `C:\code\GameEditor\target\fsv\issue1358_object_gameplay_calyx\runs\run-1783485119331756700-17420-1\fsv\issue1358-object-gameplay-calyx-readback.json`
  - SHA256: `ADB36C39B165978556AC4B7D97773B3FDB22287F358C2E76EEFC1D976ACEF3DD`
- Issue 1390 reactive notifications FSV:
  - Path: `C:\code\GameEditor\target\fsv\issue1390_editor_reactive_notifications\runs\run-1783485135082117000-73792-1\manual\issue1390-editor-reactive-notifications-readback.json`
  - SHA256: `200D6006ABCD9F939D8CE2FA7976C8C212D8905B31392E994D095E06B81DFA06`
- Synapse manual workspace capture:
  - Window HWND: `41491342`
  - Window title: `scenario.rs - GameEditor - Visual Studio Code`
  - Path: `C:\code\GameEditor\target\fsv\issue1451_calyx_update\manual\synapse-gameeditor-vscode-20260708.png`
  - SHA256: `17a49c406f6d6f1db28faa45ed060e3a61207de660123de0028bda82edf617c2`
  - Manual inspection: bitmap shows the live GameEditor VS Code workspace with the `Calyx`, `crates`, `docs`, and `target` trees visible.
