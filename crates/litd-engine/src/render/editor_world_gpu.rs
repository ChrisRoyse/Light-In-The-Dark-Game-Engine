use crate::EngineError;

use super::{
    DEPTH_FORMAT, EDITOR_WORLD_WGSL, EditorWorldFoliageInstance, EditorWorldTextureData,
    EditorWorldVertex,
};

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 6] = [
    wgpu::VertexAttribute {
        offset: 0,
        shader_location: 0,
        format: wgpu::VertexFormat::Float32x3,
    },
    wgpu::VertexAttribute {
        offset: 12,
        shader_location: 1,
        format: wgpu::VertexFormat::Float32x3,
    },
    wgpu::VertexAttribute {
        offset: 24,
        shader_location: 2,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 40,
        shader_location: 3,
        format: wgpu::VertexFormat::Float32x2,
    },
    wgpu::VertexAttribute {
        offset: 48,
        shader_location: 4,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 64,
        shader_location: 5,
        format: wgpu::VertexFormat::Float32x4,
    },
];

const FOLIAGE_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 4] = [
    wgpu::VertexAttribute {
        offset: 0,
        shader_location: 6,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 16,
        shader_location: 7,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 32,
        shader_location: 8,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 48,
        shader_location: 9,
        format: wgpu::VertexFormat::Float32x4,
    },
];

pub(super) struct DepthTarget {
    pub(super) view: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl DepthTarget {
    pub(super) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("litd-editor-world-depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            width,
            height,
        }
    }

    pub(super) fn resize_if_needed(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.width != width || self.height != height {
            *self = Self::new(device, width, height);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextureColorSpace {
    Srgb,
    Linear,
}

pub(super) struct GpuEditorWorldTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl GpuEditorWorldTexture {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &EditorWorldTextureData,
        color_space: TextureColorSpace,
    ) -> Result<Self, EngineError> {
        let size = wgpu::Extent3d {
            width: data.width,
            height: data.height,
            depth_or_array_layers: 1,
        };
        let texture_label = format!("litd-editor-world-{}", data.label);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&texture_label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: match color_space {
                TextureColorSpace::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
                TextureColorSpace::Linear => wgpu::TextureFormat::Rgba8Unorm,
            },
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let bytes_per_row = data.width.checked_mul(4).ok_or_else(|| {
            EngineError::EditorWorldRendererInvalidConfig {
                field: data.label.clone(),
                reason: "texture width overflows rgba8 row byte count".to_string(),
            }
        })?;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data.rgba8,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(data.height),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Self {
            _texture: texture,
            view,
        })
    }
}

fn world_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("litd-editor-world-material-sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    })
}

pub(super) fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-editor-world-bind-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
        ],
    })
}

pub(super) fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    base_color_texture: &GpuEditorWorldTexture,
    normal_texture: &GpuEditorWorldTexture,
    metallic_roughness_texture: &GpuEditorWorldTexture,
) -> wgpu::BindGroup {
    let sampler = world_sampler(device);
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-editor-world-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&base_color_texture.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&normal_texture.view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&metallic_roughness_texture.view),
            },
        ],
    })
}

pub(super) fn pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    fog_bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    world_pipeline(WorldPipelineArgs {
        device,
        bind_group_layout,
        fog_bind_group_layout,
        format,
        label: "litd-editor-world-pipeline",
        vertex_entry: "litd_editor_world_vs",
        vertex_buffers: &[vertex_layout()],
        fragment_entry: "litd_editor_world_fs",
        blend: None,
        depth_write_enabled: true,
    })
}

pub(super) fn additive_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    fog_bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    world_pipeline(WorldPipelineArgs {
        device,
        bind_group_layout,
        fog_bind_group_layout,
        format,
        label: "litd-editor-world-additive-emissive-pipeline",
        vertex_entry: "litd_editor_world_vs",
        vertex_buffers: &[vertex_layout()],
        fragment_entry: "litd_editor_world_emissive_fs",
        blend: Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        }),
        depth_write_enabled: false,
    })
}

pub(super) fn foliage_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    fog_bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    world_pipeline(WorldPipelineArgs {
        device,
        bind_group_layout,
        fog_bind_group_layout,
        format,
        label: "litd-editor-world-instanced-foliage-pipeline",
        vertex_entry: "litd_editor_foliage_vs",
        vertex_buffers: &[vertex_layout(), foliage_instance_layout()],
        fragment_entry: "litd_editor_world_foliage_fs",
        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
        depth_write_enabled: false,
    })
}

struct WorldPipelineArgs<'a> {
    device: &'a wgpu::Device,
    bind_group_layout: &'a wgpu::BindGroupLayout,
    fog_bind_group_layout: &'a wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    label: &'static str,
    vertex_entry: &'static str,
    vertex_buffers: &'a [wgpu::VertexBufferLayout<'static>],
    fragment_entry: &'static str,
    blend: Option<wgpu::BlendState>,
    depth_write_enabled: bool,
}

fn world_pipeline(args: WorldPipelineArgs<'_>) -> wgpu::RenderPipeline {
    let WorldPipelineArgs {
        device,
        bind_group_layout,
        fog_bind_group_layout,
        format,
        label,
        vertex_entry,
        vertex_buffers,
        fragment_entry,
        blend,
        depth_write_enabled,
    } = args;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-editor-world-shader"),
        source: wgpu::ShaderSource::Wgsl(EDITOR_WORLD_WGSL.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-editor-world-pipeline-layout"),
        bind_group_layouts: &[bind_group_layout, fog_bind_group_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some(vertex_entry),
            buffers: vertex_buffers,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(fragment_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<EditorWorldVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &VERTEX_ATTRIBUTES,
    }
}

fn foliage_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<EditorWorldFoliageInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &FOLIAGE_INSTANCE_ATTRIBUTES,
    }
}
