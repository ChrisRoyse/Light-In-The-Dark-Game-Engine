//! Real headless-GPU driver for the issue #1740 two-phase Hi-Z cull FSV.
//!
//! Seeds the Hi-Z base level from the previous frame's visible set (CPU), then runs
//! the genuine GPU pipeline: `hzb_build.wgsl` max-reduces the pyramid level by level,
//! `two_phase_cull.wgsl` frustum + Hi-Z tests every instance and stream-compacts the
//! survivors into an indirect draw buffer whose length is the atomic count buffer.
//! The counters, per-instance classes, and built pyramid are read back and returned
//! so the harness can prove the compacted set matches the CPU oracle exactly.

use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use litd_engine::{
    EngineError,
    gpu::{AdapterReport, GpuContext, GpuDeviceLostReport},
    render::{
        batching::{Frustum, RenderInstance},
        gpu_driven::{HZB_BUILD_WGSL, HzbPyramidDims, TWO_PHASE_CULL_WGSL, TwoPhaseSettings},
        gpu_driven_cpu::{build_base_depth, build_pyramid},
    },
};

#[path = "wgpu_helpers.rs"]
mod wgpu_helpers;
use wgpu_helpers::{
    bind, buffer_init, bytes_or_pad, compute_pipeline, map_readback, poll_error, read_u32,
    readback_buffer, storage_entry, uniform_entry, zeroed_storage,
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceBoundsGpu {
    aabb_min_depth: [f32; 4],
    aabb_max: [f32; 4],
    rect: [f32; 4],
    meta: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrustumPlaneGpu {
    normal_distance: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BuildParamsGpu {
    src_offset: u32,
    dst_offset: u32,
    src_dim: u32,
    dst_dim: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CullParamsGpu {
    instance_count: u32,
    plane_count: u32,
    base_dim: u32,
    level_count: u32,
    flags: u32,
    occlusion_bias: f32,
    _pad0: u32,
    _pad1: u32,
}

/// Everything the harness reads back from the real GPU run.
pub struct GpuTwoPhaseReadback {
    pub visible_count: u32,
    pub phase1_count: u32,
    pub phase2_count: u32,
    pub visible_indices: Vec<u32>,
    pub classes: Vec<u32>,
    pub pyramid_matches_cpu: bool,
    pub adapter: AdapterReport,
    pub indirect_execution_supported: bool,
    pub multi_draw_indirect_count_supported: bool,
    pub dispatch_ns: u128,
    pub device_lost: Option<GpuDeviceLostReport>,
}

pub async fn run_gpu_two_phase(
    settings: TwoPhaseSettings,
    frustum: &Frustum,
    instances: &[RenderInstance],
    prev_visible: &[bool],
) -> Result<GpuTwoPhaseReadback, EngineError> {
    settings.validate()?;
    let dims = HzbPyramidDims::new(settings.base_dim)?;
    let base = build_base_depth(&dims, instances, prev_visible)?;
    let cpu_pyramid = build_pyramid(&dims, &base)?;

    let context = GpuContext::new_headless().await?;
    let device = context.device();
    let queue = context.queue();
    let downlevel = context.adapter().get_downlevel_capabilities();
    let indirect_execution_supported = downlevel
        .flags
        .contains(wgpu::DownlevelFlags::INDIRECT_EXECUTION);
    let multi_draw_indirect_count_supported = context
        .adapter()
        .features()
        .contains(wgpu::Features::MULTI_DRAW_INDIRECT_COUNT);

    let started = Instant::now();
    let pyramid = build_pyramid_on_gpu(device, queue, &dims, &base)?;
    let (counters, classes, compacted) = cull_on_gpu(
        device,
        queue,
        settings,
        &dims,
        frustum,
        instances,
        prev_visible,
    )?;
    let dispatch_ns = started.elapsed().as_nanos();

    let visible_count = counters[0];
    let mut visible_indices = compacted[..visible_count as usize].to_vec();
    visible_indices.sort_unstable();

    Ok(GpuTwoPhaseReadback {
        visible_count,
        phase1_count: counters[1],
        phase2_count: counters[2],
        visible_indices,
        classes,
        pyramid_matches_cpu: pyramid == cpu_pyramid,
        adapter: context.adapter_report().clone(),
        indirect_execution_supported,
        multi_draw_indirect_count_supported,
        dispatch_ns,
        device_lost: context.device_lost_report(),
    })
}

fn build_pyramid_on_gpu(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    dims: &HzbPyramidDims,
    base: &[f32],
) -> Result<Vec<f32>, EngineError> {
    let total = dims.total_texels() as usize;
    let mut seed = vec![1.0f32; total];
    seed[..base.len()].copy_from_slice(base);
    let pyramid_buffer = buffer_init(
        device,
        "litd-1740-pyramid",
        bytemuck::cast_slice(&seed),
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("litd-1740-build-params"),
        size: size_of::<BuildParamsGpu>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-1740-build-layout"),
        entries: &[storage_entry(0, false), uniform_entry(1)],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-1740-build-bg"),
        layout: &layout,
        entries: &[bind(0, &pyramid_buffer), bind(1, &params_buffer)],
    });
    let pipeline = compute_pipeline(device, &layout, HZB_BUILD_WGSL, "litd_hzb_build", "build");

    for level in 1..dims.level_count() {
        let params = BuildParamsGpu {
            src_offset: dims.level_offset(level - 1),
            dst_offset: dims.level_offset(level),
            src_dim: dims.level_dim(level - 1),
            dst_dim: dims.level_dim(level),
        };
        queue.write_buffer(&params_buffer, 0, bytemuck::bytes_of(&params));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("litd-1740-build-encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("litd-1740-build-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let groups = params.dst_dim.div_ceil(8);
            pass.dispatch_workgroups(groups, groups, 1);
        }
        queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| poll_error("build", error))?;
    }

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("litd-1740-pyramid-readback"),
        size: (total * size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-1740-pyramid-copy"),
    });
    encoder.copy_buffer_to_buffer(&pyramid_buffer, 0, &readback, 0, readback.size());
    queue.submit([encoder.finish()]);
    let bytes = map_readback(device, &readback)?;
    Ok(bytemuck::cast_slice::<u8, f32>(&bytes).to_vec())
}

type CullReadback = (Vec<u32>, Vec<u32>, Vec<u32>);

fn cull_on_gpu(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    settings: TwoPhaseSettings,
    dims: &HzbPyramidDims,
    frustum: &Frustum,
    instances: &[RenderInstance],
    prev_visible: &[bool],
) -> Result<CullReadback, EngineError> {
    // Re-seed and rebuild the pyramid inside this pass's own buffer so the cull reads a
    // freshly built pyramid (the build pass above is verified separately via readback).
    let total = dims.total_texels() as usize;
    let base = build_base_depth(dims, instances, prev_visible)?;
    let pyramid = build_pyramid(dims, &base)?;
    debug_assert_eq!(pyramid.len(), total);

    let slots = instances.len().max(1);
    let bounds = instance_bounds(instances);
    let planes = frustum_planes(frustum);
    let prev: Vec<u32> = prev_visible.iter().map(|&v| u32::from(v)).collect();
    let params = CullParamsGpu {
        instance_count: instances.len() as u32,
        plane_count: frustum.planes.len() as u32,
        base_dim: dims.base_dim(),
        level_count: dims.level_count(),
        flags: u32::from(settings.hzb_enabled),
        occlusion_bias: settings.occlusion_bias,
        _pad0: 0,
        _pad1: 0,
    };

    let bounds_buffer = buffer_init(
        device,
        "litd-1740-bounds",
        &bytes_or_pad(
            bytemuck::cast_slice(&bounds),
            size_of::<InstanceBoundsGpu>(),
        ),
        wgpu::BufferUsages::STORAGE,
    );
    let plane_buffer = buffer_init(
        device,
        "litd-1740-planes",
        bytemuck::cast_slice(&planes),
        wgpu::BufferUsages::STORAGE,
    );
    let pyramid_buffer = buffer_init(
        device,
        "litd-1740-cull-pyramid",
        bytemuck::cast_slice(&pyramid),
        wgpu::BufferUsages::STORAGE,
    );
    let prev_buffer = buffer_init(
        device,
        "litd-1740-prev",
        &bytes_or_pad(bytemuck::cast_slice(&prev), size_of::<u32>()),
        wgpu::BufferUsages::STORAGE,
    );
    let params_buffer = buffer_init(
        device,
        "litd-1740-cull-params",
        bytemuck::bytes_of(&params),
        wgpu::BufferUsages::UNIFORM,
    );
    let counters_buffer = buffer_init(
        device,
        "litd-1740-counters",
        bytemuck::cast_slice(&[0u32, 0, 0]),
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    );
    let compacted_buffer = zeroed_storage(
        device,
        "litd-1740-compacted",
        (slots * size_of::<u32>()) as u64,
    );
    let indirect_buffer = zeroed_storage(
        device,
        "litd-1740-indirect",
        (slots * 5 * size_of::<u32>()) as u64,
    );
    let class_buffer = zeroed_storage(device, "litd-1740-class", (slots * size_of::<u32>()) as u64);

    let layout = cull_bind_group_layout(device);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-1740-cull-bg"),
        layout: &layout,
        entries: &[
            bind(0, &bounds_buffer),
            bind(1, &plane_buffer),
            bind(2, &pyramid_buffer),
            bind(3, &prev_buffer),
            bind(4, &params_buffer),
            bind(5, &counters_buffer),
            bind(6, &compacted_buffer),
            bind(7, &indirect_buffer),
            bind(8, &class_buffer),
        ],
    });
    let pipeline = compute_pipeline(
        device,
        &layout,
        TWO_PHASE_CULL_WGSL,
        "litd_two_phase_cull",
        "cull",
    );

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-1740-cull-encoder"),
    });
    if !instances.is_empty() {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("litd-1740-cull-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups((instances.len() as u32).div_ceil(64), 1, 1);
    }
    let counters_readback =
        readback_buffer(device, "litd-1740-counters-rb", 3 * size_of::<u32>() as u64);
    let class_readback = readback_buffer(
        device,
        "litd-1740-class-rb",
        (slots * size_of::<u32>()) as u64,
    );
    let compacted_readback = readback_buffer(
        device,
        "litd-1740-compacted-rb",
        (slots * size_of::<u32>()) as u64,
    );
    encoder.copy_buffer_to_buffer(
        &counters_buffer,
        0,
        &counters_readback,
        0,
        counters_readback.size(),
    );
    encoder.copy_buffer_to_buffer(&class_buffer, 0, &class_readback, 0, class_readback.size());
    encoder.copy_buffer_to_buffer(
        &compacted_buffer,
        0,
        &compacted_readback,
        0,
        compacted_readback.size(),
    );
    queue.submit([encoder.finish()]);

    let counters = read_u32(device, &counters_readback)?;
    let classes = read_u32(device, &class_readback)?;
    let compacted = read_u32(device, &compacted_readback)?;
    Ok((counters, classes[..instances.len()].to_vec(), compacted))
}

fn instance_bounds(instances: &[RenderInstance]) -> Vec<InstanceBoundsGpu> {
    instances
        .iter()
        .map(|i| InstanceBoundsGpu {
            aabb_min_depth: [
                i.world_bounds.min.x,
                i.world_bounds.min.y,
                i.world_bounds.min.z,
                i.nearest_depth,
            ],
            aabb_max: [
                i.world_bounds.max.x,
                i.world_bounds.max.y,
                i.world_bounds.max.z,
                0.0,
            ],
            rect: [
                i.screen_rect.min.x,
                i.screen_rect.min.y,
                i.screen_rect.max.x,
                i.screen_rect.max.y,
            ],
            meta: [i.index_count, 0, 0, 0],
        })
        .collect()
}

fn frustum_planes(frustum: &Frustum) -> Vec<FrustumPlaneGpu> {
    frustum
        .planes
        .iter()
        .map(|p| FrustumPlaneGpu {
            normal_distance: [p.normal.x, p.normal.y, p.normal.z, p.distance],
        })
        .collect()
}

fn cull_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-1740-cull-layout"),
        entries: &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, true),
            storage_entry(3, true),
            uniform_entry(4),
            storage_entry(5, false),
            storage_entry(6, false),
            storage_entry(7, false),
            storage_entry(8, false),
        ],
    })
}
