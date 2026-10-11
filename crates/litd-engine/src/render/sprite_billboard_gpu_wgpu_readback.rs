use std::sync::mpsc;

use wgpu::util::DeviceExt;

use crate::{EngineError, render::sprite_billboard::sprite_billboard_error};

pub(super) fn target_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-sprite-billboard-output"),
        size: extent(width, height),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub(super) fn buffer(
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

pub(super) fn copy_target_to_readback(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> (wgpu::Buffer, u32, usize) {
    let row_pitch = align_to(width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let size = row_pitch as u64 * height as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("litd-sprite-billboard-readback"),
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

pub(super) fn map_image_readback(
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
        .map_err(|error| {
            sprite_billboard_error("gpu.map", format!("device poll failed: {error}"))
        })?;
    rx.recv()
        .map_err(|error| {
            sprite_billboard_error("gpu.map", format!("map callback failed: {error}"))
        })?
        .map_err(|error| {
            sprite_billboard_error("gpu.map", format!("buffer map failed: {error:?}"))
        })?;
    let mapped = slice.get_mapped_range();
    let row_bytes = width as usize * 4;
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for row in mapped.chunks(row_pitch as usize).take(height as usize) {
        let source = row.get(..row_bytes).ok_or_else(|| {
            sprite_billboard_error("gpu.layout", "mapped row was shorter than RGBA8 width")
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
        return Err(sprite_billboard_error(
            "gpu.pixels",
            format!("expected {expected} pixels, read {}", pixels.len()),
        ));
    }
    Ok(pixels)
}

pub(super) fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

pub(super) fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}
