use super::*;

pub(crate) struct WriteWithLedgerArgs<'a> {
    pub(crate) route: &'a McpDomainTool,
    pub(crate) state_path: &'a str,
    pub(crate) ledger_path: &'a str,
    pub(crate) ledger_id: &'a str,
    pub(crate) request_id: &'a str,
    pub(crate) requested_by: &'a str,
    pub(crate) action_summary: &'a str,
    pub(crate) affected_ids: Vec<String>,
    pub(crate) state: mcp_project_tools_store::McpProjectState,
    pub(crate) result: Value,
}

pub(super) fn write_with_ledger(
    args: WriteWithLedgerArgs<'_>,
) -> Result<McpProjectToolCallReadback, EngineError> {
    let WriteWithLedgerArgs {
        route,
        state_path,
        ledger_path,
        ledger_id,
        request_id,
        requested_by,
        action_summary,
        affected_ids,
        state,
        result,
    } = args;
    let (state_readback, ledger_readback) = write_state_and_ledger(
        route,
        state_path,
        ledger_path,
        ledger_id,
        request_id,
        requested_by,
        action_summary,
        affected_ids,
        state,
    )?;
    Ok(call_readback(
        route,
        "Calyx Aster MCP project state + tool ledger",
        result,
        state_readback,
        Some(ledger_readback),
    ))
}
