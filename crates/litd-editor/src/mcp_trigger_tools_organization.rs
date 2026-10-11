//! #835 / #736: trigger organization mutation (`set_trigger_organization`) and
//! Object Manager cross-reference readback (`query_cross_ref`). Both operate over
//! the same persisted `McpTriggerRuntimeState` as the rest of the trigger tools --
//! the cross-reference is recomputed from that state, never a side index (#1348).

use litd_engine::EngineError;
use serde_json::{Value, json};

use crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths;
use crate::mcp_tools::McpDomainTool;
use crate::trigger_cross_ref::answer_cross_ref;

use super::mcp_trigger_tools_store::load_or_create_mcp_trigger_state;
use super::mcp_trigger_tools_validation::validate_eca_graph;
use super::{
    FinishTriggerMutationArgs, McpTriggerToolCallOutput, QUERY_CROSS_REF_TOOL, QueryCrossRefArgs,
    SET_TRIGGER_ORGANIZATION_TOOL, SetTriggerOrganizationArgs, finish_trigger_mutation,
    invalid_mcp_trigger, output, parse_args, read_mcp_trigger_state, validate_mutating_route,
    validate_request_identity,
};

pub(super) fn set_trigger_organization(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpTriggerToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: SetTriggerOrganizationArgs = parse_args(SET_TRIGGER_ORGANIZATION_TOOL, arguments)?;
    // #1518: confine both write sinks to the project root before any fs effect.
    confine_tool_paths(&[
        ("state_path", args.state_path.as_path()),
        ("ledger_path", args.ledger_path.as_path()),
    ])?;
    validate_request_identity(&args.request_id, &args.requested_by)?;
    let mut state = load_or_create_mcp_trigger_state(&args.state_path, &args.state_id)?;
    let index = state
        .graphs
        .iter()
        .position(|graph| graph.trigger_id == args.trigger_id)
        .ok_or_else(|| {
            invalid_mcp_trigger(
                "set_trigger_organization.trigger_id",
                format!("no trigger graph {} to organize", args.trigger_id),
            )
        })?;
    // Mutate only the organization fields, then re-validate the whole graph so a
    // bad folder/comment is rejected before it is persisted.
    let mut graph = state.graphs[index].clone();
    graph.enabled = args.enabled;
    graph.initially_on = args.initially_on;
    graph.folder_path = args.folder_path;
    graph.comment = args.comment;
    validate_eca_graph(&state, &graph)?;
    state.graphs[index] = graph;
    state.update_count += 1;
    let trigger_id = args.trigger_id;
    finish_trigger_mutation(FinishTriggerMutationArgs {
        route,
        state_path: &args.state_path,
        ledger_path: &args.ledger_path,
        ledger_id: args.ledger_id,
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        action_summary: "set trigger organization flags/folder/comment",
        state,
        affected_ids: vec![trigger_id],
    })
}

pub(super) fn query_cross_ref(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpTriggerToolCallOutput, EngineError> {
    let args: QueryCrossRefArgs = parse_args(QUERY_CROSS_REF_TOOL, arguments)?;
    // #1518: confine the read path to the project root before reading from disk.
    confine_tool_paths(&[("state_path", args.state_path.as_path())])?;
    // Independent readback of the source of truth, then compute edges from it.
    let trigger_readback = read_mcp_trigger_state(&args.state_path)?;
    let report = answer_cross_ref(&trigger_readback.state, &args.query)?;
    let report_json =
        serde_json::to_value(&report).map_err(|error| EngineError::SerializationFailed {
            format: "trigger-cross-ref-json",
            reason: error.to_string(),
        })?;
    Ok(output(
        route,
        "MCP trigger/runtime state JSON (cross-reference recomputed from persisted graphs)",
        json!({ "cross_ref": report_json }),
        Some(trigger_readback),
        None,
        None,
        None,
    ))
}
