use litd_hash::sha256_hex;
use std::{fs, path::Path};

use litd_engine::{
    EngineError,
    math::{Aabb, Vec2, Vec3},
    render::{
        batching::{BatchingSettings, Frustum, ScreenRect},
        vfx::{
            VfxEffectAsset, VfxGpuParticleSettings, VfxRenderWorkloadSettings,
            VfxSceneEffectInstance, build_vfx_render_workload, render_vfx_particles_gpu,
        },
    },
};

use super::VfxRenderedPreviewLens;

pub fn render_vfx_library_preview(
    effect: &VfxEffectAsset,
    output_path: impl AsRef<Path>,
) -> Result<VfxRenderedPreviewLens, EngineError> {
    let workload = preview_workload(effect)?;
    let frame = pollster::block_on(render_vfx_particles_gpu(
        VfxGpuParticleSettings {
            width: 640,
            height: 360,
            max_particles: 120_000,
        },
        &workload,
    ))?;
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            preview_error(
                "dir",
                format!("create preview dir {}: {error}", parent.display()),
            )
        })?;
    }
    frame.image.write_bmp(output_path)?;
    let bytes = fs::read(output_path).map_err(|error| {
        preview_error(
            "readback",
            format!("read {}: {error}", output_path.display()),
        )
    })?;
    Ok(VfxRenderedPreviewLens {
        image_path: output_path.display().to_string(),
        image_sha256: sha256_hex(&bytes),
        image_bytes: bytes.len() as u64,
        width: frame.stats.width,
        height: frame.stats.height,
        drawn_particles: frame.stats.drawn_particles,
        draw_calls: frame.stats.draw_calls,
        lit_pixel_ratio: frame.stats.metrics.lit_pixel_ratio,
        hot_pixel_ratio: frame.stats.metrics.hot_pixel_ratio,
    })
}

fn preview_workload(
    effect: &VfxEffectAsset,
) -> Result<litd_engine::render::vfx::VfxRenderWorkloadStats, EngineError> {
    let bounds = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(3.0))?;
    let frustum = Frustum::orthographic(Aabb::from_center_half_extents(
        Vec3::ZERO,
        Vec3::splat(8.0),
    )?)?;
    let scene = [VfxSceneEffectInstance {
        instance_id: 750,
        effect: effect.clone(),
        world_bounds: bounds,
        screen_rect: ScreenRect::new(Vec2::new(0.22, 0.16), Vec2::new(0.78, 0.86))?,
        nearest_depth: 0.35,
        distance_meters: 12.0,
    }];
    build_vfx_render_workload(
        VfxRenderWorkloadSettings {
            target_width: 640,
            target_height: 360,
            max_effect_instances: 8,
            max_simulated_particles: 120_000,
            max_particle_buffer_bytes: 64 * 1024 * 1024,
            particle_attribute_stride_bytes: 64,
            far_lod_start_meters: 80.0,
            far_lod_particle_scale: 0.25,
            batching: BatchingSettings {
                hzb_enabled: false,
                ..BatchingSettings::default()
            },
        },
        &frustum,
        &[],
        &scene,
    )
}

fn preview_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.vfx_library.preview.{}", field.into()),
        reason: reason.into(),
    }
}
