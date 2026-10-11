use std::sync::mpsc;

use wgpu::util::DeviceExt;

use super::*;

pub(super) fn compute_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-forward-plus-compute-bind-layout"),
        entries: &[
            buffer_layout(
                0,
                wgpu::ShaderStages::COMPUTE,
                wgpu::BufferBindingType::Uniform,
            ),
            storage_layout(1, wgpu::ShaderStages::COMPUTE, true),
            storage_layout(2, wgpu::ShaderStages::COMPUTE, true),
            storage_layout(3, wgpu::ShaderStages::COMPUTE, false),
            storage_layout(4, wgpu::ShaderStages::COMPUTE, false),
        ],
    })
}

pub(super) fn render_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-forward-plus-render-bind-layout"),
        entries: &[
            buffer_layout(
                0,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BufferBindingType::Uniform,
            ),
            storage_layout(1, wgpu::ShaderStages::FRAGMENT, true),
            storage_layout(2, wgpu::ShaderStages::FRAGMENT, true),
            storage_layout(3, wgpu::ShaderStages::FRAGMENT, true),
            buffer_layout(
                4,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BufferBindingType::Uniform,
            ),
        ],
    })
}

pub(super) fn compute_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    params: &wgpu::Buffer,
    bounds: &wgpu::Buffer,
    lights: &wgpu::Buffer,
    headers: &wgpu::Buffer,
    indices: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-forward-plus-compute-bind-group"),
        layout,
        entries: &[
            bind(0, params),
            bind(1, bounds),
            bind(2, lights),
            bind(3, headers),
            bind(4, indices),
        ],
    })
}

pub(super) fn render_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    params: &wgpu::Buffer,
    lights: &wgpu::Buffer,
    ambient: &wgpu::Buffer,
    headers: &wgpu::Buffer,
    indices: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-forward-plus-render-bind-group"),
        layout,
        entries: &[
            bind(0, params),
            bind(1, lights),
            bind(2, headers),
            bind(3, indices),
            bind(4, ambient),
        ],
    })
}

pub(super) fn compute_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
) -> wgpu::ComputePipeline {
    let shader = shader(device);
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-forward-plus-compute-pipeline-layout"),
        bind_group_layouts: &[layout],
        push_constant_ranges: &[],
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("litd-forward-plus-compute-pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("litd_cluster_lights"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

pub(super) fn render_pipeline(
    device: &wgpu::Device,
    empty_layout: &wgpu::BindGroupLayout,
    render_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = shader(device);
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-forward-plus-render-pipeline-layout"),
        bind_group_layouts: &[empty_layout, render_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("litd-forward-plus-render-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("litd_forward_plus_vs"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("litd_forward_plus_fs"),
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

pub(super) fn render_target(
    device: &wgpu::Device,
    settings: ForwardPlusGpuSettings,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-forward-plus-target"),
        size: texture_extent(settings),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub(super) fn color_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
            store: wgpu::StoreOp::Store,
        },
    }
}

pub(super) fn copy_target_to_readback(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::Texture,
    settings: ForwardPlusGpuSettings,
) -> (wgpu::Buffer, u32, usize) {
    let row_pitch = align_to(settings.width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let size = row_pitch as u64 * settings.height as u64;
    let readback = readback_buffer(device, "litd-forward-plus-image-readback", size);
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

pub(super) fn map_image_readback(
    device: &wgpu::Device,
    readback: &wgpu::Buffer,
    settings: ForwardPlusGpuSettings,
    row_pitch: u32,
) -> Result<Vec<[u8; 4]>, EngineError> {
    let bytes = map_readback_bytes(device, readback, "forward_plus.image.map")?;
    let row_bytes = settings.width as usize * 4;
    let mut pixels = Vec::with_capacity((settings.width * settings.height) as usize);
    for row in bytes
        .chunks(row_pitch as usize)
        .take(settings.height as usize)
    {
        let source = row.get(..row_bytes).ok_or_else(|| {
            invalid_lighting_error(
                "forward_plus.image.layout",
                "mapped row was shorter than the required RGBA8 width",
            )
        })?;
        pixels.extend(
            source
                .chunks_exact(4)
                .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]]),
        );
    }
    let expected = (settings.width * settings.height) as usize;
    if pixels.len() != expected {
        return Err(invalid_lighting_error(
            "forward_plus.image.pixels",
            format!("expected {expected} pixels, read {}", pixels.len()),
        ));
    }
    Ok(pixels)
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
        .map_err(|error| invalid_lighting_error(field, format!("device poll failed: {error}")))?;
    rx.recv()
        .map_err(|error| invalid_lighting_error(field, format!("map callback failed: {error}")))?
        .map_err(|error| invalid_lighting_error(field, format!("buffer map failed: {error:?}")))?;
    let mapped = slice.get_mapped_range();
    let bytes = mapped.to_vec();
    drop(mapped);
    readback.unmap();
    Ok(bytes)
}

pub(super) fn parse_pods<T: Pod>(bytes: &[u8]) -> Vec<T> {
    bytemuck::cast_slice(bytes).to_vec()
}

pub(super) fn readback_buffer(
    device: &wgpu::Device,
    label: &'static str,
    size: u64,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
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

fn shader(device: &wgpu::Device) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-forward-plus-shader"),
        source: wgpu::ShaderSource::Wgsl(FORWARD_PLUS_WGSL.into()),
    })
}

fn bind(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

fn buffer_layout(
    binding: u32,
    visibility: wgpu::ShaderStages,
    ty: wgpu::BufferBindingType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_layout(
    binding: u32,
    visibility: wgpu::ShaderStages,
    read_only: bool,
) -> wgpu::BindGroupLayoutEntry {
    buffer_layout(
        binding,
        visibility,
        wgpu::BufferBindingType::Storage { read_only },
    )
}

fn texture_extent(settings: ForwardPlusGpuSettings) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: settings.width,
        height: settings.height,
        depth_or_array_layers: 1,
    }
}

fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}
