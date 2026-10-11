//! Live preview render-to-texture for the no-code PBR material editor (#862).
//!
//! A [`PbrMaterial`] is lowered to a deterministic, fingerprinted GPU uniform
//! ([`PbrMaterialGpu`]) -- the frozen artifact the renderer consumes. The preview
//! draws an analytic sphere lit by one directional light plus ambient using the
//! same GGX model as the runtime PBR shader, renders headless into an off-screen
//! texture, and reads the pixels back so an editor (GUI or MCP) can show a live
//! material ball and an FSV can prove the material actually rendered.

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::{EngineError, gpu::GpuContext, math::Vec3};

use super::material::PbrMaterial;

#[path = "material_preview_gpu.rs"]
mod gpu;

use gpu::{
    bind_group_layout, buffer_init, copy_target_to_readback, map_readback, pipeline, render_target,
};

pub const MATERIAL_PREVIEW_WGSL: &str = include_str!("material_preview.wgsl");

/// Lowered, fingerprinted material uniform consumed by the renderer. `repr(C)`
/// and Pod so its exact bytes are the deterministic artifact; the WGSL
/// `PbrMaterialGpu` struct mirrors this layout field-for-field.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PbrMaterialGpu {
    pub base_color: [f32; 4],
    pub emissive: [f32; 4],
    /// metallic, roughness, normal_scale, occlusion_strength
    pub scalars: [f32; 4],
    /// texture_mask, has_base_color, has_normal, has_emissive
    pub flags: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct PreviewSceneGpu {
    light_dir: [f32; 4],
    light_radiance: [f32; 4],
    ambient: [f32; 4],
    /// width, height, aspect, sphere_radius
    viewport: [f32; 4],
}

impl PbrMaterial {
    /// Lower this authored material to the frozen GPU uniform the renderer draws.
    pub fn to_preview_gpu(&self) -> PbrMaterialGpu {
        use super::material::TextureSlot;
        PbrMaterialGpu {
            base_color: self.base_color_factor.to_array(),
            emissive: [
                self.emissive_factor.x,
                self.emissive_factor.y,
                self.emissive_factor.z,
                0.0,
            ],
            scalars: [
                self.metallic_factor,
                self.roughness_factor,
                self.normal_scale,
                self.occlusion_strength,
            ],
            flags: [
                self.texture_mask(),
                u32::from(self.has_texture(TextureSlot::BaseColor)),
                u32::from(self.has_texture(TextureSlot::Normal)),
                u32::from(self.has_texture(TextureSlot::Emissive)),
            ],
        }
    }

    /// Deterministic fingerprint of the lowered preview artifact: the SHA-256 of
    /// the exact uniform bytes the renderer uploads. Two materials that render
    /// identically share a fingerprint; any factor/map change shifts it.
    pub fn preview_fingerprint(&self) -> String {
        sha256_hex(bytemuck::bytes_of(&self.to_preview_gpu()))
    }
}

/// Preview scene configuration (lighting + target size). Distinct from the
/// material so the same material can be previewed under different studio lights.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MaterialPreviewSettings {
    pub width: u32,
    pub height: u32,
    pub light_direction: Vec3,
    pub light_radiance: Vec3,
    pub ambient: Vec3,
    pub sphere_radius: f32,
}

impl Default for MaterialPreviewSettings {
    fn default() -> Self {
        Self {
            width: 128,
            height: 128,
            light_direction: Vec3::new(0.4, 0.6, 1.0),
            light_radiance: Vec3::splat(3.0),
            ambient: Vec3::splat(0.08),
            sphere_radius: 0.85,
        }
    }
}

impl MaterialPreviewSettings {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.width == 0 || self.height == 0 {
            return Err(preview_error(
                "dimensions",
                "preview width and height must be non-zero",
            ));
        }
        if self.width > 4096 || self.height > 4096 {
            return Err(preview_error(
                "dimensions",
                "preview dimensions must be <= 4096",
            ));
        }
        if !self.light_direction.is_finite()
            || self.light_direction.length_squared() <= f32::EPSILON
        {
            return Err(preview_error(
                "light_direction",
                "light direction must be a finite non-zero vector",
            ));
        }
        if !self.light_radiance.is_finite() || !self.ambient.is_finite() {
            return Err(preview_error(
                "lighting",
                "light radiance and ambient must be finite",
            ));
        }
        if !(0.05..=0.98).contains(&self.sphere_radius) {
            return Err(preview_error(
                "sphere_radius",
                "sphere radius must be within [0.05, 0.98] of the frame",
            ));
        }
        Ok(())
    }

    fn scene_uniform(&self) -> PreviewSceneGpu {
        let dir = self.light_direction.normalize();
        PreviewSceneGpu {
            light_dir: [dir.x, dir.y, dir.z, 0.0],
            light_radiance: [
                self.light_radiance.x,
                self.light_radiance.y,
                self.light_radiance.z,
                0.0,
            ],
            ambient: [self.ambient.x, self.ambient.y, self.ambient.z, 0.0],
            viewport: [
                self.width as f32,
                self.height as f32,
                self.width as f32 / self.height as f32,
                self.sphere_radius,
            ],
        }
    }
}

/// Result of a headless preview render, with enough independently-checkable
/// statistics for an FSV to assert the ball actually rendered and that a
/// material edit changed the pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialPreviewFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
    pub foreground_pixels: u32,
    pub background_pixels: u32,
    pub foreground_mean_rgb: [f32; 3],
    pub mean_luminance: f32,
    pub uniform_fingerprint: String,
    pub adapter_name: String,
    pub backend: String,
    pub texture_format: String,
    pub readback_bytes: usize,
    pub row_pitch: u32,
}

impl MaterialPreviewFrame {
    /// SHA-256 over the full RGBA8 pixel buffer -- the pixel-level proof of what
    /// was drawn. Distinct materials yield distinct image hashes.
    pub fn image_sha256(&self) -> String {
        let flat: Vec<u8> = self.pixels.iter().flatten().copied().collect();
        sha256_hex(&flat)
    }
}

/// Render a live preview of `material` under `settings` on a fresh headless GPU
/// context (allocates an adapter; prefer [`render_material_preview_gpu_with_context`]
/// for throttled per-edit re-renders that reuse one device).
pub async fn render_material_preview_gpu(
    material: &PbrMaterial,
    settings: MaterialPreviewSettings,
) -> Result<MaterialPreviewFrame, EngineError> {
    material.validate()?;
    settings.validate()?;
    let context = GpuContext::new_headless().await?;
    render_material_preview_gpu_with_context(&context, material, settings).await
}

/// Render a preview reusing an existing GPU context (the throttled editor path).
pub async fn render_material_preview_gpu_with_context(
    context: &GpuContext,
    material: &PbrMaterial,
    settings: MaterialPreviewSettings,
) -> Result<MaterialPreviewFrame, EngineError> {
    material.validate()?;
    settings.validate()?;
    let device = context.device();
    let queue = context.queue();
    let format = wgpu::TextureFormat::Rgba8Unorm;

    let material_gpu = material.to_preview_gpu();
    let scene_gpu = settings.scene_uniform();
    let material_buffer = buffer_init(
        device,
        "litd-material-preview-material",
        bytemuck::bytes_of(&material_gpu),
    );
    let scene_buffer = buffer_init(
        device,
        "litd-material-preview-scene",
        bytemuck::bytes_of(&scene_gpu),
    );

    let target = render_target(device, settings, format);
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group_layout = bind_group_layout(device);
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("litd-material-preview-bind-group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: material_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: scene_buffer.as_entire_binding(),
            },
        ],
    });
    let pipeline = pipeline(device, &bind_group_layout, format);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("litd-material-preview-encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("litd-material-preview-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.025,
                        g: 0.029,
                        b: 0.038,
                        a: 0.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    let (readback, row_pitch, readback_bytes) =
        copy_target_to_readback(device, &mut encoder, &target, settings);
    queue.submit([encoder.finish()]);
    let pixels = map_readback(device, &readback, settings, row_pitch)?;

    let stats = summarize_pixels(&pixels);
    Ok(MaterialPreviewFrame {
        width: settings.width,
        height: settings.height,
        pixels,
        foreground_pixels: stats.foreground,
        background_pixels: stats.background,
        foreground_mean_rgb: stats.foreground_mean_rgb,
        mean_luminance: stats.mean_luminance,
        uniform_fingerprint: material.preview_fingerprint(),
        adapter_name: context.adapter_report().name.clone(),
        backend: context.adapter_report().backend.clone(),
        texture_format: format!("{format:?}"),
        readback_bytes,
        row_pitch,
    })
}

struct PixelStats {
    foreground: u32,
    background: u32,
    foreground_mean_rgb: [f32; 3],
    mean_luminance: f32,
}

fn summarize_pixels(pixels: &[[u8; 4]]) -> PixelStats {
    let mut foreground = 0u32;
    let mut sum = [0f64; 3];
    let mut lum = 0f64;
    for pixel in pixels {
        let luminance =
            0.2126 * pixel[0] as f64 + 0.7152 * pixel[1] as f64 + 0.0722 * pixel[2] as f64;
        lum += luminance;
        if pixel[3] > 127 {
            foreground += 1;
            sum[0] += pixel[0] as f64;
            sum[1] += pixel[1] as f64;
            sum[2] += pixel[2] as f64;
        }
    }
    let fg = foreground.max(1) as f64;
    PixelStats {
        foreground,
        background: pixels.len() as u32 - foreground,
        foreground_mean_rgb: [
            (sum[0] / fg) as f32,
            (sum[1] / fg) as f32,
            (sum[2] / fg) as f32,
        ],
        mean_luminance: (lum / pixels.len().max(1) as f64) as f32,
    }
}

fn preview_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::Rendering(
        litd_engine::error::RenderingError::MaterialCatalogInvalidConfig {
            field: format!("material_preview.{}", field.into()),
            reason: reason.into(),
        },
    )
}

#[cfg(test)]
#[path = "material_preview_tests.rs"]
mod tests;
