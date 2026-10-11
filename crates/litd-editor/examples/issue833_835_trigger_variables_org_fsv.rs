//! FSV harness for #833 (typed trigger variables + arrays) and #835 (trigger
//! organization flags + Object Manager cross-reference). Exercises the REAL MCP
//! trigger tools over Calyx Aster: authors typed variables + graphs, sets org
//! flags, reads the source of truth back independently (fresh handle + decoded
//! WAL frame), then three edge cases (cross-ref edit, type-mismatch rejection,
//! lowering round-trip) plus negative controls (sabotaged type/array shape).

use std::{fs, path::Path, path::PathBuf};

use litd_editor::mcp_trigger_tools::{
    CrossRefObjectKind, CrossRefQuery, EcaGraph, EcaNode, McpTriggerRuntimeState,
    QUERY_CROSS_REF_TOOL, SET_TRIGGER_ORGANIZATION_TOOL, TriggerValueType, TriggerVariable,
    TriggerVariableScope, UPSERT_ECA_GRAPH_TOOL, UPSERT_VARIABLE_TOOL, call_mcp_trigger_tool,
    read_mcp_trigger_state,
};
use litd_editor::trigger_organization_lowering::{
    encode_lowered_trigger_document, lower_trigger_state, verify_lowered_trigger_document,
};
use litd_engine::calyx_json_store::read_calyx_json_wal_write;
use serde_json::{Value, json};

const STATE_ID: &str = "issue833-835-state";
const LEDGER_ID: &str = "issue833835ledger";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    let root = PathBuf::from("Calyx/target/fsv/issue833_835");
    reset_calyx_fsv_root(&root)?;
    let state_path = root.join("trigger-state-aster");
    let ledger_path = root.join("trigger-ledger-aster");

    // ---- 1. Author typed variables (incl. a fixed-size array) --------------
    author_variable(
        &state_path,
        &ledger_path,
        scalar("gate_open", TriggerValueType::Boolean, "false"),
    )?;
    author_variable(
        &state_path,
        &ledger_path,
        int_array("wave_counts", 3, &["1", "2", "3"]),
    )?;
    author_variable(
        &state_path,
        &ledger_path,
        scalar("spawn_point", TriggerValueType::Point, "12,0,8"),
    )?;
    author_variable(
        &state_path,
        &ledger_path,
        scalar("host_player", TriggerValueType::Player, "0"),
    )?;

    // ---- Author two ECA graphs referencing shared objects ------------------
    upsert_graph(
        &state_path,
        &ledger_path,
        graph("trg_alpha", "region_north", "gate_open", "guard_type"),
    )?;
    upsert_graph(
        &state_path,
        &ledger_path,
        graph("trg_beta", "region_north", "gate_open", "scout_type"),
    )?;

    // ---- Set organization flags on one trigger (#835) ----------------------
    let org = call_mcp_trigger_tool(
        SET_TRIGGER_ORGANIZATION_TOOL,
        json!({
            "state_path": state_path, "ledger_path": ledger_path,
            "state_id": STATE_ID, "ledger_id": LEDGER_ID,
            "request_id": "org-alpha", "requested_by": "issue835-fsv",
            "trigger_id": "trg_alpha",
            "enabled": false, "initially_on": false,
            "folder_path": "Combat/Reinforcements", "comment": "reinforcement wave gate",
        }),
    )?;
    let org_state = org.trigger_readback.ok_or("org set omitted readback")?;
    let alpha = org_state
        .state
        .graphs
        .iter()
        .find(|g| g.trigger_id == "trg_alpha")
        .ok_or("trg_alpha missing after org set")?;
    let org_flags_ok = !alpha.enabled
        && !alpha.initially_on
        && alpha.folder_path.as_deref() == Some("Combat/Reinforcements")
        && alpha.comment.as_deref() == Some("reinforcement wave gate");

    // ---- 2. Independent readback of the source of truth --------------------
    let logical = read_mcp_trigger_state(&state_path)?;
    let wal = read_calyx_json_wal_write(&state_path, logical.seq)?;
    let physical: McpTriggerRuntimeState = serde_json::from_slice(&wal.bytes)?;
    let independent_ok = physical == logical.state && wal.sha256 == logical.sha256;
    let variable_count = logical.variable_count;
    let array_var = logical
        .state
        .variables
        .iter()
        .find(|v| v.name == "wave_counts")
        .ok_or("wave_counts missing")?;
    let array_ok = array_var.is_array
        && array_var.array_size == Some(3)
        && array_var.array_initial_values == vec!["1", "2", "3"];

    // ---- 3A. Cross-reference updates when a trigger is edited --------------
    let before = cross_ref_triggers(&state_path, CrossRefObjectKind::Region, "region_south")?;
    // Edit trg_beta to fire on region_south instead of region_north.
    upsert_graph(
        &state_path,
        &ledger_path,
        graph("trg_beta", "region_south", "gate_open", "scout_type"),
    )?;
    let after = cross_ref_triggers(&state_path, CrossRefObjectKind::Region, "region_south")?;
    let cross_ref_edit_ok = before.is_empty() && after == vec!["trg_beta".to_string()];
    println!(
        "ISSUE835_CROSS_REF_EDIT before={before:?} after={after:?} passed={cross_ref_edit_ok}"
    );

    // gate_open is shared by both triggers; objects-by-trigger for trg_alpha.
    let gate_users = cross_ref_triggers(&state_path, CrossRefObjectKind::Variable, "gate_open")?;
    let alpha_objects = cross_ref_objects(&state_path, "trg_alpha")?;
    let cross_ref_shape_ok = gate_users == vec!["trg_alpha".to_string(), "trg_beta".to_string()]
        && alpha_objects.iter().any(|o| o == "Region:region_north")
        && alpha_objects.iter().any(|o| o == "UnitType:guard_type")
        && alpha_objects.iter().any(|o| o == "Variable:gate_open");

    // ---- 3B. Type-mismatch assignment rejected with a typed error ----------
    let mismatch = call_mcp_trigger_tool(
        UPSERT_ECA_GRAPH_TOOL,
        json!({
            "state_path": state_path, "ledger_path": ledger_path,
            "state_id": STATE_ID, "ledger_id": LEDGER_ID,
            "request_id": "bad-assign", "requested_by": "issue833-fsv",
            "graph": set_variable_graph("trg_bad", "gate_open", "not_a_boolean"),
        }),
    );
    let assign_field = error_field(&mismatch);
    let assign_rejected =
        assign_field.as_deref() == Some("editor.mcp_trigger_tools.assignment.value");
    println!(
        "ISSUE833_TYPE_MISMATCH before=<gate_open:Boolean> assign=\"not_a_boolean\" rejected={assign_rejected} field={assign_field:?}"
    );

    // Bad array shape (size 3 but 2 initial values) is rejected too.
    let bad_array = call_mcp_trigger_tool(
        UPSERT_VARIABLE_TOOL,
        json!({
            "state_path": state_path, "ledger_path": ledger_path,
            "state_id": STATE_ID, "ledger_id": LEDGER_ID,
            "request_id": "bad-array", "requested_by": "issue833-fsv",
            "variable": int_array("broken", 3, &["1", "2"]),
        }),
    );
    let array_reject_field = error_field(&bad_array);
    let array_rejected = array_reject_field
        .as_deref()
        .map(|f| f.contains("array_initial_values"))
        .unwrap_or(false);

    // ---- 3C. Lowering round-trip of org flags + array ----------------------
    let latest = read_mcp_trigger_state(&state_path)?;
    let lowered = lower_trigger_state(&latest.state)?;
    let lowered_bytes = encode_lowered_trigger_document(&lowered)?;
    let lower_path = PathBuf::from("target/fsv/issue833_835/lowered-triggers.json");
    if let Some(parent) = lower_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&lower_path, &lowered_bytes)?;
    let reread = fs::read(&lower_path)?;
    let verified = verify_lowered_trigger_document(&reread, &lowered.lowering_fingerprint)?;
    let lowered_alpha = verified
        .triggers
        .iter()
        .find(|t| t.trigger_id == "trg_alpha")
        .ok_or("lowered trg_alpha missing")?;
    let lowering_ok = !lowered_alpha.enabled
        && !lowered_alpha.initially_on
        && lowered_alpha.folder_path.as_deref() == Some("Combat/Reinforcements")
        && verified
            .variables
            .iter()
            .any(|v| v.name == "wave_counts" && v.array_size == Some(3));

    // ---- 4. Negative control: sabotage a variable's type -------------------
    let text = String::from_utf8(reread.clone())?;
    let sabotaged = text.replacen("\"Boolean\"", "\"Integer\"", 1);
    let sabotage_changed = sabotaged != text;
    let sabotage_result =
        verify_lowered_trigger_document(sabotaged.as_bytes(), &lowered.lowering_fingerprint);
    let sabotage_field = sabotage_result.as_ref().err().map(engine_error_field);
    let sabotage_rejected = sabotage_result.is_err() && sabotage_changed;
    println!(
        "ISSUE833_SABOTAGE before=<gate_open:Boolean valid> after=<gate_open:Integer init=false> rejected={sabotage_rejected} field={sabotage_field:?}"
    );

    let accepted = org_flags_ok
        && independent_ok
        && array_ok
        && cross_ref_edit_ok
        && cross_ref_shape_ok
        && assign_rejected
        && array_rejected
        && lowering_ok
        && sabotage_rejected;

    let report = json!({
        "schema": "litd.issue833_835.trigger-vars-org-fsv.v1",
        "issues": [833, 835],
        "source_of_truth": "real MCP trigger tools over Calyx Aster + independently decoded WAL frame + fingerprinted lowering artifact",
        "persistence": {
            "state_path": state_path,
            "state_sha256": logical.sha256,
            "state_seq": logical.seq,
            "variable_count": variable_count,
            "physical_wal_payload_sha256": wal.sha256,
            "independent_readback_matches": independent_ok,
        },
        "typed_variables": {
            "array_shape_ok": array_ok,
            "array_size": array_var.array_size,
            "array_initial_values": array_var.array_initial_values,
        },
        "organization": {
            "org_flags_round_trip": org_flags_ok,
            "enabled": alpha.enabled,
            "initially_on": alpha.initially_on,
            "folder_path": alpha.folder_path,
            "comment": alpha.comment,
        },
        "cross_ref": {
            "edit_updates_edges": cross_ref_edit_ok,
            "region_south_before": before,
            "region_south_after": after,
            "gate_open_referencing_triggers": gate_users,
            "trg_alpha_referenced_objects": alpha_objects,
            "shape_ok": cross_ref_shape_ok,
        },
        "lowering": {
            "fingerprint": lowered.lowering_fingerprint,
            "org_and_array_round_trip": lowering_ok,
            "artifact_path": lower_path,
            "artifact_sha256": litd_hash::sha256_hex(&reread),
        },
        "negative_controls": {
            "type_mismatch_assignment_rejected": assign_rejected,
            "type_mismatch_field": assign_field,
            "bad_array_shape_rejected": array_rejected,
            "sabotaged_lowered_type_rejected": sabotage_rejected,
            "sabotage_field": sabotage_field,
        },
        "acceptance": { "passed": accepted },
    });
    if !accepted {
        return Err(format!("issue833/835 acceptance failed: {}", report["acceptance"]).into());
    }
    let report_path = PathBuf::from("target/fsv/issue833_835/report.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    let report_bytes = fs::read(&report_path)?;
    let reread_report: Value = serde_json::from_slice(&report_bytes)?;
    if reread_report["acceptance"]["passed"] != true {
        return Err("persisted report did not read back as passed".into());
    }
    println!(
        "ISSUE833_835_FSV_OK report={} report_sha256={} variables={} state_sha256={}",
        report_path.display(),
        litd_hash::sha256_hex(&report_bytes),
        variable_count,
        logical.sha256,
    );
    Ok(())
}

fn reset_calyx_fsv_root(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let expected = fs::canonicalize("Calyx")?
        .join("target")
        .join("fsv")
        .join("issue833_835");
    if root.exists() {
        let resolved = fs::canonicalize(root)?;
        if resolved != expected {
            return Err(format!("refusing to reset non-owned path {}", resolved.display()).into());
        }
        fs::remove_dir_all(&resolved)?;
    }
    fs::create_dir_all(&expected)?;
    Ok(())
}

fn author_variable(
    state_path: &Path,
    ledger_path: &Path,
    variable: TriggerVariable,
) -> Result<(), Box<dyn std::error::Error>> {
    call_mcp_trigger_tool(
        UPSERT_VARIABLE_TOOL,
        json!({
            "state_path": state_path, "ledger_path": ledger_path,
            "state_id": STATE_ID, "ledger_id": LEDGER_ID,
            "request_id": format!("var-{}", variable.name),
            "requested_by": "issue833-fsv",
            "variable": variable,
        }),
    )?;
    Ok(())
}

fn upsert_graph(
    state_path: &Path,
    ledger_path: &Path,
    graph: EcaGraph,
) -> Result<(), Box<dyn std::error::Error>> {
    call_mcp_trigger_tool(
        UPSERT_ECA_GRAPH_TOOL,
        json!({
            "state_path": state_path, "ledger_path": ledger_path,
            "state_id": STATE_ID, "ledger_id": LEDGER_ID,
            "request_id": format!("graph-{}", graph.trigger_id),
            "requested_by": "issue835-fsv",
            "graph": graph,
        }),
    )?;
    Ok(())
}

fn cross_ref_triggers(
    state_path: &Path,
    object_kind: CrossRefObjectKind,
    object_id: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let query = CrossRefQuery::TriggersReferencingObject {
        object_kind,
        object_id: object_id.to_string(),
    };
    let out = call_mcp_trigger_tool(
        QUERY_CROSS_REF_TOOL,
        json!({ "state_path": state_path, "query": query }),
    )?;
    let ids = out.result["cross_ref"]["trigger_ids"]
        .as_array()
        .ok_or("cross_ref trigger_ids missing")?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    Ok(ids)
}

fn cross_ref_objects(
    state_path: &Path,
    trigger_id: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let query = CrossRefQuery::ObjectsReferencedByTrigger {
        trigger_id: trigger_id.to_string(),
    };
    let out = call_mcp_trigger_tool(
        QUERY_CROSS_REF_TOOL,
        json!({ "state_path": state_path, "query": query }),
    )?;
    let objects = out.result["cross_ref"]["objects"]
        .as_array()
        .ok_or("cross_ref objects missing")?
        .iter()
        .map(|v| {
            format!(
                "{}:{}",
                object_kind_label(v["object_kind"].as_str().unwrap_or("?")),
                v["object_id"].as_str().unwrap_or("?")
            )
        })
        .collect();
    Ok(objects)
}

fn object_kind_label(raw: &str) -> &'static str {
    match raw {
        "variable" => "Variable",
        "unit" => "Unit",
        "unit_type" => "UnitType",
        "region" => "Region",
        "player" => "Player",
        "timer" => "Timer",
        "unit_group" => "UnitGroup",
        _ => "Unknown",
    }
}

fn error_field(result: &Result<impl Sized, litd_engine::EngineError>) -> Option<String> {
    result.as_ref().err().map(engine_error_field)
}

fn engine_error_field(error: &litd_engine::EngineError) -> String {
    match error {
        litd_engine::EngineError::UiRenderInvalidConfig { field, .. } => field.clone(),
        litd_engine::EngineError::SerializedFingerprintMismatch { .. } => {
            "fingerprint_mismatch".to_string()
        }
        other => format!("{other:?}"),
    }
}

fn scalar(name: &str, value_type: TriggerValueType, value: &str) -> TriggerVariable {
    TriggerVariable {
        name: name.to_string(),
        value_type,
        scope: TriggerVariableScope::Global,
        is_array: false,
        initial_value: Some(value.to_string()),
        array_size: None,
        array_initial_values: Vec::new(),
    }
}

fn int_array(name: &str, size: u32, values: &[&str]) -> TriggerVariable {
    TriggerVariable {
        name: name.to_string(),
        value_type: TriggerValueType::Integer,
        scope: TriggerVariableScope::Global,
        is_array: true,
        initial_value: None,
        array_size: Some(size),
        array_initial_values: values.iter().map(|v| v.to_string()).collect(),
    }
}

fn eca_node(node_id: &str, op: &str, order: u32, args: &[(&str, &str)], refs: &[&str]) -> EcaNode {
    EcaNode {
        node_id: node_id.to_string(),
        operation: op.to_string(),
        order,
        args: args
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        variable_refs: refs.iter().map(|r| r.to_string()).collect(),
    }
}

fn graph(trigger_id: &str, region: &str, var: &str, unit_type: &str) -> EcaGraph {
    EcaGraph {
        trigger_id: trigger_id.to_string(),
        display_name: trigger_id.to_string(),
        enabled: true,
        folder_path: None,
        initially_on: true,
        comment: None,
        events: vec![eca_node(
            "evt",
            "unit_enters_region",
            0,
            &[("region_id", region)],
            &[],
        )],
        conditions: vec![eca_node(
            "cond",
            "game_flag_true",
            1,
            &[("flag", var)],
            &[var],
        )],
        actions: vec![eca_node(
            "act",
            "create_unit_at_point",
            2,
            &[("unit_type_id", unit_type), ("owner_player_id", "0")],
            &[],
        )],
        wasm_escape: None,
    }
}

fn set_variable_graph(trigger_id: &str, var: &str, value: &str) -> EcaGraph {
    EcaGraph {
        trigger_id: trigger_id.to_string(),
        display_name: trigger_id.to_string(),
        enabled: true,
        folder_path: None,
        initially_on: true,
        comment: None,
        events: vec![eca_node("evt", "map_init", 0, &[], &[])],
        conditions: Vec::new(),
        actions: vec![eca_node(
            "act",
            "set_variable",
            1,
            &[("variable", var), ("value", value)],
            &[var],
        )],
        wasm_escape: None,
    }
}
