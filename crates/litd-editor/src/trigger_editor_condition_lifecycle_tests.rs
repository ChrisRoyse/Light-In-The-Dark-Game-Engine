use super::{
    TriggerConditionMove, append_default_integer_greater_equal_condition, delete_trigger_condition,
    fire_unit_dies, issue733_sample_project, move_trigger_condition,
    set_integer_greater_equal_payload,
};

const TRIGGER_ID: &str = "trg_unit_death_reinforcement";
const ORIGINAL_CONDITION_ID: &str = "cond_reinforcements_enabled";

#[test]
fn condition_move_changes_reported_authored_evaluation_order() {
    let mut project = issue733_sample_project().expect("sample project");
    let condition = append_false_integer_condition(&mut project);
    let before_fingerprint = project.stable_fingerprint().expect("before fingerprint");
    assert_eq!(
        fire_unit_dies(&project, "unit_scout")
            .expect("original order evaluates")
            .condition_results,
        vec![true, false]
    );

    assert!(
        move_trigger_condition(
            &mut project,
            TRIGGER_ID,
            &condition.node_id,
            TriggerConditionMove::Earlier,
        )
        .expect("move condition earlier")
    );

    assert_ne!(
        project.stable_fingerprint().expect("after fingerprint"),
        before_fingerprint
    );
    assert_eq!(project.graphs[0].conditions[0].order, 2);
    assert_eq!(project.graphs[0].conditions[1].order, 1);
    assert_eq!(
        fire_unit_dies(&project, "unit_scout")
            .expect("authored order evaluates")
            .condition_results,
        vec![false, true]
    );
}

#[test]
fn condition_move_boundary_is_an_exact_noop() {
    let mut project = issue733_sample_project().expect("sample project");
    append_false_integer_condition(&mut project);
    let before = project.clone();

    assert!(
        !move_trigger_condition(
            &mut project,
            TRIGGER_ID,
            ORIGINAL_CONDITION_ID,
            TriggerConditionMove::Earlier,
        )
        .expect("first condition cannot move earlier")
    );
    assert_eq!(project, before);
}

#[test]
fn invalid_condition_moves_fail_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    append_false_integer_condition(&mut project);
    let before = project.clone();
    let error = move_trigger_condition(
        &mut project,
        TRIGGER_ID,
        "missing_condition",
        TriggerConditionMove::Later,
    )
    .expect_err("unknown condition must fail");
    assert!(
        error
            .to_string()
            .contains("unknown condition node missing_condition")
    );
    assert_eq!(project, before);

    project.graphs[0].conditions[1].order = project.graphs[0].conditions[0].order;
    let invalid_before = project.clone();
    let error = move_trigger_condition(
        &mut project,
        TRIGGER_ID,
        ORIGINAL_CONDITION_ID,
        TriggerConditionMove::Later,
    )
    .expect_err("duplicate authored order must fail");
    assert!(error.to_string().contains("duplicate condition order"));
    assert_eq!(project, invalid_before);
}

#[test]
fn condition_delete_returns_node_and_changes_execution() {
    let mut project = issue733_sample_project().expect("sample project");
    let condition = append_false_integer_condition(&mut project);
    assert!(
        !fire_unit_dies(&project, "unit_scout")
            .expect("false condition blocks actions")
            .fired
    );

    let removed =
        delete_trigger_condition(&mut project, TRIGGER_ID, &condition.node_id).expect("delete");

    assert_eq!(removed, condition);
    assert!(
        fire_unit_dies(&project, "unit_scout")
            .expect("remaining true condition fires")
            .fired
    );
}

#[test]
fn deleting_final_condition_is_valid_and_unknown_delete_is_transactional() {
    let mut project = issue733_sample_project().expect("sample project");
    let removed = delete_trigger_condition(&mut project, TRIGGER_ID, ORIGINAL_CONDITION_ID)
        .expect("final condition is optional");
    assert_eq!(removed.node_id, ORIGINAL_CONDITION_ID);
    assert!(project.graphs[0].conditions.is_empty());
    project.validate().expect("condition-free graph validates");
    assert!(
        fire_unit_dies(&project, "unit_scout")
            .expect("condition-free graph fires")
            .fired
    );

    let before = project.clone();
    let error = delete_trigger_condition(&mut project, TRIGGER_ID, "missing_condition")
        .expect_err("unknown delete must fail");
    assert!(
        error
            .to_string()
            .contains("unknown condition node missing_condition")
    );
    assert_eq!(project, before);
}

fn append_false_integer_condition(
    project: &mut super::TriggerEditorProject,
) -> crate::mcp_trigger_tools::EcaNode {
    let condition = append_default_integer_greater_equal_condition(project, TRIGGER_ID)
        .expect("append integer condition");
    assert!(
        set_integer_greater_equal_payload(project, TRIGGER_ID, &condition.node_id, 0, 1)
            .expect("make condition false")
    );
    project.graphs[0]
        .conditions
        .iter()
        .find(|candidate| candidate.node_id == condition.node_id)
        .expect("edited condition")
        .clone()
}
