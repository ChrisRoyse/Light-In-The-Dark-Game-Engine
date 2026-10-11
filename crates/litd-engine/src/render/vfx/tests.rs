use std::{fs, path::PathBuf};

use super::*;
use crate::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    math::{Aabb, FixedVec3, Vec3},
    sim::{FixedTickSim, SimConfig},
};

#[test]
fn fireball_effect_validates_and_round_trips_from_physical_file() {
    let effect = fireball_effect("issue745.fireball", 4096);
    let report = validate_effect(&effect).unwrap();
    assert_eq!(report.emitter_count, 1);
    assert_eq!(report.total_max_particles, 4096);
    assert_eq!(report.sprite_output_count, 1);
    assert_eq!(report.light_output_count, 1);
    assert_eq!(report.draw_group_count, 2);

    let root = target_root();
    let legacy_path = root.join("fireball-effect.json");
    if legacy_path.exists() {
        fs::remove_file(&legacy_path).unwrap();
    }
    let path = root.join("fireball-effect-aster");
    let writeback = write_effect_json(&path, &effect).unwrap();
    let readback = read_effect_json(&path).unwrap();
    assert_eq!(writeback.sha256, readback.sha256);
    assert_eq!(writeback.bytes, readback.bytes);
    assert_eq!(readback.validation.total_max_particles, 4096);
    assert!(!legacy_path.exists());
    println!(
        "issue745 happy path={} bytes={} sha={} emitters={} particles={} outputs={}",
        readback.path.display(),
        readback.bytes,
        readback.sha256,
        readback.validation.emitter_count,
        readback.validation.total_max_particles,
        readback.validation.output_module_count
    );
}

#[test]
fn vfx_validation_does_not_mutate_sim_state_hash() {
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(60).unwrap(), 745).unwrap();
    sim.spawn_body(
        FixedVec3::ZERO,
        FixedVec3::from_i32s(1, 0, 0),
        FixedVec3::ZERO,
    )
    .unwrap();
    let before_hash = sim.state_hash();

    let report = validate_effect(&fireball_effect("issue745.render_only", 2048)).unwrap();
    let after_hash = sim.state_hash();

    assert!(report.presentation_only);
    assert_eq!(before_hash, after_hash);
    println!("issue745 render_only before_hash={before_hash:016X} after_hash={after_hash:016X}");
}

#[test]
fn empty_effect_fails_loud_without_mutating_state() {
    let sim = FixedTickSim::<1>::new(SimConfig::new(30).unwrap(), 1).unwrap();
    let before_hash = sim.state_hash();
    let effect = VfxEffectAsset {
        schema_version: VFX_EFFECT_SCHEMA_VERSION,
        effect_id: "issue745.empty".to_string(),
        display_name: "Empty Effect".to_string(),
        presentation_only: true,
        emitters: Vec::new(),
    };
    let error = validate_effect(&effect).expect_err("empty effect must fail");
    let after_hash = sim.state_hash();

    assert_eq!(before_hash, after_hash);
    assert!(error.to_string().contains("render.vfx.emitters"));
    println!(
        "issue745 edge=empty before_hash={before_hash:016X} after_hash={after_hash:016X} error={error}"
    );
}

#[test]
fn max_particle_cap_boundary_is_explicit() {
    let mut boundary = fireball_effect("issue745.boundary", 50_000);
    boundary
        .emitters
        .push(fireball_emitter("secondary", 50_000).unwrap());
    let report = validate_effect(&boundary).unwrap();
    assert_eq!(report.total_max_particles, VFX_MAX_PARTICLES_PER_EFFECT);

    let mut over = boundary;
    over.emitters[1].max_particles += 1;
    let error = validate_effect(&over).expect_err("over budget effect must fail");
    assert!(error.to_string().contains("total_max_particles"));
    println!(
        "issue745 edge=max_cap accepted={} rejected_error={error}",
        report.total_max_particles
    );
}

#[test]
fn malformed_curve_and_missing_lifecycle_modules_fail_loud() {
    let mut bad_curve = fireball_effect("issue745.bad_curve", 1024);
    bad_curve.emitters[0]
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
    let curve_error = validate_effect(&bad_curve).expect_err("unsorted curve must fail");
    assert!(curve_error.to_string().contains("strictly increasing"));

    let mut missing_lifetime = fireball_effect("issue745.missing_lifetime", 1024);
    missing_lifetime.emitters[0]
        .initialize
        .retain(|module| !matches!(module, VfxInitializeModule::Lifetime { .. }));
    let lifecycle_error =
        validate_effect(&missing_lifetime).expect_err("missing lifetime must fail");
    assert!(lifecycle_error.to_string().contains("initialize.lifetime"));
    println!("issue745 edge=malformed curve_error={curve_error} lifecycle_error={lifecycle_error}");
}

#[test]
fn content_store_dedups_and_resolves_ability_render_cue_from_aster() {
    let root = unique_target_root("issue747-content-store");
    let effect = fireball_effect("issue747.fireball", 4096);
    let first = write_effect_to_content_store(&root, &effect).unwrap();
    let duplicate = write_effect_to_content_store(&root, &effect).unwrap();
    let stored = read_effect_from_content_store(&root, &first.descriptor).unwrap();
    let cue = VfxRenderCue::for_ability(
        "cue.issue747.fireball.cast",
        "ability.issue747.fireball",
        first.descriptor.clone(),
    )
    .unwrap();
    let resolved = resolve_render_cue_effect(&root, &cue).unwrap();

    assert!(!first.object_path.exists());
    assert!(!first.deduplicated);
    assert!(duplicate.deduplicated);
    assert_eq!(first.descriptor, duplicate.descriptor);
    assert_eq!(stored.bytes, first.descriptor.size);
    assert_eq!(stored.effect, effect);
    assert_eq!(
        first.descriptor.store_relative_path,
        effect_store_relative_path(&first.readback.sha256).unwrap()
    );
    assert_eq!(resolved.cue.source_kind, VfxRenderCueSourceKind::Ability);
    assert_eq!(resolved.effect_readback.effect, effect);
    println!(
        "issue747 happy aster_root={} virtual_object={} bytes={} digest={} duplicate_dedup={} cue_source={}",
        root.display(),
        first.object_path.display(),
        stored.bytes,
        first.descriptor.digest,
        duplicate.deduplicated,
        resolved.cue.source_id
    );
}

#[test]
fn content_store_edges_fail_loud_without_silent_fallbacks() {
    let root = unique_target_root("issue747-edges");
    let effect = fireball_effect("issue747.edge_fireball", 1024);
    let write = write_effect_to_content_store(&root, &effect).unwrap();
    let cue = VfxRenderCue::for_trigger(
        "cue.issue747.edge.trigger",
        "trigger.issue747.cast",
        write.descriptor.clone(),
    )
    .unwrap();
    let before = read_effect_from_content_store(&root, &write.descriptor).unwrap();
    assert_eq!(
        resolve_render_cue_effect(&root, &cue)
            .unwrap()
            .effect_readback
            .effect,
        effect
    );

    let mut wrong_size = write.descriptor.clone();
    wrong_size.size += 1;
    let wrong_size_cue = VfxRenderCue::for_trigger(
        "cue.issue747.edge.wrong_size",
        "trigger.issue747.cast",
        wrong_size,
    )
    .unwrap();
    let tamper_error = resolve_render_cue_effect(&root, &wrong_size_cue).unwrap_err();
    assert!(tamper_error.to_string().contains("content_store.size"));

    let missing_sha = "b".repeat(64);
    let missing_descriptor = VfxEffectDescriptor::new(
        "issue747.missing",
        VFX_EFFECT_SCHEMA_VERSION,
        format!("sha256:{missing_sha}"),
        8,
    )
    .unwrap();
    let missing_cue = VfxRenderCue::for_ability(
        "cue.issue747.missing",
        "ability.issue747.missing",
        missing_descriptor,
    )
    .unwrap();
    let missing_error = resolve_render_cue_effect(&root, &missing_cue).unwrap_err();
    assert!(missing_error.to_string().contains("content_store.read"));

    let mut missing_effect_id = write.descriptor;
    missing_effect_id.effect_id.clear();
    let missing_id_error = VfxRenderCue::for_ability(
        "cue.issue747.missing_effect_id",
        "ability.issue747.missing_effect_id",
        missing_effect_id,
    )
    .unwrap_err();
    assert!(
        missing_id_error
            .to_string()
            .contains("content_store.effect_id")
    );
    println!(
        "issue747 edges before_aster_bytes={} before_sha={} tamper_error={} missing_error={} missing_id_error={}",
        before.bytes, before.sha256, tamper_error, missing_error, missing_id_error
    );
}

#[test]
fn version_update_keeps_old_render_cue_refs_valid() {
    let root = unique_target_root("issue747-version");
    let old_effect = fireball_effect("issue747.versioned_fireball", 1024);
    let new_effect = fireball_effect("issue747.versioned_fireball", 2048);
    let old_write = write_effect_to_content_store(&root, &old_effect).unwrap();
    let old_cue = VfxRenderCue::for_ability(
        "cue.issue747.version.old",
        "ability.issue747.versioned_fireball",
        old_write.descriptor.clone(),
    )
    .unwrap();
    let new_write = write_effect_to_content_store(&root, &new_effect).unwrap();
    let new_cue = VfxRenderCue::for_ability(
        "cue.issue747.version.new",
        "ability.issue747.versioned_fireball",
        new_write.descriptor.clone(),
    )
    .unwrap();

    assert_ne!(old_write.descriptor.digest, new_write.descriptor.digest);
    let old_after_update = resolve_render_cue_effect(&root, &old_cue).unwrap();
    let new_after_update = resolve_render_cue_effect(&root, &new_cue).unwrap();
    assert_eq!(
        old_after_update.effect_readback.effect.emitters[0].max_particles,
        1024
    );
    assert_eq!(
        new_after_update.effect_readback.effect.emitters[0].max_particles,
        2048
    );
    println!(
        "issue747 version old_digest={} new_digest={} old_particles={} new_particles={}",
        old_write.descriptor.digest,
        new_write.descriptor.digest,
        old_after_update.effect_readback.effect.emitters[0].max_particles,
        new_after_update.effect_readback.effect.emitters[0].max_particles
    );
}

fn fireball_effect(effect_id: &str, max_particles: u32) -> VfxEffectAsset {
    VfxEffectAsset {
        schema_version: VFX_EFFECT_SCHEMA_VERSION,
        effect_id: effect_id.to_string(),
        display_name: "Fireball Impact".to_string(),
        presentation_only: true,
        emitters: vec![fireball_emitter("core", max_particles).unwrap()],
    }
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

fn target_root() -> PathBuf {
    begin_issue_run("issue745_vfx_effect_model")
        .unwrap()
        .run_dir()
        .to_path_buf()
}

fn unique_target_root(name: &str) -> PathBuf {
    begin_issue_run("issue747_vfx_effect_store")
        .unwrap()
        .artifact_dir(name)
        .unwrap()
}
