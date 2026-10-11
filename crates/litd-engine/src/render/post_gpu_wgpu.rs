use std::sync::mpsc;

use wgpu::util::DeviceExt;

use crate::EngineError;

use super::super::POST_WGSL;
use super::{PostGpuInputImage, post_gpu_error};

#[path = "post_gpu_chain.rs"]
mod post_gpu_chain;

pub(super) use post_gpu_chain::{RenderPostChainArgs, render_post_chain};

pub(super) struct PostPipelines {
    bloom_prefilter: wgpu::RenderPipeline,
    bloom_downsample: wgpu::RenderPipeline,
    bloom_upsample: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
}

pub(super) fn source_texture(
    device: &wgpu::Device,
    input: &PostGpuInputImage,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-post-hdr-source"),
        size: texture_extent(input.width, input.height),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

pub(super) fn target_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-post-target"),
        size: texture_extent(width, height),
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
        label: Some("litd-post-bind-layout"),
        entries: &[
            texture_layout(0),
            texture_layout(1),
            buffer_layout(2),
            buffer_layout(3),
        ],
    })
}

pub(super) fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    source_view: &wgpu::TextureView,
    bloom_view: &wgpu::TextureView,
    settings: &wgpu::Buffer,
    grade: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-post-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(bloom_view),
            },
            bind_buffer(2, settings),
            bind_buffer(3, grade),
        ],
    })
}

pub(super) fn post_pipelines(
    device: &wgpu::Device,
    bind_layout: &wgpu::BindGroupLayout,
    bloom_format: wgpu::TextureFormat,
    target_format: wgpu::TextureFormat,
) -> PostPipelines {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-post-shader"),
        source: wgpu::ShaderSource::Wgsl(POST_WGSL.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-post-pipeline-layout"),
        bind_group_layouts: &[bind_layout],
        push_constant_ranges: &[],
    });
    PostPipelines {
        bloom_prefilter: pipeline(
            device,
            &shader,
            &pipeline_layout,
            bloom_format,
            "litd_post_bloom_prefilter_fs",
            "litd-post-bloom-prefilter-pipeline",
        ),
        bloom_downsample: pipeline(
            device,
            &shader,
            &pipeline_layout,
            bloom_format,
            "litd_post_bloom_downsample_fs",
            "litd-post-bloom-downsample-pipeline",
        ),
        bloom_upsample: pipeline(
            device,
            &shader,
            &pipeline_layout,
            bloom_format,
            "litd_post_bloom_upsample_fs",
            "litd-post-bloom-upsample-pipeline",
        ),
        composite: pipeline(
            device,
            &shader,
            &pipeline_layout,
            target_format,
            "litd_post_composite_fs",
            "litd-post-composite-pipeline",
        ),
    }
}

fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    pipeline_layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    fragment_entry: &'static str,
    label: &'static str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("litd_post_fullscreen_vs"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
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

pub(super) fn copy_target_to_readback(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::Texture,
    width: u32,
    height: u32,
) -> (wgpu::Buffer, u32, usize) {
    let row_pitch = align_to(width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let size = row_pitch as u64 * height as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("litd-post-image-readback"),
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
                rows_per_image: Some(height),
            },
        },
        texture_extent(width, height),
    );
    (readback, row_pitch, size as usize)
}

pub(super) fn map_image_readback(
    device: &wgpu::Device,
    readback: &wgpu::Buffer,
    width: u32,
    height: u32,
    row_pitch: u32,
) -> Result<Vec<[u8; 4]>, EngineError> {
    let bytes = map_readback_bytes(device, readback, "post_gpu.image.map")?;
    let row_bytes = width as usize * 4;
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for row in bytes.chunks(row_pitch as usize).take(height as usize) {
        let source = row.get(..row_bytes).ok_or_else(|| {
            post_gpu_error(
                "post_gpu.image.layout",
                "mapped row was shorter than the required RGBA8 width",
            )
        })?;
        pixels.extend(
            source
                .chunks_exact(4)
                .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]]),
        );
    }
    let expected = (width * height) as usize;
    if pixels.len() != expected {
        return Err(post_gpu_error(
            "post_gpu.image.pixels",
            format!("expected {expected} pixels, read {}", pixels.len()),
        ));
    }
    Ok(pixels)
}

pub(super) fn buffer_init(
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
    field: &'static str,
) -> Result<Vec<u8>, EngineError> {
    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| post_gpu_error(field, format!("device poll failed: {error}")))?;
    rx.recv()
        .map_err(|error| post_gpu_error(field, format!("map callback failed: {error}")))?
        .map_err(|error| post_gpu_error(field, format!("buffer map failed: {error:?}")))?;
    let mapped = slice.get_mapped_range();
    let bytes = mapped.to_vec();
    drop(mapped);
    readback.unmap();
    Ok(bytes)
}

fn bind_buffer(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

fn texture_layout(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn buffer_layout(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub(super) fn texture_extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}
