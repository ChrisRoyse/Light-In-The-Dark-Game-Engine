use litd_hash::sha256_hex;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use litd_engine::{
    EngineError,
    diagnostics::ErrorReport,
    math::{Aabb, Vec3},
    render::{
        lighting::ClusterGrid,
        vfx::{
            VFX_EFFECT_SCHEMA_VERSION, VFX_MAX_LOCAL_LIGHTS, VfxAtlasCatalog, VfxAtlasSlot,
            VfxBlendMode, VfxColor, VfxCurve, VfxCurvePoint, VfxEffectAsset, VfxEmitter,
            VfxGradient, VfxGradientStop, VfxInitializeModule, VfxOutputModule,
            VfxPresentationTerms, VfxRange, VfxShape, VfxSpawnModule, VfxUpdateModule, VfxVelocity,
            cluster_vfx_output_lights, read_effect_from_content_store, resolve_vfx_output_plan,
            write_effect_to_content_store,
        },
    },
};
use serde_json::json;

fn main() -> Result<(), EngineError> {
    litd_engine::init_logging();
    let root = fsv_root()?;
    let store_root = root.join("content-store");
    fs::create_dir_all(&root).map_err(|error| EngineError::VfxInvalidConfig {
        field: "render.vfx.fsv.root".to_string(),
        reason: format!("failed to create {}: {error}", root.display()),
    })?;

    let effect = lightning_effect("issue752.lightning_flash", 1)?;
    let stored = write_effect_to_content_store(&store_root, &effect)?;
    let readback = read_effect_from_content_store(&store_root, &stored.descriptor)?;
    let atlas = lightning_atlas();
    let happy = happy_path(&readback.effect, &atlas)?;
    let edge_light_cap = edge_light_cap(&atlas)?;
    let edge_atlas_range = edge_atlas_range(&readback.effect)?;
    let edge_fog_dim = edge_fog_dim(&readback.effect, &atlas)?;

    let report_path = root.join("vfx-light-atlas-fsv-readback.json");
    let report = json!({
        "issue": 752,
        "source_of_truth": {
            "fsv_report": report_path,
            "content_store_root": store_root,
            "stored_effect_object": stored.object_path,
            "resolved_plan": "persisted inside happy_path.after.plan after content-store readback"
        },
        "stored_effect": {
            "descriptor": stored.descriptor,
            "object_state": object_state(&stored.object_path),
            "readback": {
                "effect_id": readback.effect.effect_id,
                "bytes": readback.bytes,
                "sha256": readback.sha256,
                "emitter_count": readback.validation.emitter_count,
                "light_outputs": readback.validation.light_output_count,
                "sprite_outputs": readback.validation.sprite_output_count
            }
        },
        "happy_path": happy,
        "edges": {
            "light_count_cap": edge_light_cap,
            "atlas_frame_out_of_range": edge_atlas_range,
            "fog_dims_effect": edge_fog_dim
        }
    });
    let bytes =
        serde_json::to_vec_pretty(&report).map_err(|error| EngineError::SerializationFailed {
            format: "issue752-vfx-light-atlas-fsv-json",
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
            format: "issue752-vfx-light-atlas-fsv-readback-json",
            reason: error.to_string(),
        })?;
    if persisted_json["issue"] != 752
        || persisted_json["happy_path"]["after"]["plan"]["local_light_count"] != 1
    {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: "issue 752 report with one local light".to_string(),
            actual: persisted_json.to_string(),
        });
    }
    println!(
        "issue752 vfx_light_atlas_fsv report={} report_bytes={} report_sha256={} effect_object={} effect_sha256={} frame={} light_count={} fog_dim_intensity={}",
        report_path.display(),
        persisted.len(),
        sha256_hex(&persisted),
        stored.object_path.display(),
        readback.sha256,
        persisted_json["happy_path"]["after"]["plan"]["sprite_samples"][0]["frame_index"],
        persisted_json["happy_path"]["after"]["plan"]["local_light_count"],
        persisted_json["edges"]["fog_dims_effect"]["after"]["plan"]["local_lights"][0]["intensity"]
    );
    Ok(())
}

fn happy_path(
    effect: &VfxEffectAsset,
    atlas: &VfxAtlasCatalog,
) -> Result<serde_json::Value, EngineError> {
    let terms = presentation_terms(0.375, 1.0);
    let before = json!({
        "effect_id": effect.effect_id,
        "atlas": atlas,
        "terms": terms,
        "expected_frame": 3,
        "expected_light_cap": VFX_MAX_LOCAL_LIGHTS
    });
    let plan = resolve_vfx_output_plan(effect, atlas, terms)?;
    let assignment = cluster_vfx_output_lights(&plan, cluster_grid())?;
    Ok(json!({
        "before": before,
        "after": {
            "plan": {
                "effect_id": plan.effect_id,
                "atlas_id": plan.atlas_id,
                "sprite_count": plan.sprite_count(),
                "local_light_count": plan.local_light_count(),
                "sprite_samples": plan.sprite_samples,
                "local_lights": plan.local_lights
            },
            "clustered_lighting": cluster_summary(&assignment)
        }
    }))
}

fn edge_light_cap(atlas: &VfxAtlasCatalog) -> Result<serde_json::Value, EngineError> {
    let accepted = lightning_effect("issue752.light_cap.accepted", VFX_MAX_LOCAL_LIGHTS)?;
    let rejected = lightning_effect("issue752.light_cap.rejected", VFX_MAX_LOCAL_LIGHTS + 1)?;
    let accepted_plan = resolve_vfx_output_plan(&accepted, atlas, presentation_terms(0.0, 1.0))?;
    let result = resolve_vfx_output_plan(&rejected, atlas, presentation_terms(0.0, 1.0));
    Ok(json!({
        "before": {
            "accepted_light_outputs": VFX_MAX_LOCAL_LIGHTS,
            "accepted_plan_light_count": accepted_plan.local_light_count()
        },
        "after": {
            "attempted_light_outputs": VFX_MAX_LOCAL_LIGHTS + 1,
            "result": error_result(result)
        }
    }))
}

fn edge_atlas_range(effect: &VfxEffectAsset) -> Result<serde_json::Value, EngineError> {
    let mut atlas = lightning_atlas();
    let before = json!({
        "capacity": atlas.columns * atlas.rows,
        "slot": atlas.slots[0]
    });
    atlas.slots[0].first_frame = 15;
    atlas.slots[0].frame_count = 2;
    let result = resolve_vfx_output_plan(effect, &atlas, presentation_terms(0.0, 1.0));
    Ok(json!({
        "before": before,
        "after": {
            "capacity": atlas.columns * atlas.rows,
            "slot": atlas.slots[0],
            "result": error_result(result)
        }
    }))
}

fn edge_fog_dim(
    effect: &VfxEffectAsset,
    atlas: &VfxAtlasCatalog,
) -> Result<serde_json::Value, EngineError> {
    let bright = resolve_vfx_output_plan(effect, atlas, presentation_terms(0.0, 1.0))?;
    let dim = resolve_vfx_output_plan(effect, atlas, presentation_terms(0.0, 0.25))?;
    Ok(json!({
        "before": {
            "fog_visibility": 1.0,
            "plan": {
                "sprite_alpha_multiplier": bright.sprite_samples[0].fog_alpha_multiplier,
                "local_lights": bright.local_lights
            }
        },
        "after": {
            "fog_visibility": 0.25,
            "plan": {
                "sprite_alpha_multiplier": dim.sprite_samples[0].fog_alpha_multiplier,
                "local_lights": dim.local_lights
            }
        }
    }))
}

fn lightning_effect(effect_id: &str, light_count: usize) -> Result<VfxEffectAsset, EngineError> {
    let mut emitters = vec![lightning_emitter("primary", true)?];
    for index in 1..light_count {
        emitters.push(lightning_emitter(&format!("flash.{index}"), false)?);
    }
    Ok(VfxEffectAsset {
        schema_version: VFX_EFFECT_SCHEMA_VERSION,
        effect_id: effect_id.to_string(),
        display_name: "Lightning Flash".to_string(),
        presentation_only: true,
        emitters,
    })
}

fn lightning_emitter(id: &str, include_sprite: bool) -> Result<VfxEmitter, EngineError> {
    let mut output = Vec::new();
    if include_sprite {
        output.push(VfxOutputModule::Sprite {
            material_id: "mat.vfx.lightning.additive".to_string(),
            texture_atlas_slot: "atlas.vfx.lightning.arc".to_string(),
            blend_mode: VfxBlendMode::Additive,
        });
    }
    output.push(VfxOutputModule::Light {
        color: VfxColor::rgba(0.55, 0.78, 1.0, 1.0),
        intensity_lumens: 6400.0,
        radius_meters: 6.0,
    });
    Ok(VfxEmitter {
        emitter_id: format!("emitter.lightning.{id}"),
        display_name: format!("Lightning {id}"),
        max_particles: 1024,
        local_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(4.0))?,
        spawn: vec![VfxSpawnModule::Burst {
            count: 96,
            time_seconds: 0.0,
        }],
        initialize: vec![
            VfxInitializeModule::Shape(VfxShape::Sphere { radius_meters: 0.2 }),
            VfxInitializeModule::Velocity(VfxVelocity::Radial {
                speed_meters_per_second: VfxRange::new(1.0, 7.0),
            }),
            VfxInitializeModule::Color {
                color: VfxColor::rgba(0.55, 0.78, 1.0, 1.0),
            },
            VfxInitializeModule::Size {
                pixels: VfxRange::new(24.0, 72.0),
            },
            VfxInitializeModule::Lifetime {
                seconds: VfxRange::new(0.12, 0.38),
            },
        ],
        update: vec![
            VfxUpdateModule::Drag { coefficient: 0.1 },
            VfxUpdateModule::ColorOverLife {
                gradient: lightning_gradient(),
            },
            VfxUpdateModule::SizeOverLife {
                curve: lightning_curve(),
            },
        ],
        output,
    })
}

fn lightning_atlas() -> VfxAtlasCatalog {
    VfxAtlasCatalog {
        atlas_id: "atlas.vfx.shared".to_string(),
        columns: 4,
        rows: 4,
        slots: vec![VfxAtlasSlot {
            slot_id: "atlas.vfx.lightning.arc".to_string(),
            first_frame: 0,
            frame_count: 4,
            frames_per_second: 8.0,
        }],
    }
}

fn presentation_terms(elapsed_seconds: f32, fog_visibility: f32) -> VfxPresentationTerms {
    VfxPresentationTerms::new(
        Vec3::new(1.0, 0.25, 1.5),
        elapsed_seconds,
        VfxColor::rgba(0.8, 0.95, 1.0, 1.0),
        fog_visibility,
    )
}

fn cluster_grid() -> ClusterGrid {
    ClusterGrid::new(
        2,
        2,
        2,
        Vec3::new(-2.0, -2.0, -1.0),
        Vec3::new(3.0, 2.0, 4.0),
        VFX_MAX_LOCAL_LIGHTS,
    )
}

fn lightning_gradient() -> VfxGradient {
    VfxGradient {
        stops: vec![
            VfxGradientStop {
                t: 0.0,
                color: VfxColor::rgba(0.7, 0.9, 1.0, 1.0),
            },
            VfxGradientStop {
                t: 0.5,
                color: VfxColor::rgba(0.4, 0.65, 1.0, 0.85),
            },
            VfxGradientStop {
                t: 1.0,
                color: VfxColor::rgba(0.05, 0.1, 0.35, 0.0),
            },
        ],
    }
}

fn lightning_curve() -> VfxCurve {
    VfxCurve {
        points: vec![
            VfxCurvePoint { t: 0.0, value: 1.0 },
            VfxCurvePoint {
                t: 0.35,
                value: 0.65,
            },
            VfxCurvePoint { t: 1.0, value: 0.0 },
        ],
    }
}

fn cluster_summary(
    assignment: &litd_engine::render::lighting::ClusteredLightAssignment,
) -> serde_json::Value {
    json!({
        "cluster_count": assignment.cluster_count(),
        "directional_count": assignment.directional_count(),
        "total_light_refs": assignment.total_light_refs(),
        "non_empty_cluster_count": assignment.non_empty_cluster_count(),
        "max_cluster_occupancy": assignment.max_cluster_occupancy(),
        "refs_summary": assignment.refs_summary()
    })
}

fn object_state(path: &Path) -> serde_json::Value {
    match fs::read(path) {
        Ok(bytes) => json!({
            "path": path,
            "exists": true,
            "bytes": bytes.len(),
            "sha256": sha256_hex(&bytes)
        }),
        Err(error) => json!({
            "path": path,
            "exists": false,
            "read_error": error.to_string()
        }),
    }
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
    match env::var("LITD_VFX_LIGHT_ATLAS_FSV_ROOT") {
        Ok(value) if value.trim().is_empty() => Err(EngineError::VfxInvalidConfig {
            field: "render.vfx.fsv.root".to_string(),
            reason: "LITD_VFX_LIGHT_ATLAS_FSV_ROOT must not be empty".to_string(),
        }),
        Ok(value) => Ok(PathBuf::from(value)),
        Err(_) => Ok(PathBuf::from("target")
            .join("fsv")
            .join("issue752_vfx_light_atlas")
            .join("manual")),
    }
}
