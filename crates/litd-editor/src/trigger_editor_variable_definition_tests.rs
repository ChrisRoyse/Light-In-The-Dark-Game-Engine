use super::{
    TriggerEditorVariableType, TriggerVariablePaletteKind, add_default_trigger_variable,
    append_default_trigger_action, delete_trigger_variable, fire_unit_dies,
    issue733_sample_project, update_trigger_variable_definition,
};

const TRIGGER_ID: &str = "trg_unit_death_reinforcement";

#[test]
fn scalar_variable_defaults_and_ids_are_deterministic() {
    let mut project = issue733_sample_project().expect("sample project");
    let boolean = add_default_trigger_variable(&mut project, TriggerVariablePaletteKind::Boolean)
        .expect("Boolean variable");
    let integer = add_default_trigger_variable(&mut project, TriggerVariablePaletteKind::Integer)
        .expect("Integer variable");
    let string = add_default_trigger_variable(&mut project, TriggerVariablePaletteKind::String)
        .expect("String variable");
    let boolean_two =
        add_default_trigger_variable(&mut project, TriggerVariablePaletteKind::Boolean)
            .expect("second Boolean variable");

    assert_eq!(
        (
            boolean.name.as_str(),
            boolean.value_type,
            boolean.initial_value.as_str()
        ),
        (
            "variable_bool_1",
            TriggerEditorVariableType::Boolean,
            "false"
        )
    );
    assert_eq!(
        (
            integer.name.as_str(),
            integer.value_type,
            integer.initial_value.as_str()
        ),
        (
            "variable_integer_1",
            TriggerEditorVariableType::Integer,
            "0"
        )
    );
    assert_eq!(
        (
            string.name.as_str(),
            string.value_type,
            string.initial_value.as_str()
        ),
        (
            "variable_string_1",
            TriggerEditorVariableType::String,
            "value"
        )
    );
    assert_eq!(boolean_two.name, "variable_bool_2");
}

#[test]
fn referenced_variable_rename_rewrites_graph_integrity() {
    let mut project = issue733_sample_project().expect("sample project");
    append_default_trigger_action(
        &mut project,
        TRIGGER_ID,
        super::TriggerActionPaletteKind::SetVariable,
    )
    .expect("set-variable action");

    assert!(
        update_trigger_variable_definition(
            &mut project,
            "reinforcements_enabled",
            "reinforcements_active",
            TriggerEditorVariableType::Boolean,
            "true",
        )
        .expect("rename referenced variable")
    );

    assert!(!project.variables.contains_key("reinforcements_enabled"));
    assert!(project.variables.contains_key("reinforcements_active"));
    assert_eq!(
        project.graphs[0].conditions[0].args["flag"],
        "reinforcements_active"
    );
    assert_eq!(
        project.graphs[0].actions[2].args["variable"],
        "reinforcements_active"
    );
    project.validate().expect("renamed project valid");
    assert!(
        fire_unit_dies(&project, "unit_scout")
            .expect("renamed trigger fires")
            .fired
    );
}

#[test]
fn referenced_delete_and_type_change_fail_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    let before = project.clone();
    let error = delete_trigger_variable(&mut project, "reinforcements_enabled")
        .expect_err("referenced delete fails");
    assert!(
        error
            .to_string()
            .contains("cannot delete referenced variable")
    );
    assert_eq!(project, before);

    let error = update_trigger_variable_definition(
        &mut project,
        "reinforcements_enabled",
        "reinforcements_enabled",
        TriggerEditorVariableType::Integer,
        "1",
    )
    .expect_err("referenced type change fails");
    assert!(
        error
            .to_string()
            .contains("cannot change type of referenced variable")
    );
    assert_eq!(project, before);
}

#[test]
fn unreferenced_update_delete_and_invalid_inputs_are_transactional() {
    let mut project = issue733_sample_project().expect("sample project");
    let added = add_default_trigger_variable(&mut project, TriggerVariablePaletteKind::Integer)
        .expect("Integer variable");
    assert!(
        update_trigger_variable_definition(
            &mut project,
            &added.name,
            "wave_count",
            TriggerEditorVariableType::String,
            "seven",
        )
        .expect("rename and type unreferenced variable")
    );
    let before_invalid = project.clone();
    for (name, value, expected) in [
        ("bad name", "value", "ASCII identifier"),
        (
            "reinforcements_enabled",
            "value",
            "duplicate trigger variable",
        ),
        ("wave_count", "", "bounded single-line"),
    ] {
        let error = update_trigger_variable_definition(
            &mut project,
            "wave_count",
            name,
            TriggerEditorVariableType::String,
            value,
        )
        .expect_err("invalid variable update");
        assert!(error.to_string().contains(expected), "error={error}");
        assert_eq!(project, before_invalid);
    }
    let removed = delete_trigger_variable(&mut project, "wave_count").expect("delete unreferenced");
    assert_eq!(removed.name, "wave_count");
}
