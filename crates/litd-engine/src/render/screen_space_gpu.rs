use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::{EngineError, gpu::GpuContext};

use super::{
    SCREEN_SPACE_WGSL, ScreenSpaceGpuImage, ScreenSpaceGpuMetrics, ScreenSpacePassKind,
    ScreenSpacePlan, ScreenSpaceSettings, build_screen_space_plan, frame_jitter,
    screen_space_gpu_image, screen_space_gpu_input::ScreenSpaceGpuInputImage,
};

#[path = "screen_space_gpu_wgpu.rs"]
mod screen_space_gpu_wgpu;

use screen_space_gpu_wgpu::*;

const MAX_TARGET_PIXELS: u64 = 4096 * 4096;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(super) struct ScreenSpaceGpuUniform {
    dims: [u32; 4],
    jitter: [f32; 4],
    ssao: [f32; 4],
    taa: [f32; 4],
    ssr: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenSpaceGpuFrame {
    pub ssao_image: Option<ScreenSpaceGpuImage>,
    pub taa_image: Option<ScreenSpaceGpuImage>,
    pub output_image: ScreenSpaceGpuImage,
    pub stats: ScreenSpaceGpuStats,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenSpaceGpuStats {
    pub gpu_context_id: u64,
    pub adapter_name: String,
    pub backend: String,
    pub shader_bytes: usize,
    pub target_texture_format: String,
    pub width: u32,
    pub height: u32,
    pub msaa_samples: u32,
    pub jitter_sequence_len: u32,
    pub jitter_sample_index: u32,
    pub jitter_pixel_offset: [f32; 2],
    pub jitter_ndc_offset: [f32; 2],
    pub plan_pass_count: usize,
    pub msaa_resolve_pass_count: usize,
    pub ssao_gather_pass_count: usize,
    pub ssao_blur_pass_count: usize,
    pub motion_vectors_pass_count: usize,
    pub stylized_ssr_pass_count: usize,
    pub taa_resolve_pass_count: usize,
    pub fxaa_pass_count: usize,
    pub ssr_enabled: bool,
    pub taa_enabled: bool,
    pub fxaa_enabled: bool,
    pub draw_calls: u32,
    pub executed_pass_labels: Vec<String>,
    pub readback_bytes: usize,
    pub row_pitch: u32,
    pub ssao_checksum: Option<String>,
    pub taa_checksum: Option<String>,
    pub output_checksum: String,
    pub ssao_metrics: Option<ScreenSpaceGpuMetrics>,
    pub taa_metrics: Option<ScreenSpaceGpuMetrics>,
    pub output_metrics: ScreenSpaceGpuMetrics,
}

pub async fn render_screen_space_gpu(
    settings: ScreenSpaceSettings,
    input: &ScreenSpaceGpuInputImage,
    frame_index: u64,
) -> Result<ScreenSpaceGpuFrame, EngineError> {
    let context = GpuContext::new_headless().await?;
    render_screen_space_gpu_with_context(&context, settings, input, frame_index).await
}

pub async fn render_screen_space_gpu_with_context(
    context: &GpuContext,
    settings: ScreenSpaceSettings,
    input: &ScreenSpaceGpuInputImage,
    frame_index: u64,
) -> Result<ScreenSpaceGpuFrame, EngineError> {
    validate_input(input)?;
    let plan = build_screen_space_plan(settings, input.width, input.height)?;
    validate_plan_for_gpu(&plan)?;
    let jitter = frame_jitter(
        settings.anti_aliasing,
        frame_index,
        input.width,
        input.height,
    )?;
    let device = context.device();
    let queue = context.queue();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let textures = upload_textures(device, queue, input);
    let bind_layout = bind_group_layout(device);
    let uniform = uniform_buffer(
        device,
        &uniform(settings, input.width, input.height, jitter),
    );
    let pipelines = pipelines(device, &bind_layout, format);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-screen-space-encoder"),
    });
    let mut current = TextureSlot::Source;
    let mut ssao_readback = None;
    let mut taa_readback = None;
    let mut executed_pass_labels = Vec::with_capacity(plan.passes.len());
    for pass in &plan.passes {
        current = render_pass(RenderPassArgs {
            device,
            encoder: &mut encoder,
            layout: &bind_layout,
            pipelines: &pipelines,
            textures: &textures,
            uniform: &uniform,
            pass_kind: pass.kind,
            current,
        });
        executed_pass_labels.push(label_for_pass(pass.kind).to_string());
        if pass.kind == ScreenSpacePassKind::SsaoBlur {
            ssao_readback = Some(copy_slot_to_readback(
                device,
                &mut encoder,
                &textures,
                current,
                input.width,
                input.height,
                "litd-screen-space-ssao-readback",
            ));
        }
        if pass.kind == ScreenSpacePassKind::TaaResolve {
            taa_readback = Some(copy_slot_to_readback(
                device,
                &mut encoder,
                &textures,
                current,
                input.width,
                input.height,
                "litd-screen-space-taa-readback",
            ));
        }
    }
    let output_readback = copy_slot_to_readback(
        device,
        &mut encoder,
        &textures,
        current,
        input.width,
        input.height,
        "litd-screen-space-output-readback",
    );
    queue.submit([encoder.finish()]);

    let (output_pixels, output_row_pitch, output_bytes) = read_pixels(
        device,
        output_readback,
        input.width,
        input.height,
        "screen_space.output.map",
    )?;
    let output_image = ScreenSpaceGpuImage::from_pixels(input.width, input.height, output_pixels);
    let ssao_image = optional_image(
        device,
        ssao_readback,
        input.width,
        input.height,
        "screen_space.ssao.map",
    )?;
    let taa_image = optional_image(
        device,
        taa_readback,
        input.width,
        input.height,
        "screen_space.taa.map",
    )?;
    let output_metrics = screen_space_gpu_image::measure(&output_image);
    let ssao_metrics = ssao_image.as_ref().map(screen_space_gpu_image::measure);
    let taa_metrics = taa_image.as_ref().map(screen_space_gpu_image::measure);
    let readback_bytes = output_bytes
        + ssao_image
            .as_ref()
            .map(|_| aligned_readback_bytes(input.width, input.height))
            .unwrap_or(0)
        + taa_image
            .as_ref()
            .map(|_| aligned_readback_bytes(input.width, input.height))
            .unwrap_or(0);
    let stats = stats(StatsArgs {
        context,
        plan: &plan,
        settings,
        jitter,
        format,
        executed_pass_labels,
        row_pitch: output_row_pitch,
        readback_bytes,
        ssao_image: &ssao_image,
        taa_image: &taa_image,
        output_image: &output_image,
        ssao_metrics,
        taa_metrics,
        output_metrics,
    });
    Ok(ScreenSpaceGpuFrame {
        ssao_image,
        taa_image,
        output_image,
        stats,
    })
}

pub(super) fn validate_input(input: &ScreenSpaceGpuInputImage) -> Result<(), EngineError> {
    if input.width == 0 || input.height == 0 {
        return invalid_screen_gpu(
            "screen_space_gpu.viewport",
            "width and height must be non-zero",
        );
    }
    let pixel_count = input.width as u64 * input.height as u64;
    if pixel_count > MAX_TARGET_PIXELS {
        return invalid_screen_gpu(
            "screen_space_gpu.viewport",
            "width * height must be <= 4096 * 4096",
        );
    }
    let expected = pixel_count as usize;
    if input.color_pixels().len() != expected
        || input.history_pixels().len() != expected
        || input.depth_pixels().len() != expected
        || input.motion_pixels().len() != expected
    {
        return invalid_screen_gpu(
            "screen_space_gpu.buffers",
            format!("all scene buffers must contain {expected} pixels"),
        );
    }
    if input
        .color_pixels()
        .iter()
        .chain(input.history_pixels().iter())
        .flatten()
        .any(|component| !component.is_finite() || !(0.0..=1.0).contains(component))
    {
        return invalid_screen_gpu(
            "screen_space_gpu.color",
            "color and history pixels must be finite normalized RGBA",
        );
    }
    if input
        .depth_pixels()
        .iter()
        .any(|depth| !depth.is_finite() || !(0.0..=1.0).contains(depth))
    {
        return invalid_screen_gpu(
            "screen_space_gpu.depth",
            "depth pixels must be finite normalized values",
        );
    }
    if input
        .motion_pixels()
        .iter()
        .flatten()
        .any(|motion| !motion.is_finite() || !(-1.0..=1.0).contains(motion))
    {
        return invalid_screen_gpu(
            "screen_space_gpu.motion",
            "motion vectors must be finite and inside -1..=1",
        );
    }
    Ok(())
}

fn validate_plan_for_gpu(plan: &ScreenSpacePlan) -> Result<(), EngineError> {
    if plan.passes.is_empty() {
        return invalid_screen_gpu(
            "screen_space_gpu.passes",
            "at least one screen-space pass must be enabled",
        );
    }
    Ok(())
}

fn uniform(
    settings: ScreenSpaceSettings,
    width: u32,
    height: u32,
    jitter: super::FrameJitter,
) -> ScreenSpaceGpuUniform {
    ScreenSpaceGpuUniform {
        dims: [
            width,
            height,
            settings.anti_aliasing.msaa_samples,
            jitter.sample_index,
        ],
        jitter: [
            jitter.pixel_offset.x,
            jitter.pixel_offset.y,
            jitter.ndc_offset.x,
            jitter.ndc_offset.y,
        ],
        ssao: [
            settings.ssao.radius,
            settings.ssao.bias,
            settings.ssao.power,
            settings.ssao.kernel_size as f32,
        ],
        taa: [
            settings.anti_aliasing.taa_feedback,
            settings.anti_aliasing.motion_rejection,
            0.0,
            0.0,
        ],
        ssr: [
            settings.ssr.intensity,
            settings.ssr.max_steps as f32,
            settings.ssr.thickness,
            0.0,
        ],
    }
}

fn optional_image(
    device: &wgpu::Device,
    readback: Option<(wgpu::Buffer, u32, usize)>,
    width: u32,
    height: u32,
    field: &'static str,
) -> Result<Option<ScreenSpaceGpuImage>, EngineError> {
    readback
        .map(|readback| {
            read_pixels(device, readback, width, height, field)
                .map(|(pixels, _, _)| ScreenSpaceGpuImage::from_pixels(width, height, pixels))
        })
        .transpose()
}

fn read_pixels(
    device: &wgpu::Device,
    readback: (wgpu::Buffer, u32, usize),
    width: u32,
    height: u32,
    field: &'static str,
) -> Result<(Vec<[u8; 4]>, u32, usize), EngineError> {
    let (buffer, row_pitch, readback_bytes) = readback;
    let pixels = map_image_readback(device, &buffer, width, height, row_pitch, field)?;
    Ok((pixels, row_pitch, readback_bytes))
}

struct StatsArgs<'a> {
    context: &'a GpuContext,
    plan: &'a ScreenSpacePlan,
    settings: ScreenSpaceSettings,
    jitter: super::FrameJitter,
    format: wgpu::TextureFormat,
    executed_pass_labels: Vec<String>,
    row_pitch: u32,
    readback_bytes: usize,
    ssao_image: &'a Option<ScreenSpaceGpuImage>,
    taa_image: &'a Option<ScreenSpaceGpuImage>,
    output_image: &'a ScreenSpaceGpuImage,
    ssao_metrics: Option<ScreenSpaceGpuMetrics>,
    taa_metrics: Option<ScreenSpaceGpuMetrics>,
    output_metrics: ScreenSpaceGpuMetrics,
}

fn stats(args: StatsArgs<'_>) -> ScreenSpaceGpuStats {
    let StatsArgs {
        context,
        plan,
        settings,
        jitter,
        format,
        executed_pass_labels,
        row_pitch,
        readback_bytes,
        ssao_image,
        taa_image,
        output_image,
        ssao_metrics,
        taa_metrics,
        output_metrics,
    } = args;
    ScreenSpaceGpuStats {
        gpu_context_id: context.context_id(),
        adapter_name: context.adapter_report().name.clone(),
        backend: context.adapter_report().backend.clone(),
        shader_bytes: SCREEN_SPACE_WGSL.len(),
        target_texture_format: format!("{format:?}"),
        width: plan.width,
        height: plan.height,
        msaa_samples: settings.anti_aliasing.msaa_samples,
        jitter_sequence_len: settings.anti_aliasing.jitter_sequence_len,
        jitter_sample_index: jitter.sample_index,
        jitter_pixel_offset: [jitter.pixel_offset.x, jitter.pixel_offset.y],
        jitter_ndc_offset: [jitter.ndc_offset.x, jitter.ndc_offset.y],
        plan_pass_count: plan.passes.len(),
        msaa_resolve_pass_count: pass_count(plan, ScreenSpacePassKind::MsaaResolve),
        ssao_gather_pass_count: pass_count(plan, ScreenSpacePassKind::SsaoGather),
        ssao_blur_pass_count: pass_count(plan, ScreenSpacePassKind::SsaoBlur),
        motion_vectors_pass_count: pass_count(plan, ScreenSpacePassKind::MotionVectors),
        stylized_ssr_pass_count: pass_count(plan, ScreenSpacePassKind::StylizedSsr),
        taa_resolve_pass_count: pass_count(plan, ScreenSpacePassKind::TaaResolve),
        fxaa_pass_count: pass_count(plan, ScreenSpacePassKind::Fxaa),
        ssr_enabled: settings.ssr.enabled,
        taa_enabled: settings.anti_aliasing.taa_enabled,
        fxaa_enabled: settings.anti_aliasing.fxaa_enabled,
        draw_calls: executed_pass_labels.len() as u32,
        executed_pass_labels,
        readback_bytes,
        row_pitch,
        ssao_checksum: ssao_image.as_ref().map(ScreenSpaceGpuImage::checksum),
        taa_checksum: taa_image.as_ref().map(ScreenSpaceGpuImage::checksum),
        output_checksum: output_image.checksum(),
        ssao_metrics,
        taa_metrics,
        output_metrics,
    }
}

fn pass_count(plan: &ScreenSpacePlan, kind: ScreenSpacePassKind) -> usize {
    plan.pass_count(kind)
}

fn aligned_readback_bytes(width: u32, height: u32) -> usize {
    (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) as usize
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize
        * height as usize
}

pub(super) fn invalid_screen_gpu<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(screen_gpu_error(field, reason))
}

pub(super) fn screen_gpu_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::ScreenSpaceInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}
