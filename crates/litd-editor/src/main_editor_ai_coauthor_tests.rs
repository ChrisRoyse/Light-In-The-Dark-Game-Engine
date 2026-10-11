use super::*;
use std::path::PathBuf;

#[test]
fn happy_path_generates_applies_reads_back_from_aster_and_supports_edit_undo() {
    let (_temp_root, state_path, aster_dir) = test_paths("happy");
    let mut runtime = AiCoAuthorRuntime::new(state_path.clone(), aster_dir.clone()).unwrap();
    println!("before happy: {}", runtime.evidence().state_summary);

    runtime.generate_proposal().unwrap();
    runtime.apply_to_world().unwrap();

    let readback = read_aster_state(&aster_dir).unwrap().unwrap();
    let state = parse_state(&readback.bytes).unwrap();
    println!(
        "after apply: objects={} seq={} graph_rows={}",
        state.document.objects.len(),
        readback.seq,
        readback.graph_rows
    );
    assert_eq!(state.document.objects.len(), 1);
    let object = state
        .document
        .objects
        .get("ai_coauthor.ability_dawnward_cinder_bloom")
        .unwrap();
    assert_eq!(
        object.properties.get("display_name").unwrap(),
        "Cinder Bloom Ward"
    );
    assert_eq!(
        object.properties.get("capability_operation_id").unwrap(),
        AI_CAPABILITY_OPERATION_ID
    );
    assert!(
        object
            .properties
            .get("source_refs")
            .unwrap()
            .contains("Aster://")
    );

    runtime
        .hand_edit_last_object("Cinder Bloom Ward FSV Edit")
        .unwrap();
    let edited = parse_state(&read_aster_state(&aster_dir).unwrap().unwrap().bytes).unwrap();
    let edited_object = edited
        .document
        .objects
        .get("ai_coauthor.ability_dawnward_cinder_bloom")
        .unwrap();
    println!(
        "after hand edit: display_name={} revision={}",
        edited_object.properties.get("display_name").unwrap(),
        edited_object.properties.get("hand_edit_revision").unwrap()
    );
    assert_eq!(
        edited_object.properties.get("display_name").unwrap(),
        "Cinder Bloom Ward FSV Edit"
    );

    runtime.undo_last_apply().unwrap();
    let undone = parse_state(&read_aster_state(&aster_dir).unwrap().unwrap().bytes).unwrap();
    println!(
        "after undo: objects={} transactions={}",
        undone.document.objects.len(),
        undone.transactions.len()
    );
    assert!(undone.document.objects.is_empty());
    // ai co-author state persists to the Aster vault only (save_state writes just the
    // vault); the legacy JSON mirror is never written, so it must be absent -- proving
    // the Aster source of truth with no side-store leak (#1489).
    assert!(
        !state_path.exists(),
        "legacy JSON mirror must not be written; ai co-author state lives in the Aster vault"
    );
}

#[test]
fn edge_empty_prompt_is_refused_and_does_not_mutate_aster_document() {
    let (_temp_root, state_path, aster_dir) = test_paths("empty");
    let mut runtime = AiCoAuthorRuntime::new(state_path, aster_dir.clone()).unwrap();
    runtime.prompt.clear();
    let before = parse_state(&read_aster_state(&aster_dir).unwrap().unwrap().bytes).unwrap();
    println!("before empty: objects={}", before.document.objects.len());

    runtime.generate_proposal().unwrap();

    let after = parse_state(&read_aster_state(&aster_dir).unwrap().unwrap().bytes).unwrap();
    println!(
        "after empty: objects={} admission={}",
        after.document.objects.len(),
        after.outcome.as_ref().unwrap().admission_code
    );
    assert_eq!(after.document.objects.len(), before.document.objects.len());
    assert_eq!(
        after.outcome.unwrap().admission_code,
        "LITD-AUTHORING-EMPTY-REQUEST"
    );
}

#[test]
fn edge_out_of_domain_prompt_is_refused_by_ward_without_mutation() {
    let (_temp_root, state_path, aster_dir) = test_paths("ood");
    let mut runtime = AiCoAuthorRuntime::new(state_path, aster_dir.clone()).unwrap();
    runtime.prompt = "make a spaceship laser stock market ability".to_string();
    let before = parse_state(&read_aster_state(&aster_dir).unwrap().unwrap().bytes).unwrap();
    println!("before ood: objects={}", before.document.objects.len());

    runtime.generate_proposal().unwrap();

    let after = parse_state(&read_aster_state(&aster_dir).unwrap().unwrap().bytes).unwrap();
    let outcome = after.outcome.unwrap();
    println!(
        "after ood: objects={} admission={} blocked={:?}",
        after.document.objects.len(),
        outcome.admission_code,
        outcome.grounding.blocked_terms
    );
    assert_eq!(after.document.objects.len(), before.document.objects.len());
    assert_eq!(outcome.admission_code, "LITD-AUTHORING-REFUSED-OOD");
    assert_eq!(
        outcome.grounding.blocked_terms,
        vec!["spaceship", "laser", "stock market"]
    );
}

#[test]
fn edge_apply_failures_are_transactional_and_leave_aster_state_unchanged() {
    let (_temp_root, state_path, aster_dir) = test_paths("transactional");
    let mut runtime = AiCoAuthorRuntime::new(state_path, aster_dir.clone()).unwrap();
    let before = read_aster_state(&aster_dir).unwrap().unwrap();
    println!(
        "before bad apply: seq={} bytes={}",
        before.seq,
        before.bytes.len()
    );

    let err = runtime.apply_to_world().unwrap_err();
    let after = read_aster_state(&aster_dir).unwrap().unwrap();
    println!("after no proposal apply: seq={} error={}", after.seq, err);
    assert_eq!(after.bytes, before.bytes);

    runtime.generate_proposal().unwrap();
    runtime
        .outcome
        .as_mut()
        .unwrap()
        .lowered
        .as_mut()
        .unwrap()
        .fingerprint = "0000000000000000".to_string();
    let before_tamper = read_aster_state(&aster_dir).unwrap().unwrap();
    let err = runtime.apply_to_world().unwrap_err();
    let after_tamper = read_aster_state(&aster_dir).unwrap().unwrap();
    println!(
        "after tampered apply: seq={} objects={} error={}",
        after_tamper.seq,
        parse_state(&after_tamper.bytes)
            .unwrap()
            .document
            .objects
            .len(),
        err
    );
    assert_eq!(after_tamper.bytes, before_tamper.bytes);
}

// #1535: own a `TestTempDir` guard so the temp root is `remove_dir_all`'d on Drop instead of
// leaking under `%TEMP%/litd-ai-coauthor-*` forever (the raw `env::temp_dir().join(...)` this
// replaced had no cleanup). Callers MUST bind the returned guard for the whole test.
fn test_paths(label: &str) -> (litd_engine::test_temp::TestTempDir, PathBuf, PathBuf) {
    let root = litd_engine::test_temp::TestTempDir::new(&format!("ai-coauthor-{label}"));
    let state_path = root.path().join("state.json");
    let aster_dir = root.path().join("aster");
    (root, state_path, aster_dir)
}
