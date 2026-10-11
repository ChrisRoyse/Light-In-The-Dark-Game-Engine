use litd_hash::sha256_hex;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use litd_engine::{
    EngineError,
    diagnostics::ErrorReport,
    math::{Aabb, Vec2, Vec3},
    render::{
        batching::{BatchingSettings, Frustum, ScreenRect},
        vfx::{
            VFX_EFFECT_SCHEMA_VERSION, VfxBlendMode, VfxColor, VfxCurve, VfxCurvePoint,
            VfxEffectAsset, VfxEmitter, VfxGpuParticleSettings, VfxGradient, VfxGradientStop,
            VfxInitializeModule, VfxOutputModule, VfxRange, VfxRenderWorkloadSettings,
            VfxSceneEffectInstance, VfxShape, VfxSpawnModule, VfxUpdateModule, VfxVelocity,
            build_vfx_render_workload, render_vfx_particles_gpu,
        },
    },
};
use serde_json::json;

fn main() -> Result<(), EngineError> {
    litd_engine::init_logging();
    pollster::block_on(run())
}

async fn run() -> Result<(), EngineError> {
    let root = fsv_root()?;
    fs::create_dir_all(&root).map_err(|error| EngineError::VfxInvalidConfig {
        field: "render.vfx.fsv.root".to_string(),
        reason: format!("failed to create {}: {error}", root.display()),
    })?;
    let image_path = root.join("vfx-gpu-particles.bmp");
    let report_path = root.join("vfx-gpu-particles-fsv-readback.json");

    let workload_settings = workload_settings();
    let gpu_settings = VfxGpuParticleSettings {
        width: 1280,
        height: 720,
        max_particles: 100_000,
    };
    let scene = dense_scene(128, Vec3::new(0.0, 0.0, 4.0))?;
    let workload = build_vfx_render_workload(workload_settings, &frustum()?, &[], &scene)?;
    let before = json!({
        "gpu_settings": gpu_settings,
        "workload": {
            "effects": workload.input_effect_instances,
            "visible_effects": workload.visible_effect_instances,
            "particles": workload.simulated_particles,
            "draw_batches": workload.draw_batches
        }
    });
    let frame = render_vfx_particles_gpu(gpu_settings, &workload).await?;
    if frame.stats.metrics.lit_pixel_ratio <= 0.01 || frame.stats.metrics.hot_pixel_ratio <= 0.001 {
        return Err(EngineError::VfxInvalidConfig {
            field: "render.vfx.gpu_particles.metrics".to_string(),
            reason: format!(
                "rendered image metrics indicate blank/failed output: {:?}",
                frame.stats.metrics
            ),
        });
    }
    frame.image.write_bmp(&image_path)?;
    let image_state = image_state(&image_path)?;

    let edge_over_max = edge_over_max(gpu_settings, &workload).await?;
    let edge_zero_workload = edge_zero_workload(gpu_settings).await?;
    let edge_invalid_target = edge_invalid_target(&workload).await?;
    let report = json!({
        "issue": 1129,
        "source_of_truth": {
            "fsv_report": report_path,
            "gpu_image": image_path,
            "gpu_stats": "persisted inside happy_path.after.stats after texture readback",
            "workload_contract": "render::vfx::build_vfx_render_workload from #746"
        },
        "happy_path": {
            "before": before,
            "after": {
                "stats": frame.stats,
                "image": image_state
            }
        },
        "edges": {
            "over_max_particles": edge_over_max,
            "zero_workload": edge_zero_workload,
            "invalid_target_size": edge_invalid_target
        }
    });
    let bytes =
        serde_json::to_vec_pretty(&report).map_err(|error| EngineError::SerializationFailed {
            format: "issue1129-vfx-gpu-particles-fsv-json",
            reason: error.to_string(),
        })?;
    fs::write(&report_path, &bytes).map_err(|error| EngineError::VfxInvalidConfig {
        field: "render.vfx.fsv.write".to_string(),
        reason: format!("failed to write {}: {error}", report_path.display()),
    })?;
    let persisted = fs::read(&report_path).map_err(|error| EngineError::VfxInvalidConfig {
        field: "render.vfx.fsv.read".to_string(),
        reason: format!("failed to read {}: {error}", report_path.display()),
    })?;
    if persisted != bytes {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: format!("{} bytes", bytes.len()),
            actual: format!("{} bytes", persisted.len()),
        });
    }
    let persisted_json: serde_json::Value =
        serde_json::from_slice(&persisted).map_err(|error| EngineError::SerializationFailed {
            format: "issue1129-vfx-gpu-particles-fsv-readback-json",
            reason: error.to_string(),
        })?;
    if persisted_json["issue"] != 1129
        || persisted_json["happy_path"]["after"]["stats"]["drawn_particles"] != 65536
        || persisted_json["edges"]["over_max_particles"]["after"]["result"]["status"] != "rejected"
    {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: "issue 1129 GPU report with 65536 particles and rejected edge".to_string(),
            actual: persisted_json.to_string(),
        });
    }
    println!(
        "issue1129 vfx_gpu_particles_fsv report={} report_bytes={} report_sha256={} image={} image_bytes={} image_sha256={} particles={} draw_calls={} lit_ratio={}",
        report_path.display(),
        persisted.len(),
        sha256_hex(&persisted),
        image_path.display(),
        image_state["bytes"],
        image_state["sha256"],
        persisted_json["happy_path"]["after"]["stats"]["drawn_particles"],
        persisted_json["happy_path"]["after"]["stats"]["draw_calls"],
        persisted_json["happy_path"]["after"]["stats"]["metrics"]["lit_pixel_ratio"]
    );
    Ok(())
}

async fn edge_over_max(
    mut settings: VfxGpuParticleSettings,
    workload: &litd_engine::render::vfx::VfxRenderWorkloadStats,
) -> Result<serde_json::Value, EngineError> {
    settings.max_particles = 1024;
    let result = render_vfx_particles_gpu(settings, workload).await;
    Ok(json!({
        "before": { "max_particles": 1024, "workload_particles": workload.simulated_particles },
        "after": { "result": error_result(result) }
    }))
}

async fn edge_zero_workload(
    settings: VfxGpuParticleSettings,
) -> Result<serde_json::Value, EngineError> {
    let scene = vec![scene_effect(
        901,
        Vec3::new(12.0, 0.0, 4.0),
        25.0,
        rect(0.2, 0.2, 0.3, 0.3)?,
    )?];
    let workload = build_vfx_render_workload(workload_settings(), &frustum()?, &[], &scene)?;
    let result = render_vfx_particles_gpu(settings, &workload).await;
    Ok(json!({
        "before": {
            "frustum_culled_effects": workload.frustum_culled_effect_ids,
            "simulated_particles": workload.simulated_particles
        },
        "after": { "result": error_result(result) }
    }))
}

async fn edge_invalid_target(
    workload: &litd_engine::render::vfx::VfxRenderWorkloadStats,
) -> Result<serde_json::Value, EngineError> {
    let settings = VfxGpuParticleSettings {
        width: 1,
        height: 1,
        max_particles: 100_000,
    };
    let result = render_vfx_particles_gpu(settings, workload).await;
    Ok(json!({
        "before": { "width": 1, "height": 1 },
        "after": { "result": error_result(result) }
    }))
}

fn workload_settings() -> VfxRenderWorkloadSettings {
    VfxRenderWorkloadSettings {
        max_simulated_particles: 100_000,
        max_particle_buffer_bytes: 16 * 1024 * 1024,
        batching: BatchingSettings {
            max_draw_batches: 16,
            ..BatchingSettings::default()
        },
        ..VfxRenderWorkloadSettings::default()
    }
}

fn dense_scene(count: u64, center: Vec3) -> Result<Vec<VfxSceneEffectInstance>, EngineError> {
    (1..=count)
        .map(|id| {
            scene_effect(
                id,
                center + Vec3::new((id % 16) as f32 * 0.04, 0.0, 0.0),
                25.0,
                rect(0.08, 0.08, 0.92, 0.92)?,
            )
        })
        .collect()
}

fn scene_effect(
    id: u64,
    center: Vec3,
    distance_meters: f32,
    screen_rect: ScreenRect,
) -> Result<VfxSceneEffectInstance, EngineError> {
    Ok(VfxSceneEffectInstance {
        instance_id: id,
        effect: spark_effect(&format!("issue1129.spark.{id}"))?,
        world_bounds: Aabb::from_center_half_extents(center, Vec3::splat(0.5))?,
        screen_rect,
        nearest_depth: 0.1,
        distance_meters,
    })
}

fn spark_effect(effect_id: &str) -> Result<VfxEffectAsset, EngineError> {
    Ok(VfxEffectAsset {
        schema_version: VFX_EFFECT_SCHEMA_VERSION,
        effect_id: effect_id.to_string(),
        display_name: "GPU Spark".to_string(),
        presentation_only: true,
        emitters: vec![VfxEmitter {
            emitter_id: "emitter.gpu.spark".to_string(),
            display_name: "GPU Spark".to_string(),
            max_particles: 512,
            local_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(1.0))?,
            spawn: vec![VfxSpawnModule::ConstantRate {
                particles_per_second: 512.0,
            }],
            initialize: vec![
                VfxInitializeModule::Shape(VfxShape::Sphere { radius_meters: 0.1 }),
                VfxInitializeModule::Velocity(VfxVelocity::Radial {
                    speed_meters_per_second: VfxRange::new(1.0, 3.0),
                }),
                VfxInitializeModule::Color {
                    color: VfxColor::rgba(1.0, 0.6, 0.1, 1.0),
                },
                VfxInitializeModule::Size {
                    pixels: VfxRange::new(4.0, 12.0),
                },
                VfxInitializeModule::Lifetime {
                    seconds: VfxRange::new(0.2, 0.8),
                },
            ],
            update: vec![
                VfxUpdateModule::Drag { coefficient: 0.2 },
                VfxUpdateModule::ColorOverLife {
                    gradient: spark_gradient(),
                },
                VfxUpdateModule::SizeOverLife {
                    curve: spark_curve(),
                },
            ],
            output: vec![VfxOutputModule::Sprite {
                material_id: "mat.vfx.gpu.spark.additive".to_string(),
                texture_atlas_slot: "atlas.vfx.gpu.spark".to_string(),
                blend_mode: VfxBlendMode::Additive,
            }],
        }],
    })
}

fn spark_gradient() -> VfxGradient {
    VfxGradient {
        stops: vec![
            VfxGradientStop {
                t: 0.0,
                color: VfxColor::rgba(1.0, 0.8, 0.2, 1.0),
            },
            VfxGradientStop {
                t: 1.0,
                color: VfxColor::rgba(0.2, 0.02, 0.0, 0.0),
            },
        ],
    }
}

fn spark_curve() -> VfxCurve {
    VfxCurve {
        points: vec![
            VfxCurvePoint { t: 0.0, value: 1.0 },
            VfxCurvePoint { t: 1.0, value: 0.0 },
        ],
    }
}

fn frustum() -> Result<Frustum, EngineError> {
    Frustum::orthographic(Aabb::new(
        Vec3::new(-5.0, -5.0, 0.0),
        Vec3::new(5.0, 5.0, 10.0),
    )?)
}

fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Result<ScreenRect, EngineError> {
    ScreenRect::new(Vec2::new(min_x, min_y), Vec2::new(max_x, max_y))
}

fn image_state(path: &Path) -> Result<serde_json::Value, EngineError> {
    let bytes = fs::read(path).map_err(|error| EngineError::VfxInvalidConfig {
        field: "render.vfx.fsv.image_read".to_string(),
        reason: format!("failed to read {}: {error}", path.display()),
    })?;
    let width = u32::from_le_bytes(bytes[18..22].try_into().map_err(|error| {
        EngineError::VfxInvalidConfig {
            field: "render.vfx.fsv.image_width".to_string(),
            reason: format!("failed to decode BMP width: {error}"),
        }
    })?);
    let height = u32::from_le_bytes(bytes[22..26].try_into().map_err(|error| {
        EngineError::VfxInvalidConfig {
            field: "render.vfx.fsv.image_height".to_string(),
            reason: format!("failed to decode BMP height: {error}"),
        }
    })?);
    Ok(json!({
        "path": path,
        "exists": true,
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
        "bmp_signature": String::from_utf8_lossy(&bytes[0..2]),
        "bmp_width": width,
        "bmp_height": height
    }))
}

fn error_result<T>(result: Result<T, EngineError>) -> serde_json::Value {
    match result {
        Ok(_) => json!({ "status": "accepted" }),
        Err(error) => {
            let report: ErrorReport = error.report();
            json!({
                "status": "rejected",
                "error": {
                    "code": report.code,
                    "message": report.message,
                    "remediation": report.remediation,
                    "source_error": report.source_error
                }
            })
        }
    }
}

fn fsv_root() -> Result<PathBuf, EngineError> {
    match env::var("LITD_VFX_GPU_PARTICLES_FSV_ROOT") {
        Ok(value) if value.trim().is_empty() => Err(EngineError::VfxInvalidConfig {
            field: "render.vfx.fsv.root".to_string(),
            reason: "LITD_VFX_GPU_PARTICLES_FSV_ROOT must not be empty".to_string(),
        }),
        Ok(value) => Ok(PathBuf::from(value)),
        Err(_) => Ok(PathBuf::from("target")
            .join("fsv")
            .join("issue1129_vfx_gpu_particles")
            .join("manual")),
    }
}
