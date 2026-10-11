use super::{
    TriggerActionPaletteKind, TriggerEditorVariable, TriggerEditorVariableType,
    append_default_trigger_action, fire_unit_dies, issue733_sample_project,
    set_trigger_variable_action_value,
};

const TRIGGER_ID: &str = "trg_unit_death_reinforcement";

#[test]
fn variable_type_defaults_for_old_serialized_records() {
    let project = issue733_sample_project().expect("sample project");
    let mut value = serde_json::to_value(project).expect("serialize project");
    value["variables"]["reinforcements_enabled"]
        .as_object_mut()
        .expect("variable object")
        .remove("value_type");
    let old = serde_json::from_value::<super::TriggerEditorProject>(value)
        .expect("deserialize old project");
    assert_eq!(
        old.variables["reinforcements_enabled"].value_type,
        TriggerEditorVariableType::Boolean
    );
    old.validate().expect("old project remains valid");
}

#[test]
fn set_variable_palette_action_edits_and_executes() {
    let mut project = issue733_sample_project().expect("sample project");
    let action = append_default_trigger_action(
        &mut project,
        TRIGGER_ID,
        TriggerActionPaletteKind::SetVariable,
    )
    .expect("append set-variable action");
    assert_eq!(action.node_id, "act_set_variable_1");
    assert_eq!(action.order, 3);
    assert_eq!(action.args["variable"], "reinforcements_enabled");
    assert_eq!(action.args["value"], "true");

    assert!(
        set_trigger_variable_action_value(
            &mut project,
            TRIGGER_ID,
            &action.node_id,
            "reinforcements_enabled",
            "false",
        )
        .expect("edit Boolean value")
    );
    let report = fire_unit_dies(&project, "unit_scout").expect("trigger executes");
    assert_eq!(report.variable_values["reinforcements_enabled"], "false");
    assert!(
        !set_trigger_variable_action_value(
            &mut project,
            TRIGGER_ID,
            &action.node_id,
            "reinforcements_enabled",
            "false",
        )
        .expect("same value is a no-op")
    );
}

#[test]
fn invalid_variable_edits_fail_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    let action = append_default_trigger_action(
        &mut project,
        TRIGGER_ID,
        TriggerActionPaletteKind::SetVariable,
    )
    .expect("append set-variable action");
    let before = project.clone();
    for (node_id, variable, value, expected) in [
        (
            action.node_id.as_str(),
            "missing_variable",
            "false",
            "unknown variable missing_variable",
        ),
        (
            action.node_id.as_str(),
            "reinforcements_enabled",
            "not_boolean",
            "does not match Boolean",
        ),
        (
            "act_create_guard",
            "reinforcements_enabled",
            "false",
            "expected set_variable",
        ),
    ] {
        let error =
            set_trigger_variable_action_value(&mut project, TRIGGER_ID, node_id, variable, value)
                .expect_err("invalid variable edit");
        assert!(error.to_string().contains(expected), "error={error}");
        assert_eq!(project, before);
    }
}

#[test]
fn integer_variable_rejects_boolean_payload() {
    let mut project = issue733_sample_project().expect("sample project");
    project.variables.insert(
        "integer_counter".to_string(),
        TriggerEditorVariable {
            name: "integer_counter".to_string(),
            value_type: TriggerEditorVariableType::Integer,
            initial_value: "7".to_string(),
            array_values: Vec::new(),
        },
    );
    project
        .validate()
        .expect("Boolean condition and integer edit target are valid");
    let action = append_default_trigger_action(
        &mut project,
        TRIGGER_ID,
        TriggerActionPaletteKind::SetVariable,
    )
    .expect("append typed variable action");
    let before = project.clone();

    let error = set_trigger_variable_action_value(
        &mut project,
        TRIGGER_ID,
        &action.node_id,
        "integer_counter",
        "true",
    )
    .expect_err("type mismatch");
    assert!(error.to_string().contains("does not match Integer"));
    assert_eq!(project, before);
}
