use std::path::Path;

use litd_engine::EngineError;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::mcp_tools::{McpDomainTool, default_mcp_domain_toolset_manifest};

use super::{
    McpRenderToolCallOutput, READ_RENDER_TOOL,
    mcp_render_tools_store::{
        McpRenderStateReadback, McpRenderToolLedgerReadback, McpRenderToolRecord,
    },
};

pub(super) fn output(
    route: McpDomainTool,
    source_of_truth: &str,
    result: Value,
    state_readback: Option<McpRenderStateReadback>,
    ledger_readback: Option<McpRenderToolLedgerReadback>,
) -> McpRenderToolCallOutput {
    McpRenderToolCallOutput {
        tool_name: route.tool_name,
        operation_id: route.operation_id,
        source_of_truth: source_of_truth.to_string(),
        result,
        state_readback,
        ledger_readback,
    }
}

pub(crate) struct RenderRecordArgs<'a> {
    pub(crate) request_id: &'a str,
    pub(crate) requested_by: &'a str,
    pub(crate) route: &'a McpDomainTool,
    pub(crate) state_path: &'a Path,
    pub(crate) state: &'a McpRenderStateReadback,
    pub(crate) action_summary: &'a str,
    pub(crate) affected_material_ids: Vec<String>,
    pub(crate) post_pass_count: Option<usize>,
}

pub(super) fn render_record(
    args: RenderRecordArgs<'_>,
) -> Result<McpRenderToolRecord, EngineError> {
    let RenderRecordArgs {
        request_id,
        requested_by,
        route,
        state_path,
        state,
        action_summary,
        affected_material_ids,
        post_pass_count,
    } = args;
    validate_single_line("request_id", request_id)?;
    validate_single_line("requested_by", requested_by)?;
    Ok(McpRenderToolRecord {
        request_id: request_id.to_string(),
        requested_by: requested_by.to_string(),
        tool_name: route.tool_name.clone(),
        operation_id: route.operation_id.clone(),
        state_path: state_path.display().to_string(),
        state_sha256: state.sha256.clone(),
        state_bytes: state.bytes,
        state_update_count: state.state.update_count,
        action_summary: action_summary.to_string(),
        affected_material_ids,
        light_count: state.light_count,
        animation_clip_count: state.animation_clip_count,
        scene_id: state
            .state
            .scene
            .as_ref()
            .map(|scene| scene.scene_id.clone()),
        scene_placement_count: state.scene_placement_count,
        post_pass_count,
        shader_gallery_selected_preset: state.shader_gallery_selected_preset.clone(),
        shader_gallery_material_count: state.shader_gallery_material_count,
        custom_wgsl_shader_id: state.shader_gallery_custom_wgsl_id.clone(),
        primitive_gallery_count: state.primitive_gallery_count,
        primitive_gallery_selected_id: state.primitive_gallery_selected_id.clone(),
        primitive_gallery_topology_ids: state.primitive_gallery_topology_ids.clone(),
    })
}

pub(super) fn render_route(tool_name: &str) -> Result<McpDomainTool, EngineError> {
    let manifest = default_mcp_domain_toolset_manifest()?;
    let domain = manifest
        .domains
        .into_iter()
        .find(|domain| domain.domain_id == "render_materials")
        .ok_or_else(|| invalid_mcp_render("domain", "missing render_materials domain"))?;
    domain
        .tools
        .into_iter()
        .find(|tool| tool.tool_name == tool_name)
        .ok_or_else(|| {
            invalid_mcp_render(
                "tool_name",
                format!("{tool_name} is not registered in render_materials"),
            )
        })
}

pub(super) fn validate_mutating_route(route: &McpDomainTool) -> Result<(), EngineError> {
    if !route.mutates_state
        || !route.requires_lowering
        || !route.requires_ward
        || !route.records_ledger
        || route.fsv_readback_tool_name != READ_RENDER_TOOL
    {
        return Err(invalid_mcp_render(
            "route",
            format!("{} is not a guarded render mutation route", route.tool_name),
        ));
    }
    Ok(())
}

pub(super) fn validate_mutation_paths(
    state_path: &Path,
    ledger_path: &Path,
) -> Result<(), EngineError> {
    validate_path("state_path", state_path)?;
    validate_path("ledger_path", ledger_path)?;
    // #1518: confine both write sinks to the configured project root before any fs effect.
    crate::mcp_project_tools::mcp_project_tools_validation::confine_tool_paths(&[
        ("state_path", state_path),
        ("ledger_path", ledger_path),
    ])
}

pub(super) fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_mcp_render(field, "path must be non-empty"));
    }
    Ok(())
}

pub(super) fn parse_args<T: DeserializeOwned>(
    tool_name: &str,
    value: Value,
) -> Result<T, EngineError> {
    serde_json::from_value(value).map_err(|error| {
        invalid_mcp_render(
            "arguments",
            format!("decode arguments for {tool_name}: {error}"),
        )
    })
}

pub(super) fn invalid_mcp_render(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    let field = format!("editor.mcp_render_tools.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_render_tools",
        field = %field,
        reason = %reason,
        "invalid render MCP tool call"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(invalid_mcp_render(
            field,
            "value must be a non-empty single line",
        ));
    }
    Ok(())
}
