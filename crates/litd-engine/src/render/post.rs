//! HDR post-processing plan, bloom pyramid, tonemapping, and color grading.

use serde::{Deserialize, Serialize};

use crate::{EngineError, math::Vec3};

pub const POST_WGSL: &str = include_str!("post.wgsl");

#[path = "post_gpu.rs"]
mod post_gpu;
#[path = "post_gpu_image.rs"]
mod post_gpu_image;

pub use post_gpu::{
    PostGpuFrame, PostGpuInputImage, PostGpuStats, render_post_process_gpu,
    render_post_process_gpu_with_context,
};
pub use post_gpu_image::{PostGpuImage, PostGpuMetrics};

#[cfg(test)]
#[path = "post_gpu_tests.rs"]
mod post_gpu_tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HdrFormat {
    Rgba16Float,
    Rgba32Float,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TonemapOperator {
    Aces,
    Reinhard,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BloomSettings {
    pub enabled: bool,
    pub threshold: f32,
    pub knee: f32,
    pub intensity: f32,
    pub mip_count: u32,
}

impl Default for BloomSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 1.0,
            knee: 0.5,
            intensity: 0.22,
            mip_count: 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColorGrade {
    pub lut_size: u32,
    pub lut_strength: f32,
    pub saturation: f32,
    pub contrast: f32,
    pub lift: Vec3,
    pub gamma: Vec3,
    pub gain: Vec3,
}

impl Default for ColorGrade {
    fn default() -> Self {
        Self {
            lut_size: 32,
            lut_strength: 0.65,
            saturation: 1.12,
            contrast: 1.08,
            lift: Vec3::new(0.015, 0.01, 0.0),
            gamma: Vec3::splat(1.0),
            gain: Vec3::new(1.04, 1.02, 0.98),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PostProcessSettings {
    pub hdr_format: HdrFormat,
    pub exposure_ev: f32,
    pub tonemap: TonemapOperator,
    pub bloom: BloomSettings,
    pub color_grade: ColorGrade,
    pub output_gamma: f32,
}

impl Default for PostProcessSettings {
    fn default() -> Self {
        Self {
            hdr_format: HdrFormat::Rgba16Float,
            exposure_ev: 0.0,
            tonemap: TonemapOperator::Aces,
            bloom: BloomSettings::default(),
            color_grade: ColorGrade::default(),
            output_gamma: 2.2,
        }
    }
}

impl PostProcessSettings {
    pub fn validate(&self) -> Result<(), EngineError> {
        if !self.exposure_ev.is_finite() || !(-16.0..=16.0).contains(&self.exposure_ev) {
            return invalid_post("exposure_ev", "must be finite and inside -16..=16");
        }
        if !self.output_gamma.is_finite() || self.output_gamma <= 0.0 {
            return invalid_post("output_gamma", "must be finite and greater than zero");
        }
        validate_bloom(self.bloom)?;
        validate_grade(self.color_grade)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PostProcessPlan {
    pub settings: PostProcessSettings,
    pub width: u32,
    pub height: u32,
    pub passes: Vec<PostProcessPass>,
}

impl PostProcessPlan {
    pub fn downsample_pass_count(&self) -> usize {
        self.passes
            .iter()
            .filter(|pass| matches!(pass.kind, PostPassKind::BloomDownsample { .. }))
            .count()
    }

    pub fn upsample_pass_count(&self) -> usize {
        self.passes
            .iter()
            .filter(|pass| matches!(pass.kind, PostPassKind::BloomUpsample { .. }))
            .count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostProcessPass {
    pub kind: PostPassKind,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostPassKind {
    BloomPrefilter,
    BloomDownsample { mip: u32 },
    BloomUpsample { mip: u32 },
    CompositeTonemapGrade,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PostProcessResult {
    pub exposed_hdr: Vec3,
    pub bloom: Vec3,
    pub tonemapped: Vec3,
    pub graded: Vec3,
    pub output: Vec3,
}

pub fn build_post_process_plan(
    settings: PostProcessSettings,
    width: u32,
    height: u32,
) -> Result<PostProcessPlan, EngineError> {
    settings.validate()?;
    if width == 0 || height == 0 {
        return invalid_post("viewport", "width and height must be non-zero");
    }
    validate_mip_count(settings.bloom, width, height)?;

    let mut passes = Vec::new();
    if settings.bloom.enabled {
        passes.push(PostProcessPass {
            kind: PostPassKind::BloomPrefilter,
            width,
            height,
        });
        for mip in 1..settings.bloom.mip_count {
            passes.push(PostProcessPass {
                kind: PostPassKind::BloomDownsample { mip },
                width: mip_size(width, mip),
                height: mip_size(height, mip),
            });
        }
        for mip in (1..settings.bloom.mip_count).rev() {
            passes.push(PostProcessPass {
                kind: PostPassKind::BloomUpsample { mip },
                width: mip_size(width, mip - 1),
                height: mip_size(height, mip - 1),
            });
        }
    }
    passes.push(PostProcessPass {
        kind: PostPassKind::CompositeTonemapGrade,
        width,
        height,
    });

    Ok(PostProcessPlan {
        settings,
        width,
        height,
        passes,
    })
}

pub fn apply_post_process_reference(
    settings: PostProcessSettings,
    hdr_color: Vec3,
) -> Result<PostProcessResult, EngineError> {
    settings.validate()?;
    if !hdr_color.is_finite() || hdr_color.cmplt(Vec3::ZERO).any() {
        return invalid_post("hdr_color", "must be finite and non-negative");
    }
    let exposed_hdr = hdr_color * 2.0_f32.powf(settings.exposure_ev);
    let bloom = bloom_prefilter(exposed_hdr, settings.bloom) * settings.bloom.intensity;
    let tonemapped = tonemap(settings.tonemap, exposed_hdr + bloom);
    let graded = apply_color_grade(tonemapped, settings.color_grade);
    let output = gamma_encode(graded, settings.output_gamma);
    Ok(PostProcessResult {
        exposed_hdr,
        bloom,
        tonemapped,
        graded,
        output,
    })
}

pub fn tonemap(operator: TonemapOperator, color: Vec3) -> Vec3 {
    match operator {
        TonemapOperator::Aces => {
            let a = 2.51;
            let b = 0.03;
            let c = 2.43;
            let d = 0.59;
            let e = 0.14;
            ((color * (color * a + Vec3::splat(b)))
                / (color * (color * c + Vec3::splat(d)) + Vec3::splat(e)))
            .clamp(Vec3::ZERO, Vec3::ONE)
        }
        TonemapOperator::Reinhard => color / (Vec3::ONE + color),
        TonemapOperator::None => color.clamp(Vec3::ZERO, Vec3::ONE),
    }
}

pub fn bloom_prefilter(color: Vec3, settings: BloomSettings) -> Vec3 {
    if !settings.enabled {
        return Vec3::ZERO;
    }
    let brightness = color.max_element();
    let knee = settings.knee.max(0.0001);
    let soft = ((brightness - settings.threshold + knee)
        .clamp(0.0, knee * 2.0)
        .powi(2))
        / (4.0 * knee);
    let contribution =
        (brightness - settings.threshold).max(soft).max(0.0) / brightness.max(0.0001);
    color * contribution
}

pub fn apply_color_grade(color: Vec3, grade: ColorGrade) -> Vec3 {
    let lifted = (color + grade.lift).max(Vec3::ZERO);
    let corrected = Vec3::new(
        lifted.x.powf(1.0 / grade.gamma.x),
        lifted.y.powf(1.0 / grade.gamma.y),
        lifted.z.powf(1.0 / grade.gamma.z),
    ) * grade.gain;
    let luma = corrected.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    let saturated = Vec3::splat(luma).lerp(corrected, grade.saturation);
    let contrasted = (saturated - Vec3::splat(0.5)) * grade.contrast + Vec3::splat(0.5);
    color
        .lerp(contrasted, grade.lut_strength)
        .clamp(Vec3::ZERO, Vec3::ONE)
}

fn validate_bloom(bloom: BloomSettings) -> Result<(), EngineError> {
    if !bloom.threshold.is_finite() || bloom.threshold < 0.0 {
        return invalid_post("bloom.threshold", "must be finite and non-negative");
    }
    if !bloom.knee.is_finite() || !(0.0..=4.0).contains(&bloom.knee) {
        return invalid_post("bloom.knee", "must be finite and inside 0..=4");
    }
    if !bloom.intensity.is_finite() || !(0.0..=10.0).contains(&bloom.intensity) {
        return invalid_post("bloom.intensity", "must be finite and inside 0..=10");
    }
    if bloom.mip_count == 0 || bloom.mip_count > 12 {
        return invalid_post("bloom.mip_count", "must be in the range 1..=12");
    }
    Ok(())
}

fn validate_grade(grade: ColorGrade) -> Result<(), EngineError> {
    if grade.lut_size < 2 || grade.lut_size > 64 {
        return invalid_post("color_grade.lut_size", "must be in the range 2..=64");
    }
    if !grade.lut_strength.is_finite() || !(0.0..=1.0).contains(&grade.lut_strength) {
        return invalid_post(
            "color_grade.lut_strength",
            "must be finite and inside 0..=1",
        );
    }
    if !grade.saturation.is_finite() || !(0.0..=4.0).contains(&grade.saturation) {
        return invalid_post("color_grade.saturation", "must be finite and inside 0..=4");
    }
    if !grade.contrast.is_finite() || !(0.0..=4.0).contains(&grade.contrast) {
        return invalid_post("color_grade.contrast", "must be finite and inside 0..=4");
    }
    if !grade.lift.is_finite() || !grade.gamma.is_finite() || !grade.gain.is_finite() {
        return invalid_post(
            "color_grade.vectors",
            "lift, gamma, and gain must be finite",
        );
    }
    if grade.gamma.cmple(Vec3::ZERO).any() {
        return invalid_post("color_grade.gamma", "components must be greater than zero");
    }
    Ok(())
}

fn validate_mip_count(bloom: BloomSettings, width: u32, height: u32) -> Result<(), EngineError> {
    let longest = width.max(height).max(1);
    let max_mips = u32::BITS - longest.leading_zeros();
    if bloom.enabled && bloom.mip_count > max_mips {
        return invalid_post(
            "bloom.mip_count",
            "requested more mips than the viewport supports",
        );
    }
    Ok(())
}

fn mip_size(size: u32, mip: u32) -> u32 {
    (size >> mip).max(1)
}

fn gamma_encode(color: Vec3, gamma: f32) -> Vec3 {
    Vec3::new(
        color.x.powf(1.0 / gamma),
        color.y.powf(1.0 / gamma),
        color.z.powf(1.0 / gamma),
    )
    .clamp(Vec3::ZERO, Vec3::ONE)
}

fn invalid_post<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::PostProcessInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bloom_plan_has_down_and_up_sample_passes() {
        let plan = build_post_process_plan(PostProcessSettings::default(), 1920, 1080).unwrap();
        assert_eq!(plan.passes.len(), 10);
        assert_eq!(plan.downsample_pass_count(), 4);
        assert_eq!(plan.upsample_pass_count(), 4);
        assert!(matches!(
            plan.passes.last().unwrap().kind,
            PostPassKind::CompositeTonemapGrade
        ));
    }

    #[test]
    fn aces_tonemap_keeps_hdr_color_in_display_range() {
        let result =
            apply_post_process_reference(PostProcessSettings::default(), Vec3::new(4.0, 1.0, 0.25))
                .unwrap();
        assert!(result.output.x <= 1.0);
        assert!(result.output.x > result.output.z);
        assert!(result.bloom.length() > 0.0);
    }

    #[test]
    fn invalid_lut_size_fails_loud() {
        let mut settings = PostProcessSettings::default();
        settings.color_grade.lut_size = 1;
        match settings.validate() {
            Err(EngineError::PostProcessInvalidConfig { field, .. }) => {
                assert_eq!(field, "color_grade.lut_size");
            }
            other => panic!("expected PostProcessInvalidConfig, got {other:?}"),
        }
    }
}
