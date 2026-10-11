use litd_engine::EngineError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::mcp_tools::{McpDomainTool, default_mcp_domain_toolset_manifest};

use super::mcp_story_tools_store::{McpStoryStateReadback, McpStoryToolLedgerReadback};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpStoryToolCallOutput {
    pub tool_name: String,
    pub operation_id: String,
    pub source_of_truth: String,
    pub result: Value,
    pub state_readback: Option<McpStoryStateReadback>,
    pub ledger_readback: Option<McpStoryToolLedgerReadback>,
}

pub(super) fn output(
    route: McpDomainTool,
    source_of_truth: &str,
    result: Value,
    state_readback: Option<McpStoryStateReadback>,
    ledger_readback: Option<McpStoryToolLedgerReadback>,
) -> McpStoryToolCallOutput {
    McpStoryToolCallOutput {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: source_of_truth.to_string(),
        result,
        state_readback,
        ledger_readback,
    }
}

pub(super) fn story_route(tool_name: &str) -> Result<McpDomainTool, EngineError> {
    let manifest = default_mcp_domain_toolset_manifest()?;
    let domain = manifest
        .domains
        .into_iter()
        .find(|domain| domain.domain_id == "visual_story")
        .ok_or_else(|| invalid_mcp_story("domain", "missing visual_story domain"))?;
    domain
        .tools
        .into_iter()
        .find(|tool| tool.tool_name == tool_name)
        .ok_or_else(|| {
            invalid_mcp_story(
                "tool_name",
                format!("{tool_name} is not registered in visual_story"),
            )
        })
}

pub(super) fn validate_mutating_route(route: &McpDomainTool) -> Result<(), EngineError> {
    if !route.mutates_state
        || !route.requires_lowering
        || !route.requires_ward
        || !route.records_ledger
        || route.fsv_readback_tool_name != super::READ_STORY_TOOL
    {
        return Err(invalid_mcp_story(
            "route",
            format!("{} is not a guarded story/lore route", route.tool_name),
        ));
    }
    Ok(())
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

pub(super) fn parse_args<T: DeserializeOwned>(
    tool_name: &str,
    value: Value,
) -> Result<T, EngineError> {
    serde_json::from_value(value).map_err(|error| {
        invalid_mcp_story(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
    })
}

pub(super) fn invalid_mcp_story(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    let field = format!("editor.mcp_story_tools.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_story_tools",
        field = %field,
        reason = %reason,
        "invalid story/lore MCP tool call"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}
