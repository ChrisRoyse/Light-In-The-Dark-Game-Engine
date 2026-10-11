//! wgpu pipeline, depth-atlas, readback and stats helpers for GPU shadow rendering (#1466
//! split of `shadow_gpu.rs`).
//!
//! Everything below the two public `render_shadow_maps_gpu*` entry points: plan validation,
//! texture/pipeline/bind-group construction, depth copy + map readback, and the per-tile /
//! per-cascade statistics. The entry points and the readback value types stay in
//! `shadow_gpu.rs`; these helpers are `pub(super)` so it calls them unchanged.

use std::sync::mpsc;

use wgpu::util::DeviceExt;

use super::super::{
    CubeFace, SHADOW_WGSL, ShadowMapEntry, ShadowMapKind, ShadowPlan, ShadowSettings,
};
use super::{
    ShadowCascadeReadback, ShadowDepthParamsGpu, ShadowGpuImage, ShadowGpuStats, ShadowTileReadback,
};
use crate::{EngineError, gpu::GpuContext};

pub(super) fn validate_plan_for_gpu(plan: &ShadowPlan) -> Result<(), EngineError> {
    if plan.settings.atlas_size > 4096 {
        return invalid_shadow_gpu("shadow_gpu.atlas_size", "must be <= 4096 for readback FSV");
    }
    Ok(())
}

pub(super) fn depth_atlas(
    device: &wgpu::Device,
    settings: ShadowSettings,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-shadow-depth-atlas"),
        size: texture_extent(settings),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub(super) fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-shadow-depth-bind-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

pub(super) fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-shadow-depth-bind-group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    })
}

pub(super) fn depth_param_buffer(device: &wgpu::Device, entry: &ShadowMapEntry) -> wgpu::Buffer {
    let params = ShadowDepthParamsGpu {
        depth_pad: [depth_for_entry(entry), 0.0, 0.0, 0.0],
    };
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("litd-shadow-depth-params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    })
}

pub(super) fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-shadow-depth-shader"),
        source: wgpu::ShaderSource::Wgsl(SHADOW_WGSL.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-shadow-depth-pipeline-layout"),
        bind_group_layouts: &[layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("litd-shadow-depth-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("litd_shadow_depth_vs"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: None,
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

pub(super) fn depth_attachment(
    view: &wgpu::TextureView,
) -> wgpu::RenderPassDepthStencilAttachment<'_> {
    wgpu::RenderPassDepthStencilAttachment {
        view,
        depth_ops: Some(wgpu::Operations {
            load: wgpu::LoadOp::Clear(1.0),
            store: wgpu::StoreOp::Store,
        }),
        stencil_ops: None,
    }
}

pub(super) fn copy_depth_to_readback(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    depth: &wgpu::Texture,
    settings: ShadowSettings,
) -> (wgpu::Buffer, u32, usize) {
    let row_pitch = align_to(settings.atlas_size * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let size = row_pitch as u64 * settings.atlas_size as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("litd-shadow-depth-readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: depth,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::DepthOnly,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_pitch),
                rows_per_image: Some(settings.atlas_size),
            },
        },
        texture_extent(settings),
    );
    (readback, row_pitch, size as usize)
}

pub(super) fn map_depth_readback(
    device: &wgpu::Device,
    readback: &wgpu::Buffer,
    settings: ShadowSettings,
    row_pitch: u32,
) -> Result<Vec<f32>, EngineError> {
    let bytes = map_readback_bytes(device, readback, "shadow_gpu.depth.map")?;
    let row_bytes = settings.atlas_size as usize * 4;
    let mut depths = Vec::with_capacity((settings.atlas_size * settings.atlas_size) as usize);
    for row in bytes
        .chunks(row_pitch as usize)
        .take(settings.atlas_size as usize)
    {
        let source = row.get(..row_bytes).ok_or_else(|| {
            shadow_gpu_error(
                "shadow_gpu.depth.layout",
                "mapped row was shorter than atlas width",
            )
        })?;
        depths.extend(source.chunks_exact(4).map(|chunk| {
            f32::from_le_bytes(chunk.try_into().expect("chunks_exact guarantees 4 bytes"))
        }));
    }
    let expected = (settings.atlas_size * settings.atlas_size) as usize;
    if depths.len() != expected {
        return Err(shadow_gpu_error(
            "shadow_gpu.depth.pixels",
            format!("expected {expected} depth values, read {}", depths.len()),
        ));
    }
    Ok(depths)
}

pub(super) fn map_readback_bytes(
    device: &wgpu::Device,
    readback: &wgpu::Buffer,
    field: &'static str,
) -> Result<Vec<u8>, EngineError> {
    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| shadow_gpu_error(field, format!("device poll failed: {error}")))?;
    rx.recv()
        .map_err(|error| shadow_gpu_error(field, format!("map callback failed: {error}")))?
        .map_err(|error| shadow_gpu_error(field, format!("buffer map failed: {error:?}")))?;
    let mapped = slice.get_mapped_range();
    let bytes = mapped.to_vec();
    drop(mapped);
    readback.unmap();
    Ok(bytes)
}

pub(super) struct StatsArgs<'a> {
    pub(super) context: &'a GpuContext,
    pub(super) settings: ShadowSettings,
    pub(super) format: wgpu::TextureFormat,
    pub(super) plan: &'a ShadowPlan,
    pub(super) depths: &'a [f32],
    pub(super) row_pitch: u32,
    pub(super) readback_bytes: usize,
    pub(super) image: &'a ShadowGpuImage,
}

pub(super) fn stats(args: StatsArgs<'_>) -> Result<ShadowGpuStats, EngineError> {
    let StatsArgs {
        context,
        settings,
        format,
        plan,
        depths,
        row_pitch,
        readback_bytes,
        image,
    } = args;
    let tile_depths = plan
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| tile_readback(index, entry, depths, settings.atlas_size))
        .collect::<Result<Vec<_>, _>>()?;
    if !tile_depths.is_empty() && tile_depths.iter().any(|tile| tile.shadow_pixel_count == 0) {
        return Err(shadow_gpu_error(
            "shadow_gpu.tile_depths",
            "every planned shadow tile must contain real depth writes",
        ));
    }
    let shadow_pixel_count = depths.iter().filter(|depth| **depth < 0.999).count();
    let shadow_depths = depths.iter().copied().filter(|depth| *depth < 0.999);
    let min_shadow_depth = shadow_depths.clone().fold(1.0_f32, f32::min);
    let max_shadow_depth = shadow_depths.fold(1.0_f32, f32::max);
    let total_pixels = depths.len().max(1) as f32;
    Ok(ShadowGpuStats {
        gpu_context_id: context.context_id(),
        adapter_name: context.adapter_report().name.clone(),
        backend: context.adapter_report().backend.clone(),
        texture_format: format!("{format:?}"),
        atlas_size: settings.atlas_size,
        tile_size: settings.tile_size,
        entry_count: plan.entries.len(),
        directional_cascade_count: plan.directional_cascade_count(),
        point_face_count: plan.point_face_count(),
        spot_map_count: plan.spot_map_count(),
        cascade_splits: cascade_splits(plan),
        tile_depths,
        shadow_pixel_count,
        shadow_pixel_ratio: shadow_pixel_count as f32 / total_pixels,
        min_shadow_depth,
        max_shadow_depth,
        draw_calls: plan.entries.len() as u32,
        readback_bytes,
        row_pitch,
        image_checksum: image.checksum(),
    })
}

pub(super) fn tile_readback(
    entry_index: usize,
    entry: &ShadowMapEntry,
    depths: &[f32],
    atlas_size: u32,
) -> Result<ShadowTileReadback, EngineError> {
    let mut shadow_pixel_count = 0;
    let mut min_depth = 1.0_f32;
    let mut max_depth = 0.0_f32;
    for y in entry.tile.y..entry.tile.y + entry.tile.size {
        for x in entry.tile.x..entry.tile.x + entry.tile.size {
            let depth = depths
                .get((y * atlas_size + x) as usize)
                .copied()
                .ok_or_else(|| shadow_gpu_error("shadow_gpu.tile", "tile exceeded atlas bounds"))?;
            if depth < 0.999 {
                shadow_pixel_count += 1;
                min_depth = min_depth.min(depth);
                max_depth = max_depth.max(depth);
            }
        }
    }
    Ok(ShadowTileReadback {
        entry_index,
        light_index: entry.light_index,
        light_name: entry.light_name.clone(),
        kind: kind_name(entry.kind),
        tile: entry.tile,
        shadow_pixel_count,
        min_depth,
        max_depth,
    })
}

pub(super) fn cascade_splits(plan: &ShadowPlan) -> Vec<ShadowCascadeReadback> {
    plan.entries
        .iter()
        .filter_map(|entry| match entry.kind {
            ShadowMapKind::DirectionalCascade {
                cascade_index,
                split_near,
                split_far,
            } => Some(ShadowCascadeReadback {
                cascade_index,
                split_near,
                split_far,
            }),
            _ => None,
        })
        .collect()
}

pub(super) fn depth_for_entry(entry: &ShadowMapEntry) -> f32 {
    match entry.kind {
        ShadowMapKind::DirectionalCascade { cascade_index, .. } => {
            0.20 + cascade_index as f32 * 0.08
        }
        ShadowMapKind::PointFace { face } => 0.42 + face_index(face) as f32 * 0.04,
        ShadowMapKind::Spot => 0.16,
    }
}

pub(super) fn kind_name(kind: ShadowMapKind) -> String {
    match kind {
        ShadowMapKind::DirectionalCascade { cascade_index, .. } => {
            format!("directional_cascade_{cascade_index}")
        }
        ShadowMapKind::PointFace { face } => format!("point_face_{face:?}"),
        ShadowMapKind::Spot => "spot".to_string(),
    }
}

pub(super) fn face_index(face: CubeFace) -> u32 {
    CubeFace::ALL
        .iter()
        .position(|candidate| *candidate == face)
        .unwrap_or(0) as u32
}

pub(super) fn texture_extent(settings: ShadowSettings) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: settings.atlas_size,
        height: settings.atlas_size,
        depth_or_array_layers: 1,
    }
}

pub(super) fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

fn invalid_shadow_gpu<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(shadow_gpu_error(field, reason))
}

pub(super) fn shadow_gpu_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::ShadowInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}
