use litd_hash::sha256_hex;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use litd_engine::{
    EngineError,
    diagnostics::ErrorReport,
    math::{Aabb, Vec3},
    render::vfx::{
        VFX_EFFECT_SCHEMA_VERSION, VfxBlendMode, VfxColor, VfxCurve, VfxCurvePoint, VfxEffectAsset,
        VfxEffectDescriptor, VfxEmitter, VfxGradient, VfxGradientStop, VfxInitializeModule,
        VfxOutputModule, VfxRange, VfxRenderCue, VfxShape, VfxSpawnModule, VfxUpdateModule,
        VfxVelocity, effect_store_relative_path, effect_to_json_bytes,
        read_effect_from_content_store, resolve_render_cue_effect, validate_render_cue,
        write_effect_to_content_store,
    },
};
use serde_json::json;

fn main() -> Result<(), EngineError> {
    litd_engine::init_logging();
    let root = fsv_root();
    fs::create_dir_all(&root).map_err(|error| EngineError::VfxInvalidConfig {
        field: "render.vfx.fsv.root".to_string(),
        reason: format!("failed to create {}: {error}", root.display()),
    })?;

    let effect_v1 = fireball_effect("issue747.fireball", 4096)?;
    let before_write = expected_state(&root, &effect_v1)?;
    let write_v1 = write_effect_to_content_store(&root, &effect_v1)?;
    let physical_v1 =
        fs::read(&write_v1.object_path).map_err(|error| EngineError::VfxInvalidConfig {
            field: "render.vfx.fsv.read.object".to_string(),
            reason: format!("failed to read {}: {error}", write_v1.object_path.display()),
        })?;
    let cue_v1 = VfxRenderCue::for_ability(
        "cue.issue747.fireball.cast",
        "ability.issue747.fireball",
        write_v1.descriptor.clone(),
    )?;
    let resolved_v1 = resolve_render_cue_effect(&root, &cue_v1)?;
    let duplicate = write_effect_to_content_store(&root, &effect_v1)?;

    let mut effect_v2 = fireball_effect("issue747.fireball", 6144)?;
    effect_v2.display_name = "Fireball Impact Revised".to_string();
    let write_v2 = write_effect_to_content_store(&root, &effect_v2)?;
    let cue_v2 = VfxRenderCue::for_ability(
        "cue.issue747.fireball.cast.revised",
        "ability.issue747.fireball",
        write_v2.descriptor.clone(),
    )?;
    let old_after_update = resolve_render_cue_effect(&root, &cue_v1)?;
    let new_after_update = resolve_render_cue_effect(&root, &cue_v2)?;

    let edge_missing = edge_missing_object(&root)?;
    let edge_missing_id = edge_missing_effect_id(&root, &write_v1.descriptor);
    let edge_tamper = edge_tampered_object(&root)?;

    let readback_path = root.join("vfx-effect-store-fsv-readback.json");
    let report = json!({
        "issue": 747,
        "source_of_truth": {
            "content_store_root": root,
            "content_store_layout": "particles/sha256/<first-two-hex>/<sha256>",
            "descriptor_contract": "media_type + sha256 digest + byte size + schema + effect id",
            "render_cue_contract": "ability/trigger id resolves only after descriptor and object bytes verify"
        },
        "happy_path": {
            "before_write": before_write,
            "after_write": object_state(&write_v1.object_path),
            "descriptor": write_v1.descriptor,
            "physical_readback_bytes": physical_v1.len(),
            "physical_readback_sha256": sha256_hex(&physical_v1),
            "resolved_cue": {
                "cue_id": resolved_v1.cue.cue_id,
                "source_kind": resolved_v1.cue.source_kind,
                "source_id": resolved_v1.cue.source_id,
                "effect_id": resolved_v1.effect_readback.effect.effect_id,
                "object_path": resolved_v1.effect_readback.object_path,
                "sha256": resolved_v1.effect_readback.sha256,
                "bytes": resolved_v1.effect_readback.bytes,
                "emitter_count": resolved_v1.effect_readback.validation.emitter_count,
                "total_max_particles": resolved_v1.effect_readback.validation.total_max_particles
            }
        },
        "edges": {
            "duplicate_dedup": {
                "before": object_state(&write_v1.object_path),
                "after": object_state(&duplicate.object_path),
                "deduplicated": duplicate.deduplicated,
                "same_descriptor": duplicate.descriptor == write_v1.descriptor
            },
            "version_update_keeps_old_refs": {
                "old_digest": old_after_update.cue.effect.digest,
                "new_digest": new_after_update.cue.effect.digest,
                "old_object_after_update": object_state(&old_after_update.effect_readback.object_path),
                "new_object_after_update": object_state(&new_after_update.effect_readback.object_path),
                "old_particles_after_update": old_after_update.effect_readback.effect.emitters[0].max_particles,
                "new_particles_after_update": new_after_update.effect_readback.effect.emitters[0].max_particles
            },
            "missing_object": edge_missing,
            "missing_effect_id": edge_missing_id,
            "tampered_object": edge_tamper
        }
    });
    let report_bytes =
        serde_json::to_vec_pretty(&report).map_err(|error| EngineError::SerializationFailed {
            format: "issue747-vfx-store-fsv-json",
            reason: error.to_string(),
        })?;
    fs::write(&readback_path, &report_bytes).map_err(|error| EngineError::VfxInvalidConfig {
        field: "render.vfx.fsv.write".to_string(),
        reason: format!("failed to write {}: {error}", readback_path.display()),
    })?;
    let persisted = fs::read(&readback_path).map_err(|error| EngineError::VfxInvalidConfig {
        field: "render.vfx.fsv.read".to_string(),
        reason: format!("failed to read {}: {error}", readback_path.display()),
    })?;
    if persisted != report_bytes {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: format!("{} bytes", report_bytes.len()),
            actual: format!("{} bytes", persisted.len()),
        });
    }
    println!(
        "issue747 vfx_store_fsv root={} object={} digest={} report={} report_bytes={} duplicate_dedup={} old_digest={} new_digest={}",
        root.display(),
        write_v1.object_path.display(),
        write_v1.descriptor.digest,
        readback_path.display(),
        persisted.len(),
        duplicate.deduplicated,
        old_after_update.cue.effect.digest,
        new_after_update.cue.effect.digest
    );
    Ok(())
}

fn edge_missing_object(root: &Path) -> Result<serde_json::Value, EngineError> {
    let missing_sha = "c".repeat(64);
    let descriptor = VfxEffectDescriptor::new(
        "issue747.missing",
        VFX_EFFECT_SCHEMA_VERSION,
        format!("sha256:{missing_sha}"),
        12,
    )?;
    let path = root.join(effect_store_relative_path(&missing_sha)?);
    let cue = VfxRenderCue::for_ability(
        "cue.issue747.missing",
        "ability.issue747.missing",
        descriptor,
    )?;
    let before = object_state(&path);
    let result = resolve_render_cue_effect(root, &cue);
    let after = object_state(&path);
    Ok(json!({
        "before": before,
        "after": after,
        "result": error_result(result)
    }))
}

fn edge_missing_effect_id(root: &Path, descriptor: &VfxEffectDescriptor) -> serde_json::Value {
    let mut missing = descriptor.clone();
    missing.effect_id.clear();
    let before = object_state(&root.join(&descriptor.store_relative_path));
    let result = VfxRenderCue::for_ability(
        "cue.issue747.missing_effect_id",
        "ability.issue747.missing_effect_id",
        missing,
    )
    .and_then(|cue| {
        validate_render_cue(&cue)?;
        read_effect_from_content_store(root, &cue.effect).map(|_| cue)
    });
    let after = object_state(&root.join(&descriptor.store_relative_path));
    json!({
        "before": before,
        "after": after,
        "result": error_result(result)
    })
}

fn edge_tampered_object(root: &Path) -> Result<serde_json::Value, EngineError> {
    let edge_root = root.join("tampered-edge");
    let effect = fireball_effect("issue747.tampered", 512)?;
    let write = write_effect_to_content_store(&edge_root, &effect)?;
    let cue = VfxRenderCue::for_trigger(
        "cue.issue747.tampered",
        "trigger.issue747.cast",
        write.descriptor.clone(),
    )?;
    let before = object_state(&write.object_path);
    fs::write(&write.object_path, b"{\"tampered\":true}").map_err(|error| {
        EngineError::VfxInvalidConfig {
            field: "render.vfx.fsv.tamper".to_string(),
            reason: format!("failed to tamper {}: {error}", write.object_path.display()),
        }
    })?;
    let result = resolve_render_cue_effect(&edge_root, &cue);
    let after = object_state(&write.object_path);
    Ok(json!({
        "before": before,
        "after": after,
        "result": error_result(result)
    }))
}

fn expected_state(root: &Path, effect: &VfxEffectAsset) -> Result<serde_json::Value, EngineError> {
    let bytes = effect_to_json_bytes(effect)?;
    let sha256 = sha256_hex(&bytes);
    let path = root.join(effect_store_relative_path(&sha256)?);
    Ok(object_state(&path))
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
                    "remediation": report.remediation
                }
            })
        }
    }
}

fn fireball_effect(effect_id: &str, max_particles: u32) -> Result<VfxEffectAsset, EngineError> {
    Ok(VfxEffectAsset {
        schema_version: VFX_EFFECT_SCHEMA_VERSION,
        effect_id: effect_id.to_string(),
        display_name: "Fireball Impact".to_string(),
        presentation_only: true,
        emitters: vec![fireball_emitter("core", max_particles)?],
    })
}

fn fireball_emitter(id: &str, max_particles: u32) -> Result<VfxEmitter, EngineError> {
    Ok(VfxEmitter {
        emitter_id: format!("emitter.{id}"),
        display_name: format!("Fireball {id}"),
        max_particles,
        local_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(6.0))?,
        spawn: vec![
            VfxSpawnModule::Burst {
                count: 128,
                time_seconds: 0.0,
            },
            VfxSpawnModule::ConstantRate {
                particles_per_second: 256.0,
            },
        ],
        initialize: vec![
            VfxInitializeModule::Shape(VfxShape::Sphere { radius_meters: 0.8 }),
            VfxInitializeModule::Velocity(VfxVelocity::Radial {
                speed_meters_per_second: VfxRange::new(4.0, 14.0),
            }),
            VfxInitializeModule::Color {
                color: VfxColor::rgba(1.0, 0.48, 0.08, 1.0),
            },
            VfxInitializeModule::Size {
                pixels: VfxRange::new(18.0, 48.0),
            },
            VfxInitializeModule::Lifetime {
                seconds: VfxRange::new(0.3, 1.2),
            },
        ],
        update: vec![
            VfxUpdateModule::Drag { coefficient: 0.18 },
            VfxUpdateModule::ColorOverLife {
                gradient: fire_gradient(),
            },
            VfxUpdateModule::SizeOverLife {
                curve: fade_curve(),
            },
        ],
        output: vec![
            VfxOutputModule::Sprite {
                material_id: "mat.vfx.fire.additive".to_string(),
                texture_atlas_slot: "atlas.vfx.fireball.impact_01".to_string(),
                blend_mode: VfxBlendMode::SoftAdditive,
            },
            VfxOutputModule::Light {
                color: VfxColor::rgba(1.0, 0.36, 0.08, 1.0),
                intensity_lumens: 4800.0,
                radius_meters: 5.5,
            },
        ],
    })
}

fn fire_gradient() -> VfxGradient {
    VfxGradient {
        stops: vec![
            VfxGradientStop {
                t: 0.0,
                color: VfxColor::rgba(1.0, 0.70, 0.18, 1.0),
            },
            VfxGradientStop {
                t: 0.55,
                color: VfxColor::rgba(1.0, 0.18, 0.03, 0.75),
            },
            VfxGradientStop {
                t: 1.0,
                color: VfxColor::rgba(0.02, 0.02, 0.02, 0.0),
            },
        ],
    }
}

fn fade_curve() -> VfxCurve {
    VfxCurve {
        points: vec![
            VfxCurvePoint { t: 0.0, value: 0.2 },
            VfxCurvePoint {
                t: 0.25,
                value: 1.0,
            },
            VfxCurvePoint { t: 1.0, value: 0.0 },
        ],
    }
}

fn fsv_root() -> PathBuf {
    env::var("LITD_VFX_STORE_FSV_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target/fsv/issue747_vfx_effect_store/manual"))
}
