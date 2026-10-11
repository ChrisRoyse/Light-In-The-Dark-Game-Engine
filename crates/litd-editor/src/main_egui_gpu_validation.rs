//! Editor per-frame GPU-validation opt-in, read from the environment.
//!
//! Extracted from `main_egui_renderer` (#1532) to keep that renderer file under the
//! <500-line ratchet (#1466) after the real per-frame allocation metric was added.

/// #1485 finding 1: per-frame GPU validation is a diagnostics aid, OFF by default so the release
/// frame never serializes CPU/GPU on a full device drain. Enable by setting
/// `LITD_EDITOR_GPU_VALIDATION` to `1`/`true` (case-insensitive) — the editor analogue of
/// litd-game's `--gpu-validation` flag (#1460). Any other value (or unset) keeps it off.
pub const EDITOR_GPU_VALIDATION_ENV: &str = "LITD_EDITOR_GPU_VALIDATION";

pub fn gpu_validation_each_frame_from_env() -> bool {
    // Note: main_egui_renderer is `#[cfg(not(test))]` (it links wgpu/winit), so this cannot carry
    // a unit test. Finding 1's fix is verified structurally — `cargo build -p litd-editor --bin
    // litd-editor` compiles the gated release path (no push/pop/poll when the flag is off) — and
    // the frame-time/GPU-drain profile FSV runs on the RTX box per #1485's FSV gate.
    match std::env::var(EDITOR_GPU_VALIDATION_ENV) {
        Ok(value) => {
            let value = value.trim();
            value == "1" || value.eq_ignore_ascii_case("true")
        }
        Err(_) => false,
    }
}
