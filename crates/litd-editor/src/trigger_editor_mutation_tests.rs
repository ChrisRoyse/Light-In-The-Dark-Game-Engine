use std::collections::BTreeMap;

use super::{
    TRIGGER_EDITOR_SCHEMA, TriggerActionDraft, TriggerActionMove, TriggerActionPaletteKind,
    append_default_trigger_action, append_trigger_action, delete_trigger_action, fire_unit_dies,
    issue733_sample_project, move_trigger_action, next_trigger_action_node_id, set_trigger_enabled,
    set_trigger_wait_duration,
};

const SAMPLE_TRIGGER_ID: &str = "trg_unit_death_reinforcement";

#[test]
fn enabled_edit_controls_trigger_execution_and_reports_noop() {
    let mut project = issue733_sample_project().expect("sample project");

    assert!(set_trigger_enabled(&mut project, SAMPLE_TRIGGER_ID, false).expect("disable trigger"));
    assert!(!project.graphs[0].enabled);
    let error = fire_unit_dies(&project, "unit_scout").expect_err("disabled graph must not fire");
    assert!(
        error
            .to_string()
            .contains("missing enabled unit_dies graph")
    );

    assert!(
        !set_trigger_enabled(&mut project, SAMPLE_TRIGGER_ID, false)
            .expect("repeated disable is a no-op")
    );
    assert!(set_trigger_enabled(&mut project, SAMPLE_TRIGGER_ID, true).expect("enable trigger"));
    assert!(
        fire_unit_dies(&project, "unit_scout")
            .expect("enabled graph fires")
            .fired
    );
}

#[test]
fn unknown_trigger_id_fails_without_mutating_project() {
    let mut project = issue733_sample_project().expect("sample project");
    let before = project.clone();

    let error =
        set_trigger_enabled(&mut project, "missing_trigger", false).expect_err("unknown id fails");

    assert!(
        error
            .to_string()
            .contains("unknown trigger graph missing_trigger")
    );
    assert_eq!(project, before);
}

#[test]
fn invalid_project_fails_before_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    project.schema = format!("{TRIGGER_EDITOR_SCHEMA}.invalid");
    let before = project.clone();

    let error = set_trigger_enabled(&mut project, SAMPLE_TRIGGER_ID, false)
        .expect_err("invalid input fails");

    assert!(
        error
            .to_string()
            .contains("unexpected trigger editor schema")
    );
    assert_eq!(project, before);
}

#[test]
fn project_validation_rejects_duplicate_persisted_action_orders() {
    let mut project = issue733_sample_project().expect("sample project");
    project.graphs[0].actions[1].order = project.graphs[0].actions[0].order;

    let error = project
        .validate()
        .expect_err("duplicate action order must fail");

    assert!(error.to_string().contains("duplicate action order"));
}

#[test]
fn action_move_changes_semantic_execution_order() {
    let mut project = issue733_sample_project().expect("sample project");
    let before_fingerprint = project.stable_fingerprint().expect("before fingerprint");

    assert!(
        move_trigger_action(
            &mut project,
            SAMPLE_TRIGGER_ID,
            "act_create_guard",
            TriggerActionMove::Earlier,
        )
        .expect("move create action earlier")
    );

    assert_ne!(
        project.stable_fingerprint().expect("after fingerprint"),
        before_fingerprint
    );
    assert_eq!(
        fire_unit_dies(&project, "unit_scout")
            .expect("reordered trigger fires")
            .actions_executed,
        vec!["act_create_guard", "act_wait_for_death_animation"]
    );
}

#[test]
fn action_move_at_boundary_is_a_noop() {
    let mut project = issue733_sample_project().expect("sample project");
    let before = project.clone();

    assert!(
        !move_trigger_action(
            &mut project,
            SAMPLE_TRIGGER_ID,
            "act_wait_for_death_animation",
            TriggerActionMove::Earlier,
        )
        .expect("first action cannot move earlier")
    );
    assert_eq!(project, before);
}

#[test]
fn invalid_action_move_targets_fail_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    let before = project.clone();
    let error = move_trigger_action(
        &mut project,
        SAMPLE_TRIGGER_ID,
        "missing_action",
        TriggerActionMove::Later,
    )
    .expect_err("unknown action must fail");
    assert!(
        error
            .to_string()
            .contains("unknown action node missing_action")
    );
    assert_eq!(project, before);

    let duplicate_order = project.graphs[0].actions[0].order;
    project.graphs[0].actions[1].order = duplicate_order;
    let ambiguous = project.clone();
    let error = move_trigger_action(
        &mut project,
        SAMPLE_TRIGGER_ID,
        "act_create_guard",
        TriggerActionMove::Earlier,
    )
    .expect_err("duplicate order must fail");
    assert!(error.to_string().contains("duplicate action order"));
    assert_eq!(project, ambiguous);
}

#[test]
fn action_delete_removes_node_from_runtime_execution() {
    let mut project = issue733_sample_project().expect("sample project");

    let removed = delete_trigger_action(
        &mut project,
        SAMPLE_TRIGGER_ID,
        "act_wait_for_death_animation",
    )
    .expect("delete sleep action");

    assert_eq!(removed.node_id, "act_wait_for_death_animation");
    assert_eq!(project.graphs[0].actions.len(), 1);
    let report = fire_unit_dies(&project, "unit_scout").expect("edited trigger fires");
    assert!(report.sleep_actions_ms.is_empty());
    assert_eq!(report.actions_executed, vec!["act_create_guard"]);
}

#[test]
fn deleting_final_action_fails_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    delete_trigger_action(
        &mut project,
        SAMPLE_TRIGGER_ID,
        "act_wait_for_death_animation",
    )
    .expect("delete first of two actions");
    let before = project.clone();

    let error = delete_trigger_action(&mut project, SAMPLE_TRIGGER_ID, "act_create_guard")
        .expect_err("final action is required");

    assert!(error.to_string().contains("cannot delete final action"));
    assert_eq!(project, before);
}

#[test]
fn unknown_action_delete_target_fails_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    let before = project.clone();

    let error = delete_trigger_action(&mut project, SAMPLE_TRIGGER_ID, "missing_action")
        .expect_err("unknown action must fail");

    assert!(
        error
            .to_string()
            .contains("unknown action node missing_action")
    );
    assert_eq!(project, before);
}

#[test]
fn action_append_uses_next_order_and_changes_runtime_execution() {
    let mut project = issue733_sample_project().expect("sample project");

    let appended = append_trigger_action(
        &mut project,
        SAMPLE_TRIGGER_ID,
        wait_action_draft("act_wait_extra", "125"),
    )
    .expect("append wait action");

    assert_eq!(appended.order, 3);
    assert_eq!(project.graphs[0].actions.len(), 3);
    let report = fire_unit_dies(&project, "unit_scout").expect("extended trigger fires");
    assert_eq!(
        report.actions_executed,
        vec![
            "act_wait_for_death_animation",
            "act_create_guard",
            "act_wait_extra"
        ]
    );
    assert_eq!(report.sleep_actions_ms, vec![250, 125]);
}

#[test]
fn action_id_generation_is_stable_and_unique() {
    let mut project = issue733_sample_project().expect("sample project");
    let first =
        next_trigger_action_node_id(&project, SAMPLE_TRIGGER_ID, "act_wait").expect("first id");
    assert_eq!(first, "act_wait_1");
    append_trigger_action(
        &mut project,
        SAMPLE_TRIGGER_ID,
        wait_action_draft(&first, "250"),
    )
    .expect("append first generated id");

    let second =
        next_trigger_action_node_id(&project, SAMPLE_TRIGGER_ID, "act_wait").expect("second id");
    assert_eq!(second, "act_wait_2");
}

#[test]
fn duplicate_or_invalid_action_append_rolls_back() {
    let mut project = issue733_sample_project().expect("sample project");
    let before = project.clone();
    let error = append_trigger_action(
        &mut project,
        SAMPLE_TRIGGER_ID,
        wait_action_draft("act_create_guard", "250"),
    )
    .expect_err("duplicate node id must fail");
    assert!(
        error
            .to_string()
            .contains("duplicate node id act_create_guard")
    );
    assert_eq!(project, before);

    let error = append_trigger_action(
        &mut project,
        SAMPLE_TRIGGER_ID,
        wait_action_draft("act_invalid_wait", "0"),
    )
    .expect_err("invalid wait duration must fail");
    assert!(error.to_string().contains("sleep must be 1..=60000 ms"));
    assert_eq!(project, before);
}

fn wait_action_draft(node_id: &str, duration_ms: &str) -> TriggerActionDraft {
    TriggerActionDraft {
        node_id: node_id.to_string(),
        operation: "sleep".to_string(),
        args: BTreeMap::from([
            ("duration_ms".to_string(), duration_ms.to_string()),
            ("reason".to_string(), "authored wait".to_string()),
        ]),
        variable_refs: Vec::new(),
    }
}

#[test]
fn wait_duration_edit_changes_runtime_and_reports_noop() {
    let mut project = issue733_sample_project().expect("sample project");

    assert!(
        set_trigger_wait_duration(
            &mut project,
            SAMPLE_TRIGGER_ID,
            "act_wait_for_death_animation",
            375,
        )
        .expect("edit wait duration")
    );
    assert_eq!(
        fire_unit_dies(&project, "unit_scout")
            .expect("edited trigger fires")
            .sleep_actions_ms,
        vec![375]
    );
    let before_noop = project.clone();
    assert!(
        !set_trigger_wait_duration(
            &mut project,
            SAMPLE_TRIGGER_ID,
            "act_wait_for_death_animation",
            375,
        )
        .expect("same duration is a no-op")
    );
    assert_eq!(project, before_noop);
}

#[test]
fn invalid_wait_duration_edits_fail_without_mutation() {
    let project = issue733_sample_project().expect("sample project");
    for (trigger_id, node_id, duration, expected) in [
        (
            "missing_trigger",
            "act_wait_for_death_animation",
            250,
            "unknown trigger graph missing_trigger",
        ),
        (
            SAMPLE_TRIGGER_ID,
            "missing_action",
            250,
            "unknown action node missing_action",
        ),
        (SAMPLE_TRIGGER_ID, "act_create_guard", 250, "expected sleep"),
        (
            SAMPLE_TRIGGER_ID,
            "act_wait_for_death_animation",
            0,
            "wait duration must be 1..=60000 ms",
        ),
        (
            SAMPLE_TRIGGER_ID,
            "act_wait_for_death_animation",
            60_001,
            "wait duration must be 1..=60000 ms",
        ),
    ] {
        let mut candidate = project.clone();
        let error = set_trigger_wait_duration(&mut candidate, trigger_id, node_id, duration)
            .expect_err("invalid edit must fail");
        assert!(error.to_string().contains(expected), "error={error}");
        assert_eq!(candidate, project);
    }
}

#[test]
fn create_unit_palette_action_has_typed_defaults_and_executes() {
    let mut project = issue733_sample_project().expect("sample project");

    let action = append_default_trigger_action(
        &mut project,
        SAMPLE_TRIGGER_ID,
        TriggerActionPaletteKind::CreateUnitAtPoint,
    )
    .expect("append create-unit action");

    assert_eq!(action.node_id, "act_create_unit_1");
    assert_eq!(action.operation, "create_unit_at_point");
    assert_eq!(action.order, 3);
    assert_eq!(
        action.args.get("unit_id").map(String::as_str),
        Some("unit_authored_1")
    );
    assert_eq!(
        action.args.get("unit_type_id").map(String::as_str),
        Some("unit_guard_type")
    );
    assert_eq!(
        action.args.get("region_id").map(String::as_str),
        Some("reinforcement_point")
    );
    let report = fire_unit_dies(&project, "unit_scout").expect("palette action executes");
    assert_eq!(
        report.created_unit_ids,
        vec!["unit_reinforcement_guard", "unit_authored_1"]
    );
    assert_eq!(report.after_unit_count, 3);
}

#[test]
fn create_unit_palette_ids_advance_deterministically() {
    let mut project = issue733_sample_project().expect("sample project");
    append_default_trigger_action(
        &mut project,
        SAMPLE_TRIGGER_ID,
        TriggerActionPaletteKind::CreateUnitAtPoint,
    )
    .expect("append first create-unit action");
    let second = append_default_trigger_action(
        &mut project,
        SAMPLE_TRIGGER_ID,
        TriggerActionPaletteKind::CreateUnitAtPoint,
    )
    .expect("append second create-unit action");

    assert_eq!(second.node_id, "act_create_unit_2");
    assert_eq!(
        second.args.get("unit_id").map(String::as_str),
        Some("unit_authored_2")
    );
    assert_eq!(second.order, 4);
}

#[test]
fn palette_unknown_graph_fails_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    let before = project.clone();

    let error = append_default_trigger_action(
        &mut project,
        "missing_trigger",
        TriggerActionPaletteKind::CreateUnitAtPoint,
    )
    .expect_err("unknown graph must fail");

    assert!(
        error
            .to_string()
            .contains("unknown trigger graph missing_trigger")
    );
    assert_eq!(project, before);
}
