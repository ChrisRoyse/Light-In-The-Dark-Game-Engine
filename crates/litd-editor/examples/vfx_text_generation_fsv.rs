use litd_hash::sha256_hex;
use std::fs;

use litd_editor::vfx_text_generation::{
    VfxTextGenerationRequest, generate_and_store_vfx_text_effect, read_vfx_text_generation_ledger,
};
use litd_engine::{
    math::{Aabb, Vec2, Vec3},
    render::{
        batching::{BatchingSettings, Frustum, ScreenRect},
        vfx::{
            VfxGpuParticleSettings, VfxRenderWorkloadSettings, VfxSceneEffectInstance,
            build_vfx_render_workload, read_effect_from_content_store, render_vfx_particles_gpu,
        },
    },
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_EDITOR_VFX_TEXT_FSV_ROOT",
        "issue749_vfx_text_generation",
        "manual",
    )?;
    let store_root = root.join("store");
    let aster_dir = root.join("aster");
    let state_path = root.join("vfx-text-generation-ledger.json");
    let preview_path = root.join("vfx-text-generation-preview.bmp");
    let happy_before = state_snapshot(&store_root, &aster_dir, "issue749-manual-ledger");
    let stored = generate_and_store_vfx_text_effect(
        &state_path,
        &aster_dir,
        &store_root,
        "issue749-manual-ledger",
        request(
            "issue749-manual-fireball",
            "a fiery explosion with rising embers",
        ),
    )?;
    let effect_readback = read_effect_from_content_store(&store_root, &stored.record.descriptor)?;
    let ledger_readback = read_vfx_text_generation_ledger(&aster_dir, "issue749-manual-ledger")?
        .ok_or("manual happy ledger missing from Aster")?;
    let mirror_bytes = fs::read(&state_path)?;
    if mirror_bytes != ledger_readback.bytes {
        return Err("JSON mirror differs from Aster ledger bytes".into());
    }
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
        &Frustum::orthographic(Aabb::from_center_half_extents(
            Vec3::ZERO,
            Vec3::splat(8.0),
        )?)?,
        &[],
        &[scene_instance(effect_readback.effect.clone())?],
    )?;
    let frame = pollster::block_on(render_vfx_particles_gpu(
        VfxGpuParticleSettings {
            width: 960,
            height: 540,
            max_particles: 120_000,
        },
        &workload,
    ))?;
    frame.image.write_bmp(&preview_path)?;
    let preview_bytes = fs::read(&preview_path)?;
    let happy_after = state_snapshot(&store_root, &aster_dir, "issue749-manual-ledger");

    let reject_state = root.join("reject-ledger.json");
    let reject_aster = root.join("reject-aster");
    let reject_store = root.join("reject-store");
    let reject_before = state_snapshot(&reject_store, &reject_aster, "issue749-reject-ledger");
    let reject_error = generate_and_store_vfx_text_effect(
        &reject_state,
        &reject_aster,
        &reject_store,
        "issue749-reject-ledger",
        request("issue749-manual-reject", "make the thing nicer"),
    )
    .expect_err("ungrounded prompt must fail closed")
    .to_string();
    let reject_after = state_snapshot(&reject_store, &reject_aster, "issue749-reject-ledger");

    let cap_state = root.join("cap-ledger.json");
    let cap_aster = root.join("cap-aster");
    let cap_store = root.join("cap-store");
    let cap_before = state_snapshot(&cap_store, &cap_aster, "issue749-cap-ledger");
    let capped = generate_and_store_vfx_text_effect(
        &cap_state,
        &cap_aster,
        &cap_store,
        "issue749-cap-ledger",
        request(
            "issue749-manual-cap",
            "a massive fiery explosion with 250000 particles and rising embers",
        ),
    )?;
    let cap_after = state_snapshot(&cap_store, &cap_aster, "issue749-cap-ledger");

    let empty_state = root.join("empty-ledger.json");
    let empty_aster = root.join("empty-aster");
    let empty_store = root.join("empty-store");
    let empty_before = state_snapshot(&empty_store, &empty_aster, "issue749-empty-ledger");
    let empty_error = generate_and_store_vfx_text_effect(
        &empty_state,
        &empty_aster,
        &empty_store,
        "issue749-empty-ledger",
        request("issue749-manual-empty", ""),
    )
    .expect_err("empty prompt must fail closed")
    .to_string();
    let empty_after = state_snapshot(&empty_store, &empty_aster, "issue749-empty-ledger");

    let report = json!({
        "issues": [749, 1130],
        "source_of_truth": {
            "effect_content_store": store_root,
            "aster_ledger_dir": aster_dir,
            "json_mirror": state_path,
            "preview_bitmap": preview_path
        },
        "happy_path": {
            "before": happy_before,
            "after": happy_after,
            "prompt": stored.record.prompt,
            "effect_readback": {
                "effect_id": effect_readback.effect.effect_id,
                "object_path": effect_readback.object_path,
                "sha256": effect_readback.sha256,
                "bytes": effect_readback.bytes,
                "validation": effect_readback.validation
            },
            "ledger_readback": {
                "seq": ledger_readback.seq,
                "graph_rows": ledger_readback.graph_rows,
                "sha256": ledger_readback.sha256,
                "bytes": ledger_readback.bytes.len(),
                "record_count": ledger_readback.record_count
            },
            "preview_readback": {
                "path": preview_path,
                "bytes": preview_bytes.len(),
                "sha256": sha256_hex(&preview_bytes),
                "adapter": frame.stats.adapter_name,
                "backend": format!("{:?}", frame.stats.backend),
                "particles": frame.stats.drawn_particles,
                "draw_calls": frame.stats.draw_calls,
                "lit_pixel_ratio": frame.stats.metrics.lit_pixel_ratio,
                "hot_pixel_ratio": frame.stats.metrics.hot_pixel_ratio
            },
            "workload": workload
        },
        "edges": {
            "nonsense_prompt_rejected_no_write": {
                "before": reject_before,
                "after": reject_after,
                "error": reject_error
            },
            "over_budget_prompt_capped_and_recorded": {
                "before": cap_before,
                "after": cap_after,
                "requested": capped.record.budget.requested_max_particles,
                "emitted": capped.record.budget.emitted_max_particles,
                "capped": capped.record.budget.capped,
                "reason": capped.record.budget.cap_reason,
                "object": capped.record.content_object_path
            },
            "empty_prompt_rejected_no_write": {
                "before": empty_before,
                "after": empty_after,
                "error": empty_error
            }
        }
    });

    let report_path = root.join("vfx-text-generation-fsv-readback.json");
    let report_bytes = serde_json::to_vec_pretty(&report)?;
    fs::write(&report_path, &report_bytes)?;
    let persisted = fs::read(&report_path)?;
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue749 vfx_text_generation_fsv report={} report_bytes={} report_sha256={} object={} object_sha256={} ledger_seq={} ledger_sha256={} preview={} preview_sha256={} particles={} draw_calls={} lit_ratio={}",
        report_path.display(),
        persisted.len(),
        report_readback.sha256,
        effect_readback.object_path.display(),
        effect_readback.sha256,
        ledger_readback.seq,
        ledger_readback.sha256,
        preview_path.display(),
        sha256_hex(&preview_bytes),
        frame.stats.drawn_particles,
        frame.stats.draw_calls,
        frame.stats.metrics.lit_pixel_ratio
    );
    Ok(())
}

fn request(id: &str, prompt: &str) -> VfxTextGenerationRequest {
    VfxTextGenerationRequest {
        request_id: id.to_string(),
        prompt: prompt.to_string(),
        seed: 7_491,
        requested_by: "litd-editor-vfx-text-generation-fsv".to_string(),
    }
}

fn state_snapshot(
    store_root: &std::path::Path,
    aster_dir: &std::path::Path,
    ledger_id: &str,
) -> serde_json::Value {
    json!({
        "store_objects": count_store_objects(store_root),
        "aster_dir_exists": aster_dir.exists(),
        "ledger_present": read_vfx_text_generation_ledger(aster_dir, ledger_id)
            .ok()
            .flatten()
            .map(|readback| json!({
                "seq": readback.seq,
                "bytes": readback.bytes.len(),
                "sha256": readback.sha256,
                "record_count": readback.record_count
            }))
    })
}

fn scene_instance(
    effect: litd_engine::render::vfx::VfxEffectAsset,
) -> Result<VfxSceneEffectInstance, Box<dyn std::error::Error>> {
    Ok(VfxSceneEffectInstance {
        instance_id: 749,
        effect,
        world_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(4.0))?,
        screen_rect: ScreenRect::new(Vec2::new(0.22, 0.16), Vec2::new(0.78, 0.86))?,
        nearest_depth: 0.25,
        distance_meters: 18.0,
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
