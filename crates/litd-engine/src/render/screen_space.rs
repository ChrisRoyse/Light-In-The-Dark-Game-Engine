//! Screen-space SSAO, anti-aliasing, motion vectors, TAA, and stylized SSR.

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Vec2, Vec3},
};

pub const SCREEN_SPACE_WGSL: &str = include_str!("screen_space.wgsl");

#[path = "screen_space_gpu.rs"]
mod screen_space_gpu;
#[path = "screen_space_gpu_image.rs"]
mod screen_space_gpu_image;
#[path = "screen_space_gpu_input.rs"]
mod screen_space_gpu_input;

pub use screen_space_gpu::{
    ScreenSpaceGpuFrame, ScreenSpaceGpuStats, render_screen_space_gpu,
    render_screen_space_gpu_with_context,
};
pub use screen_space_gpu_image::{ScreenSpaceGpuImage, ScreenSpaceGpuMetrics};
pub use screen_space_gpu_input::ScreenSpaceGpuInputImage;

#[cfg(test)]
#[path = "screen_space_gpu_tests.rs"]
mod screen_space_gpu_tests;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SsaoSettings {
    pub enabled: bool,
    pub kernel_size: u32,
    pub radius: f32,
    pub bias: f32,
    pub power: f32,
    pub noise_tile_size: u32,
}

impl Default for SsaoSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            kernel_size: 16,
            radius: 0.6,
            bias: 0.025,
            power: 1.5,
            noise_tile_size: 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AntiAliasingSettings {
    pub msaa_samples: u32,
    pub fxaa_enabled: bool,
    pub taa_enabled: bool,
    pub jitter_sequence_len: u32,
    pub taa_feedback: f32,
    pub motion_rejection: f32,
}

impl Default for AntiAliasingSettings {
    fn default() -> Self {
        Self {
            msaa_samples: 1,
            fxaa_enabled: true,
            taa_enabled: true,
            jitter_sequence_len: 8,
            taa_feedback: 0.9,
            motion_rejection: 0.65,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SsrSettings {
    pub enabled: bool,
    pub max_steps: u32,
    pub thickness: f32,
    pub intensity: f32,
}

impl Default for SsrSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            max_steps: 48,
            thickness: 0.12,
            intensity: 0.35,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ScreenSpaceSettings {
    pub ssao: SsaoSettings,
    pub anti_aliasing: AntiAliasingSettings,
    pub ssr: SsrSettings,
}

impl ScreenSpaceSettings {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_ssao(self.ssao)?;
        validate_aa(self.anti_aliasing)?;
        validate_ssr(self.ssr)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScreenSpacePlan {
    pub settings: ScreenSpaceSettings,
    pub width: u32,
    pub height: u32,
    pub passes: Vec<ScreenSpacePass>,
}

impl ScreenSpacePlan {
    pub fn pass_count(&self, kind: ScreenSpacePassKind) -> usize {
        self.passes.iter().filter(|pass| pass.kind == kind).count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenSpacePass {
    pub kind: ScreenSpacePassKind,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenSpacePassKind {
    MsaaResolve,
    SsaoGather,
    SsaoBlur,
    MotionVectors,
    StylizedSsr,
    TaaResolve,
    Fxaa,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameJitter {
    pub frame_index: u64,
    pub sample_index: u32,
    pub pixel_offset: Vec2,
    pub ndc_offset: Vec2,
}

pub fn build_screen_space_plan(
    settings: ScreenSpaceSettings,
    width: u32,
    height: u32,
) -> Result<ScreenSpacePlan, EngineError> {
    settings.validate()?;
    if width == 0 || height == 0 {
        return invalid_screen("viewport", "width and height must be non-zero");
    }

    let mut passes = Vec::new();
    if settings.anti_aliasing.msaa_samples > 1 {
        passes.push(pass(ScreenSpacePassKind::MsaaResolve, width, height));
    }
    if settings.ssao.enabled {
        passes.push(pass(ScreenSpacePassKind::SsaoGather, width, height));
        passes.push(pass(ScreenSpacePassKind::SsaoBlur, width, height));
    }
    if settings.anti_aliasing.taa_enabled {
        passes.push(pass(ScreenSpacePassKind::MotionVectors, width, height));
    }
    if settings.ssr.enabled {
        passes.push(pass(ScreenSpacePassKind::StylizedSsr, width, height));
    }
    if settings.anti_aliasing.taa_enabled {
        passes.push(pass(ScreenSpacePassKind::TaaResolve, width, height));
    }
    if settings.anti_aliasing.fxaa_enabled {
        passes.push(pass(ScreenSpacePassKind::Fxaa, width, height));
    }

    Ok(ScreenSpacePlan {
        settings,
        width,
        height,
        passes,
    })
}

pub fn frame_jitter(
    aa: AntiAliasingSettings,
    frame_index: u64,
    width: u32,
    height: u32,
) -> Result<FrameJitter, EngineError> {
    validate_aa(aa)?;
    if width == 0 || height == 0 {
        return invalid_screen("viewport", "width and height must be non-zero");
    }
    let sample_index = (frame_index % aa.jitter_sequence_len as u64) as u32 + 1;
    let pixel_offset = Vec2::new(halton(sample_index, 2) - 0.5, halton(sample_index, 3) - 0.5);
    let ndc_offset = Vec2::new(
        pixel_offset.x * 2.0 / width as f32,
        -pixel_offset.y * 2.0 / height as f32,
    );
    Ok(FrameJitter {
        frame_index,
        sample_index,
        pixel_offset,
        ndc_offset,
    })
}

pub fn motion_vector_uv(current_ndc: Vec2, previous_ndc: Vec2) -> Result<Vec2, EngineError> {
    if !current_ndc.is_finite() || !previous_ndc.is_finite() {
        return invalid_screen("motion_vector", "NDC positions must be finite");
    }
    Ok((current_ndc - previous_ndc) * 0.5)
}

pub fn ssao_reference(
    settings: SsaoSettings,
    sample_depth: f32,
    sample_normal: Vec3,
    neighbor_depths: &[f32],
) -> Result<f32, EngineError> {
    validate_ssao(settings)?;
    if !settings.enabled {
        return Ok(1.0);
    }
    if !sample_depth.is_finite() || sample_depth < 0.0 {
        return invalid_screen("ssao.sample_depth", "must be finite and non-negative");
    }
    if !sample_normal.is_finite() || sample_normal.length_squared() <= f32::EPSILON {
        return invalid_screen("ssao.sample_normal", "must be finite and non-zero");
    }
    if neighbor_depths.is_empty() {
        return invalid_screen("ssao.neighbor_depths", "must contain at least one sample");
    }
    let mut occlusion = 0.0;
    for &neighbor in neighbor_depths {
        if !neighbor.is_finite() || neighbor < 0.0 {
            return invalid_screen(
                "ssao.neighbor_depths",
                "samples must be finite and non-negative",
            );
        }
        let delta = sample_depth - neighbor;
        if delta > settings.bias {
            occlusion += (delta / settings.radius).clamp(0.0, 1.0);
        }
    }
    let average = occlusion / neighbor_depths.len() as f32;
    Ok((1.0 - average).clamp(0.0, 1.0).powf(settings.power))
}

pub fn taa_resolve(
    aa: AntiAliasingSettings,
    current_color: Vec3,
    history_color: Vec3,
    motion_vector: Vec2,
) -> Result<Vec3, EngineError> {
    validate_aa(aa)?;
    if !current_color.is_finite()
        || !history_color.is_finite()
        || current_color.cmplt(Vec3::ZERO).any()
        || history_color.cmplt(Vec3::ZERO).any()
        || !motion_vector.is_finite()
    {
        return invalid_screen(
            "taa",
            "colors must be finite and non-negative, and motion vector must be finite",
        );
    }
    let rejection = (motion_vector.length() * aa.motion_rejection).clamp(0.0, 1.0);
    let history_weight = aa.taa_feedback * (1.0 - rejection);
    Ok(current_color
        .lerp(history_color, history_weight)
        .clamp(Vec3::ZERO, Vec3::ONE))
}

fn validate_ssao(ssao: SsaoSettings) -> Result<(), EngineError> {
    if ssao.kernel_size < 4 || ssao.kernel_size > 64 {
        return invalid_screen("ssao.kernel_size", "must be in the range 4..=64");
    }
    if !ssao.radius.is_finite() || ssao.radius <= 0.0 {
        return invalid_screen("ssao.radius", "must be finite and greater than zero");
    }
    if !ssao.bias.is_finite() || ssao.bias < 0.0 {
        return invalid_screen("ssao.bias", "must be finite and non-negative");
    }
    if !ssao.power.is_finite() || ssao.power <= 0.0 {
        return invalid_screen("ssao.power", "must be finite and greater than zero");
    }
    if ssao.noise_tile_size < 2 || ssao.noise_tile_size > 16 {
        return invalid_screen("ssao.noise_tile_size", "must be in the range 2..=16");
    }
    Ok(())
}

fn validate_aa(aa: AntiAliasingSettings) -> Result<(), EngineError> {
    if !matches!(aa.msaa_samples, 1 | 2 | 4 | 8) {
        return invalid_screen("aa.msaa_samples", "must be one of 1, 2, 4, or 8");
    }
    if aa.jitter_sequence_len == 0 || aa.jitter_sequence_len > 64 {
        return invalid_screen("aa.jitter_sequence_len", "must be in the range 1..=64");
    }
    if !aa.taa_feedback.is_finite() || !(0.0..=1.0).contains(&aa.taa_feedback) {
        return invalid_screen("aa.taa_feedback", "must be finite and inside 0..=1");
    }
    if !aa.motion_rejection.is_finite() || !(0.0..=4.0).contains(&aa.motion_rejection) {
        return invalid_screen("aa.motion_rejection", "must be finite and inside 0..=4");
    }
    Ok(())
}

fn validate_ssr(ssr: SsrSettings) -> Result<(), EngineError> {
    if ssr.enabled && (ssr.max_steps == 0 || ssr.max_steps > 128) {
        return invalid_screen(
            "ssr.max_steps",
            "must be in the range 1..=128 when SSR is enabled",
        );
    }
    if !ssr.thickness.is_finite() || ssr.thickness <= 0.0 {
        return invalid_screen("ssr.thickness", "must be finite and greater than zero");
    }
    if !ssr.intensity.is_finite() || !(0.0..=1.0).contains(&ssr.intensity) {
        return invalid_screen("ssr.intensity", "must be finite and inside 0..=1");
    }
    Ok(())
}

fn pass(kind: ScreenSpacePassKind, width: u32, height: u32) -> ScreenSpacePass {
    ScreenSpacePass {
        kind,
        width,
        height,
    }
}

fn halton(mut index: u32, base: u32) -> f32 {
    let mut result = 0.0;
    let mut fraction = 1.0 / base as f32;
    while index > 0 {
        result += (index % base) as f32 * fraction;
        index /= base;
        fraction /= base as f32;
    }
    result
}

fn invalid_screen<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::ScreenSpaceInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_includes_ssao_motion_taa_fxaa_and_optional_ssr() {
        let settings = ScreenSpaceSettings {
            anti_aliasing: AntiAliasingSettings {
                msaa_samples: 4,
                ..AntiAliasingSettings::default()
            },
            ssr: SsrSettings {
                enabled: true,
                ..SsrSettings::default()
            },
            ..ScreenSpaceSettings::default()
        };
        let plan = build_screen_space_plan(settings, 1920, 1080).unwrap();
        assert_eq!(plan.passes.len(), 7);
        assert_eq!(plan.pass_count(ScreenSpacePassKind::SsaoGather), 1);
        assert_eq!(plan.pass_count(ScreenSpacePassKind::MotionVectors), 1);
        assert_eq!(plan.pass_count(ScreenSpacePassKind::TaaResolve), 1);
    }

    #[test]
    fn jitter_sequence_is_deterministic() {
        let jitter = frame_jitter(AntiAliasingSettings::default(), 0, 1920, 1080).unwrap();
        assert_eq!(jitter.sample_index, 1);
        assert!((jitter.pixel_offset.x - 0.0).abs() < 0.0001);
        assert!((jitter.pixel_offset.y + 0.16666666).abs() < 0.0001);
    }

    #[test]
    fn ssao_and_taa_references_are_bounded() {
        let ao = ssao_reference(
            SsaoSettings::default(),
            0.6,
            Vec3::Z,
            &[0.55, 0.58, 0.62, 0.50, 0.70],
        )
        .unwrap();
        let motion = motion_vector_uv(Vec2::new(0.12, -0.08), Vec2::new(0.10, -0.05)).unwrap();
        let taa = taa_resolve(
            AntiAliasingSettings::default(),
            Vec3::new(0.8, 0.72, 0.6),
            Vec3::new(0.7, 0.75, 0.65),
            motion,
        )
        .unwrap();
        assert!(ao > 0.0 && ao <= 1.0);
        assert!(taa.min_element() >= 0.0 && taa.max_element() <= 1.0);
    }

    #[test]
    fn invalid_msaa_fails_loud() {
        let mut settings = ScreenSpaceSettings::default();
        settings.anti_aliasing.msaa_samples = 3;
        match settings.validate() {
            Err(EngineError::ScreenSpaceInvalidConfig { field, .. }) => {
                assert_eq!(field, "aa.msaa_samples");
            }
            other => panic!("expected ScreenSpaceInvalidConfig, got {other:?}"),
        }
    }
}
