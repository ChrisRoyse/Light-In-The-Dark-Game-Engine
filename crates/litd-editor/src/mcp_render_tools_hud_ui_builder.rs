use std::path::PathBuf;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    hud_ui_builder::{HudUiBuilderSelection, build_hud_ui_builder_readback},
    mcp_tools::McpDomainTool,
};

use super::{
    McpRenderToolCallOutput, RenderRecordArgs,
    mcp_render_tools_store::{
        load_or_create_mcp_render_state, write_mcp_render_state, write_mcp_render_tool_ledger,
    },
    output, parse_args, render_record, validate_mutating_route, validate_mutation_paths,
};

pub const SET_HUD_UI_BUILDER_TOOL: &str = "litd.engine.render.set_hud_ui_builder";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetHudUiBuilderArgs {
    pub state_path: PathBuf,
    pub state_id: String,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub selection: HudUiBuilderSelection,
}

pub(super) fn set_hud_ui_builder(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpRenderToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: SetHudUiBuilderArgs = parse_args(SET_HUD_UI_BUILDER_TOOL, arguments)?;
    validate_mutation_paths(&args.state_path, &args.ledger_path)?;
    let builder_readback = build_hud_ui_builder_readback(&args.selection)?;
    let mut state = load_or_create_mcp_render_state(&args.state_path, args.state_id)?;
    state.set_hud_ui_builder(args.selection);
    let state_readback = write_mcp_render_state(&args.state_path, &state)?;
    let record = render_record(RenderRecordArgs {
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        route: &route,
        state_path: &args.state_path,
        state: &state_readback,
        action_summary: "set no-code HUD UI builder selection and lowered render plan",
        affected_material_ids: Vec::new(),
        post_pass_count: None,
    })?;
    let ledger_readback = write_mcp_render_tool_ledger(&args.ledger_path, args.ledger_id, record)?;
    Ok(output(
        route,
        "MCP render state JSON + HUD UI builder render-plan readback + render MCP ledger JSON",
        json!({
            "state_path": state_readback.path,
            "state_sha256": state_readback.sha256,
            "hud_ui_builder_id": builder_readback.builder_id,
            "hud_ui_builder_widget_count": builder_readback.widget_count,
            "hud_ui_builder_text_run_count": builder_readback.text_run_count,
            "hud_ui_builder_minimap_marker_count": builder_readback.minimap_marker_count,
            "hud_ui_builder_overlay_policy_mode": builder_readback.overlay_policy_mode,
            "hud_ui_builder_map_surface_visible": builder_readback.map_surface_visible,
            "hud_ui_builder_player_position_indicator": builder_readback.player_position_indicator,
            "hud_ui_builder_viewport_indicator": builder_readback.viewport_indicator,
            "hud_ui_builder_quest_markers": builder_readback.quest_markers,
            "hud_ui_builder_route_lines": builder_readback.route_lines,
            "hud_ui_builder_pass_count": builder_readback.pass_count,
            "hud_ui_builder_reference_images": builder_readback.reference_images,
            "hud_ui_builder_plan_sha256": builder_readback.plan_sha256,
            "builder_readback": builder_readback
        }),
        Some(state_readback),
        Some(ledger_readback),
    ))
}
