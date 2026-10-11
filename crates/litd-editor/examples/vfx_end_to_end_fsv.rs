use litd_hash::sha256_hex;
use std::fs;

use litd_editor::vfx_text_generation::{VfxTextGenerationRequest, generate_vfx_text_proposal};
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_EDITOR_VFX_E2E_FSV_ROOT",
        "issue753_vfx_e2e",
        "manual",
    )?;
    let store_root = root.join("store");
    let frame_path = root.join("fireball-cast.bmp");

    let before_state = source_state(&store_root, &frame_path);
    let generated =
        generate_vfx_text_proposal(request("issue753-manual-fireball", "fireball explosion"))?;
    let stored = write_effect_to_content_store(&store_root, &generated.effect)?;
    let asset = read_effect_from_content_store(&store_root, &stored.descriptor)?;
    let cue = VfxRenderCue::for_ability(
        "cue.issue753.fireball.cast",
        "ability.issue753.fireball",
        stored.descriptor.clone(),
    )?;

    let mut sim_with_vfx = seeded_sim(753)?;
    let body = sim_with_vfx.spawn_body(v(2, 0, 1), v(1, 0, 0), FixedVec3::ZERO)?;
    let before_hash = sim_with_vfx.state_hash();
    let snapshot = SimSnapshot::capture(&sim_with_vfx);
    let event = VfxRenderEvent::from_snapshot(
        &snapshot,
        VfxRenderEventKind::SpellCue,
        "event.issue753.fireball.cast",
        cue.clone(),
        Vec3::new(2.0, 0.0, 1.0),
        Some(body),
    )?;
    let mut queue = VfxRenderEventQueue::new(4)?;
    let enqueue = queue.enqueue(event)?;
    let drain = queue.drain();
    let resolved = resolve_render_event_effect(&store_root, &drain.events[0])?;
    let after_event_hash = sim_with_vfx.state_hash();
    let final_with_vfx = sim_with_vfx.run_ticks(6)?.state_hash;

    let mut sim_without_vfx = seeded_sim(753)?;
    sim_without_vfx.spawn_body(v(2, 0, 1), v(1, 0, 0), FixedVec3::ZERO)?;
    let final_without_vfx = sim_without_vfx.run_ticks(6)?.state_hash;

    let frame = render_cast_frame(&resolved.cue_resolution.effect_readback.effect, &frame_path)?;
    let frame_bytes = fs::read(&frame_path)?;
    let over_budget = generate_vfx_text_proposal(request(
        "issue753-manual-over-budget",
        "a massive fiery explosion with 250000 particles",
    ))?;
    let missing_error = resolve_render_cue_effect(root.join("missing-store"), &cue)
        .expect_err("missing asset must fail")
        .to_string();
    let after_state = source_state(&store_root, &frame_path);

    let report = json!({
        "issue": 753,
        "source_of_truth": {
            "content_store": store_root,
            "effect_object": asset.object_path,
            "frame": frame_path
        },
        "states": {
            "before": before_state,
            "after": after_state
        },
        "happy_path": {
            "prompt": generated.request.prompt,
            "effect_id": asset.effect.effect_id,
            "object_sha256": asset.sha256,
            "descriptor_digest": stored.descriptor.digest,
            "cue": cue,
            "enqueue": enqueue,
            "drain_count": drain.drained_count,
            "event_source_hash": format!("{:016X}", resolved.event.source_state_hash),
            "before_hash": format!("{before_hash:016X}"),
            "after_event_hash": format!("{after_event_hash:016X}"),
            "final_with_vfx": format!("{final_with_vfx:016X}"),
            "final_without_vfx": format!("{final_without_vfx:016X}"),
            "state_hashes_equal": final_with_vfx == final_without_vfx,
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
    let report_path = root.join("vfx-end-to-end-fsv-readback.json");
    let report_bytes = serde_json::to_vec_pretty(&report)?;
    fs::write(&report_path, &report_bytes)?;
    let persisted = fs::read(&report_path)?;
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue753 vfx_end_to_end_fsv report={} report_bytes={} report_sha256={} object={} object_sha256={} frame={} frame_sha256={} before={before_hash:016X} after_event={after_event_hash:016X} final_with={final_with_vfx:016X} final_without={final_without_vfx:016X}",
        report_path.display(),
        persisted.len(),
        report_readback.sha256,
        asset.object_path.display(),
        asset.sha256,
        frame_path.display(),
        sha256_hex(&frame_bytes)
    );
    Ok(())
}

fn render_cast_frame(
    effect: &litd_engine::render::vfx::VfxEffectAsset,
    path: &std::path::Path,
) -> Result<litd_engine::render::vfx::VfxGpuParticleFrame, Box<dyn std::error::Error>> {
    let bounds = Aabb::from_center_half_extents(Vec3::new(2.0, 0.0, 1.0), Vec3::splat(4.0))?;
    let frustum = Frustum::orthographic(Aabb::from_center_half_extents(
        Vec3::ZERO,
        Vec3::splat(8.0),
    )?)?;
    let scene = [VfxSceneEffectInstance {
        instance_id: 753,
        effect: effect.clone(),
        world_bounds: bounds,
        screen_rect: ScreenRect::new(Vec2::new(0.24, 0.18), Vec2::new(0.82, 0.88))?,
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
    )?;
    let frame = pollster::block_on(render_vfx_particles_gpu(
        VfxGpuParticleSettings {
            width: 960,
            height: 540,
            max_particles: 120_000,
        },
        &workload,
    ))?;
    frame.image.write_bmp(path)?;
    Ok(frame)
}

fn source_state(store_root: &std::path::Path, frame_path: &std::path::Path) -> serde_json::Value {
    json!({
        "store_objects": count_store_objects(store_root),
        "frame_exists": frame_path.exists(),
        "frame_bytes": fs::metadata(frame_path).map(|meta| meta.len()).unwrap_or(0)
    })
}

fn count_store_objects(root: &std::path::Path) -> usize {
    let object_root = root.join("particles").join("sha256");
    if !object_root.exists() {
        return 0;
    }
    let mut count = 0;
    let mut stack = vec![object_root];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                count += 1;
            }
        }
    }
    count
}

fn request(id: &str, prompt: &str) -> VfxTextGenerationRequest {
    VfxTextGenerationRequest {
        request_id: id.to_string(),
        prompt: prompt.to_string(),
        seed: 753,
        requested_by: "litd-editor-issue753-fsv".to_string(),
    }
}

fn seeded_sim(seed: u64) -> Result<FixedTickSim<4>, litd_engine::EngineError> {
    FixedTickSim::<4>::new(SimConfig::new(60)?, seed)
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}
