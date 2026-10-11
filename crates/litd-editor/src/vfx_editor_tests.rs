use super::*;
use litd_engine::render::vfx::{
    VFX_EFFECT_SCHEMA_VERSION, VfxEffectDescriptor, read_effect_from_content_store,
};

#[test]
fn issue1464_invalid_effect_is_reported_loud_not_as_zero_counts() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1464_vfx_evidence", "unit").unwrap();
    let _ = fs::remove_dir_all(&root);
    let mut runtime = EditorVfxRuntime::with_store_root(&root).unwrap();

    // Valid effect: evidence reports it valid with real module counts.
    let valid = runtime.evidence();
    assert!(valid.effect_valid, "default fireball must validate");
    assert!(valid.validation_error.is_empty());
    assert!(
        valid.spawn_module_count > 0,
        "a valid effect has non-zero module counts"
    );

    // Corrupt the in-editor effect, then read the evidence panel. Before #1464
    // it showed zero counts indistinguishable from an empty effect; now it must
    // report effect_valid=false with the validation error.
    runtime.set_invalid_size_curve_for_fsv().unwrap();
    let invalid = runtime.evidence();
    assert!(
        !invalid.effect_valid,
        "a corrupt effect must be reported invalid, not as an empty effect"
    );
    assert!(
        !invalid.validation_error.is_empty(),
        "the validation error must be surfaced"
    );
    assert_eq!(
        invalid.spawn_module_count, 0,
        "counts are 0 because the effect is invalid — disambiguated by effect_valid=false"
    );
}

#[test]
fn vfx_editor_saves_and_renders_real_preview_artifacts() {
    let (_fsv_run, root) = crate::fsv_support::artifact_dir("issue748_vfx_editor", "unit").unwrap();
    let _ = fs::remove_dir_all(&root);
    let mut runtime = EditorVfxRuntime::with_store_root(&root).unwrap();
    let before = runtime.evidence();
    println!(
        "issue748 happy before effect={} emitters={} saves={} previews={}",
        before.effect_id, before.emitter_count, before.save_count, before.preview_refresh_count
    );

    let save = runtime.save_effect().unwrap();
    let preview = runtime.refresh_preview().unwrap();
    let after = runtime.evidence();
    let descriptor = VfxEffectDescriptor::new(
        save.effect_id.clone(),
        VFX_EFFECT_SCHEMA_VERSION,
        save.digest.clone(),
        save.bytes,
    )
    .unwrap();
    let object_readback = read_effect_from_content_store(&root, &descriptor).unwrap();
    let image_bytes = fs::read(&preview.image_path).unwrap();
    println!(
        "issue748 happy after object={} object_sha={} image={} image_sha={} particles={} draw_calls={} lit={:.6}",
        save.object_path,
        object_readback.sha256,
        preview.image_path,
        sha256_hex(&image_bytes),
        preview.drawn_particles,
        preview.draw_calls,
        preview.lit_pixel_ratio
    );

    assert_eq!(save.effect_id, "editor.vfx.fireball");
    assert_eq!(save.sha256, object_readback.sha256);
    assert_eq!(save.bytes, object_readback.bytes);
    assert_eq!(preview.image_sha256, sha256_hex(&image_bytes));
    assert!(preview.image_bytes > 0);
    assert!(preview.drawn_particles > 0);
    assert!(preview.draw_calls > 0);
    assert!(preview.lit_pixel_ratio > 0.01);
    assert_eq!(after.save_count, 1);
    assert_eq!(after.preview_refresh_count, 1);
    assert!(after.last_error.is_empty());
}

#[test]
fn vfx_editor_edge_cases_mutate_or_reject_real_state() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue748_vfx_editor", "edges").unwrap();
    let _ = fs::remove_dir_all(&root);
    let mut runtime = EditorVfxRuntime::with_store_root(&root).unwrap();
    let save_before = runtime.save_effect().unwrap();
    let before_remove = runtime.evidence();
    println!(
        "issue748 remove before update_modules={} digest={}",
        before_remove.update_module_count, save_before.digest
    );

    assert!(runtime.remove_size_over_life().unwrap());
    let save_after = runtime.save_effect().unwrap();
    let after_remove = runtime.evidence();
    println!(
        "issue748 remove after update_modules={} digest={}",
        after_remove.update_module_count, save_after.digest
    );
    assert_eq!(
        after_remove.update_module_count,
        before_remove.update_module_count - 1
    );
    assert_ne!(save_before.digest, save_after.digest);

    runtime.restore_size_over_life().unwrap();
    let preview_a = runtime.refresh_preview().unwrap();
    let preview_b = runtime.refresh_preview().unwrap();
    println!(
        "issue748 consistency preview_a={} preview_b={} draw_calls={}",
        preview_a.image_sha256, preview_b.image_sha256, preview_b.draw_calls
    );
    assert_eq!(preview_a.image_sha256, preview_b.image_sha256);
    assert_eq!(preview_a.draw_calls, preview_b.draw_calls);

    runtime.set_invalid_size_curve_for_fsv().unwrap();
    let invalid_before = runtime.evidence();
    let error = runtime
        .save_effect()
        .expect_err("invalid curve must fail before writing");
    let invalid_after = runtime.evidence();
    println!(
        "issue748 invalid_curve before_error={} after_error={}",
        invalid_before.last_error, invalid_after.last_error
    );
    assert!(error.to_string().contains("render.vfx"));
    assert!(error.to_string().contains("curve"));
    assert!(invalid_after.last_error.contains("curve"));
}
