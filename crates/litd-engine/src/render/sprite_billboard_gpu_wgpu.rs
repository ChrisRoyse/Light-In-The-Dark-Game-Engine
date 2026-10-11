use std::collections::BTreeMap;

use bytemuck::{Pod, Zeroable};

use crate::{EngineError, gpu::GpuContext};

use super::super::{
    ATLAS_ROW_ALIGNMENT, SpriteBillboardLibraryReadback, SpriteBillboardLibrarySelection,
    SpriteTextureRecord, sprite_billboard_error,
};

pub const SPRITE_BILLBOARD_WGSL: &str = include_str!("sprite_billboard.wgsl");

#[path = "sprite_billboard_gpu_wgpu_readback.rs"]
mod sprite_billboard_gpu_wgpu_readback;

use sprite_billboard_gpu_wgpu_readback::{
    buffer, copy_target_to_readback, extent, map_image_readback, target_texture,
};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GpuSpriteVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

pub(super) struct SpriteBillboardGpuReadback {
    pub pixels: Vec<[u8; 4]>,
    pub row_pitch: u32,
    pub readback_bytes: usize,
    pub draw_calls: u32,
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub regions: Vec<[u32; 4]>,
}

struct SpriteAtlas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    regions: BTreeMap<String, [u32; 4]>,
}

pub(super) fn render_sprite_billboards(
    context: &GpuContext,
    width: u32,
    height: u32,
    library: &SpriteBillboardLibrarySelection,
    readback: &SpriteBillboardLibraryReadback,
) -> Result<SpriteBillboardGpuReadback, EngineError> {
    let atlas = build_atlas(&library.textures)?;
    let (vertices, indices, regions) = preview_geometry(width, height, library, readback, &atlas)?;
    let device = context.device();
    let queue = context.queue();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = target_texture(device, width, height, format);
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let atlas_texture = atlas_texture(device, queue, &atlas)?;
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-sprite-billboard-bind-group-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("litd-sprite-billboard-sampler"),
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..wgpu::SamplerDescriptor::default()
    });
    let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-sprite-billboard-bind-group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-sprite-billboard-shader"),
        source: wgpu::ShaderSource::Wgsl(SPRITE_BILLBOARD_WGSL.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-sprite-billboard-pipeline-layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });
    let pipeline = pipeline(device, &shader, &layout, format);
    let vertex_buffer = buffer(
        device,
        "litd-sprite-billboard-vertices",
        bytemuck::cast_slice(&vertices),
        wgpu::BufferUsages::VERTEX,
    );
    let index_buffer = buffer(
        device,
        "litd-sprite-billboard-indices",
        bytemuck::cast_slice(&indices),
        wgpu::BufferUsages::INDEX,
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-sprite-billboard-encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("litd-sprite-billboard-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.010,
                        g: 0.012,
                        b: 0.018,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
    }
    let (copy, row_pitch, readback_bytes) =
        copy_target_to_readback(device, &mut encoder, &target, width, height);
    queue.submit([encoder.finish()]);
    let pixels = map_image_readback(device, &copy, width, height, row_pitch)?;
    Ok(SpriteBillboardGpuReadback {
        pixels,
        row_pitch,
        readback_bytes,
        draw_calls: 1,
        atlas_width: atlas.width,
        atlas_height: atlas.height,
        regions,
    })
}

fn build_atlas(textures: &[SpriteTextureRecord]) -> Result<SpriteAtlas, EngineError> {
    // Checked throughout: the combined atlas of individually-valid textures can
    // exceed u32 (see `validate_combined_atlas`); a raw sum/mul here wraps in
    // release and panics on the undersized `copy_from_slice`. Fail loud instead.
    let mut source_width = 0_u32;
    for texture in textures {
        source_width = source_width.checked_add(texture.width).ok_or_else(|| {
            sprite_billboard_error("atlas.width", "combined atlas width overflows u32")
        })?;
    }
    let width = source_width
        .max(1)
        .checked_next_multiple_of(ATLAS_ROW_ALIGNMENT)
        .ok_or_else(|| {
            sprite_billboard_error("atlas.width", "row-aligned atlas width overflows u32")
        })?;
    let height = textures
        .iter()
        .map(|texture| texture.height)
        .max()
        .unwrap_or(1);
    let byte_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            sprite_billboard_error("atlas.bytes", "combined atlas byte length overflows u32")
        })? as usize;
    let mut pixels = vec![0_u8; byte_len];
    let mut regions = BTreeMap::new();
    let mut x0 = 0_u32;
    for texture in textures {
        for y in 0..texture.height {
            let dst = ((y * width + x0) * 4) as usize;
            let src = (y * texture.width * 4) as usize;
            let len = (texture.width * 4) as usize;
            pixels[dst..dst + len].copy_from_slice(&texture.rgba8[src..src + len]);
        }
        regions.insert(
            texture.texture_id.clone(),
            [x0, 0, texture.width, texture.height],
        );
        x0 = x0.checked_add(texture.width).ok_or_else(|| {
            sprite_billboard_error("atlas.width", "combined atlas width overflows u32")
        })?;
    }
    Ok(SpriteAtlas {
        width,
        height,
        pixels,
        regions,
    })
}

/// Interleaved CPU-side geometry for one sprite-billboard preview draw:
/// (vertices, indices, per-sprite atlas regions). Named so the builder's return
/// type stays legible instead of a bare 3-tuple of vectors.
type PreviewGeometry = (Vec<GpuSpriteVertex>, Vec<u32>, Vec<[u32; 4]>);

fn preview_geometry(
    width: u32,
    height: u32,
    library: &SpriteBillboardLibrarySelection,
    readback: &SpriteBillboardLibraryReadback,
    atlas: &SpriteAtlas,
) -> Result<PreviewGeometry, EngineError> {
    let mut vertices = Vec::with_capacity(readback.meshes.len() * 4);
    let mut indices = Vec::with_capacity(readback.meshes.len() * 6);
    let mut regions = Vec::with_capacity(readback.meshes.len());
    for (index, mesh) in readback.meshes.iter().enumerate() {
        let material = library
            .materials
            .iter()
            .find(|material| material.material_id == mesh.material_id)
            .ok_or_else(|| sprite_billboard_error("gpu.material", "missing material"))?;
        let region = stripe_region(index, readback.meshes.len(), width, height);
        let texture_region = atlas.regions[mesh.texture_id.as_str()];
        let uv = atlas_frame_uv(mesh.frame_uv, texture_region, atlas.width, atlas.height);
        let quad = preview_quad(region, width, height, mesh.bounds_max, mesh.bounds_min);
        let base = vertices.len() as u32;
        vertices.extend([
            GpuSpriteVertex {
                position: quad[0],
                uv: [uv[0], uv[3]],
                color: material.tint,
            },
            GpuSpriteVertex {
                position: quad[1],
                uv: [uv[2], uv[3]],
                color: material.tint,
            },
            GpuSpriteVertex {
                position: quad[2],
                uv: [uv[2], uv[1]],
                color: material.tint,
            },
            GpuSpriteVertex {
                position: quad[3],
                uv: [uv[0], uv[1]],
                color: material.tint,
            },
        ]);
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        regions.push(region);
    }
    Ok((vertices, indices, regions))
}

fn atlas_frame_uv(
    frame_uv: [f32; 4],
    region: [u32; 4],
    atlas_width: u32,
    atlas_height: u32,
) -> [f32; 4] {
    let [x, y, width, height] = region;
    let u0 = (x as f32 + frame_uv[0] * width as f32) / atlas_width as f32;
    let v0 = (y as f32 + frame_uv[1] * height as f32) / atlas_height as f32;
    let u1 = (x as f32 + frame_uv[2] * width as f32) / atlas_width as f32;
    let v1 = (y as f32 + frame_uv[3] * height as f32) / atlas_height as f32;
    [u0, v0, u1, v1]
}

fn preview_quad(
    region: [u32; 4],
    target_width: u32,
    target_height: u32,
    max: [f32; 3],
    min: [f32; 3],
) -> [[f32; 2]; 4] {
    let [x0, y0, width, height] = region;
    let sprite_w = (max[0] - min[0]).abs().max(0.01);
    let sprite_h = (max[1] - min[1]).abs().max(0.01);
    let aspect = sprite_w / sprite_h;
    let mut local_w = 0.68_f32;
    let mut local_h = (local_w / aspect).min(0.78);
    if local_h < 0.24 {
        local_h = 0.24;
        local_w = (local_h * aspect).min(0.78);
    }
    let cx = x0 as f32 + width as f32 * 0.5;
    let cy = y0 as f32 + height as f32 * 0.5;
    let hw = width as f32 * local_w * 0.5;
    let hh = height as f32 * local_h * 0.5;
    [
        ndc(cx - hw, cy + hh, target_width, target_height),
        ndc(cx + hw, cy + hh, target_width, target_height),
        ndc(cx + hw, cy - hh, target_width, target_height),
        ndc(cx - hw, cy - hh, target_width, target_height),
    ]
}

fn atlas_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    atlas: &SpriteAtlas,
) -> Result<wgpu::Texture, EngineError> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-sprite-billboard-atlas"),
        size: extent(atlas.width, atlas.height),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &atlas.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(atlas.width * 4),
            rows_per_image: Some(atlas.height),
        },
        extent(atlas.width, atlas.height),
    );
    Ok(texture)
}

fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("litd-sprite-billboard-pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("litd_sprite_billboard_vs"),
            buffers: &[vertex_layout()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("litd_sprite_billboard_fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..wgpu::PrimitiveState::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn vertex_layout<'a>() -> wgpu::VertexBufferLayout<'a> {
    const ATTRS: [wgpu::VertexAttribute; 3] = [
        wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: 8,
            shader_location: 1,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: 16,
            shader_location: 2,
            format: wgpu::VertexFormat::Float32x4,
        },
    ];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<GpuSpriteVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRS,
    }
}

fn stripe_region(index: usize, count: usize, width: u32, height: u32) -> [u32; 4] {
    let x0 = (index as u32 * width) / count as u32;
    let x1 = ((index as u32 + 1) * width) / count as u32;
    [x0, 0, x1 - x0, height]
}

fn ndc(x: f32, y: f32, width: u32, height: u32) -> [f32; 2] {
    [x / width as f32 * 2.0 - 1.0, 1.0 - y / height as f32 * 2.0]
}
