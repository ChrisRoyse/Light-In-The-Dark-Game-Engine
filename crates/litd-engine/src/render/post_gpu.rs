use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::{EngineError, gpu::GpuContext};

use super::{
    HdrFormat, PostGpuImage, PostGpuMetrics, PostPassKind, PostProcessPlan, PostProcessSettings,
    build_post_process_plan, post_gpu_image,
};

#[path = "post_gpu_state.rs"]
mod post_gpu_state;
#[path = "post_gpu_wgpu.rs"]
mod post_gpu_wgpu;

use post_gpu_state::*;
use post_gpu_wgpu::*;

const MAX_TARGET_PIXELS: u64 = 4096 * 4096;
const F16_MAX_FINITE: f32 = 65_504.0;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct PostSettingsGpu {
    exposure_ev: f32,
    bloom_threshold: f32,
    bloom_knee: f32,
    bloom_intensity: f32,
    lut_strength: f32,
    saturation: f32,
    contrast: f32,
    output_gamma: f32,
    tonemap_operator: u32,
    bloom_enabled: u32,
    width: u32,
    height: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct PostGradeGpu {
    lift: [f32; 4],
    gamma: [f32; 4],
    gain: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostGpuInputImage {
    pub width: u32,
    pub height: u32,
    pixels: Vec<[f32; 4]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostGpuFrame {
    pub image: PostGpuImage,
    pub stats: PostGpuStats,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostGpuStats {
    pub gpu_context_id: u64,
    pub adapter_name: String,
    pub backend: String,
    pub source_texture_format: String,
    pub bloom_texture_format: String,
    pub target_texture_format: String,
    pub hdr_format: HdrFormat,
    pub width: u32,
    pub height: u32,
    pub plan_pass_count: usize,
    pub bloom_prefilter_pass_count: usize,
    pub bloom_downsample_pass_count: usize,
    pub bloom_upsample_pass_count: usize,
    pub composite_pass_count: usize,
    pub bloom_enabled: bool,
    pub bloom_mip_count: u32,
    pub exposure_ev: f32,
    pub tonemap_operator: String,
    pub input_max_hdr_value: f32,
    pub input_bloom_candidate_ratio: f32,
    pub draw_calls: u32,
    pub executed_pass_labels: Vec<String>,
    pub readback_bytes: usize,
    pub row_pitch: u32,
    pub image_checksum: String,
    pub metrics: PostGpuMetrics,
}

impl PostGpuInputImage {
    pub fn from_hdr_pixels(
        width: u32,
        height: u32,
        pixels: Vec<[f32; 4]>,
    ) -> Result<Self, EngineError> {
        let input = Self {
            width,
            height,
            pixels,
        };
        validate_input(&input)?;
        Ok(input)
    }

    pub fn synthetic_hdr_scene(width: u32, height: u32) -> Result<Self, EngineError> {
        if width == 0 || height == 0 {
            return invalid_post_gpu(
                "post_gpu.synthetic_size",
                "width and height must be non-zero",
            );
        }
        let mut pixels = Vec::with_capacity((width * height) as usize);
        let center = (width as f32 * 0.44, height as f32 * 0.48);
        let blue_center = (width as f32 * 0.70, height as f32 * 0.42);
        for y in 0..height {
            for x in 0..width {
                let nx = x as f32 / width as f32;
                let ny = y as f32 / height as f32;
                let base = [
                    0.04 + nx * 0.12,
                    0.05 + ny * 0.10,
                    0.08 + (1.0 - nx) * 0.08,
                    1.0,
                ];
                let ember = radial((x as f32, y as f32), center, width as f32 * 0.18);
                let arcane = radial((x as f32, y as f32), blue_center, width as f32 * 0.14);
                pixels.push([
                    base[0] + ember * 4.2 + arcane * 0.25,
                    base[1] + ember * 1.05 + arcane * 0.85,
                    base[2] + ember * 0.22 + arcane * 3.4,
                    1.0,
                ]);
            }
        }
        Self::from_hdr_pixels(width, height, pixels)
    }

    pub fn pixels(&self) -> &[[f32; 4]] {
        &self.pixels
    }

    pub fn write_bmp(&self, path: impl AsRef<std::path::Path>) -> Result<(), EngineError> {
        PostGpuImage::from_hdr_visualization(self.width, self.height, &self.pixels).write_bmp(path)
    }
}

pub async fn render_post_process_gpu(
    settings: PostProcessSettings,
    input: &PostGpuInputImage,
) -> Result<PostGpuFrame, EngineError> {
    let context = GpuContext::new_headless().await?;
    render_post_process_gpu_with_context(&context, settings, input).await
}

pub async fn render_post_process_gpu_with_context(
    context: &GpuContext,
    settings: PostProcessSettings,
    input: &PostGpuInputImage,
) -> Result<PostGpuFrame, EngineError> {
    validate_input(input)?;
    validate_input_for_format(settings.hdr_format, input)?;
    let plan = build_post_process_plan(settings, input.width, input.height)?;
    validate_plan_for_gpu(&plan)?;
    let device = context.device();
    let queue = context.queue();
    let source_format = source_format(settings.hdr_format);
    let bloom_format = wgpu::TextureFormat::Rgba16Float;
    let target_format = wgpu::TextureFormat::Rgba8Unorm;
    let source = source_texture(device, input, source_format);
    let source_bytes = source_bytes(input, settings.hdr_format);
    let source_row_pitch = source_row_pitch(input.width, settings.hdr_format);
    queue.write_texture(
        source.as_image_copy(),
        &source_bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(source_row_pitch),
            rows_per_image: Some(input.height),
        },
        texture_extent(input.width, input.height),
    );

    let target = target_texture(device, input.width, input.height, target_format);
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let source_view = source.create_view(&wgpu::TextureViewDescriptor::default());
    let settings_buffer = buffer_init(
        device,
        "litd-post-settings",
        bytemuck::bytes_of(&settings_gpu(settings, input.width, input.height)),
        wgpu::BufferUsages::UNIFORM,
    );
    let grade_buffer = buffer_init(
        device,
        "litd-post-grade",
        bytemuck::bytes_of(&grade_gpu(settings)),
        wgpu::BufferUsages::UNIFORM,
    );
    let bind_layout = bind_group_layout(device);
    let pipelines = post_pipelines(device, &bind_layout, bloom_format, target_format);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-post-encoder"),
    });
    let executed_pass_labels = render_post_chain(RenderPostChainArgs {
        device,
        encoder: &mut encoder,
        bind_layout: &bind_layout,
        pipelines: &pipelines,
        source_view: &source_view,
        target_view: &target_view,
        settings: &settings_buffer,
        grade: &grade_buffer,
        plan: &plan,
        bloom_format,
    });
    let (readback, row_pitch, readback_bytes) =
        copy_target_to_readback(device, &mut encoder, &target, input.width, input.height);
    queue.submit([encoder.finish()]);

    let pixels = map_image_readback(device, &readback, input.width, input.height, row_pitch)?;
    let image = PostGpuImage::from_pixels(input.width, input.height, pixels);
    let metrics = post_gpu_image::measure(&image);
    let stats = stats(StatsArgs {
        context,
        plan: &plan,
        settings,
        input,
        source_format,
        bloom_format,
        target_format,
        executed_pass_labels,
        row_pitch,
        readback_bytes,
        image: &image,
        metrics,
    });
    Ok(PostGpuFrame { image, stats })
}

fn validate_input(input: &PostGpuInputImage) -> Result<(), EngineError> {
    if input.width == 0 || input.height == 0 {
        return invalid_post_gpu("post_gpu.viewport", "width and height must be non-zero");
    }
    let pixel_count = input.width as u64 * input.height as u64;
    if pixel_count > MAX_TARGET_PIXELS {
        return invalid_post_gpu("post_gpu.viewport", "width * height must be <= 4096 * 4096");
    }
    if input.pixels.len() != pixel_count as usize {
        return invalid_post_gpu(
            "post_gpu.pixels",
            format!(
                "expected {pixel_count} HDR pixels, received {}",
                input.pixels.len()
            ),
        );
    }
    if input
        .pixels
        .iter()
        .flatten()
        .any(|component| !component.is_finite() || *component < 0.0)
    {
        return invalid_post_gpu(
            "post_gpu.pixels",
            "HDR pixels must be finite and non-negative",
        );
    }
    Ok(())
}

fn validate_input_for_format(
    format: HdrFormat,
    input: &PostGpuInputImage,
) -> Result<(), EngineError> {
    if format == HdrFormat::Rgba16Float
        && input
            .pixels
            .iter()
            .flatten()
            .any(|component| *component > F16_MAX_FINITE)
    {
        return invalid_post_gpu(
            "post_gpu.pixels",
            "Rgba16Float HDR pixels must fit inside finite IEEE f16 range",
        );
    }
    Ok(())
}

fn validate_plan_for_gpu(plan: &PostProcessPlan) -> Result<(), EngineError> {
    if plan.width as u64 * plan.height as u64 > MAX_TARGET_PIXELS {
        return invalid_post_gpu("post_gpu.viewport", "width * height must be <= 4096 * 4096");
    }
    Ok(())
}

struct StatsArgs<'a> {
    context: &'a GpuContext,
    plan: &'a PostProcessPlan,
    settings: PostProcessSettings,
    input: &'a PostGpuInputImage,
    source_format: wgpu::TextureFormat,
    bloom_format: wgpu::TextureFormat,
    target_format: wgpu::TextureFormat,
    executed_pass_labels: Vec<String>,
    row_pitch: u32,
    readback_bytes: usize,
    image: &'a PostGpuImage,
    metrics: PostGpuMetrics,
}

fn stats(args: StatsArgs<'_>) -> PostGpuStats {
    let StatsArgs {
        context,
        plan,
        settings,
        input,
        source_format,
        bloom_format,
        target_format,
        executed_pass_labels,
        row_pitch,
        readback_bytes,
        image,
        metrics,
    } = args;
    let source_metrics = source_metrics(input, settings.bloom.threshold);
    PostGpuStats {
        gpu_context_id: context.context_id(),
        adapter_name: context.adapter_report().name.clone(),
        backend: context.adapter_report().backend.clone(),
        source_texture_format: format!("{source_format:?}"),
        bloom_texture_format: format!("{bloom_format:?}"),
        target_texture_format: format!("{target_format:?}"),
        hdr_format: settings.hdr_format,
        width: plan.width,
        height: plan.height,
        plan_pass_count: plan.passes.len(),
        bloom_prefilter_pass_count: pass_count(plan, |kind| {
            matches!(kind, PostPassKind::BloomPrefilter)
        }),
        bloom_downsample_pass_count: plan.downsample_pass_count(),
        bloom_upsample_pass_count: plan.upsample_pass_count(),
        composite_pass_count: pass_count(plan, |kind| {
            matches!(kind, PostPassKind::CompositeTonemapGrade)
        }),
        bloom_enabled: settings.bloom.enabled,
        bloom_mip_count: settings.bloom.mip_count,
        exposure_ev: settings.exposure_ev,
        tonemap_operator: format!("{:?}", settings.tonemap),
        input_max_hdr_value: source_metrics.0,
        input_bloom_candidate_ratio: source_metrics.1,
        draw_calls: executed_pass_labels.len() as u32,
        executed_pass_labels,
        readback_bytes,
        row_pitch,
        image_checksum: image.checksum(),
        metrics,
    }
}

fn pass_count(plan: &PostProcessPlan, predicate: impl Fn(PostPassKind) -> bool) -> usize {
    plan.passes
        .iter()
        .filter(|pass| predicate(pass.kind))
        .count()
}

fn source_metrics(input: &PostGpuInputImage, bloom_threshold: f32) -> (f32, f32) {
    let mut max_hdr = 0.0_f32;
    let mut bloom_candidates = 0_u32;
    for pixel in input.pixels() {
        let rgb_max = pixel[0].max(pixel[1]).max(pixel[2]);
        max_hdr = max_hdr.max(rgb_max);
        if rgb_max > bloom_threshold {
            bloom_candidates += 1;
        }
    }
    (
        max_hdr,
        bloom_candidates as f32 / input.pixels().len().max(1) as f32,
    )
}

fn radial(point: (f32, f32), center: (f32, f32), radius: f32) -> f32 {
    let dx = point.0 - center.0;
    let dy = point.1 - center.1;
    (1.0 - (dx * dx + dy * dy).sqrt() / radius).clamp(0.0, 1.0)
}

fn invalid_post_gpu<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(post_gpu_error(field, reason))
}

fn post_gpu_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::PostProcessInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}
