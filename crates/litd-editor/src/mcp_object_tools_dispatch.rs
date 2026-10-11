//! Dispatch/route helpers for the object/game-data MCP surface: resolve a tool name to its
//! guarded `object_game_data` route and validate affected-object-id lists. Split out of
//! `mcp_object_tools` to hold the <500-line ratchet (the same pattern as the other
//! `mcp_object_tools_*` submodules).

use std::collections::BTreeSet;

use litd_engine::EngineError;

use crate::mcp_tools::{McpDomainTool, default_mcp_domain_toolset_manifest};

use super::{invalid_mcp_object, validate_single_line};

pub(super) fn object_route(tool_name: &str) -> Result<McpDomainTool, EngineError> {
    let manifest = default_mcp_domain_toolset_manifest()?;
    let domain = manifest
        .domains
        .into_iter()
        .find(|domain| domain.domain_id == "object_game_data")
        .ok_or_else(|| invalid_mcp_object("domain", "missing object_game_data domain"))?;
    domain
        .tools
        .into_iter()
        .find(|tool| tool.tool_name == tool_name)
        .ok_or_else(|| {
            invalid_mcp_object(
                "tool_name",
                format!("{tool_name} is not registered in object_game_data"),
            )
        })
}

pub(super) fn validate_affected_object_ids(ids: &[String]) -> Result<(), EngineError> {
    if ids.is_empty() {
        return Err(invalid_mcp_object(
            "affected_object_ids",
            "at least one affected object id is required",
        ));
    }
    let mut unique = BTreeSet::new();
    for id in ids {
        validate_single_line("affected_object_ids", id)?;
        if !unique.insert(id.as_str()) {
            return Err(invalid_mcp_object(
                "affected_object_ids",
                format!("duplicate affected object id {id}"),
            ));
        }
    }
    Ok(())
}
