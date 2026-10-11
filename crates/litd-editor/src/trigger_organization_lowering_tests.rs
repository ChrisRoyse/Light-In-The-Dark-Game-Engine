use super::*;
use crate::mcp_trigger_tools::mcp_trigger_tools_store::McpTriggerRuntimeState;
use crate::mcp_trigger_tools::{
    EcaGraph, EcaNode, TriggerValueType, TriggerVariable, TriggerVariableScope,
};
use std::collections::BTreeMap;

fn scalar_bool(name: &str, value: &str) -> TriggerVariable {
    TriggerVariable {
        name: name.to_string(),
        value_type: TriggerValueType::Boolean,
        scope: TriggerVariableScope::Global,
        is_array: false,
        initial_value: Some(value.to_string()),
        array_size: None,
        array_initial_values: Vec::new(),
    }
}

fn int_array(name: &str, size: u32, values: Vec<&str>) -> TriggerVariable {
    TriggerVariable {
        name: name.to_string(),
        value_type: TriggerValueType::Integer,
        scope: TriggerVariableScope::Global,
        is_array: true,
        initial_value: None,
        array_size: Some(size),
        array_initial_values: values.into_iter().map(|v| v.to_string()).collect(),
    }
}

fn simple_graph() -> EcaGraph {
    EcaGraph {
        trigger_id: "trg_org".to_string(),
        display_name: "Org Trigger".to_string(),
        enabled: false,
        folder_path: Some("Cinematics/Intro".to_string()),
        initially_on: false,
        comment: Some("author note".to_string()),
        events: vec![EcaNode {
            node_id: "evt".to_string(),
            operation: "map_init".to_string(),
            order: 0,
            args: BTreeMap::new(),
            variable_refs: Vec::new(),
        }],
        conditions: Vec::new(),
        actions: vec![EcaNode {
            node_id: "act".to_string(),
            operation: "noop".to_string(),
            order: 1,
            args: BTreeMap::new(),
            variable_refs: Vec::new(),
        }],
        wasm_escape: None,
    }
}

fn state() -> McpTriggerRuntimeState {
    let mut state = McpTriggerRuntimeState::new("lowering_test").unwrap();
    state.variables.push(scalar_bool("gate_open", "true"));
    state
        .variables
        .push(int_array("scores", 3, vec!["1", "2", "3"]));
    state.graphs.push(simple_graph());
    state
}

#[test]
fn org_flags_and_variables_round_trip_through_lowering() {
    let state = state();
    let document = lower_trigger_state(&state).unwrap();
    let bytes = encode_lowered_trigger_document(&document).unwrap();
    let verified = verify_lowered_trigger_document(&bytes, &document.lowering_fingerprint).unwrap();
    assert_eq!(verified, document);

    let org = &verified.triggers[0];
    assert!(!org.enabled);
    assert!(!org.initially_on);
    assert_eq!(org.folder_path.as_deref(), Some("Cinematics/Intro"));
    assert_eq!(org.comment.as_deref(), Some("author note"));

    let array = verified
        .variables
        .iter()
        .find(|v| v.name == "scores")
        .unwrap();
    assert!(array.is_array);
    assert_eq!(array.array_size, Some(3));
    assert_eq!(array.initial_values, vec!["1", "2", "3"]);
}

#[test]
fn tampered_fingerprint_is_rejected() {
    let state = state();
    let document = lower_trigger_state(&state).unwrap();
    let bytes = encode_lowered_trigger_document(&document).unwrap();
    let error = verify_lowered_trigger_document(&bytes, "deadbeef").unwrap_err();
    assert!(matches!(
        error,
        EngineError::SerializedFingerprintMismatch { .. }
    ));
}

#[test]
fn sabotaged_variable_type_is_rejected() {
    let state = state();
    let document = lower_trigger_state(&state).unwrap();
    let bytes = encode_lowered_trigger_document(&document).unwrap();
    // Flip the boolean variable's declared type to Integer while its initial
    // value stays "true" -- the type check must reject it.
    let text = String::from_utf8(bytes).unwrap();
    let sabotaged = text.replacen("\"Boolean\"", "\"Integer\"", 1);
    let error =
        verify_lowered_trigger_document(sabotaged.as_bytes(), &document.lowering_fingerprint)
            .unwrap_err();
    match error {
        EngineError::UiRenderInvalidConfig { field, .. } => {
            assert!(field.contains("initial_value"));
        }
        other => panic!("expected typed validation error, got {other:?}"),
    }
}
