use litd_hash::sha256_hex;
use std::fs;

use litd_editor::vfx_editor::EditorVfxRuntime;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_EDITOR_VFX_FSV_ROOT",
        "issue748_vfx_editor",
        "manual",
    )?;
    let store_root = root.join("store");
    fs::create_dir_all(&store_root)?;
    let mut runtime = EditorVfxRuntime::with_store_root(&store_root)?;

    let happy_before = runtime.evidence();
    let happy_save = runtime.save_effect()?;
    let happy_preview = runtime.refresh_preview()?;
    let object_bytes = fs::read(&happy_save.object_path)?;
    let image_bytes = fs::read(&happy_preview.image_path)?;
    let happy_after = runtime.evidence();

    let remove_before = runtime.evidence();
    runtime.remove_size_over_life()?;
    let remove_save = runtime.save_effect()?;
    let remove_preview = runtime.refresh_preview()?;
    let remove_after = runtime.evidence();

    runtime.restore_size_over_life()?;
    let preview_a = runtime.refresh_preview()?;
    let preview_b = runtime.refresh_preview()?;
    let consistency_after = runtime.evidence();

    let invalid_before = runtime.evidence();
    runtime.set_invalid_size_curve_for_fsv()?;
    let invalid_error = match runtime.save_effect() {
        Ok(_) => return Err("invalid curve unexpectedly saved".into()),
        Err(error) => error.to_string(),
    };
    let invalid_after = runtime.evidence();

    let report = json!({
        "issue": 748,
        "source_of_truth": {
            "effect_store_root": store_root,
            "saved_effect_object": happy_save.object_path,
            "preview_image": happy_preview.image_path,
            "runtime_evidence": "EditorVfxRuntime::evidence after physical readback"
        },
        "happy_path": {
            "before": happy_before,
            "after": happy_after,
            "object_readback": {
                "path": happy_save.object_path,
                "bytes": object_bytes.len(),
                "sha256": sha256_hex(&object_bytes),
                "descriptor_digest": happy_save.digest
            },
            "preview_readback": {
                "path": happy_preview.image_path,
                "bytes": image_bytes.len(),
                "sha256": sha256_hex(&image_bytes),
                "adapter": happy_preview.adapter_name,
                "backend": happy_preview.backend,
                "particles": happy_preview.drawn_particles,
                "draw_calls": happy_preview.draw_calls,
                "lit_pixel_ratio": happy_preview.lit_pixel_ratio
            }
        },
        "edges": {
            "remove_size_curve_updates_preview": {
                "before": remove_before,
                "after": remove_after,
                "saved_digest": remove_save.digest,
                "preview_sha256": remove_preview.image_sha256
            },
            "preview_consistency": {
                "preview_a_sha256": preview_a.image_sha256,
                "preview_b_sha256": preview_b.image_sha256,
                "preview_a_draw_calls": preview_a.draw_calls,
                "preview_b_draw_calls": preview_b.draw_calls,
                "after": consistency_after
            },
            "invalid_curve_rejected": {
                "before": invalid_before,
                "after": invalid_after,
                "error": invalid_error
            }
        }
    });

    let report_path = root.join("vfx-editor-fsv-readback.json");
    fs::create_dir_all(&root)?;
    let report_bytes = serde_json::to_vec_pretty(&report)?;
    fs::write(&report_path, &report_bytes)?;
    let persisted = fs::read(&report_path)?;
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue748 editor_vfx_editor_fsv report={} report_bytes={} report_sha256={} object={} object_sha256={} preview={} preview_sha256={} particles={} draw_calls={} lit_ratio={}",
        report_path.display(),
        persisted.len(),
        report_readback.sha256,
        happy_save.object_path,
        sha256_hex(&object_bytes),
        happy_preview.image_path,
        sha256_hex(&image_bytes),
        happy_preview.drawn_particles,
        happy_preview.draw_calls,
        happy_preview.lit_pixel_ratio
    );
    Ok(())
}
