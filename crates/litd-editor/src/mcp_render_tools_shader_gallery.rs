use std::path::PathBuf;

use litd_engine::{
    EngineError,
    render::{
        shader::ShaderSystemSettings,
        stylized::{
            StylizedGallerySelection, build_stylized_gallery_readback,
            build_stylized_shader_library,
        },
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

pub const SET_SHADER_GALLERY_TOOL: &str = "litd.engine.render.set_shader_gallery";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetShaderGalleryArgs {
    pub state_path: PathBuf,
    pub state_id: String,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub gallery: StylizedGallerySelection,
}

pub(super) fn set_shader_gallery(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpRenderToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: SetShaderGalleryArgs = parse_args(SET_SHADER_GALLERY_TOOL, arguments)?;
    validate_mutation_paths(&args.state_path, &args.ledger_path)?;
    let library = build_stylized_shader_library(ShaderSystemSettings::default())?;
    let gallery_readback =
        build_stylized_gallery_readback(ShaderSystemSettings::default(), &library, &args.gallery)?;
    if gallery_readback.material_count != args.gallery.materials.len() {
        return Err(invalid_mcp_render(
            "shader_gallery.material_count",
            "gallery readback did not match submitted material count",
        ));
    }
    let selected_preset = gallery_readback.selected_preset.clone();
    let custom_wgsl_shader_id = gallery_readback
        .custom_wgsl
        .as_ref()
        .map(|custom| custom.shader_id.clone());
    let mut state = load_or_create_mcp_render_state(&args.state_path, args.state_id)?;
    state.set_shader_gallery(args.gallery);
    let state_readback = write_mcp_render_state(&args.state_path, &state)?;
    let record = render_record(RenderRecordArgs {
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        route: &route,
        state_path: &args.state_path,
        state: &state_readback,
        action_summary: "set no-code stylized shader gallery selection and WGSL escape",
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
            "shader_gallery_selected_preset": selected_preset,
            "shader_gallery_material_count": gallery_readback.material_count,
            "shader_gallery_preset_ids": gallery_readback.material_preset_ids,
            "shader_gallery_custom_wgsl_id": custom_wgsl_shader_id,
            "gallery_readback": gallery_readback
        }),
        Some(state_readback),
        Some(ledger_readback),
    ))
}
