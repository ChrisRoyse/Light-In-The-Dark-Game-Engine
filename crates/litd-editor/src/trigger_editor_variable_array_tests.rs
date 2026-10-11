use super::{
    TriggerActionPaletteKind, TriggerEditorVariableType, TriggerVariablePaletteKind,
    add_default_trigger_variable_array, append_default_trigger_action, delete_trigger_variable,
    fire_unit_dies, issue733_sample_project, set_trigger_variable_action_indexed_value,
    update_trigger_variable_array_definition,
};

const TRIGGER_ID: &str = "trg_unit_death_reinforcement";

#[test]
fn array_defaults_sizes_and_ids_are_deterministic() {
    let mut project = issue733_sample_project().expect("sample project");
    let boolean =
        add_default_trigger_variable_array(&mut project, TriggerVariablePaletteKind::Boolean, 4)
            .expect("Boolean array");
    let integer =
        add_default_trigger_variable_array(&mut project, TriggerVariablePaletteKind::Integer, 3)
            .expect("Integer array");
    let string =
        add_default_trigger_variable_array(&mut project, TriggerVariablePaletteKind::String, 2)
            .expect("String array");
    let boolean_two =
        add_default_trigger_variable_array(&mut project, TriggerVariablePaletteKind::Boolean, 2)
            .expect("second Boolean array");

    assert_eq!(boolean.name, "variable_bool_array_1");
    assert_eq!(boolean.array_values, vec!["false"; 4]);
    assert!(boolean.initial_value.is_empty());
    assert_eq!(integer.name, "variable_integer_array_1");
    assert_eq!(integer.array_values, vec!["0"; 3]);
    assert_eq!(string.name, "variable_string_array_1");
    assert_eq!(string.array_values, vec!["value"; 2]);
    assert_eq!(boolean_two.name, "variable_bool_array_2");

    let before = project.clone();
    let error =
        add_default_trigger_variable_array(&mut project, TriggerVariablePaletteKind::String, 1)
            .expect_err("undersized array");
    assert!(error.to_string().contains("must be 2..=256"));
    assert_eq!(project, before);
}

#[test]
fn indexed_set_variable_edits_and_executes_typed_array_element() {
    let mut project = issue733_sample_project().expect("sample project");
    let array =
        add_default_trigger_variable_array(&mut project, TriggerVariablePaletteKind::Integer, 4)
            .expect("Integer array");
    let action = append_default_trigger_action(
        &mut project,
        TRIGGER_ID,
        TriggerActionPaletteKind::SetVariable,
    )
    .expect("set-variable action");

    assert!(
        set_trigger_variable_action_indexed_value(
            &mut project,
            TRIGGER_ID,
            &action.node_id,
            &array.name,
            Some("2"),
            "17",
        )
        .expect("indexed edit")
    );
    let authored = &project.graphs[0].actions[2];
    assert_eq!(authored.args["variable"], array.name);
    assert_eq!(authored.args["index"], "2");
    assert_eq!(authored.args["value"], "17");
    let report = fire_unit_dies(&project, "unit_scout").expect("trigger executes");
    assert_eq!(
        report.variable_array_values[&array.name],
        vec!["0", "0", "17", "0"]
    );
}

#[test]
fn invalid_array_indices_and_values_fail_without_mutation() {
    let mut project = issue733_sample_project().expect("sample project");
    let array =
        add_default_trigger_variable_array(&mut project, TriggerVariablePaletteKind::Boolean, 4)
            .expect("Boolean array");
    let action = append_default_trigger_action(
        &mut project,
        TRIGGER_ID,
        TriggerActionPaletteKind::SetVariable,
    )
    .expect("set-variable action");
    let before = project.clone();
    for (index, value, expected) in [
        (None, "true", "requires an index"),
        (Some("-1"), "true", "expected unsigned integer"),
        (Some("four"), "true", "expected unsigned integer"),
        (Some("4"), "true", "out of bounds"),
        (Some("1"), "not_boolean", "does not match Boolean"),
    ] {
        let error = set_trigger_variable_action_indexed_value(
            &mut project,
            TRIGGER_ID,
            &action.node_id,
            &array.name,
            index,
            value,
        )
        .expect_err("invalid indexed edit");
        assert!(error.to_string().contains(expected), "error={error}");
        assert_eq!(project, before);
    }
}

#[test]
fn referenced_array_resize_delete_and_type_changes_preserve_integrity() {
    let mut project = issue733_sample_project().expect("sample project");
    let array =
        add_default_trigger_variable_array(&mut project, TriggerVariablePaletteKind::Integer, 4)
            .expect("Integer array");
    let action = append_default_trigger_action(
        &mut project,
        TRIGGER_ID,
        TriggerActionPaletteKind::SetVariable,
    )
    .expect("set-variable action");
    set_trigger_variable_action_indexed_value(
        &mut project,
        TRIGGER_ID,
        &action.node_id,
        &array.name,
        Some("3"),
        "9",
    )
    .expect("indexed edit");
    let referenced = project.clone();

    let error = update_trigger_variable_array_definition(
        &mut project,
        &array.name,
        &array.name,
        TriggerEditorVariableType::Integer,
        vec!["0".to_string(); 3],
    )
    .expect_err("unsafe shrink");
    assert!(error.to_string().contains("indexed references"));
    assert_eq!(project, referenced);
    let error = update_trigger_variable_array_definition(
        &mut project,
        &array.name,
        &array.name,
        TriggerEditorVariableType::String,
        vec!["value".to_string(); 4],
    )
    .expect_err("referenced type change");
    assert!(error.to_string().contains("cannot change type"));
    assert_eq!(project, referenced);
    let error = delete_trigger_variable(&mut project, &array.name).expect_err("referenced delete");
    assert!(error.to_string().contains("cannot delete referenced"));
    assert_eq!(project, referenced);

    assert!(
        update_trigger_variable_array_definition(
            &mut project,
            &array.name,
            "wave_counts",
            TriggerEditorVariableType::Integer,
            vec!["0".to_string(); 5],
        )
        .expect("safe expand and rename")
    );
    assert_eq!(project.graphs[0].actions[2].args["variable"], "wave_counts");
    project.validate().expect("expanded project valid");
}
