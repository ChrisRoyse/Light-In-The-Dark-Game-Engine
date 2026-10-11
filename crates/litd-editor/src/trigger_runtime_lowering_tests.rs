//! #1851: unit coverage for the runtime ECA trigger lowering valve. Proves the
//! first-cut constructs lower to the typed engine nodes and that every
//! out-of-scope authored construct is rejected FAIL-LOUD (never silently dropped
//! the way the pre-#1851 organization lowering dropped all ECA nodes).

use std::collections::BTreeMap;

use litd_engine::game_runtime::{TriggerAction, TriggerCondition, TriggerEvent, TriggerValue};

use super::lower_trigger_graphs_for_runtime;
use crate::mcp_trigger_tools::{
    EcaGraph, EcaNode, McpTriggerRuntimeState, TriggerValueType, TriggerVariable,
    TriggerVariableScope,
};

const TICK_RATE: u32 = 4;

fn state(variables: Vec<TriggerVariable>, graphs: Vec<EcaGraph>) -> McpTriggerRuntimeState {
    McpTriggerRuntimeState {
        schema: "litd.editor.trigger_state.test".to_string(),
        issue: 1851,
        state_id: "test".to_string(),
        source_of_truth: "test".to_string(),
        update_count: 0,
        variables,
        graphs,
        runtime_cues: Vec::new(),
    }
}

fn scalar(name: &str, value_type: TriggerValueType, initial: &str) -> TriggerVariable {
    TriggerVariable {
        name: name.to_string(),
        value_type,
        scope: TriggerVariableScope::Global,
        is_array: false,
        initial_value: Some(initial.to_string()),
        array_size: None,
        array_initial_values: Vec::new(),
    }
}

fn node(id: &str, operation: &str, order: u32, args: &[(&str, &str)]) -> EcaNode {
    EcaNode {
        node_id: id.to_string(),
        operation: operation.to_string(),
        order,
        args: args
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        variable_refs: Vec::new(),
    }
}

fn graph(
    id: &str,
    events: Vec<EcaNode>,
    conditions: Vec<EcaNode>,
    actions: Vec<EcaNode>,
) -> EcaGraph {
    EcaGraph {
        trigger_id: id.to_string(),
        display_name: format!("{id} display"),
        enabled: true,
        folder_path: None,
        initially_on: true,
        comment: None,
        events,
        conditions,
        actions,
        wasm_escape: None,
    }
}

fn handles() -> BTreeMap<String, u32> {
    [("hero".to_string(), 10_u32), ("boss".to_string(), 11)]
        .into_iter()
        .collect()
}

fn code(error: litd_engine::EngineError) -> String {
    error.code().to_string()
}

#[test]
fn issue1851_first_cut_graph_lowers_to_typed_nodes() {
    let authored = state(
        vec![
            scalar("mission_started", TriggerValueType::Boolean, "false"),
            scalar("kills", TriggerValueType::Integer, "2"),
        ],
        vec![
            graph(
                "trg_init",
                vec![node("e1", "map_initialization", 0, &[])],
                Vec::new(),
                vec![node(
                    "a1",
                    "set_boolean_variable",
                    0,
                    &[("variable_name", "mission_started"), ("value", "true")],
                )],
            ),
            graph(
                "trg_timer",
                vec![
                    node("e2", "countdown_timer_expires", 0, &[("timer_id", "0")]),
                    node("e3", "elapsed_game_time", 1, &[("elapsed_seconds", "30")]),
                ],
                vec![
                    node("c1", "unit_alive", 0, &[("unit_id", "boss")]),
                    node(
                        "c2",
                        "integer_greater_equal",
                        1,
                        &[("left", "kills"), ("right", "2")],
                    ),
                    node(
                        "c3",
                        "boolean_equal",
                        2,
                        &[("left", "mission_started"), ("right", "true")],
                    ),
                ],
                vec![
                    node("a2", "kill_unit", 0, &[("unit_id", "boss")]),
                    node(
                        "a3",
                        "move_unit",
                        1,
                        &[("unit_id", "hero"), ("cell_x", "9"), ("cell_z", "7")],
                    ),
                    node("a4", "fire_trigger", 2, &[("trigger", "7")]),
                ],
            ),
        ],
    );
    let lowered = lower_trigger_graphs_for_runtime(
        &authored,
        TICK_RATE,
        &handles(),
        &std::collections::BTreeMap::new(),
    )
    .expect("first-cut graph lowers");
    lowered.validate().expect("lowered graph valid");
    assert_eq!(lowered.variables.len(), 2);
    assert_eq!(
        lowered.variable("kills").map(|v| &v.value),
        Some(&TriggerValue::Integer(2))
    );
    let timer_trigger = &lowered.triggers[1];
    assert_eq!(
        timer_trigger.events,
        vec![
            TriggerEvent::CountdownTimerExpires { timer: 0 },
            TriggerEvent::ElapsedGameTime {
                elapsed_ticks: 30 * u64::from(TICK_RATE)
            },
        ]
    );
    assert_eq!(
        timer_trigger.conditions[0],
        TriggerCondition::UnitAlive { unit: 11 }
    );
    assert_eq!(
        timer_trigger.actions[0],
        TriggerAction::KillUnit { unit: 11 }
    );
    assert_eq!(
        timer_trigger.actions[1],
        TriggerAction::MoveUnit {
            unit: 10,
            cell_x: 9,
            cell_z: 7
        }
    );
    assert_eq!(
        timer_trigger.actions[2],
        TriggerAction::FireTrigger { trigger: 7 }
    );
}

#[test]
fn issue1851_region_constructs_rejected_naming_1856() {
    let authored = state(
        Vec::new(),
        vec![graph(
            "trg_region",
            vec![node(
                "e1",
                "unit_enters_region",
                0,
                &[("region_id", "gate"), ("unit_id", "hero")],
            )],
            Vec::new(),
            vec![node("a1", "kill_unit", 0, &[("unit_id", "boss")])],
        )],
    );
    let error = lower_trigger_graphs_for_runtime(
        &authored,
        TICK_RATE,
        &handles(),
        &std::collections::BTreeMap::new(),
    )
    .expect_err("region event must be rejected, not dropped");
    let text = error.to_string();
    assert!(
        text.contains("1856"),
        "error must name the tracking issue: {text}"
    );
    assert_eq!(code(error), "LITD-GAME-PACKAGE-INVALID-CONFIG");

    // move_unit with a region target is likewise #1856, not silently retargeted.
    let authored = state(
        Vec::new(),
        vec![graph(
            "trg_move_region",
            vec![node("e1", "map_initialization", 0, &[])],
            Vec::new(),
            vec![node(
                "a1",
                "move_unit",
                0,
                &[("unit_id", "hero"), ("region_id", "gate")],
            )],
        )],
    );
    let error = lower_trigger_graphs_for_runtime(
        &authored,
        TICK_RATE,
        &handles(),
        &std::collections::BTreeMap::new(),
    )
    .expect_err("region move target rejected");
    assert!(error.to_string().contains("1856"));
}

#[test]
fn issue1851_out_of_scope_constructs_rejected_loud() {
    // Out-of-scope event.
    let authored = state(
        Vec::new(),
        vec![graph(
            "trg_chat",
            vec![node("e1", "player_chat_message", 0, &[("player_id", "0")])],
            Vec::new(),
            vec![node("a1", "kill_unit", 0, &[("unit_id", "boss")])],
        )],
    );
    assert_eq!(
        code(
            lower_trigger_graphs_for_runtime(
                &authored,
                TICK_RATE,
                &handles(),
                &std::collections::BTreeMap::new()
            )
            .unwrap_err()
        ),
        "LITD-GAME-PACKAGE-INVALID-CONFIG"
    );
    // Out-of-scope action.
    let authored = state(
        Vec::new(),
        vec![graph(
            "trg_wait",
            vec![node("e1", "map_initialization", 0, &[])],
            Vec::new(),
            vec![node("a1", "wait_seconds", 0, &[("seconds", "5")])],
        )],
    );
    assert_eq!(
        code(
            lower_trigger_graphs_for_runtime(
                &authored,
                TICK_RATE,
                &handles(),
                &std::collections::BTreeMap::new()
            )
            .unwrap_err()
        ),
        "LITD-GAME-PACKAGE-INVALID-CONFIG"
    );
    // Out-of-scope variable type.
    let authored = state(
        vec![scalar("ratio", TriggerValueType::Real, "0.5")],
        Vec::new(),
    );
    assert_eq!(
        code(
            lower_trigger_graphs_for_runtime(
                &authored,
                TICK_RATE,
                &handles(),
                &std::collections::BTreeMap::new()
            )
            .unwrap_err()
        ),
        "LITD-GAME-PACKAGE-INVALID-CONFIG"
    );
    // Fractional elapsed seconds.
    let authored = state(
        Vec::new(),
        vec![graph(
            "trg_frac",
            vec![node(
                "e1",
                "elapsed_game_time",
                0,
                &[("elapsed_seconds", "2.5")],
            )],
            Vec::new(),
            vec![node("a1", "kill_unit", 0, &[("unit_id", "boss")])],
        )],
    );
    assert_eq!(
        code(
            lower_trigger_graphs_for_runtime(
                &authored,
                TICK_RATE,
                &handles(),
                &std::collections::BTreeMap::new()
            )
            .unwrap_err()
        ),
        "LITD-GAME-PACKAGE-INVALID-CONFIG"
    );
}

#[test]
fn issue1851_dangling_references_rejected() {
    // Unknown unit object id.
    let authored = state(
        Vec::new(),
        vec![graph(
            "trg_ghost_unit",
            vec![node("e1", "unit_dies", 0, &[("unit_id", "ghost")])],
            Vec::new(),
            vec![node("a1", "fire_trigger", 0, &[("trigger", "1")])],
        )],
    );
    let error = lower_trigger_graphs_for_runtime(
        &authored,
        TICK_RATE,
        &handles(),
        &std::collections::BTreeMap::new(),
    )
    .expect_err("unknown unit rejected");
    assert!(error.to_string().contains("ghost"));
    // Undeclared variable in a set_variable action.
    let authored = state(
        Vec::new(),
        vec![graph(
            "trg_ghost_var",
            vec![node("e1", "map_initialization", 0, &[])],
            Vec::new(),
            vec![node(
                "a1",
                "set_variable",
                0,
                &[("variable", "ghost_flag"), ("value", "true")],
            )],
        )],
    );
    assert_eq!(
        code(
            lower_trigger_graphs_for_runtime(
                &authored,
                TICK_RATE,
                &handles(),
                &std::collections::BTreeMap::new()
            )
            .unwrap_err()
        ),
        "LITD-GAME-PACKAGE-INVALID-CONFIG"
    );
    // Undeclared variable used as a comparison operand (and not a literal).
    let authored = state(
        Vec::new(),
        vec![graph(
            "trg_ghost_operand",
            vec![node("e1", "map_initialization", 0, &[])],
            vec![node(
                "c1",
                "integer_equal",
                0,
                &[("left", "not_a_number"), ("right", "1")],
            )],
            vec![node("a1", "fire_trigger", 0, &[("trigger", "1")])],
        )],
    );
    assert_eq!(
        code(
            lower_trigger_graphs_for_runtime(
                &authored,
                TICK_RATE,
                &handles(),
                &std::collections::BTreeMap::new()
            )
            .unwrap_err()
        ),
        "LITD-GAME-PACKAGE-INVALID-CONFIG"
    );
}

#[test]
fn issue1851_wasm_escape_rejected() {
    let mut wasm_graph = graph(
        "trg_wasm",
        vec![node("e1", "map_initialization", 0, &[])],
        Vec::new(),
        vec![node("a1", "kill_unit", 0, &[("unit_id", "boss")])],
    );
    wasm_graph.wasm_escape = Some(crate::mcp_trigger_tools::WasmEscape {
        script_id: "s1".to_string(),
        wat_source: "(module)".to_string(),
        wat_source_sha256: None,
        entry_point: "main".to_string(),
        fuel_budget: 1,
        approved_reason: "test".to_string(),
        gui_metadata_json: None,
        artifact: None,
    });
    let authored = state(Vec::new(), vec![wasm_graph]);
    assert_eq!(
        code(
            lower_trigger_graphs_for_runtime(
                &authored,
                TICK_RATE,
                &handles(),
                &std::collections::BTreeMap::new()
            )
            .unwrap_err()
        ),
        "LITD-GAME-PACKAGE-INVALID-CONFIG"
    );
}
