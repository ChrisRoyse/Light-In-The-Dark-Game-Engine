use litd_hash::sha256_hex;
use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    diagnostics::ErrorReport,
    math::{Aabb, Vec2, Vec3},
    render::{
        batching::{BatchingSettings, Frustum, ScreenRect},
        vfx::{
            VFX_EFFECT_SCHEMA_VERSION, VfxBlendMode, VfxColor, VfxCurve, VfxCurvePoint,
            VfxEffectAsset, VfxEmitter, VfxGradient, VfxGradientStop, VfxInitializeModule,
            VfxOutputModule, VfxRange, VfxRenderWorkloadSettings, VfxSceneEffectInstance, VfxShape,
            VfxSpawnModule, VfxUpdateModule, VfxVelocity, build_vfx_render_workload,
        },
    },
};
use serde_json::json;

fn main() -> Result<(), EngineError> {
    litd_engine::init_logging();
    let root = fsv_root()?;
    fs::create_dir_all(&root).map_err(|error| EngineError::VfxInvalidConfig {
        field: "render.vfx.fsv.root".to_string(),
        reason: format!("failed to create {}: {error}", root.display()),
    })?;

    let settings = settings();
    let frustum = frustum()?;
    let happy = happy_path(settings, &frustum)?;
    let edge_extreme = edge_extreme(settings, &frustum)?;
    let edge_offscreen = edge_offscreen(settings, &frustum)?;
    let edge_far_lod = edge_far_lod(settings, &frustum)?;
    let edge_over_budget = edge_over_budget(settings, &frustum)?;

    let report_path = root.join("vfx-render-workload-fsv-readback.json");
    let report = json!({
        "issue": 746,
        "source_of_truth": {
            "fsv_report": report_path,
            "workload_stats": "serialized VfxRenderWorkloadStats read back from this physical report",
            "batching_source": "render::batching::build_batch_plan",
            "renderer_boundary": "GPU particle drawing is downstream of these validated workload stats"
        },
        "happy_path": happy,
        "edges": {
            "extreme_simultaneous_count": edge_extreme,
            "offscreen_culled": edge_offscreen,
            "far_lod_reduces_cost": edge_far_lod,
            "over_budget_fails_loud": edge_over_budget
        }
    });
    let bytes =
        serde_json::to_vec_pretty(&report).map_err(|error| EngineError::SerializationFailed {
            format: "issue746-vfx-render-workload-fsv-json",
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
            format: "issue746-vfx-render-workload-fsv-readback-json",
            reason: error.to_string(),
        })?;
    if persisted_json["issue"] != 746
        || persisted_json["happy_path"]["after"]["stats"]["draw_batches"] != 1
        || persisted_json["edges"]["far_lod_reduces_cost"]["after"]["stats"]["lod_saved_particles"]
            != 750
    {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: "issue 746 report with batched happy path and far LOD savings".to_string(),
            actual: persisted_json.to_string(),
        });
    }
    println!(
        "issue746 vfx_render_workload_fsv report={} report_bytes={} report_sha256={} happy_effects={} happy_particles={} happy_batches={} far_lod_saved={} offscreen_culled={}",
        report_path.display(),
        persisted.len(),
        sha256_hex(&persisted),
        persisted_json["happy_path"]["after"]["stats"]["input_effect_instances"],
        persisted_json["happy_path"]["after"]["stats"]["simulated_particles"],
        persisted_json["happy_path"]["after"]["stats"]["draw_batches"],
        persisted_json["edges"]["far_lod_reduces_cost"]["after"]["stats"]["lod_saved_particles"],
        persisted_json["edges"]["offscreen_culled"]["after"]["stats"]["frustum_culled_effect_ids"]
            [0]
    );
    Ok(())
}

fn happy_path(
    settings: VfxRenderWorkloadSettings,
    frustum: &Frustum,
) -> Result<serde_json::Value, EngineError> {
    let scene = dense_scene(400, Vec3::new(0.0, 0.0, 4.0))?;
    let before = json!({
        "settings": settings,
        "scene_effects": scene.len(),
        "expected_draw_batches": 1,
        "expected_visible_particles": 400000
    });
    let stats = build_vfx_render_workload(settings, frustum, &[], &scene)?;
    Ok(json!({ "before": before, "after": { "stats": stats } }))
}

fn edge_extreme(
    settings: VfxRenderWorkloadSettings,
    frustum: &Frustum,
) -> Result<serde_json::Value, EngineError> {
    let scene = dense_scene(900, Vec3::new(0.0, 0.0, 4.0))?;
    let before = json!({ "scene_effects": scene.len(), "max_effect_instances": settings.max_effect_instances });
    let stats = build_vfx_render_workload(settings, frustum, &[], &scene)?;
    Ok(json!({
        "before": before,
        "after": {
            "visible_effect_instances": stats.visible_effect_instances,
            "draw_batches": stats.draw_batches,
            "simulated_particles": stats.simulated_particles,
            "particle_buffer_bytes": stats.particle_buffer_bytes
        }
    }))
}

fn edge_offscreen(
    settings: VfxRenderWorkloadSettings,
    frustum: &Frustum,
) -> Result<serde_json::Value, EngineError> {
    let scene = vec![scene_effect(
        101,
        Vec3::new(12.0, 0.0, 4.0),
        25.0,
        rect(0.30, 0.10, 0.40, 0.20)?,
        0.1,
    )?];
    let before = json!({ "instance_id": 101, "world_center_x": 12.0, "frustum_x_max": 5.0 });
    let stats = build_vfx_render_workload(settings, frustum, &[], &scene)?;
    Ok(json!({ "before": before, "after": { "stats": stats } }))
}

fn edge_far_lod(
    settings: VfxRenderWorkloadSettings,
    frustum: &Frustum,
) -> Result<serde_json::Value, EngineError> {
    let scene = vec![scene_effect(
        201,
        Vec3::new(0.0, 0.0, 4.0),
        120.0,
        rect(0.20, 0.20, 0.30, 0.30)?,
        0.1,
    )?];
    let before = json!({
        "distance_meters": 120.0,
        "far_lod_start_meters": settings.far_lod_start_meters,
        "far_lod_particle_scale": settings.far_lod_particle_scale,
        "input_particles": 1000
    });
    let stats = build_vfx_render_workload(settings, frustum, &[], &scene)?;
    Ok(json!({ "before": before, "after": { "stats": stats } }))
}

fn edge_over_budget(
    mut settings: VfxRenderWorkloadSettings,
    frustum: &Frustum,
) -> Result<serde_json::Value, EngineError> {
    settings.max_simulated_particles = 999;
    let scene = vec![scene_effect(
        301,
        Vec3::new(0.0, 0.0, 4.0),
        25.0,
        rect(0.20, 0.20, 0.30, 0.30)?,
        0.1,
    )?];
    let before = json!({ "max_simulated_particles": 999, "input_particles": 1000 });
    let result = build_vfx_render_workload(settings, frustum, &[], &scene);
    Ok(json!({ "before": before, "after": { "result": error_result(result) } }))
}

fn settings() -> VfxRenderWorkloadSettings {
    VfxRenderWorkloadSettings {
        max_simulated_particles: 1_000_000,
        max_particle_buffer_bytes: 96 * 1024 * 1024,
        batching: BatchingSettings {
            max_draw_batches: 64,
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
                center + Vec3::new((id % 20) as f32 * 0.05, 0.0, 0.0),
                25.0,
                rect(0.10, 0.10, 0.90, 0.90)?,
                0.1,
            )
        })
        .collect()
}

fn scene_effect(
    id: u64,
    center: Vec3,
    distance_meters: f32,
    screen_rect: ScreenRect,
    nearest_depth: f32,
) -> Result<VfxSceneEffectInstance, EngineError> {
    Ok(VfxSceneEffectInstance {
        instance_id: id,
        effect: spark_effect(&format!("issue746.spark.{id}"))?,
        world_bounds: Aabb::from_center_half_extents(center, Vec3::splat(0.5))?,
        screen_rect,
        nearest_depth,
        distance_meters,
    })
}

fn spark_effect(effect_id: &str) -> Result<VfxEffectAsset, EngineError> {
    Ok(VfxEffectAsset {
        schema_version: VFX_EFFECT_SCHEMA_VERSION,
        effect_id: effect_id.to_string(),
        display_name: "Dense Spark".to_string(),
        presentation_only: true,
        emitters: vec![VfxEmitter {
            emitter_id: "emitter.spark".to_string(),
            display_name: "Spark".to_string(),
            max_particles: 1000,
            local_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(1.0))?,
            spawn: vec![VfxSpawnModule::ConstantRate {
                particles_per_second: 1000.0,
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
                material_id: "mat.vfx.spark.additive".to_string(),
                texture_atlas_slot: "atlas.vfx.spark".to_string(),
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
    match env::var("LITD_VFX_RENDER_WORKLOAD_FSV_ROOT") {
        Ok(value) if value.trim().is_empty() => Err(EngineError::VfxInvalidConfig {
            field: "render.vfx.fsv.root".to_string(),
            reason: "LITD_VFX_RENDER_WORKLOAD_FSV_ROOT must not be empty".to_string(),
        }),
        Ok(value) => Ok(PathBuf::from(value)),
        Err(_) => Ok(PathBuf::from("target")
            .join("fsv")
            .join("issue746_vfx_render_workload")
            .join("manual")),
    }
}
