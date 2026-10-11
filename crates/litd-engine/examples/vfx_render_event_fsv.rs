use litd_hash::sha256_hex;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use litd_engine::{
    EngineError,
    diagnostics::ErrorReport,
    math::{Aabb, FixedVec3, Vec3},
    render::vfx::{
        VFX_EFFECT_SCHEMA_VERSION, VfxBlendMode, VfxColor, VfxEffectAsset, VfxEmitter,
        VfxInitializeModule, VfxOutputModule, VfxRange, VfxRenderCue, VfxRenderEvent,
        VfxRenderEventKind, VfxRenderEventQueue, VfxShape, VfxSpawnModule, VfxUpdateModule,
        VfxVelocity, resolve_render_event_effect, write_effect_to_content_store,
    },
    sim::{FixedTickSim, SimConfig},
    sim_replay::SimSnapshot,
};
use serde_json::json;

fn main() -> Result<(), EngineError> {
    litd_engine::init_logging();
    let root = fsv_root();
    let store_root = root.join("content-store");
    fs::create_dir_all(&root).map_err(|error| EngineError::VfxInvalidConfig {
        field: "render.vfx.fsv.root".to_string(),
        reason: format!("failed to create {}: {error}", root.display()),
    })?;

    let effect = fireball_effect("issue751.fireball_impact", 4096)?;
    let stored = write_effect_to_content_store(&store_root, &effect)?;
    let cue = VfxRenderCue::for_ability(
        "cue.issue751.fireball.impact",
        "ability.issue751.fireball",
        stored.descriptor.clone(),
    )?;

    let happy = happy_path(&store_root, &cue)?;
    let edge_over_cap = edge_over_cap(&cue)?;
    let edge_death = edge_death_after_advance(&store_root, &cue)?;
    let edge_invalid = edge_invalid_position(&cue)?;

    let report_path = root.join("vfx-render-event-fsv-readback.json");
    let report = json!({
        "issue": 751,
        "source_of_truth": {
            "fsv_report": report_path,
            "content_store_root": store_root,
            "stored_effect_object": stored.object_path,
            "render_event_queue": "in-memory presentation queue, persisted here by before/after report",
            "determinism": "FixedTickSim::state_hash before and after VFX event enqueue/drain"
        },
        "stored_effect": {
            "digest": stored.descriptor.digest,
            "bytes": stored.readback.bytes,
            "sha256": stored.readback.sha256,
            "object_state": object_state(&stored.object_path)
        },
        "happy_path": happy,
        "edges": {
            "over_cap_drop": edge_over_cap,
            "death_event_after_sim_advance": edge_death,
            "invalid_position": edge_invalid
        }
    });
    let bytes =
        serde_json::to_vec_pretty(&report).map_err(|error| EngineError::SerializationFailed {
            format: "issue751-vfx-render-event-fsv-json",
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
    println!(
        "issue751 vfx_render_event_fsv report={} report_bytes={} report_sha256={} effect_object={} effect_sha256={} vfx_on_hash={} vfx_off_hash={}",
        report_path.display(),
        persisted.len(),
        sha256_hex(&persisted),
        stored.object_path.display(),
        stored.readback.sha256,
        happy["vfx_on_final_hash"].as_str().unwrap_or("missing"),
        happy["vfx_off_final_hash"].as_str().unwrap_or("missing")
    );
    Ok(())
}

fn happy_path(root: &Path, cue: &VfxRenderCue) -> Result<serde_json::Value, EngineError> {
    let mut sim_on = seeded_sim(751);
    let mut sim_off = seeded_sim(751);
    let body_on = sim_on.spawn_body(v(3, 0, 1), v(1, 0, 0), FixedVec3::ZERO)?;
    sim_off.spawn_body(v(3, 0, 1), v(1, 0, 0), FixedVec3::ZERO)?;
    let before_on = sim_on.state_hash();
    let before_off = sim_off.state_hash();
    let snapshot = SimSnapshot::capture(&sim_on);
    let event = VfxRenderEvent::from_snapshot(
        &snapshot,
        VfxRenderEventKind::Impact,
        "event.issue751.impact",
        cue.clone(),
        Vec3::new(3.0, 0.0, 1.0),
        Some(body_on),
    )?;
    let mut queue = VfxRenderEventQueue::new(8)?;
    let before_queue = queue_state(&queue);
    let enqueue = queue.enqueue(event)?;
    let after_enqueue = queue_state(&queue);
    let drain = queue.drain();
    let after_drain = queue_state(&queue);
    let resolved = resolve_render_event_effect(root, &drain.events[0])?;
    let on_final = sim_on.run_ticks(5)?.state_hash;
    let off_final = sim_off.run_ticks(5)?.state_hash;
    Ok(json!({
        "before_sim_hash_vfx_on": format!("{before_on:016X}"),
        "before_sim_hash_vfx_off": format!("{before_off:016X}"),
        "before_queue": before_queue,
        "enqueue": enqueue,
        "after_enqueue": after_enqueue,
        "drain_count": drain.drained_count,
        "after_drain": after_drain,
        "resolved_event": {
            "event_id": resolved.event.event_id,
            "kind": resolved.event.kind,
            "source_tick": resolved.event.source_tick,
            "source_state_hash": format!("{:016X}", resolved.event.source_state_hash),
            "source_body_id": resolved.event.source_body_id,
            "effect_id": resolved.cue_resolution.effect_readback.effect.effect_id,
            "effect_sha256": resolved.cue_resolution.effect_readback.sha256
        },
        "vfx_on_final_hash": format!("{on_final:016X}"),
        "vfx_off_final_hash": format!("{off_final:016X}"),
        "hash_identical": on_final == off_final
    }))
}

fn edge_over_cap(cue: &VfxRenderCue) -> Result<serde_json::Value, EngineError> {
    let snapshot = SimSnapshot::capture(&seeded_sim(752));
    let mut queue = VfxRenderEventQueue::new(1)?;
    let before = queue_state(&queue);
    let first = queue.enqueue(VfxRenderEvent::from_snapshot(
        &snapshot,
        VfxRenderEventKind::SpellCue,
        "event.issue751.cap.first",
        cue.clone(),
        Vec3::ZERO,
        None,
    )?)?;
    let second = queue.enqueue(VfxRenderEvent::from_snapshot(
        &snapshot,
        VfxRenderEventKind::SpellCue,
        "event.issue751.cap.second",
        cue.clone(),
        Vec3::new(1.0, 0.0, 0.0),
        None,
    )?)?;
    let after = queue_state(&queue);
    Ok(json!({
        "before": before,
        "first": first,
        "second": second,
        "after": after
    }))
}

fn edge_death_after_advance(
    root: &Path,
    cue: &VfxRenderCue,
) -> Result<serde_json::Value, EngineError> {
    let mut sim = seeded_sim(753);
    let body = sim.spawn_body(v(2, 0, 0), v(1, 0, 0), FixedVec3::ZERO)?;
    let snapshot = SimSnapshot::capture(&sim);
    let event = VfxRenderEvent::from_snapshot(
        &snapshot,
        VfxRenderEventKind::Death,
        "event.issue751.death",
        cue.clone(),
        Vec3::new(2.0, 0.0, 0.0),
        Some(body),
    )?;
    let mut queue = VfxRenderEventQueue::new(2)?;
    let before = queue_state(&queue);
    queue.enqueue(event)?;
    let after_enqueue = queue_state(&queue);
    let sim_after_advance = sim.run_ticks(3)?.state_hash;
    let drain = queue.drain();
    let resolved = resolve_render_event_effect(root, &drain.events[0])?;
    Ok(json!({
        "before": before,
        "after_enqueue": after_enqueue,
        "after_drain": queue_state(&queue),
        "event_source_hash": format!("{:016X}", resolved.event.source_state_hash),
        "sim_hash_after_advance": format!("{sim_after_advance:016X}"),
        "source_body_id": resolved.event.source_body_id,
        "resolved_kind": resolved.event.kind,
        "resolved_effect": resolved.cue_resolution.effect_readback.effect.effect_id
    }))
}

fn edge_invalid_position(cue: &VfxRenderCue) -> Result<serde_json::Value, EngineError> {
    let snapshot = SimSnapshot::capture(&seeded_sim(754));
    let before = json!({ "attempted_world_position": "NaN,0,0" });
    let result = VfxRenderEvent::from_snapshot(
        &snapshot,
        VfxRenderEventKind::Spawn,
        "event.issue751.invalid",
        cue.clone(),
        Vec3::new(f32::NAN, 0.0, 0.0),
        None,
    );
    Ok(json!({
        "before": before,
        "after": { "queue_mutated": false },
        "result": error_result(result)
    }))
}

fn queue_state(queue: &VfxRenderEventQueue) -> serde_json::Value {
    json!({
        "capacity": queue.capacity(),
        "pending": queue.pending_count(),
        "dropped_total": queue.dropped_total()
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

fn fsv_root() -> PathBuf {
    env::var("LITD_VFX_EVENT_FSV_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target/fsv/issue751_vfx_render_events/manual"))
}
