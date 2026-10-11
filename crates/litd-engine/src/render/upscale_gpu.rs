//! GPU driver for the FSR upscaler: upload low-res inputs, dispatch the compute
//! reconstruction at output resolution, and read the result back (#1741).

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::{EngineError, gpu::GpuContext};

use super::{
    UPSCALE_WGSL, UpscaleImage, UpscaleMetrics, UpscalePlan, UpscaleSettings, build_upscale_plan,
    upscale_error, upscale_gpu_image,
};

#[path = "upscale_gpu_wgpu.rs"]
mod upscale_gpu_wgpu;
use upscale_gpu_wgpu::*;

const MAX_TARGET_PIXELS: u64 = 4096 * 4096;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(super) struct UpscaleGpuUniform {
    dims: [u32; 4],
    params: [f32; 4],
}

/// Low-resolution scene inputs plus prior-frame history for one upscaled frame.
#[derive(Debug, Clone, PartialEq)]
pub struct UpscaleGpuInput {
    pub render_width: u32,
    pub render_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    /// Render-resolution color, normalized RGBA in 0..=1.
    pub color: Vec<[f32; 4]>,
    /// Render-resolution screen-space motion vectors (UV space), each in -1..=1.
    pub motion: Vec<[f32; 2]>,
    /// Output-resolution prior frame output. Ignored when `first_frame`.
    pub history: Vec<[u8; 4]>,
    pub first_frame: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpscaleGpuFrame {
    pub output_image: UpscaleImage,
    pub stats: UpscaleGpuStats,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpscaleGpuStats {
    pub gpu_context_id: u64,
    pub adapter_name: String,
    pub backend: String,
    pub shader_bytes: usize,
    pub output_texture_format: String,
    pub render_width: u32,
    pub render_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub quality_mode: String,
    pub scale_factor: f32,
    pub pixel_ratio: f32,
    pub is_native: bool,
    pub first_frame: bool,
    pub pass_labels: Vec<String>,
    pub readback_bytes: usize,
    pub row_pitch: u32,
    pub output_checksum: String,
    pub output_metrics: UpscaleMetrics,
}

pub async fn render_upscale_gpu(
    settings: UpscaleSettings,
    input: &UpscaleGpuInput,
) -> Result<UpscaleGpuFrame, EngineError> {
    let context = GpuContext::new_headless().await?;
    render_upscale_gpu_with_context(&context, settings, input).await
}

pub async fn render_upscale_gpu_with_context(
    context: &GpuContext,
    settings: UpscaleSettings,
    input: &UpscaleGpuInput,
) -> Result<UpscaleGpuFrame, EngineError> {
    let plan = build_upscale_plan(settings, input.output_width, input.output_height)?;
    validate_input(input, &plan)?;
    let device = context.device();
    let queue = context.queue();
    let format = wgpu::TextureFormat::Rgba8Unorm;

    let textures = upload_textures(device, queue, input)?;
    let uniform = uniform_buffer(device, &uniform(&plan, input.first_frame));
    let layout = bind_group_layout(device);
    let pipeline = pipeline(device, &layout);
    let bind_group = bind_group(device, &layout, &textures, &uniform);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-upscale-encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("litd-upscale-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(
            input.output_width.div_ceil(8),
            input.output_height.div_ceil(8),
            1,
        );
    }
    let readback = copy_output_to_readback(
        device,
        &mut encoder,
        &textures.output,
        input.output_width,
        input.output_height,
    );
    queue.submit([encoder.finish()]);

    let (pixels, row_pitch, readback_bytes) =
        read_output(device, readback, input.output_width, input.output_height)?;
    let output_image = UpscaleImage::from_pixels(input.output_width, input.output_height, pixels);
    let output_metrics = upscale_gpu_image::measure(&output_image);
    let stats = UpscaleGpuStats {
        gpu_context_id: context.context_id(),
        adapter_name: context.adapter_report().name.clone(),
        backend: context.adapter_report().backend.clone(),
        shader_bytes: UPSCALE_WGSL.len(),
        output_texture_format: format!("{format:?}"),
        render_width: plan.render_width,
        render_height: plan.render_height,
        output_width: plan.output_width,
        output_height: plan.output_height,
        quality_mode: settings.mode.label().to_string(),
        scale_factor: settings.mode.scale_factor(),
        pixel_ratio: plan.pixel_ratio(),
        is_native: plan.is_native(),
        first_frame: input.first_frame,
        pass_labels: plan.passes.iter().map(|p| p.label().to_string()).collect(),
        readback_bytes,
        row_pitch,
        output_checksum: output_image.checksum(),
        output_metrics,
    };
    Ok(UpscaleGpuFrame {
        output_image,
        stats,
    })
}

fn validate_input(input: &UpscaleGpuInput, plan: &UpscalePlan) -> Result<(), EngineError> {
    if input.render_width != plan.render_width || input.render_height != plan.render_height {
        return Err(upscale_error(
            "input.render_resolution",
            format!(
                "input render {}x{} does not match plan render {}x{} for the selected quality mode",
                input.render_width, input.render_height, plan.render_width, plan.render_height
            ),
        ));
    }
    let out_pixels = input.output_width as u64 * input.output_height as u64;
    if out_pixels == 0 || out_pixels > MAX_TARGET_PIXELS {
        return Err(upscale_error(
            "input.output_resolution",
            "output width*height must be non-zero and <= 4096*4096",
        ));
    }
    let render_pixels = (input.render_width as u64 * input.render_height as u64) as usize;
    if input.color.len() != render_pixels || input.motion.len() != render_pixels {
        return Err(upscale_error(
            "input.buffers",
            format!("color and motion buffers must each contain {render_pixels} render pixels"),
        ));
    }
    if !input.first_frame && input.history.len() != out_pixels as usize {
        return Err(upscale_error(
            "input.history",
            format!(
                "history buffer must contain {out_pixels} output pixels when first_frame is false"
            ),
        ));
    }
    if input
        .color
        .iter()
        .flatten()
        .any(|c| !c.is_finite() || !(0.0..=1.0).contains(c))
    {
        return Err(upscale_error(
            "input.color",
            "color pixels must be finite normalized RGBA",
        ));
    }
    if input
        .motion
        .iter()
        .flatten()
        .any(|m| !m.is_finite() || !(-1.0..=1.0).contains(m))
    {
        return Err(upscale_error(
            "input.motion",
            "motion vectors must be finite and inside -1..=1",
        ));
    }
    Ok(())
}

fn uniform(plan: &UpscalePlan, first_frame: bool) -> UpscaleGpuUniform {
    UpscaleGpuUniform {
        dims: [
            plan.render_width,
            plan.render_height,
            plan.output_width,
            plan.output_height,
        ],
        params: [
            plan.settings.sharpness,
            plan.settings.history_feedback,
            plan.settings.disocclusion_threshold,
            if first_frame { 1.0 } else { 0.0 },
        ],
    }
}
