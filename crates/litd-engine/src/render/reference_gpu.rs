use std::sync::mpsc;

use wgpu::util::DeviceExt;

#[path = "reference_gpu_scene.rs"]
mod reference_gpu_scene;

use crate::{EngineError, gpu::GpuContext};

use self::reference_gpu_scene::build_gpu_scene_buffers;
use super::{
    ReferenceSceneComparison, ReferenceSceneImage, ReferenceSceneSettings,
    build_reference_scene_plan, compare_reference_scene_image,
};

pub const GPU_REFERENCE_SCENE_WGSL: &str = include_str!("reference_gpu.wgsl");

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] = [
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
];

const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 6] = [
    wgpu::VertexAttribute {
        offset: 0,
        shader_location: 2,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 16,
        shader_location: 3,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 32,
        shader_location: 4,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 48,
        shader_location: 5,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 64,
        shader_location: 6,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 80,
        shader_location: 7,
        format: wgpu::VertexFormat::Float32x4,
    },
];

#[derive(Debug, Clone, PartialEq)]
pub struct GpuReferenceSceneFrame {
    pub image: ReferenceSceneImage,
    pub comparison: ReferenceSceneComparison,
    pub gpu_context_id: u64,
    pub adapter_name: String,
    pub backend: String,
    pub texture_format: String,
    pub mesh_count: u32,
    pub instance_count: u32,
    pub draw_calls: u32,
    pub readback_bytes: usize,
    pub row_pitch: u32,
}

pub async fn render_reference_scene_gpu(
    settings: ReferenceSceneSettings,
) -> Result<GpuReferenceSceneFrame, EngineError> {
    settings.validate()?;
    let context = GpuContext::new_headless().await?;
    render_reference_scene_gpu_with_context(&context, settings).await
}

pub async fn render_reference_scene_gpu_with_context(
    context: &GpuContext,
    settings: ReferenceSceneSettings,
) -> Result<GpuReferenceSceneFrame, EngineError> {
    settings.validate()?;
    let _plan = build_reference_scene_plan(settings)?;
    let device = context.device();
    let queue = context.queue();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let scene = build_gpu_scene_buffers(settings)?;
    let target = render_target(device, settings, format);
    let depth = depth_target(device, settings);
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    let vertex_buffer = buffer_init(
        device,
        "litd-reference-scene-vertices",
        &scene.vertex_bytes,
        wgpu::BufferUsages::VERTEX,
    );
    let instance_buffer = buffer_init(
        device,
        "litd-reference-scene-instances",
        &scene.instance_bytes,
        wgpu::BufferUsages::VERTEX,
    );
    let camera_buffer = buffer_init(
        device,
        "litd-reference-scene-camera",
        &scene.camera_bytes,
        wgpu::BufferUsages::UNIFORM,
    );
    let bind_group_layout = bind_group_layout(device);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-reference-scene-bind-group"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });
    let pipeline = pipeline(device, &bind_group_layout, format);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-reference-scene-encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("litd-reference-scene-pass"),
            color_attachments: &[Some(color_attachment(&target_view))],
            depth_stencil_attachment: Some(depth_attachment(&depth_view)),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, instance_buffer.slice(..));
        pass.draw(0..6, 0..scene.quad_instance_count);
        pass.draw(6..12, scene.quad_instance_count..scene.instance_count);
    }
    let (readback, row_pitch, readback_bytes) =
        copy_target_to_readback(device, &mut encoder, &target, settings);
    queue.submit([encoder.finish()]);
    let pixels = map_readback(device, &readback, settings, row_pitch)?;
    let image = ReferenceSceneImage::from_pixels(settings.width, settings.height, pixels);
    let comparison = compare_reference_scene_image(&image);
    Ok(GpuReferenceSceneFrame {
        image,
        comparison,
        gpu_context_id: context.context_id(),
        adapter_name: context.adapter_report().name.clone(),
        backend: context.adapter_report().backend.clone(),
        texture_format: format!("{format:?}"),
        mesh_count: 2,
        instance_count: scene.instance_count,
        draw_calls: 2,
        readback_bytes,
        row_pitch,
    })
}

fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-reference-scene-bind-layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

fn pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-reference-scene-shader"),
        source: wgpu::ShaderSource::Wgsl(GPU_REFERENCE_SCENE_WGSL.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-reference-scene-pipeline-layout"),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("litd-reference-scene-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("litd_reference_scene_vs"),
            buffers: &[vertex_layout(), instance_layout()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("litd_reference_scene_fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
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

fn color_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color {
                r: 0.025,
                g: 0.029,
                b: 0.038,
                a: 1.0,
            }),
            store: wgpu::StoreOp::Store,
        },
    }
}

fn depth_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassDepthStencilAttachment<'_> {
    wgpu::RenderPassDepthStencilAttachment {
        view,
        depth_ops: Some(wgpu::Operations {
            load: wgpu::LoadOp::Clear(1.0),
            store: wgpu::StoreOp::Store,
        }),
        stencil_ops: None,
    }
}

fn render_target(
    device: &wgpu::Device,
    settings: ReferenceSceneSettings,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-reference-scene-target"),
        size: texture_extent(settings),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn depth_target(device: &wgpu::Device, settings: ReferenceSceneSettings) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-reference-scene-depth"),
        size: texture_extent(settings),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

fn copy_target_to_readback(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::Texture,
    settings: ReferenceSceneSettings,
) -> (wgpu::Buffer, u32, usize) {
    let row_pitch = align_to(settings.width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let size = row_pitch as u64 * settings.height as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("litd-reference-scene-readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_pitch),
                rows_per_image: Some(settings.height),
            },
        },
        texture_extent(settings),
    );
    (readback, row_pitch, size as usize)
}

fn map_readback(
    device: &wgpu::Device,
    readback: &wgpu::Buffer,
    settings: ReferenceSceneSettings,
    row_pitch: u32,
) -> Result<Vec<[u8; 4]>, EngineError> {
    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|err| gpu_scene_error("readback.poll", format!("device poll failed: {err}")))?;
    rx.recv()
        .map_err(|err| gpu_scene_error("readback.map", format!("map callback failed: {err}")))?
        .map_err(|err| gpu_scene_error("readback.map", format!("buffer map failed: {err:?}")))?;
    let mapped = slice.get_mapped_range();
    let row_bytes = settings.width as usize * 4;
    let mut pixels = Vec::with_capacity((settings.width * settings.height) as usize);
    for row in mapped
        .chunks(row_pitch as usize)
        .take(settings.height as usize)
    {
        let source = row.get(..row_bytes).ok_or_else(|| {
            gpu_scene_error(
                "readback.layout",
                "mapped row was shorter than the required RGBA8 width",
            )
        })?;
        pixels.extend(
            source
                .chunks_exact(4)
                .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]]),
        );
    }
    drop(mapped);
    readback.unmap();
    let expected = (settings.width * settings.height) as usize;
    if pixels.len() != expected {
        return Err(gpu_scene_error(
            "readback.pixels",
            format!("expected {expected} pixels, read {}", pixels.len()),
        ));
    }
    Ok(pixels)
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

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: 24,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &VERTEX_ATTRIBUTES,
    }
}

fn instance_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: 96,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &INSTANCE_ATTRIBUTES,
    }
}

fn texture_extent(settings: ReferenceSceneSettings) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: settings.width,
        height: settings.height,
        depth_or_array_layers: 1,
    }
}

fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

fn gpu_scene_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::ReferenceSceneInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}
