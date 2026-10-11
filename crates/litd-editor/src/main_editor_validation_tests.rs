use litd_editor::world_validation::WorldValidationUseCase;

use super::*;

#[test]
fn validate_command_records_clean_world_state_in_evidence() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let before = runtime.evidence();
    println!(
        "issue1218 runtime_clean before status={} runs={} summary={}",
        before.world_validation_status,
        before.world_validation_run_count,
        before.world_validation_summary
    );

    runtime.validate_active_world_command().unwrap();
    let after = runtime.evidence();
    println!(
        "issue1218 runtime_clean after status={} runs={} errors={} summary={}",
        after.world_validation_status,
        after.world_validation_run_count,
        after.world_validation_error_count,
        after.world_validation_summary
    );

    assert_eq!(after.world_validation_use_case, "Play");
    assert_eq!(after.world_validation_status, "ok");
    assert_eq!(after.world_validation_error_count, 0);
    assert_eq!(
        after.world_validation_run_count,
        before.world_validation_run_count + 1
    );
}

#[test]
fn validation_blocks_playtest_without_mutating_play_state() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let mut document = runtime.play_document_from_vault().unwrap();
    document.objects.remove("unit_playtest_hero");

    let before = runtime.evidence();
    let error = runtime
        .start_playtest_with_document_for_test(document)
        .unwrap_err()
        .to_string();
    let after = runtime.evidence();
    println!(
        "issue1218 runtime_block before play_state={} transitions={} after play_state={} transitions={} validation={} errors={} error={}",
        before.play_state,
        before.play_transition_count,
        after.play_state,
        after.play_transition_count,
        after.world_validation_status,
        after.world_validation_error_count,
        error
    );

    assert!(error.contains("editor.validation.blocking"));
    assert!(error.contains("Playtest needs at least one Unit"));
    assert_eq!(after.play_state, "Stopped");
    assert_eq!(after.play_transition_count, before.play_transition_count);
    assert_eq!(
        after.world_validation_use_case,
        format!("{:?}", WorldValidationUseCase::Play)
    );
    assert_eq!(after.world_validation_status, "blocking");
}
