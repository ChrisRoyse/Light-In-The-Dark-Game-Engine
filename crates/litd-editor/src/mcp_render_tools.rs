//! Real render MCP tool dispatch over the shared Rust/wgpu render capability layer.

use std::path::PathBuf;

use litd_engine::{
    EngineError,
    math::Vec3,
    render::{
        lighting::{AmbientLight, ClusterGrid, RenderLight},
        material::PbrMaterial,
        post::{PostProcessSettings, build_post_process_plan},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::mcp_tools::McpDomainTool;

use self::mcp_render_tools_store::{
    McpRenderToolLedgerReadback, load_or_create_mcp_render_state, write_mcp_render_state,
    write_mcp_render_tool_ledger,
};

#[path = "mcp_render_tools_helpers.rs"]
mod mcp_render_tools_helpers;
#[path = "mcp_render_tools_hud_ui_builder.rs"]
mod mcp_render_tools_hud_ui_builder;
#[path = "mcp_render_tools_primitives.rs"]
mod mcp_render_tools_primitives;
#[path = "mcp_render_tools_shader_gallery.rs"]
mod mcp_render_tools_shader_gallery;
#[path = "mcp_render_tools_sprite_billboards.rs"]
mod mcp_render_tools_sprite_billboards;
#[path = "mcp_render_tools_store.rs"]
mod mcp_render_tools_store;
#[path = "mcp_render_tools_validation.rs"]
mod mcp_render_tools_validation;

use mcp_render_tools_helpers::{
    RenderRecordArgs, invalid_mcp_render, output, parse_args, render_record, render_route,
    validate_mutating_route, validate_mutation_paths, validate_path,
};
use mcp_render_tools_hud_ui_builder::set_hud_ui_builder;
pub use mcp_render_tools_hud_ui_builder::{SET_HUD_UI_BUILDER_TOOL, SetHudUiBuilderArgs};
use mcp_render_tools_primitives::set_primitive_gallery;
pub use mcp_render_tools_primitives::{SET_PRIMITIVE_GALLERY_TOOL, SetPrimitiveGalleryArgs};
use mcp_render_tools_shader_gallery::set_shader_gallery;
pub use mcp_render_tools_shader_gallery::{SET_SHADER_GALLERY_TOOL, SetShaderGalleryArgs};
use mcp_render_tools_sprite_billboards::set_sprite_billboard_library;
pub use mcp_render_tools_sprite_billboards::{
    SET_SPRITE_BILLBOARD_LIBRARY_TOOL, SetSpriteBillboardLibraryArgs,
};
pub use mcp_render_tools_store::{
    MCP_RENDER_LEDGER_SCHEMA, MCP_RENDER_STATE_SCHEMA, McpRenderState, McpRenderStateReadback,
    McpRenderToolLedger, read_mcp_render_state, read_mcp_render_tool_ledger,
    validate_mcp_render_state, validate_mcp_render_tool_ledger,
};
pub use mcp_render_tools_validation::sample_issue923_material;
use mcp_render_tools_validation::{
    validate_animation_binding, validate_lighting_plan, validate_material_binding,
    validate_scene_composition,
};

pub const SET_PBR_MATERIAL_TOOL: &str = "litd.engine.render.set_pbr_material";
pub const SET_LIGHTING_ANIMATION_POST_TOOL: &str = "litd.engine.render.set_lighting_animation_post";
pub const COMPOSE_SCENE_TOOL: &str = "litd.engine.render.compose_scene";
pub const READ_RENDER_TOOL: &str = "litd.engine.render.read_render";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpRenderToolCallOutput {
    pub tool_name: String,
    pub operation_id: String,
    pub source_of_truth: String,
    pub result: Value,
    pub state_readback: Option<McpRenderStateReadback>,
    pub ledger_readback: Option<McpRenderToolLedgerReadback>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetPbrMaterialArgs {
    pub state_path: PathBuf,
    pub state_id: String,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub binding: RenderMaterialBinding,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetLightingAnimationPostArgs {
    pub state_path: PathBuf,
    pub state_id: String,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub lighting: RenderLightingPlan,
    pub animation: RenderAnimationBinding,
    pub post_process: PostProcessSettings,
    pub viewport_width: u32,
    pub viewport_height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComposeSceneArgs {
    pub state_path: PathBuf,
    pub state_id: String,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub composition: RenderSceneComposition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadRenderArgs {
    pub state_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderMaterialBinding {
    pub material_id: String,
    pub target_asset_id: String,
    pub material: PbrMaterial,
    pub channel_notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderLightingPlan {
    pub cluster_grid: ClusterGrid,
    pub ambient: AmbientLight,
    pub lights: Vec<RenderLight>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderAnimationBinding {
    pub target_entity_id: String,
    pub rig_id: String,
    pub blend_mode: RenderAnimationBlendMode,
    pub clips: Vec<RenderAnimationClipBlend>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderAnimationBlendMode {
    Replace,
    AdditiveUpperBody,
    LocomotionBlendSpace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderAnimationClipBlend {
    pub clip_id: String,
    pub source_relative_path: String,
    pub weight: f32,
    pub normalized_time: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderSceneComposition {
    pub scene_id: String,
    pub source_scene_id: String,
    pub camera_mode: String,
    pub layers: Vec<RenderSceneLayer>,
    pub placements: Vec<RenderScenePlacement>,
    pub material_assignments: Vec<RenderSceneMaterialAssignment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderSceneLayer {
    pub layer_id: String,
    pub kind: String,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderScenePlacement {
    pub placement_id: String,
    pub asset_id: String,
    pub layer_id: String,
    #[serde(default)]
    pub parent_placement_id: Option<String>,
    pub translation: Vec3,
    pub rotation_y_rad: f32,
    pub scale: Vec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderSceneMaterialAssignment {
    pub placement_id: String,
    pub material_id: String,
}

pub fn call_mcp_render_tool(
    tool_name: &str,
    arguments: Value,
) -> Result<McpRenderToolCallOutput, EngineError> {
    let route = render_route(tool_name)?;
    match tool_name {
        SET_PBR_MATERIAL_TOOL => set_pbr_material(route, arguments),
        SET_LIGHTING_ANIMATION_POST_TOOL => set_lighting_animation_post(route, arguments),
        COMPOSE_SCENE_TOOL => compose_scene(route, arguments),
        SET_SHADER_GALLERY_TOOL => set_shader_gallery(route, arguments),
        SET_PRIMITIVE_GALLERY_TOOL => set_primitive_gallery(route, arguments),
        SET_SPRITE_BILLBOARD_LIBRARY_TOOL => set_sprite_billboard_library(route, arguments),
        SET_HUD_UI_BUILDER_TOOL => set_hud_ui_builder(route, arguments),
        READ_RENDER_TOOL => read_render(route, arguments),
        other => Err(invalid_mcp_render(
            "tool_name",
            format!("{other} is not a registered render MCP tool"),
        )),
    }
}

fn set_pbr_material(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpRenderToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: SetPbrMaterialArgs = parse_args(SET_PBR_MATERIAL_TOOL, arguments)?;
    validate_mutation_paths(&args.state_path, &args.ledger_path)?;
    validate_material_binding(&args.binding)?;
    let material_id = args.binding.material_id.clone();
    let mut state = load_or_create_mcp_render_state(&args.state_path, args.state_id)?;
    state.upsert_material(args.binding)?;
    let state_readback = write_mcp_render_state(&args.state_path, &state)?;
    let record = render_record(RenderRecordArgs {
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        route: &route,
        state_path: &args.state_path,
        state: &state_readback,
        action_summary: "upserted validated PBR material binding",
        affected_material_ids: vec![material_id.clone()],
        post_pass_count: None,
    })?;
    let ledger_readback = write_mcp_render_tool_ledger(&args.ledger_path, args.ledger_id, record)?;
    Ok(output(
        route,
        "MCP render state JSON + render MCP ledger JSON",
        json!({
            "state_path": state_readback.path,
            "state_sha256": state_readback.sha256,
            "material_count": state_readback.state.materials.len(),
            "updated_material_id": material_id
        }),
        Some(state_readback),
        Some(ledger_readback),
    ))
}

fn set_lighting_animation_post(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpRenderToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: SetLightingAnimationPostArgs =
        parse_args(SET_LIGHTING_ANIMATION_POST_TOOL, arguments)?;
    validate_mutation_paths(&args.state_path, &args.ledger_path)?;
    let assignment = validate_lighting_plan(&args.lighting)?;
    validate_animation_binding(&args.animation)?;
    args.post_process.validate()?;
    let post_plan =
        build_post_process_plan(args.post_process, args.viewport_width, args.viewport_height)?;
    let mut state = load_or_create_mcp_render_state(&args.state_path, args.state_id)?;
    state.set_lighting_animation_post(args.lighting, args.animation, args.post_process);
    let state_readback = write_mcp_render_state(&args.state_path, &state)?;
    let record = render_record(RenderRecordArgs {
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        route: &route,
        state_path: &args.state_path,
        state: &state_readback,
        action_summary: "set clustered lights, animation blend, and post-processing plan",
        affected_material_ids: Vec::new(),
        post_pass_count: Some(post_plan.passes.len()),
    })?;
    let ledger_readback = write_mcp_render_tool_ledger(&args.ledger_path, args.ledger_id, record)?;
    Ok(output(
        route,
        "MCP render state JSON + render MCP ledger JSON",
        json!({
            "state_path": state_readback.path,
            "state_sha256": state_readback.sha256,
            "light_count": state_readback.light_count,
            "ambient_name": state_readback
                .state
                .lighting
                .as_ref()
                .map(|lighting| lighting.ambient.name.clone()),
            "directional_light_count": assignment.directional_count(),
            "cluster_count": assignment.cluster_count(),
            "non_empty_cluster_count": assignment.non_empty_cluster_count(),
            "max_cluster_occupancy": assignment.max_cluster_occupancy(),
            "animation_clip_count": state_readback.animation_clip_count,
            "post_pass_count": post_plan.passes.len()
        }),
        Some(state_readback),
        Some(ledger_readback),
    ))
}

fn compose_scene(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpRenderToolCallOutput, EngineError> {
    validate_mutating_route(&route)?;
    let args: ComposeSceneArgs = parse_args(COMPOSE_SCENE_TOOL, arguments)?;
    validate_mutation_paths(&args.state_path, &args.ledger_path)?;
    let mut state = load_or_create_mcp_render_state(&args.state_path, args.state_id)?;
    validate_scene_composition(&args.composition, &state)?;
    let scene_id = args.composition.scene_id.clone();
    state.set_scene_composition(args.composition);
    let state_readback = write_mcp_render_state(&args.state_path, &state)?;
    let record = render_record(RenderRecordArgs {
        request_id: &args.request_id,
        requested_by: &args.requested_by,
        route: &route,
        state_path: &args.state_path,
        state: &state_readback,
        action_summary: "composed renderable scene from explicit layers, placements, and materials",
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
            "scene_id": scene_id,
            "scene_layer_count": state_readback.scene_layer_count,
            "scene_placement_count": state_readback.scene_placement_count,
            "scene_parented_placement_count": state_readback.scene_parented_placement_count,
            "material_assignment_count": state_readback.material_assignment_count
        }),
        Some(state_readback),
        Some(ledger_readback),
    ))
}

fn read_render(
    route: McpDomainTool,
    arguments: Value,
) -> Result<McpRenderToolCallOutput, EngineError> {
    let args: ReadRenderArgs = parse_args(READ_RENDER_TOOL, arguments)?;
    validate_path("state_path", &args.state_path)?;
    let state_readback = read_mcp_render_state(&args.state_path)?;
    let sprite_library = state_readback.sprite_billboard_library.as_ref();
    let hud_builder = state_readback.hud_ui_builder.as_ref();
    Ok(output(
        route,
        "MCP render state JSON",
        json!({
            "state_path": state_readback.path,
            "state_sha256": state_readback.sha256,
            "state": state_readback.state,
            "material_count": state_readback.material_count,
            "light_count": state_readback.light_count,
            "ambient_name": state_readback
                .state
                .lighting
                .as_ref()
                .map(|lighting| lighting.ambient.name.clone()),
            "animation_clip_count": state_readback.animation_clip_count,
            "scene_layer_count": state_readback.scene_layer_count,
            "scene_placement_count": state_readback.scene_placement_count,
            "scene_parented_placement_count": state_readback.scene_parented_placement_count,
            "material_assignment_count": state_readback.material_assignment_count,
            "shader_gallery_material_count": state_readback.shader_gallery_material_count,
            "shader_gallery_selected_preset": state_readback.shader_gallery_selected_preset,
            "shader_gallery_custom_wgsl_id": state_readback.shader_gallery_custom_wgsl_id,
            "primitive_gallery_count": state_readback.primitive_gallery_count,
            "primitive_gallery_selected_id": state_readback.primitive_gallery_selected_id,
            "primitive_gallery_topology_ids": state_readback.primitive_gallery_topology_ids,
            "primitive_gallery_total_vertex_count": state_readback.primitive_gallery_total_vertex_count,
            "primitive_gallery_total_index_count": state_readback.primitive_gallery_total_index_count,
            "sprite_billboard_library_id": sprite_library.map(|library| library.library_id.clone()),
            "sprite_billboard_texture_count": sprite_library.map(|library| library.texture_count).unwrap_or(0),
            "sprite_billboard_material_count": sprite_library.map(|library| library.material_count).unwrap_or(0),
            "sprite_billboard_sheet_count": sprite_library.map(|library| library.sheet_count).unwrap_or(0),
            "sprite_billboard_billboard_count": sprite_library.map(|library| library.billboard_count).unwrap_or(0),
            "sprite_billboard_search_row_count": sprite_library.map(|library| library.search_row_count).unwrap_or(0),
            "sprite_billboard_texture_content_addresses": sprite_library.map(|library| library.texture_content_addresses.clone()).unwrap_or_default(),
            "hud_ui_builder_id": hud_builder.map(|builder| builder.builder_id.clone()),
            "hud_ui_builder_widget_count": state_readback.hud_ui_builder_widget_count,
            "hud_ui_builder_text_run_count": state_readback.hud_ui_builder_text_run_count,
            "hud_ui_builder_pass_count": state_readback.hud_ui_builder_pass_count,
            "hud_ui_builder_overlay_policy_mode": hud_builder.map(|builder| builder.overlay_policy_mode),
            "hud_ui_builder_map_surface_visible": hud_builder.map(|builder| builder.map_surface_visible),
            "hud_ui_builder_player_position_indicator": hud_builder.map(|builder| builder.player_position_indicator),
            "hud_ui_builder_viewport_indicator": hud_builder.map(|builder| builder.viewport_indicator),
            "hud_ui_builder_quest_markers": hud_builder.map(|builder| builder.quest_markers),
            "hud_ui_builder_route_lines": hud_builder.map(|builder| builder.route_lines),
            "hud_ui_builder_reference_images": hud_builder.map(|builder| builder.reference_images.clone()).unwrap_or_default(),
            "hud_ui_builder_plan_sha256": hud_builder.map(|builder| builder.plan_sha256.clone())
        }),
        Some(state_readback),
        None,
    ))
}

#[cfg(test)]
#[path = "mcp_render_tools_primitive_tests.rs"]
mod primitive_tests;

#[cfg(test)]
#[path = "mcp_render_tools_sprite_billboard_tests.rs"]
mod sprite_billboard_tests;

#[cfg(test)]
#[path = "mcp_render_tools_hud_ui_builder_tests.rs"]
mod hud_ui_builder_tests;

#[cfg(test)]
#[path = "mcp_render_tools_tests.rs"]
mod tests;
