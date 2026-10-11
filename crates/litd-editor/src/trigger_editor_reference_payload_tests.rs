use super::{
    TriggerEditorVariable, TriggerEditorVariableType, TriggerEventPaletteKind,
    append_default_unit_in_region_condition, fire_unit_dies, issue733_sample_project,
    replace_default_trigger_event, set_game_flag_condition_variable, set_unit_dies_event_unit,
    set_unit_enters_region_payload, set_unit_in_region_condition_payload,
};

const TRIGGER_ID: &str = "trg_unit_death_reinforcement";
const EVENT_ID: &str = "evt_unit_dies";
const CONDITION_ID: &str = "cond_reinforcements_enabled";
const ALTERNATE_REGION_ID: &str = "staging_area";

#[test]
fn unit_dies_target_edit_is_exact_and_executable() {
    let mut project = issue733_sample_project().expect("sample project");
    let mut guard = project.initial_units["unit_scout"].clone();
    guard.unit_id = "unit_guard".to_string();
    guard.unit_type_id = "unit_guard_type".to_string();
    project.initial_units.insert(guard.unit_id.clone(), guard);
    project.validate().expect("second initial unit is valid");

    assert!(
        set_unit_dies_event_unit(&mut project, TRIGGER_ID, EVENT_ID, "unit_guard")
            .expect("edit event target")
    );
    assert_eq!(project.graphs[0].events[0].args["unit_id"], "unit_guard");
    assert!(
        !set_unit_dies_event_unit(&mut project, TRIGGER_ID, EVENT_ID, "unit_guard")
            .expect("exact edit is a no-op")
    );
    assert!(
        fire_unit_dies(&project, "unit_guard")
            .expect("new target fires")
            .fired
    );
    assert!(
        fire_unit_dies(&project, "unit_scout")
            .expect_err("old target must not fire")
            .to_string()
            .contains("event did not match")
    );
}

#[test]
fn unit_dies_target_rejects_invalid_references_without_mutation() {
    let project = issue733_sample_project().expect("sample project");
    for (trigger_id, node_id, unit_id, expected) in [
        (TRIGGER_ID, EVENT_ID, "missing_unit", "unknown initial unit"),
        (
            "missing_trigger",
            EVENT_ID,
            "unit_scout",
            "unknown trigger graph",
        ),
        (
            TRIGGER_ID,
            "missing_event",
            "unit_scout",
            "unknown event node",
        ),
    ] {
        let mut candidate = project.clone();
        let error = set_unit_dies_event_unit(&mut candidate, trigger_id, node_id, unit_id)
            .expect_err("invalid edit must fail");
        assert!(error.to_string().contains(expected), "error={error}");
        assert_eq!(candidate, project);
    }
}

#[test]
fn game_flag_edit_updates_argument_and_variable_refs() {
    let mut project = issue733_sample_project().expect("sample project");
    project.variables.insert(
        "alternate_flag".to_string(),
        variable(
            "alternate_flag",
            TriggerEditorVariableType::Boolean,
            "false",
        ),
    );
    project.validate().expect("additional flag is valid");

    assert!(
        set_game_flag_condition_variable(&mut project, TRIGGER_ID, CONDITION_ID, "alternate_flag",)
            .expect("edit flag target")
    );
    let condition = &project.graphs[0].conditions[0];
    assert_eq!(condition.args["flag"], "alternate_flag");
    assert_eq!(condition.variable_refs, vec!["alternate_flag"]);
    assert!(
        !set_game_flag_condition_variable(
            &mut project,
            TRIGGER_ID,
            CONDITION_ID,
            "alternate_flag",
        )
        .expect("exact edit is a no-op")
    );
    let report = fire_unit_dies(&project, "unit_scout").expect("condition evaluates");
    assert_eq!(report.condition_results, vec![false]);
    assert!(!report.fired);
}

#[test]
fn game_flag_edit_rejects_non_scalar_boolean_targets_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    project.variables.insert(
        "integer_value".to_string(),
        variable("integer_value", TriggerEditorVariableType::Integer, "1"),
    );
    project.variables.insert(
        "flag_array".to_string(),
        TriggerEditorVariable {
            name: "flag_array".to_string(),
            value_type: TriggerEditorVariableType::Boolean,
            initial_value: String::new(),
            array_values: vec!["true".to_string(), "false".to_string()],
        },
    );
    project.validate().expect("candidate variables are valid");
    let before = project.clone();

    for variable_name in ["missing_flag", "integer_value", "flag_array"] {
        let error =
            set_game_flag_condition_variable(&mut project, TRIGGER_ID, CONDITION_ID, variable_name)
                .expect_err("invalid target must fail");
        assert!(
            error
                .to_string()
                .contains(if variable_name == "missing_flag" {
                    "unknown variable"
                } else {
                    "scalar Boolean"
                }),
            "error={error}"
        );
        assert_eq!(project, before);
    }
}

#[test]
fn unit_enters_region_edit_updates_both_references_transactionally() {
    let mut project = project_with_alternate_region();
    replace_default_trigger_event(
        &mut project,
        TRIGGER_ID,
        TriggerEventPaletteKind::UnitEntersRegion,
    )
    .expect("author region event");
    let original_event = &project.graphs[0].events[0];
    assert_eq!(
        original_event.args["region_id"], "reinforcement_point",
        "the deterministic default must differ from the edit target"
    );

    assert!(
        set_unit_enters_region_payload(
            &mut project,
            TRIGGER_ID,
            "evt_unit_enters_region",
            "unit_scout",
            ALTERNATE_REGION_ID,
        )
        .expect("edit region event")
    );
    let event = &project.graphs[0].events[0];
    assert_eq!(event.args["unit_id"], "unit_scout");
    assert_eq!(event.args["region_id"], ALTERNATE_REGION_ID);
    assert!(
        !set_unit_enters_region_payload(
            &mut project,
            TRIGGER_ID,
            "evt_unit_enters_region",
            "unit_scout",
            ALTERNATE_REGION_ID,
        )
        .expect("exact edit is a no-op")
    );
    project.validate().expect("edited region event validates");
}

#[test]
fn unit_in_region_edit_changes_evaluated_condition() {
    let mut project = project_with_alternate_region();
    let condition = append_default_unit_in_region_condition(&mut project, TRIGGER_ID)
        .expect("append condition");
    assert_eq!(
        condition.args["region_id"], "reinforcement_point",
        "the deterministic default must differ from the edit target"
    );

    assert!(
        set_unit_in_region_condition_payload(
            &mut project,
            TRIGGER_ID,
            &condition.node_id,
            "unit_scout",
            ALTERNATE_REGION_ID,
        )
        .expect("edit region condition")
    );
    let report = fire_unit_dies(&project, "unit_scout").expect("condition evaluates");
    assert_eq!(report.condition_results, vec![true, false]);
    assert!(!report.fired);
}

#[test]
fn unit_region_edits_reject_unknown_references_without_mutation() {
    let mut event_project = project_with_alternate_region();
    replace_default_trigger_event(
        &mut event_project,
        TRIGGER_ID,
        TriggerEventPaletteKind::UnitEntersRegion,
    )
    .expect("author region event");
    let event_before = event_project.clone();
    let event_error = set_unit_enters_region_payload(
        &mut event_project,
        TRIGGER_ID,
        "evt_unit_enters_region",
        "missing_unit",
        ALTERNATE_REGION_ID,
    )
    .expect_err("unknown event unit must fail");
    assert!(event_error.to_string().contains("unknown initial unit"));
    assert_eq!(event_project, event_before);

    let mut condition_project = project_with_alternate_region();
    let condition = append_default_unit_in_region_condition(&mut condition_project, TRIGGER_ID)
        .expect("append condition");
    let condition_before = condition_project.clone();
    let condition_error = set_unit_in_region_condition_payload(
        &mut condition_project,
        TRIGGER_ID,
        &condition.node_id,
        "unit_scout",
        "missing_region",
    )
    .expect_err("unknown condition region must fail");
    assert!(condition_error.to_string().contains("unknown region"));
    assert_eq!(condition_project, condition_before);
}

fn project_with_alternate_region() -> super::TriggerEditorProject {
    let mut project = issue733_sample_project().expect("sample project");
    let mut region = project.regions["reinforcement_point"].clone();
    region.region_id = ALTERNATE_REGION_ID.to_string();
    project.regions.insert(region.region_id.clone(), region);
    project.validate().expect("alternate region is valid");
    project
}

fn variable(
    name: &str,
    value_type: TriggerEditorVariableType,
    initial_value: &str,
) -> TriggerEditorVariable {
    TriggerEditorVariable {
        name: name.to_string(),
        value_type,
        initial_value: initial_value.to_string(),
        array_values: Vec::new(),
    }
}
