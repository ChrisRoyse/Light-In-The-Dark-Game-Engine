//! FSR 3.1-style temporal upscaler: the reconstruction pipeline for the produced-
//! game reduced-resolution render path (#1741, FSR-first per #1152, default ON per
//! #928).
//!
//! Reconstructs an output resolution (e.g. 1080p) from a reduced internal render
//! resolution (e.g. 720p) with temporal accumulation + robust-contrast sharpening.
//!
//! Inputs are CALLER-SUPPLIED, not sourced from `render::screen_space`: each
//! [`UpscaleGpuInput`] carries the render-resolution color, the render-resolution
//! screen-space motion vectors, and the prior-frame output history that the caller
//! provides. The world renderer does not yet emit per-pixel motion vectors, so the
//! only current caller of the GPU reconstruction pass is the #1741 FSV harness,
//! which feeds a deterministic synthetic scene (smooth gradients + a hard-edged
//! circle) with synthetic motion (static camera / zero motion, plus a mid-run
//! resize edge) and reads the output back from a real GPU for verification.
//!
//! Config integration (#928/#1152): [`UpscaleSettings`] is carried by the engine
//! graphics-quality config ([`crate::performance_quality::GraphicsQualitySettings`])
//! and the produced-game runtime renderer reads it to derive its internal render
//! resolution via [`build_upscale_plan`]. Hosting the GPU reconstruction pass in
//! the live per-frame frame graph (an offscreen reduced-resolution render target
//! plus real motion-vector production) is tracked as follow-up work; until then the
//! pass's reality is proven by the FSV harness's real GPU readback rather than
//! claimed as the live present path.
//!
//! wgpu-ffx compatibility finding (#1073, #1152): the `cwfitzgerald/wgpu-ffx`
//! crate is MIT-licensed, compute-only (no tensor cores), and a faithful FSR 3
//! port -- but it targets `wgpu 29` while this workspace is pinned to `wgpu 27`
//! (and `egui-wgpu 0.33`). It is therefore NOT a drop-in dependency. Per the
//! issue's own guidance ("vendor/port under its permissive license if needed"),
//! the upscale passes below are ported to wgpu-27-native WGSL rather than adding
//! an incompatible dependency that would force a workspace-wide wgpu bump.

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Vec2, Vec3},
};

pub const UPSCALE_WGSL: &str = include_str!("upscale.wgsl");

#[path = "upscale_gpu.rs"]
mod upscale_gpu;
#[path = "upscale_gpu_image.rs"]
mod upscale_gpu_image;

pub use upscale_gpu::{
    UpscaleGpuFrame, UpscaleGpuInput, UpscaleGpuStats, render_upscale_gpu,
    render_upscale_gpu_with_context,
};
pub use upscale_gpu_image::{UpscaleImage, UpscaleMetrics, psnr as psnr_for_readback};

#[cfg(test)]
#[path = "upscale_tests.rs"]
mod upscale_tests;

/// Named FSR quality presets and their per-dimension scale factors, matching the
/// FidelityFX Super Resolution 3.1 upscaler SDK preset table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpscaleQualityMode {
    /// 1.0x -- native resolution anti-aliasing (no upscale ratio).
    Native,
    /// 1.5x per dimension (720p -> 1080p). The default produced-game path.
    Quality,
    /// 1.7x per dimension.
    Balanced,
    /// 2.0x per dimension.
    Performance,
    /// 3.0x per dimension.
    UltraPerformance,
}

impl UpscaleQualityMode {
    pub const ALL: [UpscaleQualityMode; 5] = [
        UpscaleQualityMode::Native,
        UpscaleQualityMode::Quality,
        UpscaleQualityMode::Balanced,
        UpscaleQualityMode::Performance,
        UpscaleQualityMode::UltraPerformance,
    ];

    /// Per-dimension scale factor (output_dim / render_dim).
    pub fn scale_factor(self) -> f32 {
        match self {
            UpscaleQualityMode::Native => 1.0,
            UpscaleQualityMode::Quality => 1.5,
            UpscaleQualityMode::Balanced => 1.7,
            UpscaleQualityMode::Performance => 2.0,
            UpscaleQualityMode::UltraPerformance => 3.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            UpscaleQualityMode::Native => "native",
            UpscaleQualityMode::Quality => "quality",
            UpscaleQualityMode::Balanced => "balanced",
            UpscaleQualityMode::Performance => "performance",
            UpscaleQualityMode::UltraPerformance => "ultra_performance",
        }
    }
}

/// Data-driven upscaler knobs. `default ON` per #928: produced games render below
/// native and reconstruct, so `enabled` defaults to true and `mode` to `Quality`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UpscaleSettings {
    pub enabled: bool,
    pub mode: UpscaleQualityMode,
    /// RCAS sharpening strength, 0..=1.
    pub sharpness: f32,
    /// Temporal history blend weight, 0..=1 (FSR "accumulation").
    pub history_feedback: f32,
    /// Disocclusion rejection: motion magnitude (in UV) above which history is
    /// discarded to avoid ghosting, 0..=1.
    pub disocclusion_threshold: f32,
}

impl Default for UpscaleSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: UpscaleQualityMode::Quality,
            sharpness: 0.25,
            history_feedback: 0.9,
            disocclusion_threshold: 0.10,
        }
    }
}

impl UpscaleSettings {
    pub fn validate(&self) -> Result<(), EngineError> {
        if !self.sharpness.is_finite() || !(0.0..=1.0).contains(&self.sharpness) {
            return invalid_upscale("sharpness", "must be finite and inside 0..=1");
        }
        if !self.history_feedback.is_finite() || !(0.0..=1.0).contains(&self.history_feedback) {
            return invalid_upscale("history_feedback", "must be finite and inside 0..=1");
        }
        if !self.disocclusion_threshold.is_finite()
            || !(0.0..=1.0).contains(&self.disocclusion_threshold)
        {
            return invalid_upscale("disocclusion_threshold", "must be finite and inside 0..=1");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpscalePassKind {
    /// Reproject prior output through motion vectors into the current frame.
    Reproject,
    /// Reconstruct and dilate the low-resolution color into output resolution.
    ReconstructAndDilate,
    /// Temporally accumulate reconstructed color with rectified history.
    Accumulate,
    /// Robust Contrast Adaptive Sharpening (RCAS) final pass.
    RobustContrastSharpen,
}

impl UpscalePassKind {
    pub const ORDER: [UpscalePassKind; 4] = [
        UpscalePassKind::Reproject,
        UpscalePassKind::ReconstructAndDilate,
        UpscalePassKind::Accumulate,
        UpscalePassKind::RobustContrastSharpen,
    ];

    pub fn label(self) -> &'static str {
        match self {
            UpscalePassKind::Reproject => "reproject",
            UpscalePassKind::ReconstructAndDilate => "reconstruct_and_dilate",
            UpscalePassKind::Accumulate => "accumulate",
            UpscalePassKind::RobustContrastSharpen => "robust_contrast_sharpen",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpscalePlan {
    pub settings: UpscaleSettings,
    pub output_width: u32,
    pub output_height: u32,
    pub render_width: u32,
    pub render_height: u32,
    pub passes: Vec<UpscalePassKind>,
}

impl UpscalePlan {
    /// output pixels / render pixels -- the GPU headroom multiplier.
    pub fn pixel_ratio(&self) -> f32 {
        let out = self.output_width as f32 * self.output_height as f32;
        let render = self.render_width as f32 * self.render_height as f32;
        if render <= 0.0 { 0.0 } else { out / render }
    }

    pub fn is_native(&self) -> bool {
        self.render_width == self.output_width && self.render_height == self.output_height
    }
}

/// Compute the internal render resolution for an output resolution under a mode.
pub fn render_dimensions(
    mode: UpscaleQualityMode,
    output_width: u32,
    output_height: u32,
) -> Result<(u32, u32), EngineError> {
    if output_width == 0 || output_height == 0 {
        return invalid_upscale("output", "output width and height must be non-zero");
    }
    let scale = mode.scale_factor();
    let render_width = ((output_width as f32 / scale).round() as u32).max(1);
    let render_height = ((output_height as f32 / scale).round() as u32).max(1);
    Ok((render_width, render_height))
}

pub fn build_upscale_plan(
    settings: UpscaleSettings,
    output_width: u32,
    output_height: u32,
) -> Result<UpscalePlan, EngineError> {
    settings.validate()?;
    let (render_width, render_height) =
        render_dimensions(settings.mode, output_width, output_height)?;
    if render_width > output_width || render_height > output_height {
        return invalid_upscale(
            "render_resolution",
            "internal render resolution must not exceed the output resolution",
        );
    }
    Ok(UpscalePlan {
        settings,
        output_width,
        output_height,
        render_width,
        render_height,
        passes: UpscalePassKind::ORDER.to_vec(),
    })
}

/// Reproject an output-space UV backward through a screen-space motion vector to
/// the location it occupied in the previous frame's history buffer.
pub fn reproject_uv(output_uv: Vec2, motion_uv: Vec2) -> Result<Vec2, EngineError> {
    if !output_uv.is_finite() || !motion_uv.is_finite() {
        return invalid_upscale("reproject", "UV and motion must be finite");
    }
    Ok(output_uv - motion_uv)
}

/// Rectify (clamp) reprojected history to the neighborhood color box of the
/// reconstructed current frame. This is FSR's neighborhood-clamp anti-ghosting.
pub fn neighborhood_clamp(history: Vec3, box_min: Vec3, box_max: Vec3) -> Vec3 {
    history.clamp(box_min, box_max)
}

/// FSR disocclusion factor: 0 keeps history, 1 fully rejects it. History is
/// rejected on the first frame, when the reprojection samples outside the frame,
/// or when motion exceeds the threshold.
pub fn disocclusion_factor(
    first_frame: bool,
    reprojected_uv: Vec2,
    motion_uv: Vec2,
    threshold: f32,
) -> f32 {
    if first_frame
        || reprojected_uv.x < 0.0
        || reprojected_uv.x > 1.0
        || reprojected_uv.y < 0.0
        || reprojected_uv.y > 1.0
    {
        return 1.0;
    }
    if threshold <= 0.0 {
        return if motion_uv.length() > 0.0 { 1.0 } else { 0.0 };
    }
    (motion_uv.length() / threshold).clamp(0.0, 1.0)
}

/// Temporally accumulate reconstructed color with rectified history.
pub fn accumulate(
    current: Vec3,
    rectified_history: Vec3,
    feedback: f32,
    disocclusion: f32,
) -> Result<Vec3, EngineError> {
    if !current.is_finite() || !rectified_history.is_finite() {
        return invalid_upscale("accumulate", "colors must be finite");
    }
    let weight = (feedback * (1.0 - disocclusion)).clamp(0.0, 1.0);
    Ok(current
        .lerp(rectified_history, weight)
        .clamp(Vec3::ZERO, Vec3::ONE))
}

/// Robust Contrast Adaptive Sharpening. Sharpens `center` against its 4-neighbor
/// cross, clamped to the local min/max so it never overshoots the neighborhood
/// (the "robust" limiter that distinguishes RCAS from plain CAS).
pub fn rcas(
    center: Vec3,
    north: Vec3,
    east: Vec3,
    south: Vec3,
    west: Vec3,
    sharpness: f32,
) -> Vec3 {
    let amount = sharpness.clamp(0.0, 1.0);
    let mn = center.min(north).min(east).min(south).min(west);
    let mx = center.max(north).max(east).max(south).max(west);
    // High-pass = center minus the 4-neighbor mean. A per-channel contrast gate
    // suppresses sharpening in flat regions (no division, so no NaN), and the
    // final clamp to the neighborhood min/max is the RCAS "robust" limiter that
    // prevents ring/overshoot beyond the local color box.
    let blur = (north + east + south + west) * 0.25;
    let high_pass = center - blur;
    let contrast = mx - mn;
    let gate = (contrast * 4.0).clamp(Vec3::ZERO, Vec3::ONE);
    let sharpened = center + high_pass * amount * gate;
    sharpened.clamp(mn, mx)
}

fn invalid_upscale<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(upscale_error(field, reason))
}

pub(crate) fn upscale_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UpscaleInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}
