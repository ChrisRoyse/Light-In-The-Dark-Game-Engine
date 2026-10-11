use std::{fs, path::PathBuf};

use litd_editor::{
    mcp_trigger_tools::{
        EcaGraph, EcaNode, McpTriggerRuntimeState, UPSERT_ECA_GRAPH_TOOL, call_mcp_trigger_tool,
        read_mcp_trigger_state,
    },
    trigger_wasm_authoring::{
        WASM_GUI_SCHEMA, WasmGuiOperation, WasmGuiProgram, lower_gui_program, prepare_wasm_escape,
        raise_gui_program,
    },
};
use litd_engine::{
    calyx_json_store::read_calyx_json_wal_write,
    frame_alloc::{CountingAllocator, allocation_count},
    math::Fixed,
    wasm_script::{
        WASMTIME_PINNED_VERSION, WasmFixedState, WasmScriptArtifact, WasmScriptBinding,
        WasmScriptRuntime,
    },
};
use serde_json::{Value, json};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

const ROOT: &str = "target/fsv/issue834";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [] => {}
        [mode] if mode == "--read-only" => return read_only_persisted_state(),
        _ => return Err(format!("expected no arguments or --read-only, got {args:?}").into()),
    }
    let root = PathBuf::from(ROOT);
    fs::create_dir_all(&root)?;
    let artifact_path = root.join("fixed-mutation.wasm");
    let report_path = root.join("wasm-escape-fsv.json");

    let program = fixed_mutation_program();
    let escape = lower_gui_program(&program)?;
    let raised = raise_gui_program(&escape)?;
    let artifact = escape.artifact.as_ref().ok_or("missing lowered artifact")?;
    fs::write(&artifact_path, &artifact.wasm_bytes)?;
    let physical_wasm = fs::read(&artifact_path)?;
    let physical_wasm_sha256 = litd_hash::sha256_hex(&physical_wasm);

    let mut first = prepare_wasm_escape(&escape, WasmFixedState::default())?;
    let allocations_before = allocation_count();
    let first_stats = first.invoke()?;
    let allocations_after = allocation_count();
    let fixed_api_allocations = allocations_after.saturating_sub(allocations_before);
    let fixed_memory_allocations = measure_fixed_memory_invocation_allocations()?;
    let invocation_allocations = fixed_api_allocations.saturating_add(fixed_memory_allocations);

    let mut second = prepare_wasm_escape(&escape, WasmFixedState::default())?;
    let second_stats = second.invoke()?;
    let deterministic = first_stats == second_stats && first.state() == second.state();
    let fixed_value = first.state().get(0)?.raw();
    let gui_roundtrip = raised == program;
    let mcp_persistence = persist_wasm_content(&escape)?;

    let float_edge = float_rejection();
    let import_edge = denied_import_rejection();
    let fuel_edge = fuel_rejection()?;
    let memory_edge = memory_rejection();
    let source_edge = source_tamper_rejection(&escape);
    let stale_lua_edge = stale_lua_rejection();
    let all_edges = [
        &float_edge,
        &import_edge,
        &fuel_edge,
        &memory_edge,
        &source_edge,
        &stale_lua_edge,
    ]
    .iter()
    .all(|edge| edge["passed"] == true);
    let accepted = deterministic
        && fixed_value == Fixed::from_i32(5).raw()
        && gui_roundtrip
        && invocation_allocations == 0
        && artifact.wasm_sha256 == physical_wasm_sha256
        && mcp_persistence["passed"] == true
        && all_edges;

    let report = json!({
        "schema": "litd.issue834.wasm-escape-fsv.v1",
        "issue": 834,
        "source_of_truth": "physical .wasm artifact + editor GUI/WAT roundtrip + two independent pooled Wasmtime stores + allocation counter + explicit validation/trap errors",
        "runtime": {
            "wasmtime_version": WASMTIME_PINNED_VERSION,
            "abi_version": artifact.abi_version,
            "module_id": artifact.module_id,
            "module_sha256": artifact.wasm_sha256,
            "wat_source_sha256": escape.wat_source_sha256,
            "entry_point": escape.entry_point,
            "fuel_budget": escape.fuel_budget,
            "wasm_path": artifact_path,
            "wasm_bytes": physical_wasm.len(),
            "physical_wasm_sha256": physical_wasm_sha256,
        },
        "gui_text_roundtrip": {
            "passed": gui_roundtrip,
            "operation_count": program.operations.len(),
            "custom_metadata_present": artifact.custom_section("litd.trigger.wasm-gui.v1")?.is_some(),
            "wat_source": escape.wat_source,
        },
        "independent_runs": {
            "run_1": stats_json(first_stats, first.state().get(0)?.raw()),
            "run_2": stats_json(second_stats, second.state().get(0)?.raw()),
            "byte_identical_hashes": deterministic,
            "expected_fixed_raw": Fixed::from_i32(5).raw(),
        },
        "hot_path": {
            "preinstantiated": true,
            "pooling_allocator": true,
            "fixed_api_invocation_allocations": fixed_api_allocations,
            "fixed_memory_invocation_allocations": fixed_memory_allocations,
            "total_measured_invocation_allocations": invocation_allocations,
            "zero_allocation": invocation_allocations == 0,
        },
        "mcp_persistence": mcp_persistence,
        "edges": [float_edge, import_edge, fuel_edge, memory_edge, source_edge, stale_lua_edge],
        "acceptance": {
            "passed": accepted,
            "deterministic_replay_state": deterministic,
            "fixed_point_mutation": fixed_value == Fixed::from_i32(5).raw(),
            "gui_wat_roundtrip": gui_roundtrip,
            "zero_hot_path_allocation": invocation_allocations == 0,
            "physical_artifact_integrity": artifact.wasm_sha256 == physical_wasm_sha256,
            "calyx_content_persistence": mcp_persistence["passed"] == true,
            "all_negative_edges_fail_loud": all_edges,
        },
        "sufficiency_argument": "One two-operation fixed-point program is the minimal happy path covering GUI->WAT->wasm lowering, content hashing, host mutation, fuel accounting, independent-store replay hashes, and hot invocation allocation. Six orthogonal edges cover every #834 rejection class: float/nondeterministic opcode, ambient host import, fuel exhaustion with rollback, oversized memory, authored-source tamper, and stale Lua schema."
    });
    if !accepted {
        return Err(format!("issue834 acceptance failed: {}", report["acceptance"]).into());
    }
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    let report_bytes = fs::read(&report_path)?;
    let readback: Value = serde_json::from_slice(&report_bytes)?;
    if readback["acceptance"]["passed"] != true {
        return Err("persisted issue834 report did not read back as passed".into());
    }
    println!(
        "ISSUE834_WASM_FSV_OK report={} report_sha256={} module_sha256={} replay_hash={:016x} allocations={} edges={}",
        report_path.display(),
        litd_hash::sha256_hex(&report_bytes),
        artifact.wasm_sha256,
        first_stats.replay_state_hash,
        invocation_allocations,
        6
    );
    Ok(())
}

fn read_only_persisted_state() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    let state_path = PathBuf::from("Calyx/target/fsv/issue834/trigger-state-aster");
    let logical = read_mcp_trigger_state(&state_path)?;
    let wal = read_calyx_json_wal_write(&state_path, logical.seq)?;
    let physical: McpTriggerRuntimeState = serde_json::from_slice(&wal.bytes)?;
    let escape = physical
        .graphs
        .iter()
        .find(|graph| graph.trigger_id == "issue834_wasm_trigger")
        .and_then(|graph| graph.wasm_escape.as_ref())
        .ok_or("physical WAL omitted issue834 WASM escape")?;
    let artifact = escape
        .artifact
        .as_ref()
        .ok_or("physical WAL omitted issue834 WASM artifact")?;
    let passed = physical == logical.state && wal.sha256 == logical.sha256;
    let output = json!({
        "source_of_truth": "read-only logical Aster row + independently decoded physical WAL frame",
        "passed": passed,
        "canonical_calyx_root": fs::canonicalize("Calyx")?,
        "state_path": state_path,
        "state_seq": logical.seq,
        "state_bytes": logical.bytes,
        "state_sha256": logical.sha256,
        "physical_wal_payload_sha256": wal.sha256,
        "physical_wal_segment_path": wal.segment_path,
        "physical_wal_segment_sha256": wal.segment_sha256,
        "physical_wal_frame_sha256": wal.frame_sha256,
        "physical_wal_frame_offsets": [wal.start_offset, wal.end_offset],
        "physical_wal_rows": wal.wal_row_count,
        "physical_graph_rows": wal.graph_row_count,
        "physical_time_index_rows": wal.time_index_row_count,
        "module_sha256": artifact.wasm_sha256,
        "wat_source_sha256": escape.wat_source_sha256,
        "wasm_escape_count": logical.wasm_escape_count,
    });
    if !passed {
        return Err("logical Aster row and physical WAL payload diverged".into());
    }
    println!(
        "ISSUE834_WASM_READ_ONLY {}",
        serde_json::to_string(&output)?
    );
    Ok(())
}

fn measure_fixed_memory_invocation_allocations() -> Result<u64, Box<dyn std::error::Error>> {
    let binding = WasmScriptBinding {
        artifact: WasmScriptArtifact::from_wat(
            "issue834_fixed_memory_allocation",
            r#"
            (module
              (memory (export "memory") 1 1)
              (func (export "run")
                i32.const 0
                i32.const 7
                i32.store8))
            "#,
        )?,
        entry_point: "run".to_string(),
        fuel_budget: 10_000,
    };
    let mut runtime = WasmScriptRuntime::prepare(binding, WasmFixedState::default())?;
    let before = allocation_count();
    runtime.invoke()?;
    Ok(allocation_count().saturating_sub(before))
}

fn persist_wasm_content(
    escape: &litd_editor::mcp_trigger_tools::WasmEscape,
) -> Result<Value, Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    let root = PathBuf::from("Calyx/target/fsv/issue834");
    reset_calyx_fsv_root(&root)?;
    let state_path = root.join("trigger-state-aster");
    let ledger_path = root.join("trigger-ledger-aster");
    let graph = EcaGraph {
        trigger_id: "issue834_wasm_trigger".to_string(),
        display_name: "Issue 834 WASM Trigger".to_string(),
        enabled: true,
        folder_path: Some("FSV/WASM".to_string()),
        initially_on: true,
        comment: None,
        events: vec![node("event_start", "map_initialization", 1)],
        conditions: Vec::new(),
        actions: vec![node("action_wasm", "run_wasm", 1)],
        wasm_escape: Some(escape.clone()),
    };
    let output = call_mcp_trigger_tool(
        UPSERT_ECA_GRAPH_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger_path,
            "state_id": "issue834-wasm-state",
            "ledger_id": "issue834-wasm-ledger",
            "request_id": "issue834-wasm-upsert",
            "requested_by": "issue834-fsv",
            "graph": graph,
        }),
    )?;
    let writeback = output
        .trigger_readback
        .ok_or("WASM graph upsert omitted trigger readback")?;
    let readback = read_mcp_trigger_state(&state_path)?;
    let wal = read_calyx_json_wal_write(&state_path, readback.seq)?;
    let wal_state: McpTriggerRuntimeState = serde_json::from_slice(&wal.bytes)?;
    let persisted = readback
        .state
        .graphs
        .iter()
        .find(|candidate| candidate.trigger_id == "issue834_wasm_trigger")
        .and_then(|candidate| candidate.wasm_escape.as_ref())
        .ok_or("persisted WASM graph/escape missing")?;
    let artifact = persisted
        .artifact
        .as_ref()
        .ok_or("persisted WASM artifact missing")?;
    let passed = writeback.sha256 == readback.sha256
        && wal.sha256 == readback.sha256
        && wal_state == readback.state
        && artifact.wasm_sha256
            == escape
                .artifact
                .as_ref()
                .ok_or("authored artifact missing")?
                .wasm_sha256
        && persisted.wat_source_sha256 == escape.wat_source_sha256;
    Ok(json!({
        "passed": passed,
        "canonical_calyx_root": fs::canonicalize("Calyx")?,
        "state_path": state_path,
        "ledger_path": ledger_path,
        "state_sha256": readback.sha256,
        "state_bytes": readback.bytes,
        "state_seq": readback.seq,
        "wasm_escape_count": readback.wasm_escape_count,
        "persisted_module_sha256": artifact.wasm_sha256,
        "persisted_wat_source_sha256": persisted.wat_source_sha256,
        "physical_wal": {
            "payload_sha256": wal.sha256,
            "payload_bytes": wal.byte_len,
            "segment_path": wal.segment_path,
            "frame_offsets": [wal.start_offset, wal.end_offset],
            "rows": wal.wal_row_count,
            "graph_rows": wal.graph_row_count,
            "time_index_rows": wal.time_index_row_count,
        },
    }))
}

fn reset_calyx_fsv_root(root: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let calyx_root = fs::canonicalize("Calyx")?;
    let expected = calyx_root.join("target").join("fsv").join("issue834");
    if root.exists() {
        let resolved = fs::canonicalize(root)?;
        if resolved != expected {
            return Err(format!(
                "refusing to reset non-owned Calyx FSV path: expected {} actual {}",
                expected.display(),
                resolved.display()
            )
            .into());
        }
        fs::remove_dir_all(&resolved)?;
    }
    fs::create_dir_all(&expected)?;
    Ok(())
}

fn node(node_id: &str, operation: &str, order: u32) -> EcaNode {
    EcaNode {
        node_id: node_id.to_string(),
        operation: operation.to_string(),
        order,
        args: Default::default(),
        variable_refs: Vec::new(),
    }
}

fn fixed_mutation_program() -> WasmGuiProgram {
    WasmGuiProgram {
        schema: WASM_GUI_SCHEMA.to_string(),
        script_id: "issue834_fixed_mutation".to_string(),
        entry_point: "run".to_string(),
        fuel_budget: 10_000,
        approved_reason: "issue834 deterministic fixed-point acceptance".to_string(),
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

fn stats_json(stats: litd_engine::wasm_script::WasmInvocationStats, slot_zero: i64) -> Value {
    json!({
        "fuel_budget": stats.fuel_budget,
        "fuel_consumed": stats.fuel_consumed,
        "fixed_state_hash": format!("{:016x}", stats.fixed_state_hash),
        "replay_state_hash": format!("{:016x}", stats.replay_state_hash),
        "slot_zero_raw": slot_zero,
    })
}

fn float_rejection() -> Value {
    edge_from_error(
        "float_opcode_rejected_at_validation",
        WasmScriptArtifact::from_wat(
            "issue834_float_denied",
            r#"(module (func (export "run") f32.const 1.0 drop))"#,
        )
        .unwrap_err()
        .to_string(),
        "strict MVP/no-float validation",
    )
}

fn denied_import_rejection() -> Value {
    edge_from_error(
        "ambient_clock_import_denied",
        WasmScriptArtifact::from_wat(
            "issue834_clock_denied",
            r#"(module (import "wasi_snapshot_preview1" "clock_time_get" (func (param i32 i64 i32) (result i32))) (func (export "run")))"#,
        )
        .unwrap_err()
        .to_string(),
        "wasi_snapshot_preview1.clock_time_get",
    )
}

fn fuel_rejection() -> Result<Value, Box<dyn std::error::Error>> {
    let binding = WasmScriptBinding {
        artifact: WasmScriptArtifact::from_wat(
            "issue834_fuel_denied",
            r#"(module (func (export "run") (loop $forever br $forever)))"#,
        )?,
        entry_point: "run".to_string(),
        fuel_budget: 100,
    };
    let mut initial = WasmFixedState::default();
    initial.set(0, Fixed::from_i32(7))?;
    let mut runtime = WasmScriptRuntime::prepare(binding, initial)?;
    let before = runtime.state().state_hash();
    let error = runtime.invoke().unwrap_err().to_string();
    let after = runtime.state().state_hash();
    Ok(json!({
        "edge": "fuel_exhaustion_traps_and_rolls_back",
        "passed": error.contains("GATE_FAILED: step=script_fuel") && before == after,
        "before_state_hash": format!("{before:016x}"),
        "after_state_hash": format!("{after:016x}"),
        "error": error,
    }))
}

fn memory_rejection() -> Value {
    edge_from_error(
        "oversized_linear_memory_rejected",
        WasmScriptArtifact::from_wat(
            "issue834_memory_denied",
            r#"(module (memory 1 2) (func (export "run")))"#,
        )
        .unwrap_err()
        .to_string(),
        "linear memory exceeds 65536 byte cap",
    )
}

fn source_tamper_rejection(escape: &litd_editor::mcp_trigger_tools::WasmEscape) -> Value {
    let mut tampered = escape.clone();
    tampered.wat_source.push_str("\n;; source tamper");
    let error = prepare_wasm_escape(&tampered, WasmFixedState::default())
        .err()
        .expect("source tamper must fail")
        .to_string();
    edge_from_error(
        "authored_source_sha256_tamper_rejected",
        error,
        "source SHA-256 does not match authored source bytes",
    )
}

fn stale_lua_rejection() -> Value {
    let error = serde_json::from_value::<EcaGraph>(json!({
        "trigger_id": "stale_lua",
        "display_name": "Stale Lua",
        "enabled": true,
        "folder_path": null,
        "events": [], "conditions": [], "actions": [], "lua_escape": null
    }))
    .unwrap_err()
    .to_string();
    edge_from_error(
        "stale_lua_schema_rejected",
        error,
        "unknown field `lua_escape`",
    )
}

fn edge_from_error(edge: &str, error: String, expected: &str) -> Value {
    json!({
        "edge": edge,
        "passed": error.contains(expected),
        "expected_error_fragment": expected,
        "error": error,
    })
}
