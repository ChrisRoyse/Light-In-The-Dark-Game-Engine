//! Low-level wgpu plumbing for the material live preview (#862): pipeline,
//! off-screen target, and texture->buffer readback. Kept separate from the
//! preview capability surface so each file stays focused and under the ratchet.

use std::sync::mpsc;

use wgpu::util::DeviceExt;

use crate::EngineError;

use super::{MATERIAL_PREVIEW_WGSL, MaterialPreviewSettings, preview_error};

pub(super) fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let uniform = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-material-preview-bind-layout"),
        entries: &[uniform(0), uniform(1)],
    })
}

pub(super) fn pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-material-preview-shader"),
        source: wgpu::ShaderSource::Wgsl(MATERIAL_PREVIEW_WGSL.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-material-preview-pipeline-layout"),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("litd-material-preview-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("litd_material_preview_vs"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("litd_material_preview_fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
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
    settings: MaterialPreviewSettings,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-material-preview-target"),
        size: texture_extent(settings),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub(super) fn buffer_init(
    device: &wgpu::Device,
    label: &'static str,
    contents: &[u8],
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage: wgpu::BufferUsages::UNIFORM,
    })
}

pub(super) fn copy_target_to_readback(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::Texture,
    settings: MaterialPreviewSettings,
) -> (wgpu::Buffer, u32, usize) {
    let row_pitch = align_to(settings.width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let size = row_pitch as u64 * settings.height as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("litd-material-preview-readback"),
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

pub(super) fn map_readback(
    device: &wgpu::Device,
    readback: &wgpu::Buffer,
    settings: MaterialPreviewSettings,
    row_pitch: u32,
) -> Result<Vec<[u8; 4]>, EngineError> {
    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|err| preview_error("readback.poll", format!("device poll failed: {err}")))?;
    rx.recv()
        .map_err(|err| preview_error("readback.map", format!("map callback failed: {err}")))?
        .map_err(|err| preview_error("readback.map", format!("buffer map failed: {err:?}")))?;
    let mapped = slice.get_mapped_range();
    let row_bytes = settings.width as usize * 4;
    let mut pixels = Vec::with_capacity((settings.width * settings.height) as usize);
    for row in mapped
        .chunks(row_pitch as usize)
        .take(settings.height as usize)
    {
        let source = row.get(..row_bytes).ok_or_else(|| {
            preview_error(
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
        return Err(preview_error(
            "readback.pixels",
            format!("expected {expected} pixels, read {}", pixels.len()),
        ));
    }
    Ok(pixels)
}

pub(super) fn texture_extent(settings: MaterialPreviewSettings) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: settings.width,
        height: settings.height,
        depth_or_array_layers: 1,
    }
}

fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}
