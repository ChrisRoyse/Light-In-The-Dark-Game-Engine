use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

#[path = "lighting_gpu_image.rs"]
mod lighting_gpu_image;
#[path = "lighting_gpu_state.rs"]
mod lighting_gpu_state;
#[path = "lighting_gpu_wgpu.rs"]
mod lighting_gpu_wgpu;

pub use lighting_gpu_image::{ForwardPlusGpuImage, ForwardPlusGpuMetrics};
use lighting_gpu_state::*;
use lighting_gpu_wgpu::*;

use crate::{EngineError, gpu::GpuContext, math::Vec3};

use super::{
    AmbientLight, ClusterGrid, ClusteredLightAssignment, FORWARD_PLUS_WGSL, LightKind, LightingRig,
    RenderLight, cluster_lights,
};

const MIN_TARGET_WIDTH: u32 = 64;
const MIN_TARGET_HEIGHT: u32 = 64;
const MAX_TARGET_PIXELS: u64 = 4096 * 4096;
const MAX_LIGHTS: usize = 65_535;
const MAX_LIGHTS_PER_CLUSTER: usize = 4096;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ClusterParamsGpu {
    cluster_count: u32,
    light_count: u32,
    max_lights_per_cluster: u32,
    x_slices: u32,
    y_slices: u32,
    z_slices: u32,
    target_width: u32,
    target_height: u32,
    shading_z_slice: u32,
    _pad: [u32; 3],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ClusterBoundsGpu {
    min_pad: [f32; 4],
    max_pad: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct LightRecordGpu {
    position_radius: [f32; 4],
    direction_kind: [f32; 4],
    color_intensity: [f32; 4],
    spot_angles: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct AmbientLightGpu {
    color_intensity: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ClusterHeaderGpu {
    offset_count_clipped_pad: [u32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForwardPlusGpuSettings {
    pub width: u32,
    pub height: u32,
    pub shading_z_slice: u32,
}

impl Default for ForwardPlusGpuSettings {
    fn default() -> Self {
        Self {
            width: 640,
            height: 360,
            shading_z_slice: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForwardPlusGpuFrame {
    pub image: ForwardPlusGpuImage,
    pub stats: ForwardPlusGpuStats,
    pub assignment: ClusteredLightAssignment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForwardPlusGpuStats {
    pub gpu_context_id: u64,
    pub adapter_name: String,
    pub backend: String,
    pub texture_format: String,
    pub width: u32,
    pub height: u32,
    pub shading_z_slice: u32,
    pub cluster_count: usize,
    pub light_count: usize,
    pub directional_light_count: usize,
    pub ambient_name: String,
    pub ambient_color: Vec3,
    pub ambient_intensity: f32,
    pub ambient_contribution: Vec3,
    pub max_lights_per_cluster: usize,
    pub cpu_total_light_refs: usize,
    pub gpu_total_light_refs: usize,
    pub cpu_non_empty_clusters: usize,
    pub gpu_non_empty_clusters: usize,
    pub cpu_max_cluster_occupancy: usize,
    pub gpu_max_cluster_occupancy: usize,
    pub cpu_refs_summary: String,
    pub gpu_refs_summary: String,
    pub gpu_cluster_counts: Vec<u32>,
    pub compute_workgroups: u32,
    pub draw_calls: u32,
    pub readback_bytes: usize,
    pub row_pitch: u32,
    pub image_checksum: String,
    pub metrics: ForwardPlusGpuMetrics,
}

pub async fn render_forward_plus_lighting_gpu(
    settings: ForwardPlusGpuSettings,
    grid: ClusterGrid,
    lights: &[RenderLight],
) -> Result<ForwardPlusGpuFrame, EngineError> {
    let rig = LightingRig::from_lights(lights)?;
    let context = GpuContext::new_headless().await?;
    render_forward_plus_lighting_rig_gpu_with_context(&context, settings, grid, &rig).await
}

pub async fn render_forward_plus_lighting_rig_gpu(
    settings: ForwardPlusGpuSettings,
    grid: ClusterGrid,
    rig: &LightingRig,
) -> Result<ForwardPlusGpuFrame, EngineError> {
    validate(settings, grid, rig)?;
    let context = GpuContext::new_headless().await?;
    render_forward_plus_lighting_rig_gpu_with_context(&context, settings, grid, rig).await
}

pub async fn render_forward_plus_lighting_gpu_with_context(
    context: &GpuContext,
    settings: ForwardPlusGpuSettings,
    grid: ClusterGrid,
    lights: &[RenderLight],
) -> Result<ForwardPlusGpuFrame, EngineError> {
    let rig = LightingRig::from_lights(lights)?;
    render_forward_plus_lighting_rig_gpu_with_context(context, settings, grid, &rig).await
}

pub async fn render_forward_plus_lighting_rig_gpu_with_context(
    context: &GpuContext,
    settings: ForwardPlusGpuSettings,
    grid: ClusterGrid,
    rig: &LightingRig,
) -> Result<ForwardPlusGpuFrame, EngineError> {
    validate(settings, grid, rig)?;
    let lights = &rig.lights;
    let assignment = cluster_lights(grid, lights)?;
    let device = context.device();
    let queue = context.queue();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let cluster_count = grid.cluster_count()?;
    let max_refs = checked_ref_count(cluster_count, grid.max_lights_per_cluster)?;
    let params = params(settings, grid, lights.len(), cluster_count)?;
    let bounds = cluster_bounds(grid, cluster_count)?;
    let light_records = light_records(lights);
    let ambient_record = ambient_record(&rig.ambient);

    let params_buffer = buffer_init(
        device,
        "litd-forward-plus-params",
        bytemuck::bytes_of(&params),
        wgpu::BufferUsages::UNIFORM,
    );
    let bounds_buffer = buffer_init(
        device,
        "litd-forward-plus-cluster-bounds",
        bytemuck::cast_slice(&bounds),
        wgpu::BufferUsages::STORAGE,
    );
    let lights_buffer = buffer_init(
        device,
        "litd-forward-plus-lights",
        bytemuck::cast_slice(&light_records),
        wgpu::BufferUsages::STORAGE,
    );
    let ambient_buffer = buffer_init(
        device,
        "litd-forward-plus-ambient",
        bytemuck::bytes_of(&ambient_record),
        wgpu::BufferUsages::UNIFORM,
    );
    let headers_buffer = buffer_init(
        device,
        "litd-forward-plus-cluster-headers",
        bytemuck::cast_slice(&vec![ClusterHeaderGpu::zeroed(); cluster_count]),
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let indices_buffer = buffer_init(
        device,
        "litd-forward-plus-light-indices",
        bytemuck::cast_slice(&vec![0_u32; max_refs]),
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let headers_readback = readback_buffer(
        device,
        "litd-forward-plus-headers-readback",
        (cluster_count * size_of::<ClusterHeaderGpu>()) as u64,
    );
    let indices_readback = readback_buffer(
        device,
        "litd-forward-plus-indices-readback",
        (max_refs * size_of::<u32>()) as u64,
    );
    let compute_layout = compute_bind_group_layout(device);
    let render_layout = render_bind_group_layout(device);
    let empty_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-forward-plus-empty-bind-layout"),
        entries: &[],
    });
    let empty_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-forward-plus-empty-bind-group"),
        layout: &empty_layout,
        entries: &[],
    });
    let compute_bind_group = compute_bind_group(
        device,
        &compute_layout,
        &params_buffer,
        &bounds_buffer,
        &lights_buffer,
        &headers_buffer,
        &indices_buffer,
    );
    let render_bind_group = render_bind_group(
        device,
        &render_layout,
        &params_buffer,
        &lights_buffer,
        &ambient_buffer,
        &headers_buffer,
        &indices_buffer,
    );
    let compute_pipeline = compute_pipeline(device, &compute_layout);
    let render_pipeline = render_pipeline(device, &empty_layout, &render_layout, format);
    let target = render_target(device, settings, format);
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-forward-plus-encoder"),
    });
    let compute_workgroups = (cluster_count as u32).div_ceil(64);
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("litd-forward-plus-cluster-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&compute_pipeline);
        pass.set_bind_group(0, &compute_bind_group, &[]);
        pass.dispatch_workgroups(compute_workgroups, 1, 1);
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("litd-forward-plus-shade-pass"),
            color_attachments: &[Some(color_attachment(&target_view))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&render_pipeline);
        pass.set_bind_group(0, &empty_bind_group, &[]);
        pass.set_bind_group(1, &render_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    encoder.copy_buffer_to_buffer(
        &headers_buffer,
        0,
        &headers_readback,
        0,
        headers_readback.size(),
    );
    encoder.copy_buffer_to_buffer(
        &indices_buffer,
        0,
        &indices_readback,
        0,
        indices_readback.size(),
    );
    let (image_readback, row_pitch, image_bytes) =
        copy_target_to_readback(device, &mut encoder, &target, settings);
    queue.submit([encoder.finish()]);

    let headers = parse_pods::<ClusterHeaderGpu>(&map_readback_bytes(
        device,
        &headers_readback,
        "forward_plus.headers.map",
    )?);
    let indices = parse_pods::<u32>(&map_readback_bytes(
        device,
        &indices_readback,
        "forward_plus.indices.map",
    )?);
    let pixels = map_image_readback(device, &image_readback, settings, row_pitch)?;
    let gpu_refs = gpu_refs(
        &headers,
        &indices,
        cluster_count,
        grid.max_lights_per_cluster,
    )?;
    let gpu_refs_summary = refs_summary(&gpu_refs);
    if gpu_refs != assignment.cluster_light_indices {
        return Err(readback_mismatch(
            "forward_plus.refs_summary",
            assignment.refs_summary(),
            gpu_refs_summary,
        ));
    }
    let image = ForwardPlusGpuImage::from_pixels(settings.width, settings.height, pixels);
    let metrics = lighting_gpu_image::measure(&image);
    let gpu_cluster_counts = gpu_refs.iter().map(|refs| refs.len() as u32).collect();
    let stats = stats(LightingStatsArgs {
        context,
        settings,
        format,
        assignment: &assignment,
        ambient: &rig.ambient,
        light_count: lights.len(),
        gpu_refs: &gpu_refs,
        gpu_refs_summary,
        gpu_cluster_counts,
        compute_workgroups,
        row_pitch,
        readback_bytes: image_bytes
            + headers_readback.size() as usize
            + indices_readback.size() as usize,
        image: &image,
        metrics,
    });
    Ok(ForwardPlusGpuFrame {
        image,
        stats,
        assignment,
    })
}
