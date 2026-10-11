use std::path::PathBuf;

use super::*;
use crate::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    math::{Aabb, FixedVec3, Vec3},
    sim::{FixedTickSim, SimConfig},
    sim_replay::SimSnapshot,
};

#[test]
fn render_event_queue_resolves_without_mutating_sim_hash() {
    let root = unique_target_root("issue751-happy");
    let effect = fireball_effect("issue751.impact", 2048);
    let stored = write_effect_to_content_store(&root, &effect).unwrap();
    let cue = VfxRenderCue::for_ability(
        "cue.issue751.impact",
        "ability.issue751.fireball",
        stored.descriptor,
    )
    .unwrap();
    let mut sim = seeded_sim(751);
    let body = sim
        .spawn_body(v(3, 0, 1), v(1, 0, 0), FixedVec3::ZERO)
        .unwrap();
    let before_hash = sim.state_hash();
    let snapshot = SimSnapshot::capture(&sim);
    let event = VfxRenderEvent::from_snapshot(
        &snapshot,
        VfxRenderEventKind::Impact,
        "event.issue751.impact",
        cue,
        Vec3::new(3.0, 0.0, 1.0),
        Some(body),
    )
    .unwrap();
    let mut queue = VfxRenderEventQueue::new(8).unwrap();
    let enqueue = queue.enqueue(event).unwrap();
    let drain = queue.drain();
    let resolved = resolve_render_event_effect(&root, &drain.events[0]).unwrap();

    assert!(enqueue.accepted);
    assert_eq!(drain.drained_count, 1);
    assert_eq!(sim.state_hash(), before_hash);
    assert_eq!(resolved.event.source_state_hash, before_hash);
    assert_eq!(resolved.cue_resolution.effect_readback.effect, effect);
    println!(
        "issue751 happy before_hash={before_hash:016X} after_hash={:016X} event={} digest={}",
        sim.state_hash(),
        resolved.event.event_id,
        resolved.event.cue.effect.digest
    );
}

#[test]
fn over_cap_vfx_event_drops_are_reported_and_do_not_block_sim() {
    let root = unique_target_root("issue751-over-cap");
    let cue = stored_cue(&root, "issue751.over_cap").unwrap();
    let sim = seeded_sim(752);
    let before_hash = sim.state_hash();
    let snapshot = SimSnapshot::capture(&sim);
    let mut queue = VfxRenderEventQueue::new(1).unwrap();
    let first = queue
        .enqueue(
            VfxRenderEvent::from_snapshot(
                &snapshot,
                VfxRenderEventKind::SpellCue,
                "event.issue751.first",
                cue.clone(),
                Vec3::ZERO,
                None,
            )
            .unwrap(),
        )
        .unwrap();
    let second = queue
        .enqueue(
            VfxRenderEvent::from_snapshot(
                &snapshot,
                VfxRenderEventKind::SpellCue,
                "event.issue751.second",
                cue,
                Vec3::new(1.0, 0.0, 0.0),
                None,
            )
            .unwrap(),
        )
        .unwrap();

    assert!(first.accepted);
    assert!(!second.accepted);
    assert_eq!(second.pending_events, 1);
    assert_eq!(second.dropped_total, 1);
    assert_eq!(sim.state_hash(), before_hash);
    assert_eq!(
        second.dropped.as_ref().unwrap().reason,
        "queue_capacity_exceeded"
    );
    println!(
        "issue751 edge=over_cap before_hash={before_hash:016X} after_hash={:016X} pending={} dropped_total={} reason={}",
        sim.state_hash(),
        second.pending_events,
        second.dropped_total,
        second.dropped.unwrap().reason
    );
}

#[test]
fn death_event_payload_survives_after_sim_advances() {
    let root = unique_target_root("issue751-death");
    let cue = stored_cue(&root, "issue751.death").unwrap();
    let mut sim = seeded_sim(753);
    let body = sim
        .spawn_body(v(2, 0, 0), v(1, 0, 0), FixedVec3::ZERO)
        .unwrap();
    let snapshot = SimSnapshot::capture(&sim);
    let event = VfxRenderEvent::from_snapshot(
        &snapshot,
        VfxRenderEventKind::Death,
        "event.issue751.death",
        cue,
        Vec3::new(2.0, 0.0, 0.0),
        Some(body),
    )
    .unwrap();
    let event_hash = event.source_state_hash;
    let mut queue = VfxRenderEventQueue::new(2).unwrap();
    queue.enqueue(event).unwrap();
    let after_advance = sim.run_ticks(3).unwrap().state_hash;
    let drain = queue.drain();
    let resolved = resolve_render_event_effect(&root, &drain.events[0]).unwrap();

    assert_ne!(after_advance, event_hash);
    assert_eq!(resolved.event.source_body_id, Some(body.index()));
    assert_eq!(resolved.event.source_state_hash, event_hash);
    assert_eq!(resolved.event.kind, VfxRenderEventKind::Death);
    println!(
        "issue751 edge=death source_hash={event_hash:016X} sim_after={after_advance:016X} source_body={}",
        body.index()
    );
}

#[test]
fn malformed_render_events_fail_loud() {
    let root = unique_target_root("issue751-invalid");
    let cue = stored_cue(&root, "issue751.invalid").unwrap();
    let snapshot = SimSnapshot::capture(&seeded_sim(754));
    let error = VfxRenderEvent::from_snapshot(
        &snapshot,
        VfxRenderEventKind::Spawn,
        "event.issue751.invalid",
        cue,
        Vec3::new(f32::NAN, 0.0, 0.0),
        None,
    )
    .unwrap_err();

    assert!(error.to_string().contains("render_event.world_position"));
    println!("issue751 edge=invalid_position error={error}");
}

fn stored_cue(root: &PathBuf, effect_id: &str) -> Result<VfxRenderCue, EngineError> {
    let effect = fireball_effect(effect_id, 1024);
    let stored = write_effect_to_content_store(root, &effect)?;
    VfxRenderCue::for_trigger(
        format!("cue.{effect_id}.trigger"),
        format!("trigger.{effect_id}.cast"),
        stored.descriptor,
    )
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
        spawn: vec![VfxSpawnModule::Burst {
            count: 128,
            time_seconds: 0.0,
        }],
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
        update: vec![VfxUpdateModule::Drag { coefficient: 0.18 }],
        output: vec![VfxOutputModule::Sprite {
            material_id: "mat.vfx.fire.additive".to_string(),
            texture_atlas_slot: "atlas.vfx.fireball.impact_01".to_string(),
            blend_mode: VfxBlendMode::SoftAdditive,
        }],
    })
}

fn seeded_sim(seed: u64) -> FixedTickSim<4> {
    FixedTickSim::<4>::new(SimConfig::new(60).unwrap(), seed).unwrap()
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

fn unique_target_root(name: &str) -> PathBuf {
    begin_issue_run("issue751_vfx_render_events")
        .unwrap()
        .artifact_dir(name)
        .unwrap()
}
