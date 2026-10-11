use litd_engine::{
    math::Fixed,
    wasm_script::{WasmFixedState, WasmScriptArtifact},
};

use crate::{
    mcp_trigger_tools::{EcaGraph, McpTriggerRuntimeState, WasmEscape, validate_mcp_trigger_state},
    trigger_wasm_authoring::{
        WASM_GUI_SCHEMA, WasmGuiOperation, WasmGuiProgram, lower_gui_program,
        materialize_wasm_escape, prepare_wasm_escape, raise_gui_program,
        validate_materialized_wasm_escape,
    },
};

fn gui_program() -> WasmGuiProgram {
    WasmGuiProgram {
        schema: WASM_GUI_SCHEMA.to_string(),
        script_id: "gui_fixed_mutation".to_string(),
        entry_point: "run".to_string(),
        fuel_budget: 10_000,
        approved_reason: "deterministic fixed-point trigger mutation".to_string(),
        operations: vec![
            WasmGuiOperation::SetFixed {
                slot: 0,
                raw: Fixed::from_i32(2).raw(),
            },
            WasmGuiOperation::AddFixed {
                slot: 0,
                raw: Fixed::from_i32(3).raw(),
            },
        ],
    }
}

#[test]
fn gui_program_round_trips_through_wat_and_physical_metadata() {
    let program = gui_program();
    let escape = lower_gui_program(&program).unwrap();
    let artifact = escape.artifact.as_ref().unwrap();

    assert!(escape.wat_source.contains("call $set_fixed"));
    assert!(escape.wat_source.contains("call $add_fixed"));
    assert!(
        artifact
            .custom_section("litd.trigger.wasm-gui.v1")
            .unwrap()
            .is_some()
    );
    assert_eq!(raise_gui_program(&escape).unwrap(), program);
}

#[test]
fn editor_prepared_escape_mutates_real_fixed_state() {
    let escape = lower_gui_program(&gui_program()).unwrap();
    let mut runtime = prepare_wasm_escape(&escape, WasmFixedState::default()).unwrap();
    let stats = runtime.invoke().unwrap();

    assert_eq!(runtime.state().get(0).unwrap(), Fixed::from_i32(5));
    assert!(stats.fuel_consumed > 0);
    assert_eq!(stats.fixed_state_hash, runtime.state().state_hash());
}

#[test]
fn arbitrary_wat_is_valid_text_only_and_fails_gui_raise_loud() {
    let mut escape = WasmEscape {
        script_id: "text_only".to_string(),
        wat_source: r#"(module (func (export "run")))"#.to_string(),
        wat_source_sha256: None,
        entry_point: "run".to_string(),
        fuel_budget: 1_000,
        approved_reason: "advanced text-only deterministic script".to_string(),
        gui_metadata_json: None,
        artifact: None,
    };
    materialize_wasm_escape(&mut escape).unwrap();
    let error = raise_gui_program(&escape).unwrap_err().to_string();
    assert!(error.contains("no reversible GUI metadata"));
}

#[test]
fn authored_source_and_artifact_tamper_fail_closed() {
    let mut escape = lower_gui_program(&gui_program()).unwrap();
    escape.wat_source.push_str("\n;; tampered after lowering");
    let error = validate_materialized_wasm_escape(&escape)
        .unwrap_err()
        .to_string();
    assert!(error.contains("does not match authored source"));

    let mut escape = lower_gui_program(&gui_program()).unwrap();
    escape.artifact.as_mut().unwrap().wasm_sha256 = "0".repeat(64);
    let error = prepare_wasm_escape(&escape, WasmFixedState::default())
        .err()
        .expect("tampered artifact must fail")
        .to_string();
    assert!(error.contains("persisted artifact does not match authored source"));
}

#[test]
fn gui_slot_and_fuel_limits_are_enforced() {
    let mut program = gui_program();
    program.operations = vec![WasmGuiOperation::SetFixed { slot: 64, raw: 0 }];
    assert!(
        lower_gui_program(&program)
            .unwrap_err()
            .to_string()
            .contains("slot 64")
    );

    let mut program = gui_program();
    program.fuel_budget = 0;
    assert!(
        lower_gui_program(&program)
            .unwrap_err()
            .to_string()
            .contains("fuel_budget")
    );
}

#[test]
fn stale_lua_graph_schema_is_rejected_instead_of_ignored() {
    let json = serde_json::json!({
        "trigger_id": "stale_lua",
        "display_name": "Stale Lua",
        "enabled": true,
        "folder_path": null,
        "events": [],
        "conditions": [],
        "actions": [],
        "lua_escape": null
    });
    let error = serde_json::from_value::<EcaGraph>(json)
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown field `lua_escape`"));
}

#[test]
fn supplied_artifact_must_equal_deterministic_lowering() {
    let mut escape = WasmEscape {
        script_id: "artifact_mismatch".to_string(),
        wat_source: r#"(module (func (export "run")))"#.to_string(),
        wat_source_sha256: None,
        entry_point: "run".to_string(),
        fuel_budget: 1_000,
        approved_reason: "artifact integrity edge".to_string(),
        gui_metadata_json: None,
        artifact: Some(
            WasmScriptArtifact::from_wat(
                "artifact_mismatch",
                r#"(module (func (export "run") nop))"#,
            )
            .unwrap(),
        ),
    };
    let error = materialize_wasm_escape(&mut escape)
        .unwrap_err()
        .to_string();
    assert!(error.contains("does not match deterministic lowering output"));
}

#[test]
fn gui_metadata_must_regenerate_the_authored_wat() {
    let mut escape = lower_gui_program(&gui_program()).unwrap();
    let mut metadata = gui_program();
    metadata.operations[1] = WasmGuiOperation::AddFixed {
        slot: 0,
        raw: Fixed::from_i32(4).raw(),
    };
    escape.gui_metadata_json = Some(serde_json::to_string(&metadata).unwrap());

    let error = materialize_wasm_escape(&mut escape)
        .unwrap_err()
        .to_string();
    assert!(error.contains("does not regenerate the authored WAT"));
}

#[test]
fn persisted_trigger_state_revalidates_embedded_wasm_integrity() {
    let mut escape = lower_gui_program(&gui_program()).unwrap();
    escape.wat_source.push_str("\n;; persisted tamper");
    let mut state = McpTriggerRuntimeState::new("wasm_integrity").unwrap();
    state.graphs.push(EcaGraph {
        trigger_id: "tampered_graph".to_string(),
        display_name: "Tampered Graph".to_string(),
        enabled: true,
        folder_path: None,
        initially_on: true,
        comment: None,
        events: Vec::new(),
        conditions: Vec::new(),
        actions: Vec::new(),
        wasm_escape: Some(escape),
    });
    let error = validate_mcp_trigger_state(&state).unwrap_err().to_string();
    assert!(error.contains("source SHA-256 does not match authored source bytes"));
}
