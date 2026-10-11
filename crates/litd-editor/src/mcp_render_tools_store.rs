use litd_hash::sha256_hex;
use std::path::Path;

use litd_engine::{
    EngineError,
    calyx_json_store::{read_calyx_json_bytes, write_calyx_json_bytes},
    render::{
        primitive::{PrimitiveGallerySelection, build_primitive_gallery_readback},
        shader::ShaderSystemSettings,
        sprite_billboard::{
            SpriteBillboardLibraryReadback, SpriteBillboardLibrarySelection,
            build_sprite_billboard_library_readback,
        },
        stylized::{StylizedGallerySelection, build_stylized_shader_library},
    },
};
use serde::{Deserialize, Serialize};

use crate::hud_ui_builder::{
    HudUiBuilderReadback, HudUiBuilderSelection, build_hud_ui_builder_readback,
};

use super::{
    RenderAnimationBinding, RenderLightingPlan, RenderMaterialBinding, RenderSceneComposition,
    validate_animation_binding, validate_lighting_plan, validate_material_binding,
    validate_scene_composition,
};

#[path = "mcp_render_tools_ledger_types.rs"]
mod mcp_render_tools_ledger_types;
#[path = "mcp_render_tools_store_helpers.rs"]
mod mcp_render_tools_store_helpers;
#[path = "mcp_render_tools_store_validate.rs"]
mod mcp_render_tools_store_validate;

pub use mcp_render_tools_ledger_types::{
    McpRenderToolLedger, McpRenderToolLedgerReadback, McpRenderToolRecord,
};
use mcp_render_tools_store_helpers::{
    render_ledger_spec, render_state_spec, store_error, validate_path, validate_single_line,
};
pub use mcp_render_tools_store_validate::validate_mcp_render_tool_ledger;
use mcp_render_tools_store_validate::{build_mcp_render_tool_ledger, validate_state_identity};

pub const MCP_RENDER_STATE_SCHEMA: &str = "litd.mcp.render_tools.state.v1";
pub const MCP_RENDER_LEDGER_SCHEMA: &str = "litd.mcp.render_tools.ledger.v1";
const MCP_RENDER_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FC8";
const MCP_RENDER_VAULT_PURPOSE: &[u8] = b"litd-mcp-render-tools-issue1363";
const MCP_RENDER_COLLECTION: &str = "litd_mcp_render_tools";
const MCP_RENDER_STATE_ROW: &[u8] = b"render/state/latest";
const MCP_RENDER_LEDGER_ROW: &[u8] = b"render/ledger/latest";
const MCP_RENDER_STATE_COLUMN: &[u8] = b"state_json";
const MCP_RENDER_LEDGER_COLUMN: &[u8] = b"ledger_json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpRenderState {
    pub schema: String,
    pub issue: u32,
    pub state_id: String,
    pub source_of_truth: String,
    pub update_count: u64,
    pub materials: Vec<RenderMaterialBinding>,
    pub lighting: Option<RenderLightingPlan>,
    pub animation: Option<RenderAnimationBinding>,
    pub post_process: Option<litd_engine::render::post::PostProcessSettings>,
    pub scene: Option<RenderSceneComposition>,
    #[serde(default)]
    pub shader_gallery: Option<StylizedGallerySelection>,
    #[serde(default)]
    pub primitive_gallery: Option<PrimitiveGallerySelection>,
    #[serde(default)]
    pub sprite_billboard_library: Option<SpriteBillboardLibrarySelection>,
    #[serde(default)]
    pub hud_ui_builder: Option<HudUiBuilderSelection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpRenderStateReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub state: McpRenderState,
    pub material_count: usize,
    pub light_count: usize,
    pub animation_clip_count: usize,
    pub scene_layer_count: usize,
    pub scene_placement_count: usize,
    pub scene_parented_placement_count: usize,
    pub material_assignment_count: usize,
    pub shader_gallery_material_count: usize,
    pub shader_gallery_selected_preset: Option<String>,
    pub shader_gallery_custom_wgsl_id: Option<String>,
    pub primitive_gallery_count: usize,
    pub primitive_gallery_selected_id: Option<String>,
    pub primitive_gallery_topology_ids: Vec<String>,
    pub primitive_gallery_total_vertex_count: usize,
    pub primitive_gallery_total_index_count: usize,
    pub sprite_billboard_library: Option<SpriteBillboardLibraryReadback>,
    pub hud_ui_builder: Option<HudUiBuilderReadback>,
    pub hud_ui_builder_widget_count: usize,
    pub hud_ui_builder_text_run_count: usize,
    pub hud_ui_builder_pass_count: usize,
}

impl McpRenderState {
    pub fn new(state_id: impl Into<String>) -> Result<Self, EngineError> {
        let state = Self {
            schema: MCP_RENDER_STATE_SCHEMA.to_string(),
            issue: 923,
            state_id: state_id.into(),
            source_of_truth: "Calyx Aster MCP render state row over Rust/wgpu inputs".to_string(),
            update_count: 0,
            materials: Vec::new(),
            lighting: None,
            animation: None,
            post_process: None,
            scene: None,
            shader_gallery: None,
            primitive_gallery: None,
            sprite_billboard_library: None,
            hud_ui_builder: None,
        };
        validate_state_identity(&state)?;
        Ok(state)
    }

    pub fn upsert_material(&mut self, binding: RenderMaterialBinding) -> Result<(), EngineError> {
        validate_material_binding(&binding)?;
        if let Some(existing) = self
            .materials
            .iter_mut()
            .find(|existing| existing.material_id == binding.material_id)
        {
            *existing = binding;
        } else {
            self.materials.push(binding);
        }
        self.materials
            .sort_by(|left, right| left.material_id.cmp(&right.material_id));
        self.update_count += 1;
        Ok(())
    }

    pub fn set_lighting_animation_post(
        &mut self,
        lighting: RenderLightingPlan,
        animation: RenderAnimationBinding,
        post_process: litd_engine::render::post::PostProcessSettings,
    ) {
        self.lighting = Some(lighting);
        self.animation = Some(animation);
        self.post_process = Some(post_process);
        self.update_count += 1;
    }

    pub fn set_scene_composition(&mut self, scene: RenderSceneComposition) {
        self.scene = Some(scene);
        self.update_count += 1;
    }

    pub fn set_shader_gallery(&mut self, gallery: StylizedGallerySelection) {
        self.shader_gallery = Some(gallery);
        self.update_count += 1;
    }

    pub fn set_primitive_gallery(&mut self, gallery: PrimitiveGallerySelection) {
        self.primitive_gallery = Some(gallery);
        self.update_count += 1;
    }

    pub fn set_sprite_billboard_library(&mut self, library: SpriteBillboardLibrarySelection) {
        self.sprite_billboard_library = Some(library);
        self.update_count += 1;
    }

    pub fn set_hud_ui_builder(&mut self, selection: HudUiBuilderSelection) {
        self.hud_ui_builder = Some(selection);
        self.update_count += 1;
    }
}

pub fn load_or_create_mcp_render_state(
    path: impl AsRef<Path>,
    state_id: impl Into<String>,
) -> Result<McpRenderState, EngineError> {
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let state_id = state_id.into();
    validate_single_line("state_id", &state_id)?;
    if path.exists() {
        let state = read_mcp_render_state(path)?.state;
        if state.state_id != state_id {
            return Err(store_error(
                "state_id",
                format!(
                    "{} contains state_id={} but request specified {state_id}",
                    path.display(),
                    state.state_id
                ),
            ));
        }
        Ok(state)
    } else {
        McpRenderState::new(state_id)
    }
}

pub fn write_mcp_render_state(
    path: impl AsRef<Path>,
    state: &McpRenderState,
) -> Result<McpRenderStateReadback, EngineError> {
    validate_mcp_render_state(state)?;
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(state).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-render-state-json",
            reason: format!("serialize render MCP state: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&render_state_spec(path), &bytes)
        .map_err(|error| store_error("state.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "state.write",
            "Aster render state bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_render_state(path)?;
    if readback.state != *state {
        return Err(store_error(
            "state.readback",
            format!("{} readback did not match written state", path.display()),
        ));
    }
    Ok(readback)
}

pub fn read_mcp_render_state(
    path: impl AsRef<Path>,
) -> Result<McpRenderStateReadback, EngineError> {
    let path = path.as_ref();
    validate_path("state_path", path)?;
    let readback = read_calyx_json_bytes(&render_state_spec(path))
        .map_err(|error| store_error("state.read", error))?
        .ok_or_else(|| {
            store_error(
                "state.read",
                format!("missing Calyx Aster render state row at {}", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let state: McpRenderState =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-render-state-json",
            reason: format!("parse render MCP state {}: {error}", path.display()),
        })?;
    validate_mcp_render_state(&state)?;
    let primitive_gallery = state
        .primitive_gallery
        .as_ref()
        .map(build_primitive_gallery_readback)
        .transpose()?;
    let sprite_billboard_library = state
        .sprite_billboard_library
        .as_ref()
        .map(build_sprite_billboard_library_readback)
        .transpose()?;
    let hud_ui_builder = state
        .hud_ui_builder
        .as_ref()
        .map(build_hud_ui_builder_readback)
        .transpose()?;
    Ok(McpRenderStateReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        material_count: state.materials.len(),
        light_count: state
            .lighting
            .as_ref()
            .map(|plan| plan.lights.len())
            .unwrap_or(0),
        animation_clip_count: state
            .animation
            .as_ref()
            .map(|animation| animation.clips.len())
            .unwrap_or(0),
        scene_layer_count: state
            .scene
            .as_ref()
            .map(|scene| scene.layers.len())
            .unwrap_or(0),
        scene_placement_count: state
            .scene
            .as_ref()
            .map(|scene| scene.placements.len())
            .unwrap_or(0),
        scene_parented_placement_count: state
            .scene
            .as_ref()
            .map(|scene| {
                scene
                    .placements
                    .iter()
                    .filter(|placement| placement.parent_placement_id.is_some())
                    .count()
            })
            .unwrap_or(0),
        material_assignment_count: state
            .scene
            .as_ref()
            .map(|scene| scene.material_assignments.len())
            .unwrap_or(0),
        shader_gallery_material_count: state
            .shader_gallery
            .as_ref()
            .map(|gallery| gallery.materials.len())
            .unwrap_or(0),
        shader_gallery_selected_preset: state
            .shader_gallery
            .as_ref()
            .map(|gallery| gallery.selected_preset.id().to_string()),
        shader_gallery_custom_wgsl_id: state
            .shader_gallery
            .as_ref()
            .and_then(|gallery| gallery.custom_wgsl.as_ref())
            .map(|custom| custom.shader_id.clone()),
        primitive_gallery_count: primitive_gallery
            .as_ref()
            .map(|gallery| gallery.primitive_count)
            .unwrap_or(0),
        primitive_gallery_selected_id: primitive_gallery
            .as_ref()
            .map(|gallery| gallery.selected_primitive_id.clone()),
        primitive_gallery_topology_ids: primitive_gallery
            .as_ref()
            .map(|gallery| gallery.topology_ids.clone())
            .unwrap_or_default(),
        primitive_gallery_total_vertex_count: primitive_gallery
            .as_ref()
            .map(|gallery| gallery.total_vertex_count)
            .unwrap_or(0),
        primitive_gallery_total_index_count: primitive_gallery
            .as_ref()
            .map(|gallery| gallery.total_index_count)
            .unwrap_or(0),
        sprite_billboard_library,
        hud_ui_builder_widget_count: hud_ui_builder
            .as_ref()
            .map(|readback| readback.widget_count)
            .unwrap_or(0),
        hud_ui_builder_text_run_count: hud_ui_builder
            .as_ref()
            .map(|readback| readback.text_run_count)
            .unwrap_or(0),
        hud_ui_builder_pass_count: hud_ui_builder
            .as_ref()
            .map(|readback| readback.pass_count)
            .unwrap_or(0),
        hud_ui_builder,
        state,
    })
}

pub fn write_mcp_render_tool_ledger(
    path: impl AsRef<Path>,
    ledger_id: impl Into<String>,
    record: McpRenderToolRecord,
) -> Result<McpRenderToolLedgerReadback, EngineError> {
    let ledger = build_mcp_render_tool_ledger(ledger_id, vec![record])?;
    validate_mcp_render_tool_ledger(&ledger)?;
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let bytes =
        serde_json::to_vec_pretty(&ledger).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-render-ledger-json",
            reason: format!("serialize render MCP ledger: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&render_ledger_spec(path), &bytes)
        .map_err(|error| store_error("ledger.write", error))?;
    if stored.bytes != bytes {
        return Err(store_error(
            "ledger.write",
            "Aster render ledger bytes did not match serialized bytes",
        ));
    }
    let readback = read_mcp_render_tool_ledger(path)?;
    if readback.ledger != ledger {
        return Err(store_error(
            "ledger.readback",
            format!("{} readback did not match written ledger", path.display()),
        ));
    }
    Ok(readback)
}

pub fn read_mcp_render_tool_ledger(
    path: impl AsRef<Path>,
) -> Result<McpRenderToolLedgerReadback, EngineError> {
    let path = path.as_ref();
    validate_path("ledger_path", path)?;
    let readback = read_calyx_json_bytes(&render_ledger_spec(path))
        .map_err(|error| store_error("ledger.read", error))?
        .ok_or_else(|| {
            store_error(
                "ledger.read",
                format!(
                    "missing Calyx Aster render ledger row at {}",
                    path.display()
                ),
            )
        })?;
    let bytes = readback.bytes;
    let ledger: McpRenderToolLedger =
        serde_json::from_slice(&bytes).map_err(|error| EngineError::SerializationFailed {
            format: "mcp-render-ledger-json",
            reason: format!("parse render MCP ledger {}: {error}", path.display()),
        })?;
    validate_mcp_render_tool_ledger(&ledger)?;
    Ok(McpRenderToolLedgerReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        ledger,
    })
}

pub fn validate_mcp_render_state(state: &McpRenderState) -> Result<(), EngineError> {
    validate_state_identity(state)?;
    if state.update_count == 0
        && (state.materials.is_empty()
            || state.lighting.is_some()
            || state.animation.is_some()
            || state.post_process.is_some()
            || state.scene.is_some()
            || state.hud_ui_builder.is_some())
    {
        return Err(store_error(
            "state.update_count",
            "new states must be empty until the first verified mutation",
        ));
    }
    for material in &state.materials {
        validate_material_binding(material)?;
    }
    if let Some(lighting) = &state.lighting {
        validate_lighting_plan(lighting)?;
    }
    if let Some(animation) = &state.animation {
        validate_animation_binding(animation)?;
    }
    if let Some(post) = state.post_process {
        post.validate()?;
    }
    if let Some(scene) = &state.scene {
        validate_scene_composition(scene, state)?;
    }
    if let Some(gallery) = &state.shader_gallery {
        let library = build_stylized_shader_library(ShaderSystemSettings::default())?;
        litd_engine::render::stylized::build_stylized_gallery_readback(
            ShaderSystemSettings::default(),
            &library,
            gallery,
        )?;
    }
    if let Some(gallery) = &state.primitive_gallery {
        build_primitive_gallery_readback(gallery)?;
    }
    if let Some(library) = &state.sprite_billboard_library {
        build_sprite_billboard_library_readback(library)?;
    }
    if let Some(selection) = &state.hud_ui_builder {
        build_hud_ui_builder_readback(selection)?;
    }
    Ok(())
}
