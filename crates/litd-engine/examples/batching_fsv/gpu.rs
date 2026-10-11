use std::sync::mpsc;

use bytemuck::{Pod, Zeroable};
use litd_engine::{
    EngineError,
    gpu::GpuContext,
    render::batching::{
        BATCHING_CULLING_WGSL, BatchingSettings, Frustum, HzbOccluder, RenderInstance,
    },
};
use litd_hash::sha256_hex;
use serde_json::json;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct InstanceBoundsGpu {
    min_depth: [f32; 4],
    max_key: [f32; 4],
    rect: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct FrustumPlaneGpu {
    normal_distance: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct HzbOccluderGpu {
    rect: [f32; 4],
    depth: f32,
    _pad_to_vec3: [f32; 3],
    _pad0: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct CullingParamsGpu {
    counts_flags: [u32; 4],
    occlusion_bias: f32,
    _pad_to_vec3: [f32; 3],
    _pad0: [f32; 4],
}

pub async fn gpu_culling_readback(
    settings: BatchingSettings,
    frustum: &Frustum,
    occluders: &[HzbOccluder],
    instances: &[RenderInstance],
) -> Result<serde_json::Value, EngineError> {
    let context = GpuContext::new_headless().await?;
    let device = context.device();
    let queue = context.queue();
    let instance_bounds = instance_bounds(instances);
    let planes = frustum_planes(frustum);
    let hzb = hzb_occluders(occluders);
    let params = CullingParamsGpu {
        counts_flags: [
            instances.len() as u32,
            frustum.planes.len() as u32,
            occluders.len() as u32,
            u32::from(settings.hzb_enabled),
        ],
        occlusion_bias: settings.occlusion_bias,
        _pad_to_vec3: [0.0; 3],
        _pad0: [0.0; 4],
    };
    let visibility = vec![0_u32; instances.len()];
    let instance_buffer = buffer_init(
        device,
        "litd-batching-instance-bounds",
        bytemuck::cast_slice(&instance_bounds),
        wgpu::BufferUsages::STORAGE,
    );
    let plane_buffer = buffer_init(
        device,
        "litd-batching-frustum-planes",
        bytemuck::cast_slice(&planes),
        wgpu::BufferUsages::STORAGE,
    );
    let occluder_buffer = buffer_init(
        device,
        "litd-batching-hzb-occluders",
        bytemuck::cast_slice(&hzb),
        wgpu::BufferUsages::STORAGE,
    );
    let visibility_buffer = buffer_init(
        device,
        "litd-batching-visibility",
        bytemuck::cast_slice(&visibility),
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let params_buffer = buffer_init(
        device,
        "litd-batching-culling-params",
        bytemuck::bytes_of(&params),
        wgpu::BufferUsages::UNIFORM,
    );
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("litd-batching-visibility-readback"),
        size: (visibility.len() * size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let layout = batching_compute_bind_group_layout(device);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-batching-compute-bind-group"),
        layout: &layout,
        entries: &[
            bind(0, &instance_buffer),
            bind(1, &plane_buffer),
            bind(2, &occluder_buffer),
            bind(3, &visibility_buffer),
            bind(4, &params_buffer),
        ],
    });
    let pipeline = batching_compute_pipeline(device, &layout);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-batching-culling-encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("litd-batching-culling-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups((instances.len() as u32).div_ceil(64), 1, 1);
    }
    encoder.copy_buffer_to_buffer(&visibility_buffer, 0, &readback, 0, readback.size());
    queue.submit([encoder.finish()]);
    let bytes = map_readback_bytes(device, &readback)?;
    let words = bytemuck::cast_slice::<u8, u32>(&bytes).to_vec();
    let visible_ids = instances
        .iter()
        .zip(words.iter())
        .filter_map(|(instance, word)| (*word == 1).then_some(instance.id))
        .collect::<Vec<_>>();
    Ok(json!({
        "adapter": context.adapter_report(),
        "context_id": context.context_id(),
        "wgsl_sha256": sha256_hex(BATCHING_CULLING_WGSL.as_bytes()),
        "wgsl_has_frustum": BATCHING_CULLING_WGSL.contains("litd_frustum_visible"),
        "wgsl_has_hzb": BATCHING_CULLING_WGSL.contains("litd_hzb_occluded"),
        "wgsl_has_indirect": BATCHING_CULLING_WGSL.contains("IndirectDrawCommand"),
        "compute_pipeline_created": true,
        "visibility_words": words,
        "visible_ids": visible_ids,
        "device_lost_report": context.device_lost_report()
    }))
}

fn instance_bounds(instances: &[RenderInstance]) -> Vec<InstanceBoundsGpu> {
    instances
        .iter()
        .map(|instance| InstanceBoundsGpu {
            min_depth: [
                instance.world_bounds.min.x,
                instance.world_bounds.min.y,
                instance.world_bounds.min.z,
                instance.nearest_depth,
            ],
            max_key: [
                instance.world_bounds.max.x,
                instance.world_bounds.max.y,
                instance.world_bounds.max.z,
                0.0,
            ],
            rect: [
                instance.screen_rect.min.x,
                instance.screen_rect.min.y,
                instance.screen_rect.max.x,
                instance.screen_rect.max.y,
            ],
        })
        .collect()
}

fn frustum_planes(frustum: &Frustum) -> Vec<FrustumPlaneGpu> {
    frustum
        .planes
        .iter()
        .map(|plane| FrustumPlaneGpu {
            normal_distance: [
                plane.normal.x,
                plane.normal.y,
                plane.normal.z,
                plane.distance,
            ],
        })
        .collect()
}

fn hzb_occluders(occluders: &[HzbOccluder]) -> Vec<HzbOccluderGpu> {
    occluders
        .iter()
        .map(|occluder| HzbOccluderGpu {
            rect: [
                occluder.screen_rect.min.x,
                occluder.screen_rect.min.y,
                occluder.screen_rect.max.x,
                occluder.screen_rect.max.y,
            ],
            depth: occluder.depth,
            _pad_to_vec3: [0.0; 3],
            _pad0: [0.0; 4],
        })
        .collect()
}

fn batching_compute_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-batching-culling-shader"),
        source: wgpu::ShaderSource::Wgsl(BATCHING_CULLING_WGSL.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-batching-culling-pipeline-layout"),
        bind_group_layouts: &[layout],
        push_constant_ranges: &[],
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("litd-batching-culling-pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("litd_cull_instances"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

fn batching_compute_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-batching-culling-bind-layout"),
        entries: &[
            storage_layout(0, true),
            storage_layout(1, true),
            storage_layout(2, true),
            storage_layout(3, false),
            buffer_layout(4, wgpu::BufferBindingType::Uniform),
        ],
    })
}

fn storage_layout(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    buffer_layout(binding, wgpu::BufferBindingType::Storage { read_only })
}

fn buffer_layout(binding: u32, ty: wgpu::BufferBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn bind(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

fn buffer_init(
    device: &wgpu::Device,
    label: &'static str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage,
    })
}

fn map_readback_bytes(
    device: &wgpu::Device,
    readback: &wgpu::Buffer,
) -> Result<Vec<u8>, EngineError> {
    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| EngineError::BatchingInvalidConfig {
            field: "gpu.visibility_readback.poll".to_string(),
            reason: error.to_string(),
        })?;
    rx.recv()
        .map_err(|error| EngineError::BatchingInvalidConfig {
            field: "gpu.visibility_readback.callback".to_string(),
            reason: error.to_string(),
        })?
        .map_err(|error| EngineError::BatchingInvalidConfig {
            field: "gpu.visibility_readback.map".to_string(),
            reason: format!("{error:?}"),
        })?;
    let mapped = slice.get_mapped_range();
    let bytes = mapped.to_vec();
    drop(mapped);
    readback.unmap();
    Ok(bytes)
}
