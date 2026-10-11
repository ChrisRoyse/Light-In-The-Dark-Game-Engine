use std::collections::BTreeMap;

use super::*;
use crate::mcp_trigger_tools::mcp_trigger_tools_store::McpTriggerRuntimeState;
use crate::mcp_trigger_tools::{
    EcaGraph, EcaNode, TriggerValueType, TriggerVariable, TriggerVariableScope,
};

fn variable(name: &str) -> TriggerVariable {
    TriggerVariable {
        name: name.to_string(),
        value_type: TriggerValueType::Boolean,
        scope: TriggerVariableScope::Global,
        is_array: false,
        initial_value: Some("false".to_string()),
        array_size: None,
        array_initial_values: Vec::new(),
    }
}

fn node(node_id: &str, op: &str, order: u32, args: &[(&str, &str)], refs: &[&str]) -> EcaNode {
    EcaNode {
        node_id: node_id.to_string(),
        operation: op.to_string(),
        order,
        args: args
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>(),
        variable_refs: refs.iter().map(|r| r.to_string()).collect(),
    }
}

fn graph(trigger_id: &str, region: &str, var: &str) -> EcaGraph {
    EcaGraph {
        trigger_id: trigger_id.to_string(),
        display_name: trigger_id.to_string(),
        enabled: true,
        folder_path: Some("Combat".to_string()),
        initially_on: true,
        comment: None,
        events: vec![node(
            "evt",
            "unit_enters_region",
            0,
            &[("region_id", region)],
            &[],
        )],
        conditions: vec![node("cond", "game_flag_true", 1, &[("flag", var)], &[var])],
        actions: vec![node(
            "act",
            "create_unit_at_point",
            2,
            &[("unit_type_id", "guard_type"), ("owner_player_id", "0")],
            &[],
        )],
        wasm_escape: None,
    }
}

fn state(graphs: Vec<EcaGraph>) -> McpTriggerRuntimeState {
    let mut state = McpTriggerRuntimeState::new("cross_ref_test").unwrap();
    state.variables.push(variable("gate_open"));
    state.graphs = graphs;
    state
}

#[test]
fn objects_referenced_by_trigger_are_computed_from_state() {
    let state = state(vec![graph("trg_a", "region_north", "gate_open")]);
    let report = answer_cross_ref(
        &state,
        &CrossRefQuery::ObjectsReferencedByTrigger {
            trigger_id: "trg_a".to_string(),
        },
    )
    .unwrap();
    assert!(report.objects.contains(&TriggerObjectRef {
        object_kind: CrossRefObjectKind::Region,
        object_id: "region_north".to_string(),
    }));
    assert!(report.objects.contains(&TriggerObjectRef {
        object_kind: CrossRefObjectKind::UnitType,
        object_id: "guard_type".to_string(),
    }));
    assert!(report.objects.contains(&TriggerObjectRef {
        object_kind: CrossRefObjectKind::Variable,
        object_id: "gate_open".to_string(),
    }));
    assert!(report.objects.contains(&TriggerObjectRef {
        object_kind: CrossRefObjectKind::Player,
        object_id: "0".to_string(),
    }));
}

#[test]
fn triggers_referencing_object_returns_exact_set() {
    let state = state(vec![
        graph("trg_a", "region_north", "gate_open"),
        graph("trg_b", "region_south", "gate_open"),
    ]);
    let report = answer_cross_ref(
        &state,
        &CrossRefQuery::TriggersReferencingObject {
            object_kind: CrossRefObjectKind::Variable,
            object_id: "gate_open".to_string(),
        },
    )
    .unwrap();
    assert_eq!(report.trigger_ids, vec!["trg_a", "trg_b"]);

    let north = answer_cross_ref(
        &state,
        &CrossRefQuery::TriggersReferencingObject {
            object_kind: CrossRefObjectKind::Region,
            object_id: "region_north".to_string(),
        },
    )
    .unwrap();
    assert_eq!(north.trigger_ids, vec!["trg_a"]);
}

#[test]
fn editing_a_trigger_updates_the_cross_reference() {
    let mut state = state(vec![graph("trg_a", "region_north", "gate_open")]);
    let before = answer_cross_ref(
        &state,
        &CrossRefQuery::TriggersReferencingObject {
            object_kind: CrossRefObjectKind::Region,
            object_id: "region_south".to_string(),
        },
    )
    .unwrap();
    assert!(before.trigger_ids.is_empty());

    // Edit the trigger to point at region_south instead.
    state.graphs[0].events[0]
        .args
        .insert("region_id".to_string(), "region_south".to_string());
    let after = answer_cross_ref(
        &state,
        &CrossRefQuery::TriggersReferencingObject {
            object_kind: CrossRefObjectKind::Region,
            object_id: "region_south".to_string(),
        },
    )
    .unwrap();
    assert_eq!(after.trigger_ids, vec!["trg_a"]);
}

#[test]
fn unknown_trigger_id_is_a_typed_error() {
    let state = state(vec![graph("trg_a", "region_north", "gate_open")]);
    let error = answer_cross_ref(
        &state,
        &CrossRefQuery::ObjectsReferencedByTrigger {
            trigger_id: "missing".to_string(),
        },
    )
    .unwrap_err();
    match error {
        EngineError::UiRenderInvalidConfig { field, .. } => {
            assert_eq!(field, "editor.trigger_cross_ref.trigger_id");
        }
        other => panic!("expected typed error, got {other:?}"),
    }
}
