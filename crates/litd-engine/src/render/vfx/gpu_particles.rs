use std::sync::mpsc;

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use wgpu::util::DeviceExt;

use crate::{
    EngineError,
    gpu::GpuContext,
    render::vfx::{VfxRenderWorkloadStats, invalid_vfx, invalid_vfx_error},
};

#[path = "gpu_particles_image.rs"]
mod gpu_particles_image;

use gpu_particles_image::measure;
pub use gpu_particles_image::{VfxGpuParticleImage, VfxGpuParticleMetrics};

pub const VFX_GPU_PARTICLE_WGSL: &str = include_str!("gpu_particles.wgsl");

const QUAD: [[f32; 2]; 6] = [
    [-1.0, -1.0],
    [1.0, -1.0],
    [1.0, 1.0],
    [-1.0, -1.0],
    [1.0, 1.0],
    [-1.0, 1.0],
];

const DIAMOND: [[f32; 2]; 6] = [
    [0.0, -1.0],
    [1.0, 0.0],
    [0.0, 1.0],
    [0.0, -1.0],
    [0.0, 1.0],
    [-1.0, 0.0],
];

const QUAD_ATTRIBUTES: [wgpu::VertexAttribute; 1] = [wgpu::VertexAttribute {
    offset: 0,
    shader_location: 0,
    format: wgpu::VertexFormat::Float32x2,
}];

const PARTICLE_ATTRIBUTES: [wgpu::VertexAttribute; 2] = [
    wgpu::VertexAttribute {
        offset: 0,
        shader_location: 1,
        format: wgpu::VertexFormat::Float32x4,
    },
    wgpu::VertexAttribute {
        offset: 16,
        shader_location: 2,
        format: wgpu::VertexFormat::Float32x4,
    },
];

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ParticleInstance {
    center_size: [f32; 4],
    color: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VfxGpuParticleSettings {
    pub width: u32,
    pub height: u32,
    pub max_particles: u64,
}

impl Default for VfxGpuParticleSettings {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            max_particles: 500_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxGpuParticleFrame {
    pub image: VfxGpuParticleImage,
    pub stats: VfxGpuParticleStats,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxGpuParticleStats {
    pub gpu_context_id: u64,
    pub adapter_name: String,
    pub backend: String,
    pub texture_format: String,
    pub width: u32,
    pub height: u32,
    pub input_workload_particles: u64,
    pub drawn_particles: u32,
    pub sprite_particles: u32,
    pub mesh_particles: u32,
    pub draw_calls: u32,
    pub source_draw_batches: usize,
    pub source_visible_effects: usize,
    pub readback_bytes: usize,
    pub row_pitch: u32,
    pub image_checksum: String,
    pub metrics: VfxGpuParticleMetrics,
}

pub async fn render_vfx_particles_gpu(
    settings: VfxGpuParticleSettings,
    workload: &VfxRenderWorkloadStats,
) -> Result<VfxGpuParticleFrame, EngineError> {
    settings.validate(workload)?;
    let context = GpuContext::new_headless().await?;
    render_vfx_particles_gpu_with_context(&context, settings, workload).await
}

pub async fn render_vfx_particles_gpu_with_context(
    context: &GpuContext,
    settings: VfxGpuParticleSettings,
    workload: &VfxRenderWorkloadStats,
) -> Result<VfxGpuParticleFrame, EngineError> {
    settings.validate(workload)?;
    let device = context.device();
    let queue = context.queue();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let particles = particle_instances(workload)?;
    let target = render_target(device, settings, format);
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let quad_buffer = buffer_init(
        device,
        "litd-vfx-particle-quad",
        bytemuck::cast_slice(&QUAD),
        wgpu::BufferUsages::VERTEX,
    );
    let diamond_buffer = buffer_init(
        device,
        "litd-vfx-particle-diamond-mesh",
        bytemuck::cast_slice(&DIAMOND),
        wgpu::BufferUsages::VERTEX,
    );
    let sprite_buffer = buffer_init(
        device,
        "litd-vfx-sprite-particle-instances",
        bytemuck::cast_slice(&particles.sprites),
        wgpu::BufferUsages::VERTEX,
    );
    let mesh_buffer = buffer_init(
        device,
        "litd-vfx-mesh-particle-instances",
        bytemuck::cast_slice(&particles.meshes),
        wgpu::BufferUsages::VERTEX,
    );
    let pipeline = pipeline(device, format);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-vfx-particle-encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("litd-vfx-particle-pass"),
            color_attachments: &[Some(color_attachment(&target_view))],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, quad_buffer.slice(..));
        pass.set_vertex_buffer(1, sprite_buffer.slice(..));
        pass.draw(0..6, 0..particles.sprites.len() as u32);
        pass.set_vertex_buffer(0, diamond_buffer.slice(..));
        pass.set_vertex_buffer(1, mesh_buffer.slice(..));
        pass.draw(0..6, 0..particles.meshes.len() as u32);
    }
    let (readback, row_pitch, readback_bytes) =
        copy_target_to_readback(device, &mut encoder, &target, settings);
    queue.submit([encoder.finish()]);
    let pixels = map_readback(device, &readback, settings, row_pitch)?;
    let image = VfxGpuParticleImage::from_pixels(settings.width, settings.height, pixels);
    let metrics = measure(&image);
    let stats = VfxGpuParticleStats {
        gpu_context_id: context.context_id(),
        adapter_name: context.adapter_report().name.clone(),
        backend: context.adapter_report().backend.clone(),
        texture_format: format!("{format:?}"),
        width: settings.width,
        height: settings.height,
        input_workload_particles: workload.simulated_particles,
        drawn_particles: particles.total_len(),
        sprite_particles: particles.sprites.len() as u32,
        mesh_particles: particles.meshes.len() as u32,
        draw_calls: particles.draw_calls(),
        source_draw_batches: workload.draw_batches,
        source_visible_effects: workload.visible_effect_instances,
        readback_bytes,
        row_pitch,
        image_checksum: image.checksum(),
        metrics,
    };
    Ok(VfxGpuParticleFrame { image, stats })
}

impl VfxGpuParticleSettings {
    fn validate(&self, workload: &VfxRenderWorkloadStats) -> Result<(), EngineError> {
        if self.width < 320 || self.height < 180 {
            return invalid_vfx("gpu_particles.target_size", "must be at least 320x180");
        }
        if self.max_particles == 0 {
            return invalid_vfx("gpu_particles.max_particles", "must be non-zero");
        }
        if workload.simulated_particles == 0 {
            return invalid_vfx("gpu_particles.simulated_particles", "must be non-zero");
        }
        if workload.simulated_particles < 2 {
            return invalid_vfx(
                "gpu_particles.simulated_particles",
                "must contain at least two particles to exercise sprite and mesh paths",
            );
        }
        if workload.simulated_particles > self.max_particles {
            return invalid_vfx(
                "gpu_particles.simulated_particles",
                format!(
                    "workload has {} particles but GPU settings allow {}",
                    workload.simulated_particles, self.max_particles
                ),
            );
        }
        u32::try_from(workload.simulated_particles).map_err(|_| {
            invalid_vfx_error("gpu_particles.simulated_particles", "does not fit in u32")
        })?;
        Ok(())
    }
}

fn particle_instances(workload: &VfxRenderWorkloadStats) -> Result<ParticleBuckets, EngineError> {
    let count = usize::try_from(workload.simulated_particles)
        .map_err(|_| invalid_vfx_error("gpu_particles.instance_count", "does not fit in usize"))?;
    let mut sprites = Vec::with_capacity(count.saturating_sub(count / 5));
    let mut meshes = Vec::with_capacity(count / 5);
    let visible = workload.visible_effect_instances.max(1) as f32;
    for index in 0..count {
        let t = index as f32 / count.max(1) as f32;
        let ring = (index % 257) as f32 / 257.0;
        let angle = t * std::f32::consts::TAU * 37.0;
        let radius = 0.08 + 0.78 * ring.sqrt();
        let center_x = angle.cos() * radius * 0.92;
        let center_y = angle.sin() * radius * 0.68;
        let batch_band = ((index as f32 / visible).floor() % 6.0) / 6.0;
        let size = 0.003 + 0.009 * (1.0 - ring);
        let warm = [1.0, 0.42 + 0.28 * batch_band, 0.08, 0.075];
        let arcane = [0.18, 0.62, 1.0, 0.06];
        let color = if index % 5 == 0 { arcane } else { warm };
        let particle = ParticleInstance {
            center_size: [center_x, center_y, size, size * 1.35],
            color,
        };
        if index % 5 == 0 {
            meshes.push(particle);
        } else {
            sprites.push(particle);
        }
    }
    Ok(ParticleBuckets { sprites, meshes })
}

struct ParticleBuckets {
    sprites: Vec<ParticleInstance>,
    meshes: Vec<ParticleInstance>,
}

impl ParticleBuckets {
    fn total_len(&self) -> u32 {
        (self.sprites.len() + self.meshes.len()) as u32
    }

    fn draw_calls(&self) -> u32 {
        u32::from(!self.sprites.is_empty()) + u32::from(!self.meshes.is_empty())
    }
}

fn pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("litd-vfx-particle-shader"),
        source: wgpu::ShaderSource::Wgsl(VFX_GPU_PARTICLE_WGSL.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("litd-vfx-particle-pipeline-layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("litd-vfx-particle-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("litd_vfx_particle_vs"),
            buffers: &[quad_layout(), particle_layout()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("litd_vfx_particle_fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
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

fn render_target(
    device: &wgpu::Device,
    settings: VfxGpuParticleSettings,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("litd-vfx-particle-target"),
        size: texture_extent(settings),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn color_attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color {
                r: 0.012,
                g: 0.014,
                b: 0.022,
                a: 1.0,
            }),
            store: wgpu::StoreOp::Store,
        },
    }
}

fn copy_target_to_readback(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::Texture,
    settings: VfxGpuParticleSettings,
) -> (wgpu::Buffer, u32, usize) {
    let row_pitch = align_to(settings.width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let size = row_pitch as u64 * settings.height as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("litd-vfx-particle-readback"),
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
    settings: VfxGpuParticleSettings,
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
            invalid_vfx_error(
                "gpu_particles.readback.poll",
                format!("device poll failed: {error}"),
            )
        })?;
    rx.recv()
        .map_err(|error| {
            invalid_vfx_error(
                "gpu_particles.readback.map",
                format!("map callback failed: {error}"),
            )
        })?
        .map_err(|error| {
            invalid_vfx_error(
                "gpu_particles.readback.map",
                format!("buffer map failed: {error:?}"),
            )
        })?;
    let mapped = slice.get_mapped_range();
    let row_bytes = settings.width as usize * 4;
    let mut pixels = Vec::with_capacity((settings.width * settings.height) as usize);
    for row in mapped
        .chunks(row_pitch as usize)
        .take(settings.height as usize)
    {
        let source = row.get(..row_bytes).ok_or_else(|| {
            invalid_vfx_error(
                "gpu_particles.readback.layout",
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
        return Err(invalid_vfx_error(
            "gpu_particles.readback.pixels",
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

fn quad_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &QUAD_ATTRIBUTES,
    }
}

fn particle_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: 32,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &PARTICLE_ATTRIBUTES,
    }
}

fn texture_extent(settings: VfxGpuParticleSettings) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: settings.width,
        height: settings.height,
        depth_or_array_layers: 1,
    }
}

fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}
