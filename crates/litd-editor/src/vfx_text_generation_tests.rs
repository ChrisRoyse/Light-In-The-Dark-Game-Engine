use std::{fs, path::PathBuf};

use litd_engine::render::vfx::{count_effects_in_content_store, read_effect_from_content_store};

use super::*;

#[test]
fn text_generation_writes_effect_and_aster_ledger() {
    let root = test_root("happy");
    let _ = fs::remove_dir_all(&root);
    let state_path = root.join("ledger-legacy.json");
    let aster_dir = root.join("aster");
    let store_root = root.join("store");
    let request = request("issue749-happy", "a fiery explosion with rising embers");
    let before = count_effects_in_content_store(&store_root).unwrap();
    println!(
        "issue749 happy before store_objects={} graph_rows={}",
        before.effect_count, before.graph_rows
    );

    let stored = generate_and_store_vfx_text_effect(
        &state_path,
        &aster_dir,
        &store_root,
        "issue749-happy-ledger",
        request,
    )
    .unwrap();
    let effect_readback =
        read_effect_from_content_store(&store_root, &stored.record.descriptor).unwrap();
    let ledger_readback = read_vfx_text_generation_ledger(&aster_dir, "issue749-happy-ledger")
        .unwrap()
        .expect("ledger should exist in Aster");
    let after = count_effects_in_content_store(&store_root).unwrap();
    println!(
        "issue749 happy after store_objects={} store_graph_rows={} effect={} sha={} ledger_seq={} ledger_sha={} ledger_graph_rows={}",
        after.effect_count,
        after.graph_rows,
        effect_readback.effect.effect_id,
        effect_readback.sha256,
        ledger_readback.seq,
        ledger_readback.sha256,
        ledger_readback.graph_rows
    );

    assert_eq!(before.effect_count, 0);
    assert_eq!(after.effect_count, 1);
    assert_eq!(
        effect_readback.effect.effect_id,
        "generated.vfx.fire.explosion"
    );
    assert_eq!(effect_readback.sha256, stored.record.content_object_sha256);
    assert!(!state_path.exists());
    assert_eq!(ledger_readback.record_count, 1);
    assert!(
        stored
            .record
            .taxonomy
            .features
            .contains(&"rising_embers".to_string())
    );
    assert!(!stored.record.budget.capped);
}

#[test]
fn text_generation_rejects_ungrounded_prompt_without_writing() {
    let root = test_root("reject");
    let _ = fs::remove_dir_all(&root);
    let state_path = root.join("ledger-legacy.json");
    let aster_dir = root.join("aster");
    let store_root = root.join("store");
    let before = count_effects_in_content_store(&store_root).unwrap();
    println!(
        "issue749 reject before store_objects={} graph_rows={}",
        before.effect_count, before.graph_rows
    );

    let error = generate_and_store_vfx_text_effect(
        &state_path,
        &aster_dir,
        &store_root,
        "issue749-reject-ledger",
        request("issue749-reject", "make the thing nicer"),
    )
    .expect_err("ungrounded prompt must fail closed");
    let after = count_effects_in_content_store(&store_root).unwrap();
    println!(
        "issue749 reject after store_objects={} graph_rows={} error={error}",
        after.effect_count, after.graph_rows
    );

    assert_eq!(before.effect_count, 0);
    assert_eq!(after.effect_count, 0);
    assert!(!state_path.exists());
    assert!(
        read_vfx_text_generation_ledger(&aster_dir, "issue749-reject-ledger")
            .unwrap()
            .is_none()
    );
    assert!(error.to_string().contains("taxonomy.element"));
}

#[test]
fn text_generation_records_explicit_particle_cap() {
    let root = test_root("cap");
    let _ = fs::remove_dir_all(&root);
    let state_path = root.join("ledger-legacy.json");
    let aster_dir = root.join("aster");
    let store_root = root.join("store");
    let before = count_effects_in_content_store(&store_root).unwrap();
    println!(
        "issue749 cap before store_objects={} graph_rows={}",
        before.effect_count, before.graph_rows
    );

    let stored = generate_and_store_vfx_text_effect(
        &state_path,
        &aster_dir,
        &store_root,
        "issue749-cap-ledger",
        request(
            "issue749-cap",
            "a massive fiery explosion with 250000 particles and rising embers",
        ),
    )
    .unwrap();
    let ledger_readback = read_vfx_text_generation_ledger(&aster_dir, "issue749-cap-ledger")
        .unwrap()
        .expect("ledger should exist in Aster");
    let parsed = parse_vfx_text_generation_ledger(&ledger_readback.bytes).unwrap();
    let after = count_effects_in_content_store(&store_root).unwrap();
    println!(
        "issue749 cap after store_objects={} graph_rows={} requested={} emitted={} capped={} reason={}",
        after.effect_count,
        after.graph_rows,
        parsed.records[0].budget.requested_max_particles,
        parsed.records[0].budget.emitted_max_particles,
        parsed.records[0].budget.capped,
        parsed.records[0].budget.cap_reason
    );

    assert_eq!(before.effect_count, 0);
    assert_eq!(after.effect_count, 1);
    assert_eq!(stored.record.budget.requested_max_particles, 250_000);
    assert_eq!(stored.record.budget.emitted_max_particles, 65_536);
    assert!(stored.record.budget.capped);
    assert!(parsed.records[0].budget.cap_reason.contains("250000"));
    assert!(!state_path.exists());
}

fn request(id: &str, prompt: &str) -> VfxTextGenerationRequest {
    VfxTextGenerationRequest {
        request_id: id.to_string(),
        prompt: prompt.to_string(),
        seed: 7_490,
        requested_by: "litd-editor-vfx-text-generation-test".to_string(),
    }
}

fn test_root(name: &str) -> PathBuf {
    crate::fsv_support::artifact_dir("issue749_vfx_text_generation", "unit")
        .unwrap()
        .1
        .join(name)
}
