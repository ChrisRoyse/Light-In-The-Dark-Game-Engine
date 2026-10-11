use litd_hash::sha256_hex;
use std::{fs, path::Path};

use litd_engine::{
    math::{Aabb, FixedVec3, Vec2, Vec3},
    render::{
        batching::{BatchingSettings, Frustum, ScreenRect},
        vfx::{
            VfxGpuParticleSettings, VfxRenderCue, VfxRenderEvent, VfxRenderEventKind,
            VfxRenderEventQueue, VfxRenderWorkloadSettings, VfxSceneEffectInstance,
            build_vfx_render_workload, read_effect_from_content_store, render_vfx_particles_gpu,
            resolve_render_cue_effect, resolve_render_event_effect, write_effect_to_content_store,
        },
    },
    sim::{FixedTickSim, SimConfig},
    sim_replay::SimSnapshot,
};
use serde_json::json;

use crate::vfx_text_generation::{VfxTextGenerationRequest, generate_vfx_text_proposal};

#[test]
fn issue753_text_to_blobstore_to_ability_cast_preserves_sim_hash() {
    let (_fsv_run, root) = crate::fsv_support::artifact_dir("issue753_vfx_e2e", "unit").unwrap();
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let store_root = root.join("store");

    let generated =
        generate_vfx_text_proposal(request("issue753-fireball", "fireball explosion")).unwrap();
    let stored = write_effect_to_content_store(&store_root, &generated.effect).unwrap();
    let asset = read_effect_from_content_store(&store_root, &stored.descriptor).unwrap();
    let cue = VfxRenderCue::for_ability(
        "cue.issue753.fireball.cast",
        "ability.issue753.fireball",
        stored.descriptor.clone(),
    )
    .unwrap();

    let mut sim_with_vfx = seeded_sim(753);
    let body = sim_with_vfx
        .spawn_body(v(2, 0, 1), v(1, 0, 0), FixedVec3::ZERO)
        .unwrap();
    let before_hash = sim_with_vfx.state_hash();
    let snapshot = SimSnapshot::capture(&sim_with_vfx);
    let event = VfxRenderEvent::from_snapshot(
        &snapshot,
        VfxRenderEventKind::SpellCue,
        "event.issue753.fireball.cast",
        cue.clone(),
        Vec3::new(2.0, 0.0, 1.0),
        Some(body),
    )
    .unwrap();
    let mut queue = VfxRenderEventQueue::new(4).unwrap();
    let enqueue = queue.enqueue(event).unwrap();
    let drain = queue.drain();
    let resolved = resolve_render_event_effect(&store_root, &drain.events[0]).unwrap();
    let after_event_hash = sim_with_vfx.state_hash();
    let final_with_vfx = sim_with_vfx.run_ticks(6).unwrap().state_hash;

    let mut sim_without_vfx = seeded_sim(753);
    sim_without_vfx
        .spawn_body(v(2, 0, 1), v(1, 0, 0), FixedVec3::ZERO)
        .unwrap();
    let final_without_vfx = sim_without_vfx.run_ticks(6).unwrap().state_hash;

    let frame_path = root.join("fireball-cast.bmp");
    let frame = render_cast_frame(&resolved.cue_resolution.effect_readback.effect, &frame_path);
    let frame_bytes = fs::read(&frame_path).unwrap();

    let over_budget = generate_vfx_text_proposal(request(
        "issue753-over-budget",
        "a massive fiery explosion with 250000 particles",
    ))
    .unwrap();
    let missing_error = resolve_render_cue_effect(root.join("missing-store"), &cue)
        .expect_err("missing asset must fail loudly")
        .to_string();

    let report = json!({
        "source_of_truth": {
            "content_store": store_root,
            "effect_object": asset.object_path,
            "frame": frame_path
        },
        "happy_path": {
            "effect_id": asset.effect.effect_id,
            "object_sha256": asset.sha256,
            "descriptor_digest": stored.descriptor.digest,
            "cue": cue,
            "enqueue": enqueue,
            "drain_count": drain.drained_count,
            "event_source_hash": resolved.event.source_state_hash,
            "before_hash": format!("{before_hash:016X}"),
            "after_event_hash": format!("{after_event_hash:016X}"),
            "final_with_vfx": format!("{final_with_vfx:016X}"),
            "final_without_vfx": format!("{final_without_vfx:016X}"),
            "frame_sha256": sha256_hex(&frame_bytes),
            "frame_bytes": frame_bytes.len(),
            "particles": frame.stats.drawn_particles,
            "draw_calls": frame.stats.draw_calls,
            "lit_pixel_ratio": frame.stats.metrics.lit_pixel_ratio
        },
        "edges": {
            "over_budget": {
                "requested": over_budget.budget.requested_max_particles,
                "emitted": over_budget.budget.emitted_max_particles,
                "capped": over_budget.budget.capped,
                "reason": over_budget.budget.cap_reason
            },
            "missing_asset_error": missing_error,
            "replay_hash_equal": final_with_vfx == final_without_vfx
        }
    });
    let report_path = root.join("issue753-e2e-report.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    let readback = fs::read(&report_path).unwrap();
    println!(
        "issue753 e2e report={} report_sha={} object_sha={} frame_sha={} before={before_hash:016X} after_event={after_event_hash:016X} final_with={final_with_vfx:016X} final_without={final_without_vfx:016X}",
        report_path.display(),
        sha256_hex(&readback),
        asset.sha256,
        sha256_hex(&frame_bytes)
    );

    assert_eq!(asset.effect.effect_id, "generated.vfx.fire.explosion");
    assert!(enqueue.accepted);
    assert_eq!(drain.drained_count, 1);
    assert_eq!(after_event_hash, before_hash);
    assert_eq!(resolved.event.source_state_hash, before_hash);
    assert_eq!(final_with_vfx, final_without_vfx);
    assert!(frame.stats.drawn_particles > 0);
    assert!(frame.stats.draw_calls > 0);
    assert!(frame.stats.metrics.lit_pixel_ratio > 0.01);
    assert!(over_budget.budget.capped);
    assert!(missing_error.contains("content_store.read"));
}

fn render_cast_frame(
    effect: &litd_engine::render::vfx::VfxEffectAsset,
    path: &Path,
) -> litd_engine::render::vfx::VfxGpuParticleFrame {
    let bounds =
        Aabb::from_center_half_extents(Vec3::new(2.0, 0.0, 1.0), Vec3::splat(4.0)).unwrap();
    let frustum = Frustum::orthographic(
        Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(8.0)).unwrap(),
    )
    .unwrap();
    let scene = [VfxSceneEffectInstance {
        instance_id: 753,
        effect: effect.clone(),
        world_bounds: bounds,
        screen_rect: ScreenRect::new(Vec2::new(0.24, 0.18), Vec2::new(0.82, 0.88)).unwrap(),
        nearest_depth: 0.3,
        distance_meters: 12.0,
    }];
    let workload = build_vfx_render_workload(
        VfxRenderWorkloadSettings {
            target_width: 960,
            target_height: 540,
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
    .unwrap();
    let frame = pollster::block_on(render_vfx_particles_gpu(
        VfxGpuParticleSettings {
            width: 960,
            height: 540,
            max_particles: 120_000,
        },
        &workload,
    ))
    .unwrap();
    frame.image.write_bmp(path).unwrap();
    frame
}

fn request(id: &str, prompt: &str) -> VfxTextGenerationRequest {
    VfxTextGenerationRequest {
        request_id: id.to_string(),
        prompt: prompt.to_string(),
        seed: 753,
        requested_by: "litd-editor-issue753-test".to_string(),
    }
}

fn seeded_sim(seed: u64) -> FixedTickSim<4> {
    FixedTickSim::<4>::new(SimConfig::new(60).unwrap(), seed).unwrap()
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}
