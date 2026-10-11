use litd_engine::EngineError;

use super::{
    MCP_RENDER_LEDGER_SCHEMA, MCP_RENDER_STATE_SCHEMA, McpRenderState, McpRenderToolLedger,
    McpRenderToolRecord,
    mcp_render_tools_store_helpers::{store_error, validate_single_line},
};

pub(super) fn build_mcp_render_tool_ledger(
    ledger_id: impl Into<String>,
    records: Vec<McpRenderToolRecord>,
) -> Result<McpRenderToolLedger, EngineError> {
    let ledger = McpRenderToolLedger {
        schema: MCP_RENDER_LEDGER_SCHEMA.to_string(),
        issue: 923,
        ledger_id: ledger_id.into(),
        source_of_truth: "Calyx Aster MCP render tool ledger row + render state row".to_string(),
        record_count: records.len(),
        records,
    };
    validate_mcp_render_tool_ledger(&ledger)?;
    Ok(ledger)
}

pub(super) fn validate_state_identity(state: &McpRenderState) -> Result<(), EngineError> {
    validate_single_line("state_id", &state.state_id)?;
    if state.schema != MCP_RENDER_STATE_SCHEMA
        || state.issue != 923
        || state.source_of_truth.trim().is_empty()
    {
        return Err(store_error(
            "state",
            "schema, issue, and source_of_truth are invalid",
        ));
    }
    Ok(())
}

pub fn validate_mcp_render_tool_ledger(ledger: &McpRenderToolLedger) -> Result<(), EngineError> {
    validate_single_line("ledger_id", &ledger.ledger_id)?;
    if ledger.schema != MCP_RENDER_LEDGER_SCHEMA
        || ledger.issue != 923
        || ledger.record_count == 0
        || ledger.record_count != ledger.records.len()
    {
        return Err(store_error(
            "ledger",
            "schema, issue, and record count are invalid",
        ));
    }
    for record in &ledger.records {
        validate_record(record)?;
    }
    Ok(())
}

fn validate_record(record: &McpRenderToolRecord) -> Result<(), EngineError> {
    validate_single_line("record.request_id", &record.request_id)?;
    validate_single_line("record.requested_by", &record.requested_by)?;
    validate_single_line("record.state_path", &record.state_path)?;
    validate_single_line("record.action_summary", &record.action_summary)?;
    if !record.tool_name.starts_with("litd.engine.render.")
        || !record.operation_id.starts_with("engine.render.")
        || record.state_sha256.len() != 64
        || record.state_bytes == 0
        || record.state_update_count == 0
    {
        return Err(store_error(
            "record",
            "tool, state hash, bytes, and update count must be concrete",
        ));
    }
    for material_id in &record.affected_material_ids {
        validate_single_line("record.affected_material_id", material_id)?;
    }
    if let Some(scene_id) = &record.scene_id {
        validate_single_line("record.scene_id", scene_id)?;
    }
    if let Some(preset) = &record.shader_gallery_selected_preset {
        validate_single_line("record.shader_gallery_selected_preset", preset)?;
    }
    if let Some(shader_id) = &record.custom_wgsl_shader_id {
        validate_single_line("record.custom_wgsl_shader_id", shader_id)?;
    }
    if let Some(primitive_id) = &record.primitive_gallery_selected_id {
        validate_single_line("record.primitive_gallery_selected_id", primitive_id)?;
    }
    for topology_id in &record.primitive_gallery_topology_ids {
        validate_single_line("record.primitive_gallery_topology_id", topology_id)?;
    }
    if record.primitive_gallery_count == 0
        && (record.primitive_gallery_selected_id.is_some()
            || !record.primitive_gallery_topology_ids.is_empty())
    {
        return Err(store_error(
            "record.primitive_gallery",
            "primitive gallery ledger fields must be empty when count is zero",
        ));
    }
    if record.primitive_gallery_count > 0
        && (record.primitive_gallery_selected_id.is_none()
            || record.primitive_gallery_topology_ids.len() != record.primitive_gallery_count)
    {
        return Err(store_error(
            "record.primitive_gallery",
            "primitive gallery ledger count, selection, and topology IDs must match",
        ));
    }
    Ok(())
}
