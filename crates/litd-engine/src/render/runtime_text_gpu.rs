//! #882 production wgpu pass for cached SDF runtime-text plans.
//!
//! `runtime_text` owns projection/layout and emits pixel-space [`GlyphQuad`]s.
//! This module uploads the atlas once, expands those quads into a bounded vertex
//! buffer, and records the alpha-blended GPU pass. It contains no sim state.

use bytemuck::{Pod, Zeroable};

use crate::{
    EngineError,
    render::{
        font::FontAtlasBuild,
        runtime_text::RuntimeTextPlan,
        ui::{GlyphQuad, UiRect},
    },
};

const RUNTIME_TEXT_GPU_WGSL: &str = include_str!("runtime_text_gpu.wgsl");
const VERTICES_PER_QUAD: usize = 6;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct RuntimeTextVertex {
    position_ndc: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeTextGpuStats {
    pub quad_count: u32,
    pub vertex_count: u32,
    pub atlas_width: u32,
    pub atlas_height: u32,
}

/// Reusable GPU resources for one runtime font atlas.
pub struct RuntimeTextGpuRenderer {
    pipeline: wgpu::RenderPipeline,
    atlas_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    max_quads: usize,
    atlas_size: (u32, u32),
}

impl RuntimeTextGpuRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        atlas: &FontAtlasBuild,
        max_quads: usize,
    ) -> Result<Self, EngineError> {
        if max_quads == 0 {
            return invalid_gpu("max_quads", "must be greater than zero");
        }
        atlas.atlas.validate()?;
        if atlas.image.width as f32 != atlas.atlas.atlas_size.x
            || atlas.image.height as f32 != atlas.atlas.atlas_size.y
        {
            return invalid_gpu(
                "atlas_size",
                "font atlas metadata and image dimensions must agree",
            );
        }
        let texture = create_atlas_texture(device, queue, atlas);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("litd-runtime-text-sdf-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        });
        let bind_group_layout = create_bind_group_layout(device);
        let atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("litd-runtime-text-atlas-bind-group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipeline = create_pipeline(device, target_format, &bind_group_layout);
        let vertex_bytes = max_quads
            .checked_mul(VERTICES_PER_QUAD)
            .and_then(|count| count.checked_mul(std::mem::size_of::<RuntimeTextVertex>()))
            .ok_or_else(|| gpu_error("max_quads", "vertex buffer size overflowed usize"))?;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("litd-runtime-text-vertices"),
            size: vertex_bytes as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            pipeline,
            atlas_bind_group,
            vertex_buffer,
            max_quads,
            atlas_size: (atlas.image.width, atlas.image.height),
        })
    }

    /// Upload and draw one verified plan over an existing color target.
    pub fn render(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        viewport: [u32; 2],
        plan: &RuntimeTextPlan,
    ) -> Result<RuntimeTextGpuStats, EngineError> {
        super::runtime_text::verify_runtime_text_plan_integrity(plan)?;
        let vertices = build_vertices(viewport, &plan.quads, self.max_quads)?;
        if !vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("litd-runtime-text-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.atlas_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        Ok(RuntimeTextGpuStats {
            quad_count: plan.quads.len() as u32,
            vertex_count: vertices.len() as u32,
            atlas_width: self.atlas_size.0,
            atlas_height: self.atlas_size.1,
        })
    }
}

fn build_vertices(
    viewport: [u32; 2],
    quads: &[GlyphQuad],
    max_quads: usize,
) -> Result<Vec<RuntimeTextVertex>, EngineError> {
    if viewport[0] == 0 || viewport[1] == 0 {
        return invalid_gpu("viewport", "width and height must be greater than zero");
    }
    if quads.len() > max_quads {
        return invalid_gpu(
            "quad_capacity",
            format!(
                "plan has {} glyph quads but renderer capacity is {max_quads}",
                quads.len()
            ),
        );
    }
    let mut vertices = Vec::with_capacity(quads.len() * VERTICES_PER_QUAD);
    for quad in quads {
        quad.rect_px.validate("runtime_text_gpu.rect_px")?;
        quad.uv.validate("runtime_text_gpu.uv")?;
        if !quad.color.is_finite() {
            return invalid_gpu("color", "glyph RGBA must be finite");
        }
        append_quad(&mut vertices, viewport, quad);
    }
    Ok(vertices)
}

fn append_quad(out: &mut Vec<RuntimeTextVertex>, viewport: [u32; 2], quad: &GlyphQuad) {
    let UiRect { min, max } = quad.rect_px;
    let uv = quad.uv;
    let positions = [
        [min.x, min.y],
        [max.x, min.y],
        [max.x, max.y],
        [min.x, min.y],
        [max.x, max.y],
        [min.x, max.y],
    ];
    let uvs = [
        [uv.min.x, uv.min.y],
        [uv.max.x, uv.min.y],
        [uv.max.x, uv.max.y],
        [uv.min.x, uv.min.y],
        [uv.max.x, uv.max.y],
        [uv.min.x, uv.max.y],
    ];
    for (position, uv) in positions.into_iter().zip(uvs) {
        out.push(RuntimeTextVertex {
            position_ndc: pixel_to_ndc(position, viewport),
            uv,
            color: quad.color.to_array(),
        });
    }
}

fn pixel_to_ndc(position: [f32; 2], viewport: [u32; 2]) -> [f32; 2] {
    [
        position[0] / viewport[0] as f32 * 2.0 - 1.0,
        1.0 - position[1] / viewport[1] as f32 * 2.0,
    ]
}

fn create_atlas_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    atlas: &FontAtlasBuild,
) -> wgpu::Texture {
    let size = wgpu::Extent3d {
        width: atlas.image.width,
        height: atlas.image.height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-runtime-text-sdf-atlas"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        atlas.image.pixels(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(atlas.image.width),
            rows_per_image: Some(atlas.image.height),
        },
        size,
    );
    texture
}

fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-runtime-text-bind-group-layout"),
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
    })
}

fn create_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-runtime-text-shader"),
        source: wgpu::ShaderSource::Wgsl(RUNTIME_TEXT_GPU_WGSL.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-runtime-text-pipeline-layout"),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("litd-runtime-text-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("runtime_text_vs"),
            buffers: &[vertex_layout()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("runtime_text_fs"),
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
    const ATTRS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4
    ];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<RuntimeTextVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRS,
    }
}

fn invalid_gpu<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(gpu_error(field, reason))
}

fn gpu_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("runtime_text_gpu.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "runtime_text_gpu_tests.rs"]
mod tests;
