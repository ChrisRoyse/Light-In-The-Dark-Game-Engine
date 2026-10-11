use wgpu::util::DeviceExt;

use crate::{EngineError, gpu::GpuContext};

use super::{
    STYLIZED_EFFECTS_WGSL, StylizedGpuMaterialPass, StylizedGpuScene, identity_camera,
    stylized_gpu_error,
};

#[path = "stylized_gpu_wgpu_helpers.rs"]
mod stylized_gpu_wgpu_helpers;

use stylized_gpu_wgpu_helpers::{
    align_to, copy_layout, copy_target_to_readback, extent, map_image_readback,
};

const MASK_SIZE: u32 = 128;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct StylizedVertex {
    position: [f32; 3],
    uv: [f32; 2],
    normal: [f32; 3],
}

pub(super) struct StylizedGpuReadback {
    pub pixels: Vec<[u8; 4]>,
    pub row_pitch: u32,
    pub readback_bytes: usize,
    pub executed_fragment_entries: Vec<String>,
}

struct StylizedLayouts {
    camera: wgpu::BindGroupLayout,
    mask: wgpu::BindGroupLayout,
    params: wgpu::BindGroupLayout,
}

pub(super) fn render_material_passes(
    context: &GpuContext,
    scene: &StylizedGpuScene,
    passes: &[StylizedGpuMaterialPass],
) -> Result<StylizedGpuReadback, EngineError> {
    let device = context.device();
    let queue = context.queue();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = target_texture(device, scene.width, scene.height, format);
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let layouts = bind_group_layouts(device);
    let pipeline_layout = pipeline_layout(device, &layouts);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-stylized-effects-shader"),
        source: wgpu::ShaderSource::Wgsl(STYLIZED_EFFECTS_WGSL.into()),
    });
    let camera = uniform_buffer(device, &identity_camera(), "litd-stylized-camera");
    let camera_bind = camera_bind_group(device, &layouts.camera, &camera);
    let (mask_texture, sampler) = mask_texture(device, queue);
    let mask_view = mask_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mask_bind = mask_bind_group(device, &layouts.mask, &mask_view, &sampler);
    let vertices = vertex_buffer(device);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-stylized-gpu-encoder"),
    });
    let mut executed = Vec::with_capacity(passes.len());
    for (index, pass) in passes.iter().enumerate() {
        let pipeline = render_pipeline(device, &shader, &pipeline_layout, format, pass);
        let params = uniform_buffer(device, &pass.params, "litd-stylized-params");
        let params_bind = params_bind_group(device, &layouts.params, &params);
        {
            let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("litd-stylized-material-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if index == 0 {
                            wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.015,
                                g: 0.018,
                                b: 0.026,
                                a: 1.0,
                            })
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            render.set_viewport(
                pass.region[0] as f32,
                pass.region[1] as f32,
                pass.region[2] as f32,
                pass.region[3] as f32,
                0.0,
                1.0,
            );
            render.set_scissor_rect(
                pass.region[0],
                pass.region[1],
                pass.region[2],
                pass.region[3],
            );
            render.set_pipeline(&pipeline);
            render.set_bind_group(0, &camera_bind, &[]);
            render.set_bind_group(1, &mask_bind, &[]);
            render.set_bind_group(2, &params_bind, &[]);
            render.set_vertex_buffer(0, vertices.slice(..));
            render.draw(0..6, 0..1);
        }
        executed.push(pass.fragment_entry.to_string());
    }
    let (readback, row_pitch, readback_bytes) =
        copy_target_to_readback(device, &mut encoder, &target, scene.width, scene.height);
    queue.submit([encoder.finish()]);
    let pixels = map_image_readback(
        device,
        &readback,
        scene.width,
        scene.height,
        row_pitch,
        "stylized_gpu.output.map",
    )?;
    Ok(StylizedGpuReadback {
        pixels,
        row_pitch,
        readback_bytes,
        executed_fragment_entries: executed,
    })
}

fn target_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-stylized-output"),
        size: extent(width, height),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn bind_group_layouts(device: &wgpu::Device) -> StylizedLayouts {
    StylizedLayouts {
        camera: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("litd-stylized-camera-layout"),
            entries: &[buffer_layout(
                0,
                wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            )],
        }),
        mask: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("litd-stylized-mask-layout"),
            entries: &[texture_layout(0), sampler_layout(1)],
        }),
        params: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("litd-stylized-params-layout"),
            entries: &[buffer_layout(0, wgpu::ShaderStages::FRAGMENT)],
        }),
    }
}

fn pipeline_layout(device: &wgpu::Device, layouts: &StylizedLayouts) -> wgpu::PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-stylized-pipeline-layout"),
        bind_group_layouts: &[&layouts.camera, &layouts.mask, &layouts.params],
        push_constant_ranges: &[],
    })
}

fn render_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    pass: &StylizedGpuMaterialPass,
) -> wgpu::RenderPipeline {
    const ATTRS: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(pass.fragment_entry),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("litd_stylized_vs"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<StylizedVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &ATTRS,
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(pass.fragment_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn vertex_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    let vertices = [
        vertex([-1.0, -1.0, 0.0], [0.0, 1.0]),
        vertex([1.0, -1.0, 0.0], [1.0, 1.0]),
        vertex([1.0, 1.0, 0.0], [1.0, 0.0]),
        vertex([-1.0, -1.0, 0.0], [0.0, 1.0]),
        vertex([1.0, 1.0, 0.0], [1.0, 0.0]),
        vertex([-1.0, 1.0, 0.0], [0.0, 0.0]),
    ];
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("litd-stylized-quad-vertices"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

fn vertex(position: [f32; 3], uv: [f32; 2]) -> StylizedVertex {
    StylizedVertex {
        position,
        uv,
        normal: [0.2, 0.65, 0.74],
    }
}

fn uniform_buffer<T: bytemuck::Pod>(
    device: &wgpu::Device,
    value: &T,
    label: &'static str,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::bytes_of(value),
        usage: wgpu::BufferUsages::UNIFORM,
    })
}

fn mask_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> (wgpu::Texture, wgpu::Sampler) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-stylized-mask"),
        size: extent(MASK_SIZE, MASK_SIZE),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &mask_bytes(),
        copy_layout(MASK_SIZE),
        extent(MASK_SIZE, MASK_SIZE),
    );
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("litd-stylized-mask-sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..wgpu::SamplerDescriptor::default()
    });
    (texture, sampler)
}

fn mask_bytes() -> Vec<u8> {
    let row_pitch = align_to(MASK_SIZE * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) as usize;
    let mut bytes = Vec::with_capacity(row_pitch * MASK_SIZE as usize);
    for y in 0..MASK_SIZE {
        for x in 0..MASK_SIZE {
            let nx = x as f32 / (MASK_SIZE - 1) as f32;
            let ny = y as f32 / (MASK_SIZE - 1) as f32;
            let radial = 1.0 - ((nx - 0.5).powi(2) + (ny - 0.5).powi(2)).sqrt() * 1.45;
            let rings = ((nx * 19.0 + ny * 13.0).sin() * 0.5 + 0.5) * 0.30;
            let value = ((radial + rings).clamp(0.0, 1.0) * 255.0).round() as u8;
            bytes.extend_from_slice(&[value, value, value, 255]);
        }
        bytes.extend(std::iter::repeat_n(0, row_pitch - (MASK_SIZE * 4) as usize));
    }
    bytes
}

fn camera_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    camera: &wgpu::Buffer,
) -> wgpu::BindGroup {
    buffer_bind_group(device, layout, camera, "litd-stylized-camera-bind")
}

fn params_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    params: &wgpu::Buffer,
) -> wgpu::BindGroup {
    buffer_bind_group(device, layout, params, "litd-stylized-params-bind")
}

fn buffer_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
    label: &'static str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    })
}

fn mask_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-stylized-mask-bind"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn buffer_layout(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_layout(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_layout(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}
