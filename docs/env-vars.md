# Environment variables — runtime & authoring contract

The GameEditor runtime and editor read the environment variables below. This table
is the single documented source of the env contract (#1492 item 4). Each row cites
the source file that reads it so the entry is verifiable — grep the file to confirm
the exact semantics.

Conventions:
- **Unset default** — behavior when the variable is not set.
- Most `LITD_EDITOR_*` path/state variables are optional *overrides*; when unset the
  editor uses its built-in default location. They exist primarily so the headless
  FSV harnesses (#800) and agents can point subsystems at scratch directories.
- Windows-only (#713): path defaults resolve under the per-user profile.

## Logging & diagnostics

| Variable | Read by | Effect | Unset default |
|---|---|---|---|
| `LITD_LOG_FILE` | `litd-engine/src/lib.rs` | Absolute path of the editor's log file (overrides the default location entirely). Empty value fails loud. | `%LOCALAPPDATA%\LitD\logs\<app>.log` |
| `LOCALAPPDATA` | `litd-engine/src/lib.rs` | Base dir for the default log path when `LITD_LOG_FILE` is unset (Windows-standard). | **Required** if `LITD_LOG_FILE` is unset — fails loud if missing/empty (no CWD fallback, #1492). |
| `RUST_LOG` | `litd-engine/src/lib.rs` (`EnvFilter::try_from_default_env`) | Standard `tracing` filter directive. | `warn,litd_engine=info,litd_editor=info,litd_game=info` (file) / `info,*=debug` (console). |

## FSV / determinism harness (headless verification)

| Variable | Read by | Effect | Unset default |
|---|---|---|---|
| `CALYX_FSV_ROOT` | `litd-editor/src/main_editor_persistence_tests_part2.rs` | Root for bounded Calyx/editor FSV artifacts produced by the persistence verification harness. The local test wrappers set it to their owned report root. | `<workspace>\target\fsv` |
| `LITD_FSV_STATS` | `litd-editor/src/main_fsv.rs`, `main_lifecycle.rs` | Enables FSV stats readback output for headless verification. | off |
| `LITD_FSV_FRAMES` | `litd-editor/src/main_fsv.rs` | Number of frames the headless FSV run renders. | subsystem default |
| `LITD_FSV_STATS_EVERY_FRAMES` | `litd-editor/src/main_fsv.rs` | Cadence (in frames) of FSV stats emission. | subsystem default |
| `LITD_FSV_DEADLINE_SECS` | `litd-engine/src/fsv_deadline.rs` | Wall-clock self-abort deadline (whole seconds) for FSV example harnesses; the watchdog converts any hang into a loud, bounded abort (exit 101). A `0`/malformed value logs a warning and uses the default (#1712). | `300` (5 min) |
| `LITD_FAIL_LOUD_PROBE` | `litd-editor/src/fail_loud.rs` | Fail-loud probe hook for verifying error surfacing. | off |
| `LITD_FAIL_LOUD_STATS` | `litd-editor/src/fail_loud.rs` | Fail-loud stats hook. | off |
| `LITD_EDITOR_PANIC_PROBE` | `litd-editor/src/main_lifecycle.rs` | Synthetic panic trigger to verify the panic hook + FSV-stats readback. | off |
| `LITD_EDITOR_GPU_VALIDATION` | `litd-editor/src/main_egui_renderer.rs` | Diagnostics: when `1`/`true`, push+drain a GPU validation error scope every frame (serializes CPU/GPU). Off by default so the steady-state frame never blocks on a full device drain (#1485). Editor analogue of litd-game's `--gpu-validation` (#1460). | off |

## Editor mode & persistence

| Variable | Read by | Effect | Unset default |
|---|---|---|---|
| `LITD_EDITOR_MODE` | `litd-editor/src/main_editor_persistence.rs` | Selects the editor run mode. | built-in default mode |
| `LITD_EDITOR_SHELL_STATE` | `litd-editor/src/main_editor_persistence.rs` | Path to the editor shell-state store. | built-in default |
| `LITD_EDITOR_PREFERENCES` | `litd-editor/src/editor_preferences.rs` | Path to the editor preferences store. | built-in default |
| `LITD_EDITOR_AUTOSAVE_ASTER` | `litd-editor/src/editor_preferences.rs` | Path to the autosave Aster store. | built-in default |
| `LITD_EDITOR_MIN_SPEC_ASTER` | `litd-editor/src/editor_min_spec_store.rs` | Path to the Aster vault that persists editor min-spec/degrade settings. | built-in min-spec vault path |

## Editor world / asset state paths

| Variable | Read by | Effect | Unset default |
|---|---|---|---|
| `LITD_EDITOR_WORLD_ASTER` | `litd-editor/src/world_creation_store.rs` | Path to the world Aster (chunked persistence) store. | built-in default |
| `LITD_EDITOR_WORLD_PROFILE` | `litd-editor/src/main_egui_world_profile.rs` | World-profile selector/path. | built-in default |
| `LITD_EDITOR_MAP_BUILD_DIR` | `litd-editor/src/main_editor_build.rs` | Output dir for map builds. | built-in default |
| `LITD_EDITOR_MINIMAP_PROFILE` | `litd-editor/src/main_egui_minimap.rs` | Minimap profile selector/path. | built-in default |
| `LITD_EDITOR_VFX_STORE_ROOT` | `litd-editor/src/vfx_editor.rs` | Root dir for the VFX content store. | built-in default |
| `LITD_EDITOR_FOLIAGE_BRUSH_STATE` | `litd-editor/src/asset_scene_foliage_brush.rs` | Path to foliage-brush state. | built-in default |
| `LITD_EDITOR_FOLIAGE_CONFIG` | `litd-editor/src/asset_scene_foliage_config.rs` | Path to foliage config. | built-in default |
| `LITD_LOCAL_ASSET_VAULT` | `litd-engine/src/asset_vault_root.rs` (`LOCAL_ASSET_VAULT_ENV`) | Authoritative override for the local-asset-vault root (the large, gitignored asset directory). When set it wins over the auto-resolved primary-checkout location; used to point linked git worktrees / CI at a real vault (#1543). Surfaced in `diagnostics_remediation.rs` remediation text when the vault is missing. | auto-resolved to the primary checkout's `local-asset-vault` (via `git rev-parse --git-common-dir`) |
| `LITD_REPO_ROOT` | `litd-engine/src/asset_vault_root.rs` (`REPO_ROOT_ENV`, read by `resolve_repo_root`) | Authoritative override for the PRIMARY GameEditor checkout root that tests/tooling resolve exe paths, scripts dirs, and the asset vault against (#1559). When set it wins over the auto-resolved primary checkout; must be an existing directory (a set-but-empty value or a non-directory path fails loud). Lets exported (no-git) checkouts or unusual layouts name their own root without hardcoding a machine-specific path. | auto-resolved to the primary checkout via `git rev-parse --path-format=absolute --git-common-dir`, then the cargo workspace root; fails loud if no candidate exists |

## AI co-author & authoring intelligence

| Variable | Read by | Effect | Unset default |
|---|---|---|---|
| `LITD_EDITOR_AI_COAUTHOR_ASTER` | `litd-editor/src/main_editor_ai_coauthor.rs` | Path to the AI co-author Aster store. | built-in default |
| `LITD_EDITOR_AI_COAUTHOR_STATE` | `litd-editor/src/main_editor_ai_coauthor.rs` | Path to the AI co-author state store. | built-in default |
| `LITD_EDITOR_INTELLIGENCE_ASTER_DIR` | `litd-editor/src/main_editor_intelligence.rs` | Dir for the authoring-intelligence Aster. | built-in default |
| `LITD_EDITOR_INTELLIGENCE_DECK_ID` | `litd-editor/src/main_editor_intelligence.rs` | Deck id for the authoring-intelligence panel. | built-in default |

## GUI moderation action inputs

Read by `litd-editor/src/main_editor_moderation.rs`. These feed a moderation action's
fields for headless/scripted invocation; when unset the corresponding field is empty
and the action validates/fails loud accordingly.

| Variable | Effect | Unset default |
|---|---|---|
| `LITD_GUI_MODERATION_SEVERITY` | Severity for a moderation action. | empty |
| `LITD_GUI_MODERATION_CONTENT_ADDRESS` | SHA-256 content address the action targets. | empty |
| `LITD_GUI_MODERATION_EVIDENCE_TEXT` | Evidence text attached to the action. | empty |

## MCP server — path confinement (security)

Unlike the optional `LITD_EDITOR_*` overrides above, this is a **required** trust
boundary for the loopback MCP deputy: it anchors path confinement so a poisoned tool
argument cannot become an arbitrary-filesystem primitive (#1487, #1518). It fails loud
when unset — there is deliberately no fallback.

| Variable | Read by | Effect | Unset default |
|---|---|---|---|
| `LITD_EDITOR_PROJECT_ROOT` | `litd-editor/src/mcp_project_tools_validation.rs` (`resolve_project_root`); `litd-editor/src/bin/litd-editor-mcp-stdio.rs` | Absolute path of the trusted project root that **every** filesystem-path argument to the editor MCP tools must resolve within. Whatever launches `litd-editor-mcp-stdio` MUST set it. | **Required** for editor MCP filesystem tools — fails loud (canonicalized + must be an existing directory; no CWD/implicit fallback). |

## Calyx / ONNX Runtime

| Variable | Read by | Effect | Unset default |
|---|---|---|---|
| `CALYX_HOME` | `litd-editor/src/offline_degradation.rs` | Calyx home dir. | built-in default |
| `LITD_ENGINE_CALYX_CONTENT_PANEL_VAULT` | `litd-engine/src/calyx_paths.rs` | Vault path for the engine Calyx content-measurement panel. | built-in default |
| `LITD_CALYX_AI_ADDON` | `litd-engine/src/calyx_ai_addon_gate.rs` | Controls heavy embedder/encoder/ORT content measurement for editor/dev cycles (#1790/#1792). `auto` (default): try CPU-explicit panel, unmaterialized vault → pending (#1791/#1890). `0`/`off`/`absent`: force addon-absent pending panel (fast cycles, no ORT). `1`/`on`/`require`: fail loud with `LITD_CALYX_AI_ADDON_REQUIRED_UNAVAILABLE` if panel/CURRENT/lenses unavailable (no silent pending). | `auto` |
| `ORT_DYLIB_PATH` | Calyx (ORT `load-dynamic`) | Absolute path to a compatible ONNX Runtime dynamic library. **Required** to load the CPU-explicit lens the editor test gate needs (#1493). | none — fails loud (`CALYX_LENS_UNREACHABLE`) if the lens is loaded without it |
| `CALYX_ORT_CAPI` | Calyx | Directory of the ONNX Runtime C API (alternative to `ORT_DYLIB_PATH`). | none |
| `LITD_CALYX_MCP_BIN` | `litd-editor-mcp/src/calyx_sidecar_types.rs` | Explicit path to the installed `calyx-mcp.exe` child-process addon used by the optional sidecar transport. A configured path must name an existing file; missing configuration or a missing file fails with a typed sidecar error. | unset — sidecar resolution fails loud (`LITD-CALYX-SIDECAR-BINARY-UNSET`) |

## GPU adapter selection

| Variable | Read by | Effect | Unset default |
|---|---|---|---|
| `LITD_GPU_ADAPTER` | `litd-engine/src/gpu_adapter.rs` | Selects adapter ranking: `auto`/`discrete` prefers discrete, `integrated` prefers integrated, and `force-integrated` requires an integrated adapter. Unknown or empty values fail loud. | `discrete-preferred` |

> Kept in sync with source. To verify any row: `grep -rn "\"<VAR>\"" crates/*/src`.
> ORT setup is also covered in `AGENTS.md` (#1493).
