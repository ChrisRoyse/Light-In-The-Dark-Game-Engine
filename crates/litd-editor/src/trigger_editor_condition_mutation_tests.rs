use super::{
    TriggerEditorVariableType, append_default_game_flag_condition,
    append_default_unit_in_region_condition, fire_unit_dies, issue733_sample_project,
};

const TRIGGER_ID: &str = "trg_unit_death_reinforcement";

#[test]
fn game_flag_condition_uses_typed_variable_and_executes() {
    let mut project = issue733_sample_project().expect("sample project");

    let condition =
        append_default_game_flag_condition(&mut project, TRIGGER_ID).expect("append condition");

    assert_eq!(condition.node_id, "cond_game_flag_1");
    assert_eq!(condition.operation, "game_flag_true");
    assert_eq!(condition.order, 2);
    assert_eq!(condition.args["flag"], "reinforcements_enabled");
    assert_eq!(condition.variable_refs, vec!["reinforcements_enabled"]);
    let report = fire_unit_dies(&project, "unit_scout").expect("trigger fires");
    assert_eq!(report.condition_results, vec![true, true]);
    assert!(report.fired);
}

#[test]
fn game_flag_condition_ids_and_orders_advance_deterministically() {
    let mut project = issue733_sample_project().expect("sample project");
    append_default_game_flag_condition(&mut project, TRIGGER_ID).expect("first condition");
    let second =
        append_default_game_flag_condition(&mut project, TRIGGER_ID).expect("second condition");

    assert_eq!(second.node_id, "cond_game_flag_2");
    assert_eq!(second.order, 3);
}

#[test]
fn unit_in_region_condition_uses_stable_defaults_and_executes() {
    let mut project = issue733_sample_project().expect("sample project");

    let condition = append_default_unit_in_region_condition(&mut project, TRIGGER_ID)
        .expect("append condition");

    assert_eq!(condition.node_id, "cond_unit_in_region_1");
    assert_eq!(condition.operation, "unit_in_region");
    assert_eq!(condition.order, 2);
    assert_eq!(condition.args["unit_id"], "unit_scout");
    assert_eq!(condition.args["region_id"], "reinforcement_point");
    assert!(condition.variable_refs.is_empty());
    let report = fire_unit_dies(&project, "unit_scout").expect("trigger fires");
    assert_eq!(report.condition_results, vec![true, true]);
    assert!(report.fired);
}

#[test]
fn invalid_condition_defaults_fail_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    let before = project.clone();
    let error = append_default_game_flag_condition(&mut project, "missing_trigger")
        .expect_err("unknown graph must fail");
    assert!(
        error
            .to_string()
            .contains("unknown trigger graph missing_trigger")
    );
    assert_eq!(project, before);

    let variable = project
        .variables
        .get_mut("reinforcements_enabled")
        .expect("sample variable");
    variable.value_type = TriggerEditorVariableType::Integer;
    variable.initial_value = "1".to_string();
    project.graphs[0].conditions.clear();
    project
        .validate()
        .expect("fixture with no Boolean variables remains valid");
    let before = project.clone();
    let error = append_default_game_flag_condition(&mut project, TRIGGER_ID)
        .expect_err("Boolean variable is required");
    assert!(error.to_string().contains("no Boolean variable"));
    assert_eq!(project, before);
}
