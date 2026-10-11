use wgpu::util::DeviceExt;

use crate::EngineError;

use super::{SCREEN_SPACE_WGSL, ScreenSpacePassKind};
use super::{ScreenSpaceGpuInputImage, ScreenSpaceGpuUniform, screen_gpu_error};

#[path = "screen_space_gpu_wgpu_support.rs"]
mod screen_space_gpu_wgpu_support;
use screen_space_gpu_wgpu_support::*;

pub(super) struct ScreenSpacePipelines {
    msaa_resolve: wgpu::RenderPipeline,
    ssao_gather: wgpu::RenderPipeline,
    ssao_blur: wgpu::RenderPipeline,
    motion_vectors: wgpu::RenderPipeline,
    stylized_ssr: wgpu::RenderPipeline,
    taa_resolve: wgpu::RenderPipeline,
    fxaa: wgpu::RenderPipeline,
}

pub(super) struct ScreenSpaceTextures {
    pub source: wgpu::Texture,
    pub history: wgpu::Texture,
    pub geometry: wgpu::Texture,
    pub ping: wgpu::Texture,
    pub pong: wgpu::Texture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextureSlot {
    Source,
    Ping,
    Pong,
}

pub(super) fn upload_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    input: &ScreenSpaceGpuInputImage,
) -> ScreenSpaceTextures {
    let source = input_texture(device, input.width, input.height, "litd-screen-source");
    let history = input_texture(device, input.width, input.height, "litd-screen-history");
    let geometry = input_texture(device, input.width, input.height, "litd-screen-geometry");
    queue.write_texture(
        source.as_image_copy(),
        &rgba_bytes(input.color_pixels(), input.width, input.height),
        copy_layout(input.width),
        extent(input.width, input.height),
    );
    queue.write_texture(
        history.as_image_copy(),
        &rgba_bytes(input.history_pixels(), input.width, input.height),
        copy_layout(input.width),
        extent(input.width, input.height),
    );
    queue.write_texture(
        geometry.as_image_copy(),
        &geometry_bytes(input),
        copy_layout(input.width),
        extent(input.width, input.height),
    );
    ScreenSpaceTextures {
        source,
        history,
        geometry,
        ping: target_texture(device, input.width, input.height, "litd-screen-ping"),
        pong: target_texture(device, input.width, input.height, "litd-screen-pong"),
    }
}

pub(super) fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("litd-screen-space-bind-layout"),
        entries: &[
            texture_layout(0),
            texture_layout(1),
            texture_layout(2),
            buffer_layout(3),
        ],
    })
}

pub(super) fn uniform_buffer(
    device: &wgpu::Device,
    uniform: &ScreenSpaceGpuUniform,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("litd-screen-space-uniform"),
        contents: bytemuck::bytes_of(uniform),
        usage: wgpu::BufferUsages::UNIFORM,
    })
}

pub(super) fn pipelines(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> ScreenSpacePipelines {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-screen-space-shader"),
        source: wgpu::ShaderSource::Wgsl(SCREEN_SPACE_WGSL.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-screen-space-pipeline-layout"),
        bind_group_layouts: &[layout],
        push_constant_ranges: &[],
    });
    let make = |fragment_entry, label| {
        pipeline(
            device,
            &shader,
            &pipeline_layout,
            format,
            fragment_entry,
            label,
        )
    };
    ScreenSpacePipelines {
        msaa_resolve: make(
            "litd_screen_msaa_resolve_fs",
            "litd-screen-msaa-resolve-pipeline",
        ),
        ssao_gather: make(
            "litd_screen_ssao_gather_fs",
            "litd-screen-ssao-gather-pipeline",
        ),
        ssao_blur: make("litd_screen_ssao_blur_fs", "litd-screen-ssao-blur-pipeline"),
        motion_vectors: make(
            "litd_screen_motion_vectors_fs",
            "litd-screen-motion-vectors-pipeline",
        ),
        stylized_ssr: make(
            "litd_screen_stylized_ssr_fs",
            "litd-screen-stylized-ssr-pipeline",
        ),
        taa_resolve: make(
            "litd_screen_taa_resolve_fs",
            "litd-screen-taa-resolve-pipeline",
        ),
        fxaa: make("litd_screen_fxaa_fs", "litd-screen-fxaa-pipeline"),
    }
}

pub(super) struct RenderPassArgs<'a> {
    pub(super) device: &'a wgpu::Device,
    pub(super) encoder: &'a mut wgpu::CommandEncoder,
    pub(super) layout: &'a wgpu::BindGroupLayout,
    pub(super) pipelines: &'a ScreenSpacePipelines,
    pub(super) textures: &'a ScreenSpaceTextures,
    pub(super) uniform: &'a wgpu::Buffer,
    pub(super) pass_kind: ScreenSpacePassKind,
    pub(super) current: TextureSlot,
}

pub(super) fn render_pass(args: RenderPassArgs<'_>) -> TextureSlot {
    let RenderPassArgs {
        device,
        encoder,
        layout,
        pipelines,
        textures,
        uniform,
        pass_kind,
        current,
    } = args;
    let target_slot = match current {
        TextureSlot::Pong => TextureSlot::Ping,
        _ => TextureSlot::Pong,
    };
    let current_view =
        texture_for_slot(textures, current).create_view(&wgpu::TextureViewDescriptor::default());
    let target_view = texture_for_slot(textures, target_slot)
        .create_view(&wgpu::TextureViewDescriptor::default());
    let history_view = textures
        .history
        .create_view(&wgpu::TextureViewDescriptor::default());
    let geometry_view = textures
        .geometry
        .create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = bind_group(
        device,
        layout,
        &current_view,
        &history_view,
        &geometry_view,
        uniform,
    );
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label_for_pass(pass_kind)),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipeline_for_pass(pipelines, pass_kind));
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    target_slot
}

pub(super) fn copy_slot_to_readback(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    textures: &ScreenSpaceTextures,
    slot: TextureSlot,
    width: u32,
    height: u32,
    label: &'static str,
) -> (wgpu::Buffer, u32, usize) {
    let row_pitch = align_to(width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let size = row_pitch as u64 * height as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        texture_for_slot(textures, slot).as_image_copy(),
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
    field: &'static str,
) -> Result<Vec<[u8; 4]>, EngineError> {
    let bytes = map_readback_bytes(device, readback, field)?;
    let row_bytes = width as usize * 4;
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for row in bytes.chunks(row_pitch as usize).take(height as usize) {
        let source = row.get(..row_bytes).ok_or_else(|| {
            screen_gpu_error(
                "screen_space_gpu.image.layout",
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
        return Err(screen_gpu_error(
            "screen_space_gpu.image.pixels",
            format!("expected {expected} pixels, read {}", pixels.len()),
        ));
    }
    Ok(pixels)
}

pub(super) fn label_for_pass(kind: ScreenSpacePassKind) -> &'static str {
    match kind {
        ScreenSpacePassKind::MsaaResolve => "litd-screen-msaa-resolve-pass",
        ScreenSpacePassKind::SsaoGather => "litd-screen-ssao-gather-pass",
        ScreenSpacePassKind::SsaoBlur => "litd-screen-ssao-blur-pass",
        ScreenSpacePassKind::MotionVectors => "litd-screen-motion-vectors-pass",
        ScreenSpacePassKind::StylizedSsr => "litd-screen-stylized-ssr-pass",
        ScreenSpacePassKind::TaaResolve => "litd-screen-taa-resolve-pass",
        ScreenSpacePassKind::Fxaa => "litd-screen-fxaa-pass",
    }
}
