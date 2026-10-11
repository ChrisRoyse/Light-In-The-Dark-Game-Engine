//! wgpu-27 plumbing for the FSR upscale compute pass (#1741).

use std::sync::mpsc;

use wgpu::util::DeviceExt;

use crate::EngineError;

use super::{UPSCALE_WGSL, UpscaleGpuInput, UpscaleGpuUniform, upscale_error};

pub(super) struct UpscaleTextures {
    pub color: wgpu::Texture,
    pub motion: wgpu::Texture,
    pub history: wgpu::Texture,
    pub output: wgpu::Texture,
    pub sampler: wgpu::Sampler,
}

pub(super) fn upload_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    input: &UpscaleGpuInput,
) -> Result<UpscaleTextures, EngineError> {
    let color = sampled_texture(
        device,
        input.render_width,
        input.render_height,
        "litd-upscale-color",
    );
    let motion = sampled_texture(
        device,
        input.render_width,
        input.render_height,
        "litd-upscale-motion",
    );
    let history = sampled_texture(
        device,
        input.output_width,
        input.output_height,
        "litd-upscale-history",
    );
    let output = storage_texture(
        device,
        input.output_width,
        input.output_height,
        "litd-upscale-output",
    );
    queue.write_texture(
        color.as_image_copy(),
        &color_bytes(&input.color),
        copy_layout(input.render_width),
        extent(input.render_width, input.render_height),
    );
    queue.write_texture(
        motion.as_image_copy(),
        &motion_bytes(&input.motion),
        copy_layout(input.render_width),
        extent(input.render_width, input.render_height),
    );
    queue.write_texture(
        history.as_image_copy(),
        &history_bytes(input)?,
        copy_layout(input.output_width),
        extent(input.output_width, input.output_height),
    );
    Ok(UpscaleTextures {
        color,
        motion,
        history,
        output,
        sampler: device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("litd-upscale-linear-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        }),
    })
}

pub(super) fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-upscale-bind-layout"),
        entries: &[
            texture_entry(0),
            texture_entry(1),
            texture_entry(2),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
        ],
    })
}

pub(super) fn uniform_buffer(device: &wgpu::Device, uniform: &UpscaleGpuUniform) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("litd-upscale-uniform"),
        contents: bytemuck::bytes_of(uniform),
        usage: wgpu::BufferUsages::UNIFORM,
    })
}

pub(super) fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
) -> wgpu::ComputePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-upscale-shader"),
        source: wgpu::ShaderSource::Wgsl(UPSCALE_WGSL.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-upscale-pipeline-layout"),
        bind_group_layouts: &[layout],
        push_constant_ranges: &[],
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("litd-upscale-pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("litd_upscale_main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

pub(super) fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    textures: &UpscaleTextures,
    uniform: &wgpu::Buffer,
) -> wgpu::BindGroup {
    let color_view = textures
        .color
        .create_view(&wgpu::TextureViewDescriptor::default());
    let motion_view = textures
        .motion
        .create_view(&wgpu::TextureViewDescriptor::default());
    let history_view = textures
        .history
        .create_view(&wgpu::TextureViewDescriptor::default());
    let output_view = textures
        .output
        .create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-upscale-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&color_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&motion_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&history_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&textures.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&output_view),
            },
        ],
    })
}

pub(super) fn copy_output_to_readback(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    output: &wgpu::Texture,
    width: u32,
    height: u32,
) -> (wgpu::Buffer, u32, usize) {
    let row_pitch = align_to(width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let size = row_pitch as u64 * height as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("litd-upscale-output-readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        output.as_image_copy(),
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

pub(super) fn read_output(
    device: &wgpu::Device,
    readback: (wgpu::Buffer, u32, usize),
    width: u32,
    height: u32,
) -> Result<(Vec<[u8; 4]>, u32, usize), EngineError> {
    let (buffer, row_pitch, bytes) = readback;
    let mapped = map_readback_bytes(device, &buffer)?;
    let row_bytes = width as usize * 4;
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for row in mapped.chunks(row_pitch as usize).take(height as usize) {
        let source = row.get(..row_bytes).ok_or_else(|| {
            upscale_error(
                "upscale_gpu.output.layout",
                "mapped row shorter than the required RGBA8 width",
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
        return Err(upscale_error(
            "upscale_gpu.output.pixels",
            format!("expected {expected} pixels, read {}", pixels.len()),
        ));
    }
    Ok((pixels, row_pitch, bytes))
}

fn map_readback_bytes(
    device: &wgpu::Device,
    readback: &wgpu::Buffer,
) -> Result<Vec<u8>, EngineError> {
    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| upscale_error("upscale_gpu.readback.poll", error.to_string()))?;
    rx.recv()
        .map_err(|error| upscale_error("upscale_gpu.readback.callback", error.to_string()))?
        .map_err(|error| upscale_error("upscale_gpu.readback.map", format!("{error:?}")))?;
    let mapped = slice.get_mapped_range();
    let bytes = mapped.to_vec();
    drop(mapped);
    readback.unmap();
    Ok(bytes)
}

fn sampled_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    label: &'static str,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: extent(width, height),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn storage_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    label: &'static str,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: extent(width, height),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn color_bytes(color: &[[f32; 4]]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(color.len() * 4);
    for pixel in color {
        for c in pixel {
            bytes.push((c.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    bytes
}

fn motion_bytes(motion: &[[f32; 2]]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(motion.len() * 4);
    for pixel in motion {
        bytes.push(((pixel[0].clamp(-1.0, 1.0) * 0.5 + 0.5) * 255.0).round() as u8);
        bytes.push(((pixel[1].clamp(-1.0, 1.0) * 0.5 + 0.5) * 255.0).round() as u8);
        bytes.push(0);
        bytes.push(255);
    }
    bytes
}

fn history_bytes(input: &UpscaleGpuInput) -> Result<Vec<u8>, EngineError> {
    let count = (input.output_width as usize) * (input.output_height as usize);
    // First frame has no prior output; the shader ignores the history texture
    // (uniform first_frame=1), so a zeroed buffer of the correct size is correct.
    if input.first_frame {
        return Ok(vec![0u8; count * 4]);
    }
    // Non-first frame: a history buffer whose length does not match the output
    // pixel count is a caller error. Previously this silently substituted a
    // zeroed buffer, which discarded temporal history without any signal (a
    // fail-quiet path masked only by the upstream `validate_input`). Fail loud so
    // a mismatched history can never be silently reconstructed as if fresh.
    if input.history.len() != count {
        return Err(upscale_error(
            "upscale_gpu.history",
            format!(
                "history buffer has {} pixels, expected {count} output pixels; a mid-run \
                 resolution change must reset with first_frame=true instead of reusing stale history",
                input.history.len()
            ),
        ));
    }
    let mut bytes = Vec::with_capacity(count * 4);
    for pixel in &input.history {
        bytes.extend_from_slice(pixel);
    }
    Ok(bytes)
}

fn copy_layout(width: u32) -> wgpu::TexelCopyBufferLayout {
    wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(width * 4),
        rows_per_image: None,
    }
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

#[cfg(test)]
mod history_tests {
    use super::*;
    use crate::EngineError;

    fn input_2x2(first_frame: bool, history: Vec<[u8; 4]>) -> UpscaleGpuInput {
        UpscaleGpuInput {
            render_width: 2,
            render_height: 2,
            output_width: 2,
            output_height: 2,
            color: vec![[0.0; 4]; 4],
            motion: vec![[0.0; 2]; 4],
            history,
            first_frame,
        }
    }

    #[test]
    fn first_frame_history_is_zeroed_at_correct_size() {
        // No prior output on the first frame: a correctly sized zero buffer (the
        // shader ignores it) is the right result, not an error.
        let bytes = history_bytes(&input_2x2(true, Vec::new())).unwrap();
        assert_eq!(bytes.len(), 4 * 4);
        assert!(bytes.iter().all(|b| *b == 0));
    }

    #[test]
    fn mismatched_history_fails_loud_by_construction() {
        // 2x2 output = 4 pixels, but supply 3 pixels of history on a non-first
        // frame. BEFORE #1741 remediation this silently returned a zeroed buffer
        // (fail-quiet), discarding history without signal; AFTER it fails loud
        // independently of the upstream `validate_input` guard.
        match history_bytes(&input_2x2(false, vec![[1, 2, 3, 255]; 3])) {
            Err(EngineError::UpscaleInvalidConfig { field, .. }) => {
                assert_eq!(field, "upscale_gpu.history");
            }
            other => panic!("expected fail-loud UpscaleInvalidConfig, got {other:?}"),
        }
    }

    #[test]
    fn correct_history_passes_through_in_order() {
        let hist = vec![
            [1, 2, 3, 255],
            [4, 5, 6, 255],
            [7, 8, 9, 255],
            [10, 11, 12, 255],
        ];
        let bytes = history_bytes(&input_2x2(false, hist)).unwrap();
        assert_eq!(bytes.len(), 16);
        assert_eq!(&bytes[0..4], &[1, 2, 3, 255]);
        assert_eq!(&bytes[12..16], &[10, 11, 12, 255]);
    }
}
