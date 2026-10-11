use std::path::PathBuf;

use litd_engine::{
    EngineError,
    render::primitive::{PrimitiveGallerySelection, build_primitive_gallery_readback},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::mcp_tools::McpDomainTool;

use super::{
    McpRenderToolCallOutput, RenderRecordArgs, invalid_mcp_render,
    mcp_render_tools_store::{
        load_or_create_mcp_render_state, write_mcp_render_state, write_mcp_render_tool_ledger,
    },
    output, parse_args, render_record, validate_mutating_route, validate_mutation_paths,
};

pub const SET_PRIMITIVE_GALLERY_TOOL: &str = "litd.engine.render.set_primitive_gallery";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetPrimitiveGalleryArgs {
    pub state_path: PathBuf,
    pub state_id: String,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub gallery: PrimitiveGallerySelection,
}

pub(super) fn set_primitive_gallery(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpRenderToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: SetPrimitiveGalleryArgs = parse_args(SET_PRIMITIVE_GALLERY_TOOL, arguments)?;
    validate_mutation_paths(&args.state_path, &args.ledger_path)?;
    let gallery_readback = build_primitive_gallery_readback(&args.gallery)?;
    if gallery_readback.primitive_count != args.gallery.primitives.len() {
        return Err(invalid_mcp_render(
            "primitive_gallery.primitive_count",
            "gallery readback did not match submitted primitive count",
        ));
    }
    let selected_primitive_id = gallery_readback.selected_primitive_id.clone();
    let topology_ids = gallery_readback.topology_ids.clone();
    let mut state = load_or_create_mcp_render_state(&args.state_path, args.state_id)?;
    state.set_primitive_gallery(args.gallery);
    let state_readback = write_mcp_render_state(&args.state_path, &state)?;
    let record = render_record(RenderRecordArgs {
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        route: &route,
        state_path: &args.state_path,
        state: &state_readback,
        action_summary: "set procedural blockout primitive gallery selection",
        affected_material_ids: Vec::new(),
        post_pass_count: None,
    })?;
    let ledger_readback = write_mcp_render_tool_ledger(&args.ledger_path, args.ledger_id, record)?;
    Ok(output(
        route,
        "MCP render state JSON + render MCP ledger JSON",
        json!({
            "state_path": state_readback.path,
            "state_sha256": state_readback.sha256,
            "primitive_gallery_selected_id": selected_primitive_id,
            "primitive_gallery_count": gallery_readback.primitive_count,
            "primitive_gallery_topology_ids": topology_ids,
            "primitive_gallery_total_vertex_count": gallery_readback.total_vertex_count,
            "primitive_gallery_total_index_count": gallery_readback.total_index_count,
            "gallery_readback": gallery_readback
        }),
        Some(state_readback),
        Some(ledger_readback),
    ))
}
