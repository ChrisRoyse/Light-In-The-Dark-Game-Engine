use litd_engine::EngineError;

use crate::mcp_tools::{McpDomainTool, default_mcp_domain_toolset_manifest};

use super::invalid_mcp_project;

pub(super) fn project_route(tool_name: &str) -> Result<McpDomainTool, EngineError> {
    let manifest = default_mcp_domain_toolset_manifest()?;
    let domain = manifest
        .domains
        .into_iter()
        .find(|domain| domain.domain_id == "project_playtest")
        .ok_or_else(|| invalid_mcp_project("domain", "missing project_playtest domain"))?;
    domain
        .tools
        .into_iter()
        .find(|tool| tool.tool_name == tool_name)
        .ok_or_else(|| {
            invalid_mcp_project(
                "tool_name",
                format!("{tool_name} is not registered in project_playtest"),
            )
        })
}
