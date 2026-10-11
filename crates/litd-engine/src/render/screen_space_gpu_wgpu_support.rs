//! Texture, pipeline, bind-group and byte-packing helpers for the screen-space GPU passes
//! (#1466 split of `screen_space_gpu_wgpu.rs`).
//!
//! Pure wgpu descriptor construction, row-padded pixel packing and buffer readback mapping.
//! The pass orchestration (`upload_textures`, `render_pass`, `pipelines`, ...) stays in
//! `screen_space_gpu_wgpu.rs`; these helpers are `pub(super)` so it calls them unchanged.

use std::sync::mpsc;

use super::super::{ScreenSpaceGpuInputImage, ScreenSpacePassKind, screen_gpu_error};
use super::{ScreenSpacePipelines, ScreenSpaceTextures, TextureSlot};
use crate::EngineError;

pub(super) fn input_texture(
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

pub(super) fn target_texture(
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
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub(super) fn texture_for_slot(
    textures: &ScreenSpaceTextures,
    slot: TextureSlot,
) -> &wgpu::Texture {
    match slot {
        TextureSlot::Source => &textures.source,
        TextureSlot::Ping => &textures.ping,
        TextureSlot::Pong => &textures.pong,
    }
}

pub(super) fn rgba_bytes(pixels: &[[f32; 4]], width: u32, height: u32) -> Vec<u8> {
    padded_rows(width, height, |index, bytes| {
        let pixel = pixels[index];
        bytes.extend_from_slice(&[
            unit_to_u8(pixel[0]),
            unit_to_u8(pixel[1]),
            unit_to_u8(pixel[2]),
            unit_to_u8(pixel[3]),
        ]);
    })
}

pub(super) fn geometry_bytes(input: &ScreenSpaceGpuInputImage) -> Vec<u8> {
    padded_rows(input.width, input.height, |index, bytes| {
        let depth = input.depth_pixels()[index];
        let motion = input.motion_pixels()[index];
        bytes.extend_from_slice(&[
            unit_to_u8(depth),
            unit_to_u8(motion[0] * 0.5 + 0.5),
            unit_to_u8(motion[1] * 0.5 + 0.5),
            255,
        ]);
    })
}

pub(super) fn padded_rows(
    width: u32,
    height: u32,
    mut push_pixel: impl FnMut(usize, &mut Vec<u8>),
) -> Vec<u8> {
    let row_bytes = width as usize * 4;
    let row_pitch = align_to(width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) as usize;
    let mut bytes = Vec::with_capacity(row_pitch * height as usize);
    for y in 0..height as usize {
        for x in 0..width as usize {
            push_pixel(y * width as usize + x, &mut bytes);
        }
        bytes.extend(std::iter::repeat_n(0, row_pitch - row_bytes));
    }
    bytes
}

pub(super) fn unit_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub(super) fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    fragment_entry: &'static str,
    label: &'static str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("litd_screen_fullscreen_vs"),
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

pub(super) fn pipeline_for_pass(
    pipelines: &ScreenSpacePipelines,
    kind: ScreenSpacePassKind,
) -> &wgpu::RenderPipeline {
    match kind {
        ScreenSpacePassKind::MsaaResolve => &pipelines.msaa_resolve,
        ScreenSpacePassKind::SsaoGather => &pipelines.ssao_gather,
        ScreenSpacePassKind::SsaoBlur => &pipelines.ssao_blur,
        ScreenSpacePassKind::MotionVectors => &pipelines.motion_vectors,
        ScreenSpacePassKind::StylizedSsr => &pipelines.stylized_ssr,
        ScreenSpacePassKind::TaaResolve => &pipelines.taa_resolve,
        ScreenSpacePassKind::Fxaa => &pipelines.fxaa,
    }
}

pub(super) fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    source: &wgpu::TextureView,
    history: &wgpu::TextureView,
    geometry: &wgpu::TextureView,
    uniform: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-screen-space-bind-group"),
        layout,
        entries: &[
            texture_entry(0, source),
            texture_entry(1, history),
            texture_entry(2, geometry),
            wgpu::BindGroupEntry {
                binding: 3,
                resource: uniform.as_entire_binding(),
            },
        ],
    })
}

pub(super) fn texture_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

pub(super) fn texture_layout(binding: u32) -> wgpu::BindGroupLayoutEntry {
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

pub(super) fn buffer_layout(binding: u32) -> wgpu::BindGroupLayoutEntry {
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
        .map_err(|error| screen_gpu_error(field, format!("device poll failed: {error}")))?;
    rx.recv()
        .map_err(|error| screen_gpu_error(field, format!("map callback failed: {error}")))?
        .map_err(|error| screen_gpu_error(field, format!("buffer map failed: {error:?}")))?;
    let mapped = slice.get_mapped_range();
    let bytes = mapped.to_vec();
    drop(mapped);
    readback.unmap();
    Ok(bytes)
}

pub(super) fn copy_layout(width: u32) -> wgpu::TexelCopyBufferLayout {
    wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(align_to(width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)),
        rows_per_image: None,
    }
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
