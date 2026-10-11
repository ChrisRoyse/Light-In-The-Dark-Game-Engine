use std::sync::mpsc;

use wgpu::util::DeviceExt;

use crate::{EngineError, gpu::GpuContext};

use super::{PrimitiveMesh, primitive_error};

pub const PRIMITIVE_GALLERY_WGSL: &str = r#"
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn litd_primitive_vs(
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(position, 1.0);
    out.color = color;
    return out;
}

@fragment
fn litd_primitive_fs(in: VertexOut) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

pub(super) struct PrimitiveGpuReadback {
    pub pixels: Vec<[u8; 4]>,
    pub row_pitch: u32,
    pub readback_bytes: usize,
    pub draw_calls: u32,
    pub rendered_topologies: Vec<String>,
    pub regions: Vec<[u32; 4]>,
}

pub(super) fn render_primitive_meshes(
    context: &GpuContext,
    width: u32,
    height: u32,
    meshes: &[PrimitiveMesh],
) -> Result<PrimitiveGpuReadback, EngineError> {
    let device = context.device();
    let queue = context.queue();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = target_texture(device, width, height, format);
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-primitive-gallery-shader"),
        source: wgpu::ShaderSource::Wgsl(PRIMITIVE_GALLERY_WGSL.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-primitive-gallery-pipeline-layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-primitive-gallery-encoder"),
    });
    let regions = (0..meshes.len())
        .map(|index| stripe_region(index, meshes.len(), width, height))
        .collect::<Vec<_>>();
    let mut rendered_topologies = Vec::with_capacity(meshes.len());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("litd-primitive-gallery-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.012,
                        g: 0.015,
                        b: 0.022,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        for (mesh, region) in meshes.iter().zip(regions.iter()) {
            let pipeline = pipeline(device, &shader, &layout, format, mesh.topology.wgpu());
            let vertex_buffer = buffer(
                device,
                "litd-primitive-vertices",
                bytemuck::cast_slice(&mesh.vertices),
                wgpu::BufferUsages::VERTEX,
            );
            let index_buffer = buffer(
                device,
                "litd-primitive-indices",
                bytemuck::cast_slice(&mesh.indices),
                wgpu::BufferUsages::INDEX,
            );
            pass.set_scissor_rect(region[0], region[1], region[2], region[3]);
            pass.set_pipeline(&pipeline);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.indices.len() as u32, 0, 0..1);
            rendered_topologies.push(mesh.topology.id().to_string());
        }
    }
    let (readback, row_pitch, readback_bytes) =
        copy_target_to_readback(device, &mut encoder, &target, width, height);
    queue.submit([encoder.finish()]);
    let pixels = map_image_readback(device, &readback, width, height, row_pitch)?;
    Ok(PrimitiveGpuReadback {
        pixels,
        row_pitch,
        readback_bytes,
        draw_calls: meshes.len() as u32,
        rendered_topologies,
        regions,
    })
}

fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    topology: wgpu::PrimitiveTopology,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("litd-primitive-gallery-pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("litd_primitive_vs"),
            buffers: &[super::vertex_layout()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("litd_primitive_fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology,
            cull_mode: None,
            ..wgpu::PrimitiveState::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn target_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-primitive-gallery-output"),
        size: extent(width, height),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn buffer(
    device: &wgpu::Device,
    label: &'static str,
    bytes: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytes,
        usage,
    })
}

fn copy_target_to_readback(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> (wgpu::Buffer, u32, usize) {
    let row_pitch = align_to(width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let size = row_pitch as u64 * height as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("litd-primitive-gallery-readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_pitch),
                rows_per_image: Some(height),
            },
        },
        extent(width, height),
    );
    (readback, row_pitch, size as usize)
}

fn map_image_readback(
    device: &wgpu::Device,
    readback: &wgpu::Buffer,
    width: u32,
    height: u32,
    row_pitch: u32,
) -> Result<Vec<[u8; 4]>, EngineError> {
    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| primitive_error("gpu.map", format!("device poll failed: {error}")))?;
    rx.recv()
        .map_err(|error| primitive_error("gpu.map", format!("map callback failed: {error}")))?
        .map_err(|error| primitive_error("gpu.map", format!("buffer map failed: {error:?}")))?;
    let mapped = slice.get_mapped_range();
    let row_bytes = width as usize * 4;
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for row in mapped.chunks(row_pitch as usize).take(height as usize) {
        let source = row.get(..row_bytes).ok_or_else(|| {
            primitive_error(
                "gpu.layout",
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
    let expected = (width * height) as usize;
    if pixels.len() != expected {
        return Err(primitive_error(
            "gpu.pixels",
            format!("expected {expected} pixels, read {}", pixels.len()),
        ));
    }
    Ok(pixels)
}

fn stripe_region(index: usize, count: usize, width: u32, height: u32) -> [u32; 4] {
    let x0 = (index as u32 * width) / count as u32;
    let x1 = ((index as u32 + 1) * width) / count as u32;
    [x0, 0, x1 - x0, height]
}

fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}
