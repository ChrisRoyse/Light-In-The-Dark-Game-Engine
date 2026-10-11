use std::collections::{BTreeMap, BTreeSet};

use super::*;

#[test]
fn action_catalog_covers_core_categories() {
    validate_action_catalog_complete().unwrap();
    let categories = action_catalog()
        .iter()
        .map(|action| action.category)
        .collect::<BTreeSet<_>>();
    assert_eq!(categories.len(), REQUIRED_ACTION_CATEGORIES.len());
    assert!(action_catalog().len() >= 12);
    assert!(find_action("create_unit").is_ok());
    assert!(find_action("if_then_else").is_ok());
}

#[test]
fn unit_actions_mutate_known_context() {
    let mut context = synthetic_context();
    execute_action(
        "create_unit",
        &args([
            ("unit_id", "unit_footman_02"),
            ("unit_type_id", "footman"),
            ("player_id", "player_1"),
            ("region_id", "region_gate_01"),
        ]),
        &mut context,
    )
    .unwrap();
    execute_action(
        "move_unit",
        &args([
            ("unit_id", "unit_footman_02"),
            ("region_id", "region_bridge_01"),
        ]),
        &mut context,
    )
    .unwrap();
    assert_eq!(
        context.units["unit_footman_02"].region_id,
        "region_bridge_01"
    );
}

#[test]
fn state_flow_math_and_string_actions_mutate_known_context() {
    let mut context = synthetic_context();
    execute_action(
        "set_boolean_variable",
        &args([("variable_name", "door_open"), ("value", "true")]),
        &mut context,
    )
    .unwrap();
    execute_action(
        "integer_add",
        &args([("left", "2"), ("right", "3"), ("result_var", "sum")]),
        &mut context,
    )
    .unwrap();
    execute_action(
        "string_concat",
        &args([
            ("left", "gate"),
            ("right", "_open"),
            ("result_var", "label"),
        ]),
        &mut context,
    )
    .unwrap();
    execute_action("wait_seconds", &args([("seconds", "1.5")]), &mut context).unwrap();
    execute_action(
        "if_then_else",
        &args([
            ("condition_result", "true"),
            ("then_label", "open_gate"),
            ("else_label", "close_gate"),
        ]),
        &mut context,
    )
    .unwrap();
    assert_eq!(
        context.variables["door_open"],
        TriggerActionVariable::Boolean(true)
    );
    assert_eq!(context.variables["sum"], TriggerActionVariable::Integer(5));
    assert_eq!(
        context.variables["label"],
        TriggerActionVariable::String("gate_open".to_string())
    );
    assert_eq!(context.waits_seconds, vec![1.5]);
    assert_eq!(context.branch_records, vec!["open_gate".to_string()]);
}

#[test]
fn action_validation_fails_closed_for_unknown_action() {
    let error = find_action("summon_everything_forever").unwrap_err();
    assert_eq!(error.code(), "LITD-RENDER-UI-INVALID-CONFIG");
    assert!(error.to_string().contains("unknown trigger action"));
}

#[test]
fn action_validation_fails_closed_for_missing_required_arg() {
    let error = validate_action_args("kill_unit", &BTreeMap::new()).unwrap_err();
    assert!(error.to_string().contains("missing required arg unit_id"));
}

#[test]
fn action_validation_fails_closed_for_unknown_arg() {
    let error = validate_action_args(
        "kill_unit",
        &args([("unit_id", "unit_guard_01"), ("surprise", "nope")]),
    )
    .unwrap_err();
    assert!(error.to_string().contains("unknown arg surprise"));
}

#[test]
fn action_execution_fails_closed_for_missing_state() {
    let error = execute_action(
        "kill_unit",
        &args([("unit_id", "unit_missing_99")]),
        &mut synthetic_context(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("unknown unit unit_missing_99"));
}

fn args<const N: usize>(pairs: [(&str, &str); N]) -> BTreeMap<String, String> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn synthetic_context() -> TriggerActionContext {
    let mut units = BTreeMap::new();
    units.insert(
        "unit_guard_01".to_string(),
        TriggerActionUnitState {
            alive: true,
            unit_type_id: "guard".to_string(),
            owner_player_id: "player_1".to_string(),
            region_id: "region_gate_01".to_string(),
            current_order: None,
        },
    );
    TriggerActionContext {
        units,
        variables: BTreeMap::new(),
        waits_seconds: Vec::new(),
        loop_records: Vec::new(),
        branch_records: Vec::new(),
    }
}
