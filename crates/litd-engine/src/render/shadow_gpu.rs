use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

#[path = "shadow_gpu_image.rs"]
mod shadow_gpu_image;
#[path = "shadow_gpu_render.rs"]
mod shadow_gpu_render;

pub use shadow_gpu_image::ShadowGpuImage;
use shadow_gpu_render::{
    StatsArgs, bind_group, bind_group_layout, copy_depth_to_readback, depth_atlas,
    depth_attachment, depth_param_buffer, map_depth_readback, pipeline, stats,
    validate_plan_for_gpu,
};

use crate::{EngineError, gpu::GpuContext, render::lighting::RenderLight};

use super::{AtlasTile, ShadowCamera, ShadowSettings, build_shadow_plan};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ShadowDepthParamsGpu {
    depth_pad: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShadowGpuFrame {
    pub image: ShadowGpuImage,
    pub stats: ShadowGpuStats,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShadowGpuStats {
    pub gpu_context_id: u64,
    pub adapter_name: String,
    pub backend: String,
    pub texture_format: String,
    pub atlas_size: u32,
    pub tile_size: u32,
    pub entry_count: usize,
    pub directional_cascade_count: usize,
    pub point_face_count: usize,
    pub spot_map_count: usize,
    pub cascade_splits: Vec<ShadowCascadeReadback>,
    pub tile_depths: Vec<ShadowTileReadback>,
    pub shadow_pixel_count: usize,
    pub shadow_pixel_ratio: f32,
    pub min_shadow_depth: f32,
    pub max_shadow_depth: f32,
    pub draw_calls: u32,
    pub readback_bytes: usize,
    pub row_pitch: u32,
    pub image_checksum: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShadowCascadeReadback {
    pub cascade_index: u32,
    pub split_near: f32,
    pub split_far: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShadowTileReadback {
    pub entry_index: usize,
    pub light_index: usize,
    pub light_name: String,
    pub kind: String,
    pub tile: AtlasTile,
    pub shadow_pixel_count: usize,
    pub min_depth: f32,
    pub max_depth: f32,
}

pub async fn render_shadow_maps_gpu(
    settings: ShadowSettings,
    camera: ShadowCamera,
    lights: &[RenderLight],
) -> Result<ShadowGpuFrame, EngineError> {
    let context = GpuContext::new_headless().await?;
    render_shadow_maps_gpu_with_context(&context, settings, camera, lights).await
}

pub async fn render_shadow_maps_gpu_with_context(
    context: &GpuContext,
    settings: ShadowSettings,
    camera: ShadowCamera,
    lights: &[RenderLight],
) -> Result<ShadowGpuFrame, EngineError> {
    let plan = build_shadow_plan(settings, camera, lights)?;
    validate_plan_for_gpu(&plan)?;
    let device = context.device();
    let queue = context.queue();
    let format = wgpu::TextureFormat::Depth32Float;
    let depth = depth_atlas(device, settings, format);
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_layout = bind_group_layout(device);
    let pipeline = pipeline(device, &bind_layout, format);
    let depth_param_buffers = plan
        .entries
        .iter()
        .map(|entry| depth_param_buffer(device, entry))
        .collect::<Vec<_>>();
    let bind_groups = depth_param_buffers
        .iter()
        .map(|buffer| bind_group(device, &bind_layout, buffer))
        .collect::<Vec<_>>();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-shadow-gpu-encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("litd-shadow-depth-pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(depth_attachment(&depth_view)),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        for (entry, bind_group) in plan.entries.iter().zip(bind_groups.iter()) {
            pass.set_viewport(
                entry.tile.x as f32,
                entry.tile.y as f32,
                entry.tile.size as f32,
                entry.tile.size as f32,
                0.0,
                1.0,
            );
            pass.set_scissor_rect(entry.tile.x, entry.tile.y, entry.tile.size, entry.tile.size);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
    }
    let (readback, row_pitch, readback_bytes) =
        copy_depth_to_readback(device, &mut encoder, &depth, settings);
    queue.submit([encoder.finish()]);
    let depths = map_depth_readback(device, &readback, settings, row_pitch)?;
    let image = ShadowGpuImage::from_depths(settings.atlas_size, settings.atlas_size, &depths);
    let stats = stats(StatsArgs {
        context,
        settings,
        format,
        plan: &plan,
        depths: &depths,
        row_pitch,
        readback_bytes,
        image: &image,
    })?;
    Ok(ShadowGpuFrame { image, stats })
}
