use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn write_state_and_ledger(
    route: &McpDomainTool,
    state_path: &str,
    ledger_path: &str,
    ledger_id: &str,
    request_id: &str,
    requested_by: &str,
    action_summary: &str,
    affected_ids: Vec<String>,
    state: mcp_project_tools_store::McpProjectState,
) -> Result<(McpProjectStateReadback, McpProjectToolLedgerReadback), EngineError> {
    let state_readback = write_mcp_project_state(state_path, &state)?;
    let ledger_record = McpProjectToolRecord {
        request_id: request_id.to_string(),
        requested_by: requested_by.to_string(),
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        state_path: state_readback.path.clone(),
        state_sha256: state_readback.sha256.clone(),
        state_bytes: state_readback.bytes,
        action_summary: action_summary.to_string(),
        affected_ids,
        project_count: state_readback.project_count,
        playtest_count: state_readback.playtest_count,
        package_count: state_readback.package_count,
        published_count: state_readback.published_count,
    };
    let ledger_readback = write_mcp_project_tool_ledger(ledger_path, ledger_id, ledger_record)?;
    Ok((state_readback, ledger_readback))
}

pub(super) fn call_readback(
    route: &McpDomainTool,
    source_of_truth: &str,
    result: Value,
    state_readback: McpProjectStateReadback,
    ledger_readback: Option<McpProjectToolLedgerReadback>,
) -> McpProjectToolCallReadback {
    McpProjectToolCallReadback {
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        source_of_truth: source_of_truth.to_string(),
        result,
        state_readback,
        ledger_readback,
    }
}

pub(super) fn validate_mutating_route(
    route: &McpDomainTool,
    readback_tool_name: &str,
) -> Result<(), EngineError> {
    if !route.mutates_state
        || !route.requires_lowering
        || !route.requires_ward
        || !route.records_ledger
        || route.fsv_readback_tool_name != readback_tool_name
    {
        return Err(invalid_mcp_project(
            "route",
            format!("{} is not a guarded project route", route.tool_name),
        ));
    }
    Ok(())
}
