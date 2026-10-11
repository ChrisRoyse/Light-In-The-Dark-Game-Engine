use std::path::Path;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    content_trust::{
        ContentTrustPolicy, SharedContentImport, build_content_trust_ledger,
        evaluate_content_trust, parse_content_trust_ledger, read_content_trust_ledger,
        write_content_trust_ledger,
    },
    mcp_tools::McpDomainTool,
};

use super::{
    McpProjectToolCallReadback,
    mcp_project_tools_store::{
        McpProjectToolRecord, load_or_create_mcp_project_state, read_mcp_project_state,
        write_mcp_project_state, write_mcp_project_tool_ledger,
    },
    parse_args, validate_mutating_route,
};

use super::mcp_project_tools_validation::{validate_path, validate_request_identity};

pub const CONTENT_TRUST_TOOL: &str = "litd.editor.project.evaluate_content_trust";
pub const READ_CONTENT_TRUST_TOOL: &str = "litd.editor.project.read_content_trust";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentTrustToolArgs {
    pub request_id: String,
    pub requested_by: String,
    pub state_path: String,
    pub state_id: String,
    pub ledger_path: String,
    pub ledger_id: String,
    pub trust_state_path: String,
    pub trust_aster_dir: String,
    pub trust_ledger_id: String,
    pub policy: ContentTrustPolicy,
    pub import: SharedContentImport,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadContentTrustArgs {
    pub state_path: String,
    pub trust_aster_dir: String,
    pub trust_ledger_id: String,
}

pub(super) fn run_content_trust_gate(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    validate_mutating_route(&route, READ_CONTENT_TRUST_TOOL)?;
    let args: ContentTrustToolArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every tool-supplied path to the project root before any fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", std::path::Path::new(&args.state_path)),
        ("ledger_path", std::path::Path::new(&args.ledger_path)),
        (
            "trust_state_path",
            std::path::Path::new(&args.trust_state_path),
        ),
        (
            "trust_aster_dir",
            std::path::Path::new(&args.trust_aster_dir),
        ),
    ])?;
    validate_content_trust_args(&args)?;

    let decision = evaluate_content_trust(&args.import, &args.policy)?;
    let ledger = build_content_trust_ledger(&args.trust_ledger_id, vec![decision.clone()])?;
    let trust_readback = write_content_trust_ledger(
        Path::new(&args.trust_state_path),
        Path::new(&args.trust_aster_dir),
        &ledger,
    )?;
    let persisted =
        read_content_trust_ledger(Path::new(&args.trust_aster_dir), &args.trust_ledger_id)?
            .ok_or_else(|| {
                content_trust_tool_error("readback", "trust ledger row missing after write")
            })?;
    if persisted.bytes != trust_readback.bytes || persisted.sha256 != trust_readback.sha256 {
        return Err(content_trust_tool_error(
            "readback",
            "post-write Aster readback did not match the write readback",
        ));
    }

    let mut state = load_or_create_mcp_project_state(&args.state_path, &args.state_id)?;
    state.update_count = state.update_count.saturating_add(1);
    let state_readback = write_mcp_project_state(&args.state_path, &state)?;
    let ledger_readback = write_mcp_project_tool_ledger(
        &args.ledger_path,
        &args.ledger_id,
        McpProjectToolRecord {
            request_id: args.request_id,
            requested_by: args.requested_by,
            tool_name: route.tool_name.clone(),
            operation_id: route.operation_id.clone(),
            state_path: state_readback.path.clone(),
            state_sha256: state_readback.sha256.clone(),
            state_bytes: state_readback.bytes,
            action_summary: "evaluated shared-content trust gate and persisted Aster ledger row"
                .to_string(),
            affected_ids: vec![
                decision.import_id.clone(),
                decision.content_id.clone(),
                args.trust_ledger_id.clone(),
            ],
            project_count: state_readback.project_count,
            playtest_count: state_readback.playtest_count,
            package_count: state_readback.package_count,
            published_count: state_readback.published_count,
        },
    )?;

    Ok(McpProjectToolCallReadback {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: "Calyx Aster content-trust ledger row + MCP project tool ledger"
            .to_string(),
        result: json!({
            "decision": decision,
            "content_trust_write_readback": trust_readback,
            "content_trust_post_write_readback": persisted,
            "trust_state_path": args.trust_state_path,
            "trust_aster_dir": args.trust_aster_dir,
        }),
        state_readback,
        ledger_readback: Some(ledger_readback),
    })
}

pub(super) fn read_content_trust_gate(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpProjectToolCallReadback, EngineError> {
    if route.mutates_state {
        return Err(content_trust_tool_error(
            "route",
            "read_content_trust cannot mutate state",
        ));
    }
    let args: ReadContentTrustArgs = parse_args(&route.tool_name, arguments)?;
    // #1518: confine every read path to the project root before reading from disk.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", Path::new(&args.state_path)),
        ("trust_aster_dir", Path::new(&args.trust_aster_dir)),
    ])?;
    validate_path("trust_aster_dir", Path::new(&args.trust_aster_dir))?;
    let state_readback = read_mcp_project_state(&args.state_path)?;
    let readback =
        read_content_trust_ledger(Path::new(&args.trust_aster_dir), &args.trust_ledger_id)?
            .ok_or_else(|| {
                content_trust_tool_error("trust_ledger_id", "content-trust ledger row not found")
            })?;
    let ledger = parse_content_trust_ledger(&readback.bytes)?;
    Ok(McpProjectToolCallReadback {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: "Aster Graph CF content-trust ledger row".to_string(),
        result: json!({
            "content_trust_readback": readback,
            "ledger": ledger,
        }),
        state_readback,
        ledger_readback: None,
    })
}

fn validate_content_trust_args(args: &ContentTrustToolArgs) -> Result<(), EngineError> {
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_path("trust_state_path", Path::new(&args.trust_state_path))?;
    validate_path("trust_aster_dir", Path::new(&args.trust_aster_dir))?;
    if args.trust_ledger_id.trim().is_empty() || args.trust_ledger_id.contains(['\n', '\r', '\t']) {
        return Err(content_trust_tool_error(
            "trust_ledger_id",
            "trust ledger id must be non-empty and single-line",
        ));
    }
    Ok(())
}

fn content_trust_tool_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    let field = format!("editor.mcp_project_tools.content_trust.{}", field.into());
    tracing::error!(
        target: "litd_editor::mcp_project_tools::content_trust",
        field,
        reason = %reason.to_string(),
        "invalid content-trust MCP tool call"
    );
    EngineError::UiRenderInvalidConfig {
        field,
        reason: reason.to_string(),
    }
}
