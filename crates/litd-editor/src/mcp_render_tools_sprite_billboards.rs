use std::path::PathBuf;

use litd_engine::{
    EngineError,
    render::sprite_billboard::{
        SpriteBillboardLibrarySelection, build_sprite_billboard_library_readback,
    },
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

pub const SET_SPRITE_BILLBOARD_LIBRARY_TOOL: &str =
    "litd.engine.render.set_sprite_billboard_library";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetSpriteBillboardLibraryArgs {
    pub state_path: PathBuf,
    pub state_id: String,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub library: SpriteBillboardLibrarySelection,
}

pub(super) fn set_sprite_billboard_library(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpRenderToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: SetSpriteBillboardLibraryArgs =
        parse_args(SET_SPRITE_BILLBOARD_LIBRARY_TOOL, arguments)?;
    validate_mutation_paths(&args.state_path, &args.ledger_path)?;
    let library_readback = build_sprite_billboard_library_readback(&args.library)?;
    if library_readback.billboard_count != args.library.billboards.len() {
        return Err(invalid_mcp_render(
            "sprite_billboard_library.billboard_count",
            "library readback did not match submitted billboard count",
        ));
    }
    let mut state = load_or_create_mcp_render_state(&args.state_path, args.state_id)?;
    state.set_sprite_billboard_library(args.library);
    let state_readback = write_mcp_render_state(&args.state_path, &state)?;
    let record = render_record(RenderRecordArgs {
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        route: &route,
        state_path: &args.state_path,
        state: &state_readback,
        action_summary: "set content-addressed sprite billboard material and texture library",
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
            "sprite_billboard_library_id": library_readback.library_id,
            "sprite_billboard_texture_count": library_readback.texture_count,
            "sprite_billboard_material_count": library_readback.material_count,
            "sprite_billboard_sheet_count": library_readback.sheet_count,
            "sprite_billboard_billboard_count": library_readback.billboard_count,
            "sprite_billboard_search_row_count": library_readback.search_row_count,
            "texture_content_addresses": library_readback.texture_content_addresses,
            "library_readback": library_readback
        }),
        Some(state_readback),
        Some(ledger_readback),
    ))
}
