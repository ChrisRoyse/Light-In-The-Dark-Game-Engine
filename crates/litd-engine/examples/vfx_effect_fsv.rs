use std::{env, fs, path::PathBuf};

use litd_engine::{
    EngineError,
    diagnostics::ErrorReport,
    math::{Aabb, FixedVec3, Vec3},
    render::vfx::{
        VFX_EFFECT_SCHEMA_VERSION, VFX_MAX_PARTICLES_PER_EFFECT, VfxBlendMode, VfxColor, VfxCurve,
        VfxCurvePoint, VfxEffectAsset, VfxEmitter, VfxGradient, VfxGradientStop,
        VfxInitializeModule, VfxOutputModule, VfxRange, VfxShape, VfxSpawnModule, VfxUpdateModule,
        VfxVelocity, read_effect_json, validate_effect, write_effect_json,
    },
    sim::{FixedTickSim, SimConfig},
};
use serde_json::json;

fn main() -> Result<(), EngineError> {
    litd_engine::init_logging();
    let root = fsv_root();
    fs::create_dir_all(&root).map_err(|error| EngineError::VfxInvalidConfig {
        field: "render.vfx.fsv.root".to_string(),
        reason: format!("failed to create {}: {error}", root.display()),
    })?;

    let effect_path = root.join("fireball-effect.json");
    let readback_path = root.join("vfx-effect-fsv-readback.json");
    let effect = fireball_effect("issue745.fireball", 4096)?;
    let sim = seeded_sim()?;
    let before_hash = sim.state_hash();
    let writeback = write_effect_json(&effect_path, &effect)?;
    let readback = read_effect_json(&effect_path)?;
    let after_hash = sim.state_hash();

    let edge_empty = edge_empty(&sim);
    let edge_max = edge_max_boundary(&sim)?;
    let edge_invalid_curve = edge_invalid_curve(&sim)?;
    let edge_presentation_flag = edge_presentation_flag(&sim)?;

    let report = json!({
        "issue": 745,
        "source_of_truth": {
            "effect_asset_json": effect_path,
            "fsv_readback_json": readback_path,
            "sim_state_hash": "FixedTickSim::state_hash()",
            "validation": "litd_engine::render::vfx::validate_effect"
        },
        "happy_path": {
            "before_sim_hash": format!("{before_hash:016X}"),
            "after_sim_hash": format!("{after_hash:016X}"),
            "sim_hash_identical": before_hash == after_hash,
            "effect_bytes": writeback.bytes,
            "effect_sha256": writeback.sha256,
            "readback_sha256": readback.sha256,
            "validation": readback.validation,
        },
        "edges": {
            "empty_effect": edge_empty,
            "max_particle_cap": edge_max,
            "invalid_curve": edge_invalid_curve,
            "presentation_only_false": edge_presentation_flag
        }
    });
    let bytes =
        serde_json::to_vec_pretty(&report).map_err(|error| EngineError::SerializationFailed {
            format: "issue745-vfx-fsv-json",
            reason: error.to_string(),
        })?;
    fs::write(&readback_path, &bytes).map_err(|error| EngineError::VfxInvalidConfig {
        field: "render.vfx.fsv.write".to_string(),
        reason: format!("failed to write {}: {error}", readback_path.display()),
    })?;
    let persisted = fs::read(&readback_path).map_err(|error| EngineError::VfxInvalidConfig {
        field: "render.vfx.fsv.read".to_string(),
        reason: format!("failed to read {}: {error}", readback_path.display()),
    })?;
    if persisted != bytes {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: format!("{} bytes", bytes.len()),
            actual: format!("{} bytes", persisted.len()),
        });
    }

    println!(
        "issue745 vfx_fsv effect={} bytes={} sha={} report={} report_bytes={} sim_hash={:016X}",
        effect_path.display(),
        writeback.bytes,
        writeback.sha256,
        readback_path.display(),
        persisted.len(),
        after_hash
    );
    Ok(())
}

fn edge_empty(sim: &FixedTickSim<4>) -> serde_json::Value {
    let before_hash = sim.state_hash();
    let effect = VfxEffectAsset {
        schema_version: VFX_EFFECT_SCHEMA_VERSION,
        effect_id: "issue745.empty".to_string(),
        display_name: "Empty Effect".to_string(),
        presentation_only: true,
        emitters: Vec::new(),
    };
    edge_result(
        "empty_effect",
        before_hash,
        sim.state_hash(),
        validate_effect(&effect),
    )
}

fn edge_max_boundary(sim: &FixedTickSim<4>) -> Result<serde_json::Value, EngineError> {
    let before_hash = sim.state_hash();
    let mut effect = fireball_effect("issue745.max_boundary", 50_000)?;
    effect.emitters.push(fireball_emitter("secondary", 50_000)?);
    let accepted = validate_effect(&effect)?;
    let mut over = effect;
    over.emitters[1].max_particles += 1;
    let rejected = validate_effect(&over);
    Ok(json!({
        "name": "max_particle_cap",
        "before_sim_hash": format!("{before_hash:016X}"),
        "after_sim_hash": format!("{:016X}", sim.state_hash()),
        "accepted_total_max_particles": accepted.total_max_particles,
        "expected_total_max_particles": VFX_MAX_PARTICLES_PER_EFFECT,
        "rejected_error": error_json(rejected.expect_err("over budget must fail"))
    }))
}

fn edge_invalid_curve(sim: &FixedTickSim<4>) -> Result<serde_json::Value, EngineError> {
    let before_hash = sim.state_hash();
    let mut effect = fireball_effect("issue745.invalid_curve", 1024)?;
    effect.emitters[0]
        .update
        .push(VfxUpdateModule::SizeOverLife {
            curve: VfxCurve {
                points: vec![
                    VfxCurvePoint { t: 0.0, value: 1.0 },
                    VfxCurvePoint { t: 0.5, value: 1.2 },
                    VfxCurvePoint { t: 0.4, value: 0.1 },
                ],
            },
        });
    Ok(edge_result(
        "invalid_curve",
        before_hash,
        sim.state_hash(),
        validate_effect(&effect),
    ))
}

fn edge_presentation_flag(sim: &FixedTickSim<4>) -> Result<serde_json::Value, EngineError> {
    let before_hash = sim.state_hash();
    let mut effect = fireball_effect("issue745.presentation_flag", 1024)?;
    effect.presentation_only = false;
    Ok(edge_result(
        "presentation_only_false",
        before_hash,
        sim.state_hash(),
        validate_effect(&effect),
    ))
}

fn edge_result(
    name: &str,
    before_hash: u64,
    after_hash: u64,
    result: Result<litd_engine::render::vfx::VfxValidationReport, EngineError>,
) -> serde_json::Value {
    match result {
        Ok(report) => json!({
            "name": name,
            "status": "accepted",
            "before_sim_hash": format!("{before_hash:016X}"),
            "after_sim_hash": format!("{after_hash:016X}"),
            "sim_hash_identical": before_hash == after_hash,
            "validation": report
        }),
        Err(error) => json!({
            "name": name,
            "status": "rejected",
            "before_sim_hash": format!("{before_hash:016X}"),
            "after_sim_hash": format!("{after_hash:016X}"),
            "sim_hash_identical": before_hash == after_hash,
            "error": error_json(error)
        }),
    }
}

fn error_json(error: EngineError) -> serde_json::Value {
    let report: ErrorReport = error.report();
    json!({
        "code": report.code,
        "message": report.message,
        "remediation": report.remediation
    })
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

fn seeded_sim() -> Result<FixedTickSim<4>, EngineError> {
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(60)?, 745)?;
    sim.spawn_body(
        FixedVec3::ZERO,
        FixedVec3::from_i32s(1, 0, 0),
        FixedVec3::ZERO,
    )?;
    Ok(sim)
}

fn fsv_root() -> PathBuf {
    env::var("LITD_VFX_FSV_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target/fsv/issue745_vfx_effect_model/manual"))
}
