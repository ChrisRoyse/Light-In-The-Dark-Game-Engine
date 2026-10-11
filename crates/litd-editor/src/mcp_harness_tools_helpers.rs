//! Routing and serialization helpers for agent harness MCP tools.

use litd_engine::EngineError;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::mcp_tools::{McpDomainTool, default_mcp_domain_toolset_manifest};

use super::{
    McpHarnessToolCallReadback, READ_HARNESS_TOOL,
    mcp_harness_tools_store::{McpHarnessLedgerReadback, McpHarnessStateReadback},
    mcp_harness_tools_validation::invalid_harness,
};

pub(super) fn output(
    route: McpDomainTool,
    source_of_truth: &str,
    result: Value,
    state_readback: Option<McpHarnessStateReadback>,
    ledger_readback: Option<McpHarnessLedgerReadback>,
) -> McpHarnessToolCallReadback {
    McpHarnessToolCallReadback {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: source_of_truth.to_string(),
        result,
        state_readback,
        ledger_readback,
    }
}

pub(super) fn harness_route(tool_name: &str) -> Result<McpDomainTool, EngineError> {
    let manifest = default_mcp_domain_toolset_manifest()?;
    let domain = manifest
        .domains
        .into_iter()
        .find(|domain| domain.domain_id == "agent_harness")
        .ok_or_else(|| {
            invalid_harness::<()>("domain", "missing agent_harness domain").unwrap_err()
        })?;
    domain
        .tools
        .into_iter()
        .find(|tool| tool.tool_name == tool_name)
        .ok_or_else(|| {
            invalid_harness::<()>(
                "tool_name",
                format!("{tool_name} is not registered in agent_harness"),
            )
            .unwrap_err()
        })
}

pub(super) fn validate_mutating_route(route: &McpDomainTool) -> Result<(), EngineError> {
    if !route.mutates_state
        || !route.requires_lowering
        || !route.requires_ward
        || !route.records_ledger
        || route.fsv_readback_tool_name != READ_HARNESS_TOOL
    {
        return invalid_harness(
            "route",
            format!(
                "{} is not a guarded harness mutation route",
                route.tool_name
            ),
        );
    }
    Ok(())
}

pub(super) fn parse_args<T: DeserializeOwned>(
    tool_name: &str,
    value: Value,
) -> Result<T, EngineError> {
    serde_json::from_value(value).map_err(|error| {
        invalid_harness::<()>(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
        .unwrap_err()
    })
}

pub(super) fn upsert_by<T, F>(values: &mut Vec<T>, mut predicate: F, value: T)
where
    F: FnMut(&T) -> bool,
{
    if let Some(existing) = values.iter_mut().find(|existing| predicate(existing)) {
        *existing = value;
    } else {
        values.push(value);
    }
}
