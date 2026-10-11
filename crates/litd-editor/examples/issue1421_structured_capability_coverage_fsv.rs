use std::{env, fs, path::Path};

use calyx_mcp::{JsonRpcId, JsonRpcRequest, McpServer, tools::register_all};
use litd_editor::structured_capability_coverage::{
    StructuredCapabilityCoverageAudit, build_structured_capability_coverage_audit,
    parse_structured_capability_coverage_audit, read_structured_capability_coverage_audit,
    write_structured_capability_coverage_audit,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue1421Fsv {
    issue: u32,
    source_of_truth: String,
    research_basis: Vec<String>,
    tools_list: Value,
    audit: StructuredCapabilityCoverageAudit,
    store_readback: Value,
    edge_cases: Vec<Value>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, fsv_root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE1421_FSV_ROOT",
        "issue1421_structured_capability_coverage",
        "manual",
    )?;
    let artifact_relative =
        env::var("LITD_ISSUE1421_FSV").unwrap_or_else(|_| "manual-fsv-report.json".to_string());
    let artifact_path = fsv_run.artifact_path(&artifact_relative)?;
    if let Some(parent) = artifact_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let aster_dir = fsv_root.join("coverage-aster");
    let _ = fs::remove_dir_all(&aster_dir);
    fs::create_dir_all(&aster_dir)?;
    let tools = tools_list_readback()?;
    let live_names = tools["names"]
        .as_array()
        .ok_or("tools/list names missing")?
        .iter()
        .map(|name| {
            name.as_str()
                .ok_or("tool name not text")
                .map(str::to_string)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let audit = build_structured_capability_coverage_audit(
        "issue1421-structured-capability-coverage",
        "MCP tools/list plus Aster persisted coverage audit readback",
        live_names,
    )?;
    let writeback = write_structured_capability_coverage_audit(&aster_dir, &aster_dir, &audit)?;
    let loaded = read_structured_capability_coverage_audit(&aster_dir, &audit.audit_id)?
        .ok_or("coverage audit missing from Aster")?;
    let parsed = parse_structured_capability_coverage_audit(&loaded.bytes)?;
    if parsed != audit || loaded.bytes != writeback.bytes {
        return Err("structured coverage audit Aster readback mismatch".into());
    }
    let edge_cases = vec![
        edge_missing_tool()?,
        edge_unknown_tool()?,
        edge_invalid_audit_id()?,
    ];

    let fsv = Issue1421Fsv {
        issue: 1421,
        source_of_truth:
            "Calyx MCP tools/list and direct editor Aster Graph CF coverage-audit readback"
                .to_string(),
        research_basis: vec![
            "traceability matrix: map each requirement to implementation and verification evidence"
                .to_string(),
            "avoid false confidence from IDs without validated relationships".to_string(),
            "prefer tool-generated live traceability over manual spreadsheets".to_string(),
        ],
        tools_list: tools,
        audit,
        store_readback: json!({
            "write_sha256": writeback.sha256,
            "read_sha256": loaded.sha256,
            "bytes_match": loaded.bytes == writeback.bytes,
            "seq": loaded.seq,
            "graph_rows": loaded.graph_rows,
            "mapped_tool_count": loaded.mapped_tool_count,
            "covered_subsystem_count": loaded.covered_subsystem_count,
        }),
        edge_cases,
    };
    write_json(&artifact_path, &fsv)?;
    let readback = read_json::<Issue1421Fsv>(&artifact_path)?;
    if readback != fsv {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let recorded = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "{}",
        json!({
            "issue": 1421,
            "artifact_path": artifact_path,
            "artifact_sha256": recorded.sha256,
            "tool_count": fsv.audit.mapped_tool_count,
            "subsystem_count": fsv.audit.covered_subsystem_count,
            "store_bytes_match": fsv.store_readback["bytes_match"],
            "edge_case_count": fsv.edge_cases.len(),
            "artifact_readback_matches": true,
        })
    );
    Ok(())
}

fn tools_list_readback() -> Result<Value, Box<dyn std::error::Error>> {
    let mut server = McpServer::new();
    register_all(&mut server)?;
    let response = server.dispatch(JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "tools/list".to_string(),
        params: None,
        id: Some(JsonRpcId::Number(1421)),
    });
    let tools = response
        .result
        .as_ref()
        .ok_or("tools/list missing result")?["tools"]
        .as_array()
        .ok_or("tools/list tools missing")?;
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    let expected = litd_editor::mcp_tools::expected_calyx_mcp_tool_names()?;
    Ok(json!({
        "tool_count": names.len(),
        "names": names,
        "expected_count": expected.len(),
        "matches_live_registry": names == expected,
    }))
}

fn edge_missing_tool() -> Result<Value, Box<dyn std::error::Error>> {
    let names = current_tools()?
        .into_iter()
        .filter(|tool| tool != "calyx.bits")
        .collect();
    let error =
        build_structured_capability_coverage_audit("edge-missing-tool", "edge", names).unwrap_err();
    Ok(edge("missing_tool", error))
}

fn edge_unknown_tool() -> Result<Value, Box<dyn std::error::Error>> {
    let mut names = current_tools()?;
    names.push("calyx.ghost".to_string());
    let error =
        build_structured_capability_coverage_audit("edge-unknown-tool", "edge", names).unwrap_err();
    Ok(edge("unknown_tool", error))
}

fn edge_invalid_audit_id() -> Result<Value, Box<dyn std::error::Error>> {
    let error = build_structured_capability_coverage_audit("bad\nid", "edge", current_tools()?)
        .unwrap_err();
    Ok(edge("invalid_audit_id", error))
}

fn edge(name: &str, error: litd_engine::EngineError) -> Value {
    match error {
        litd_engine::EngineError::UiRenderInvalidConfig { field, reason } => {
            json!({ "name": name, "field": field, "reason": reason })
        }
        other => json!({ "name": name, "unexpected_error": format!("{other:?}") }),
    }
}

fn current_tools() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(litd_editor::mcp_tools::expected_calyx_mcp_tool_names()?)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
