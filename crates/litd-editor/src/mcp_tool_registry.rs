//! Unified external MCP registry for callable editor tools.

use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::Serialize;
use serde_json::Value;

use crate::{
    material_editor_mcp::{
        AUTHOR_PBR_MATERIAL_TOOL, READ_PBR_MATERIAL_TOOL, call_mcp_material_tool,
    },
    mcp_agent_tools::{
        AUTHOR_AGENT_TOOL, BIND_AGENT_TOOL, READ_AGENTS_TOOL, SET_PROMPT_MODEL_TOOL,
        SET_WARD_ENVELOPE_TOOL, call_mcp_agent_tool,
    },
    mcp_asset_tools::{
        CONFORM_INGEST_BUDGET_TOOL, GENERATE_IMPORT_ASSET_TOOL, READ_ASSETS_TOOL,
        RIG_TEXTURE_ANIMATE_TOOL, call_mcp_asset_tool,
    },
    mcp_cinematic_tools::{AUTHOR_TIMELINE_TOOL, EVALUATE_TIMELINE_TOOL, call_mcp_cinematic_tool},
    mcp_content_tools::{AUTHOR_RACE_TOOL, call_mcp_content_tool},
    mcp_editor_session_tools::{
        APPLY_DOCUMENT_EDIT_TOOL, AUTOSAVE_TOOL, COPY_SELECTION_TOOL, EXECUTE_COMMAND_TOOL,
        PASTE_CLIPBOARD_TOOL, READ_SESSION_TOOL, REDO_TOOL, UNDO_TOOL,
        call_mcp_editor_session_tool,
    },
    mcp_fsv_tools::{
        READ_OBJECT_STATE_TOOL, READ_PROJECT_ARTIFACTS_TOOL, READ_STORY_RENDER_STATE_TOOL,
        READ_TRIGGER_RUNTIME_STATE_TOOL, READ_WORLD_STATE_TOOL, call_mcp_fsv_tool,
    },
    mcp_harness_tools::{
        ASSERT_STATE_TOOL, CALYX_SIGNALS_TOOL, DRIVE_BOT_TOOL, EMIT_FEEDBACK_TOOL,
        READ_EVENTS_TOOL, READ_HARNESS_TOOL, RUN_HEADLESS_TOOL, RUN_ORACLES_TOOL, RUN_PEV_TOOL,
        RUN_SANDBOXES_TOOL, SCREENSHOT_DIFF_TOOL, SNAPSHOT_BRANCH_TOOL, call_mcp_harness_tool,
    },
    mcp_object_tools::{
        APPLY_OBJECT_DELTA_TOOL, COPY_BASE_OBJECT_TOOL, LOWER_ABILITY_TOOL, LOWER_BUFF_TOOL,
        LOWER_DESTRUCTIBLE_DOODAD_TOOL, LOWER_GAME_INTERFACE_TOOL, LOWER_GAMEPLAY_CONSTANTS_TOOL,
        LOWER_HERO_TOOL, LOWER_ITEM_TOOL, LOWER_SHOP_TOOL, LOWER_TECH_REQUIREMENTS_TOOL,
        LOWER_UNIT_TOOL, LOWER_UPGRADE_TOOL, READ_LOWERED_ABILITY_TOOL, READ_LOWERED_BUFF_TOOL,
        READ_LOWERED_DESTRUCTIBLE_DOODAD_TOOL, READ_LOWERED_GAME_INTERFACE_TOOL,
        READ_LOWERED_GAMEPLAY_CONSTANTS_TOOL, READ_LOWERED_HERO_TOOL, READ_LOWERED_ITEM_TOOL,
        READ_LOWERED_SHOP_TOOL, READ_LOWERED_TECH_REQUIREMENTS_TOOL, READ_LOWERED_UNIT_TOOL,
        READ_LOWERED_UPGRADE_TOOL, READ_OBJECT_DATA_TOOL, UPSERT_GAMEPLAY_TABLES_TOOL,
        call_mcp_object_tool,
    },
    mcp_project_tools::{
        AUTHOR_CAMPAIGN_CHAPTER_TOOL, AUTHOR_CAMPAIGN_TOOL, CONTENT_TRUST_TOOL,
        DELIVERY_LITDGAME_TOOL, GAME_UPDATES_TOOL, HUB_LAUNCHER_TOOL, MODERATION_SAFETY_TOOL,
        MULTIPLAYER_SESSION_TOOL, NEW_OPEN_SAVE_TOOL, PACKAGE_LITDGAME_TOOL, PUBLISH_LITDGAME_TOOL,
        RANKINGS_REVIEWS_TOOL, READ_CAMPAIGN_TOOL, READ_CONTENT_TRUST_TOOL, READ_PROJECT_TOOL,
        RUN_HEADLESS_PLAYTEST_TOOL, SERVER_BROWSER_TOOL, SOCIAL_GRAPH_TOOL, call_mcp_project_tool,
    },
    mcp_render_tools::{
        COMPOSE_SCENE_TOOL, READ_RENDER_TOOL, SET_HUD_UI_BUILDER_TOOL,
        SET_LIGHTING_ANIMATION_POST_TOOL, SET_PBR_MATERIAL_TOOL, SET_PRIMITIVE_GALLERY_TOOL,
        SET_SHADER_GALLERY_TOOL, SET_SPRITE_BILLBOARD_LIBRARY_TOOL, call_mcp_render_tool,
    },
    mcp_story_tools::{
        BUILD_LORE_RELATIONSHIP_GRAPH_TOOL, CO_WRITE_LORE_TOOL, GENERATE_LORE_TEMPLATES_TOOL,
        QUERY_LORE_CANON_TOOL, READ_STORY_TOOL, STAGE_CINEMATIC_TOOL, UPSERT_DIALOGUE_QUEST_TOOL,
        UPSERT_LORE_CONTEXT_TOOL, UPSERT_LORE_ENTRY_TOOL, UPSERT_LORE_SURFACE_TOOL,
        call_mcp_story_tool,
    },
    mcp_template_tools::{CLONE_TEMPLATE_TOOL, LIST_TEMPLATES_TOOL, call_mcp_template_tool},
    mcp_tools::{McpDomainTool, McpDomainToolsetManifest, default_mcp_domain_toolset_manifest},
    mcp_trigger_tools::{
        IMPORT_SOUND_TOOL, PREVIEW_SOUND_TOOL, QUERY_CROSS_REF_TOOL, READ_SOUND_EDITOR_TOOL,
        READ_TRIGGER_RUNTIME_TOOL, SET_TRIGGER_ORGANIZATION_TOOL, UPSERT_ECA_GRAPH_TOOL,
        UPSERT_RUNTIME_CUE_TOOL, UPSERT_SOUND_VARIABLE_TOOL, UPSERT_VARIABLE_TOOL,
        call_mcp_trigger_tool,
    },
    mcp_vfx_tools::{
        ATTACH_EFFECT_TOOL, DESCRIBE_EFFECT_TOOL, READ_VFX_TOOL, SET_PARTICLE_STACK_TOOL,
        call_mcp_vfx_tool,
    },
    mcp_world_tools::{
        BULK_EDIT_TERRAIN_TOOL, EVALUATE_REGION_TRANSITIONS_TOOL, PAINT_TILES_TOOL,
        PLACE_ACTOR_REGION_CAMERA_TOOL, READ_WORLD_TOOL, SET_START_LOCATIONS_TOOL,
        call_mcp_world_tool,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolRegistry {
    pub manifest: McpDomainToolsetManifest,
    pub entries: Vec<McpToolRegistryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolRegistryEntry {
    pub domain_id: String,
    pub tool: McpDomainTool,
}

pub fn default_mcp_tool_registry() -> Result<McpToolRegistry, EngineError> {
    let manifest = default_mcp_domain_toolset_manifest()?;
    let implemented = implemented_mcp_tool_names();
    validate_mcp_tool_dispatch_surface(&manifest, &implemented)?;
    let entries = manifest
        .domains
        .iter()
        .flat_map(|domain| {
            domain
                .tools
                .iter()
                .cloned()
                .map(|tool| McpToolRegistryEntry {
                    domain_id: domain.domain_id.clone(),
                    tool,
                })
        })
        .collect();
    Ok(McpToolRegistry { manifest, entries })
}

pub fn dispatch_mcp_tool_call(
    registry: &McpToolRegistry,
    tool_name: &str,
    arguments: Value,
) -> Result<Value, EngineError> {
    let entry = registry.entry(tool_name)?;
    // The #1455 content-authoring tool lives in the object_game_data domain (capability namespace
    // editor.rpg) but has its own handler that writes the canonical Calyx content vault, so it is
    // routed before the generic per-domain fan-out.
    if tool_name == AUTHOR_RACE_TOOL {
        return serialize_dispatch(call_mcp_content_tool(tool_name, arguments)?);
    }
    // The #862/#915 no-code PBR material authoring pair sits in the render_materials domain (it is a
    // parallel PBR path to engine.render.set_pbr_material) but has its own handler that authors the
    // canonical material through the real Calyx put_content vault, so it is routed before the generic
    // render fan-out that would otherwise hand it to call_mcp_render_tool.
    if tool_name == AUTHOR_PBR_MATERIAL_TOOL || tool_name == READ_PBR_MATERIAL_TOOL {
        return serialize_dispatch(call_mcp_material_tool(tool_name, arguments)?);
    }
    // #892/#901: cinematic + template tools live in the terrain_world catalog (capability
    // namespace editor.world) but have dedicated handlers, so they route before the generic
    // per-domain fan-out (the same pattern as the #1455 author_race tool above).
    if tool_name == AUTHOR_TIMELINE_TOOL || tool_name == EVALUATE_TIMELINE_TOOL {
        return serialize_dispatch(call_mcp_cinematic_tool(tool_name, arguments)?);
    }
    if tool_name == LIST_TEMPLATES_TOOL || tool_name == CLONE_TEMPLATE_TOOL {
        return serialize_dispatch(call_mcp_template_tool(tool_name, arguments)?);
    }
    match entry.domain_id.as_str() {
        "terrain_world" => serialize_dispatch(call_mcp_world_tool(tool_name, arguments)?),
        "object_game_data" => serialize_dispatch(call_mcp_object_tool(tool_name, arguments)?),
        "trigger_runtime" => serialize_dispatch(call_mcp_trigger_tool(tool_name, arguments)?),
        "visual_story" => serialize_dispatch(call_mcp_story_tool(tool_name, arguments)?),
        "vfx_particles" => serialize_dispatch(call_mcp_vfx_tool(tool_name, arguments)?),
        "render_materials" => serialize_dispatch(call_mcp_render_tool(tool_name, arguments)?),
        "in_game_agents" => serialize_dispatch(call_mcp_agent_tool(tool_name, arguments)?),
        "asset_pipeline" => serialize_dispatch(call_mcp_asset_tool(tool_name, arguments)?),
        "editor_session" => serialize_dispatch(call_mcp_editor_session_tool(tool_name, arguments)?),
        "project_playtest" => serialize_dispatch(call_mcp_project_tool(tool_name, arguments)?),
        "fsv_introspection" => serialize_dispatch(call_mcp_fsv_tool(tool_name, arguments)?),
        "agent_harness" => serialize_dispatch(call_mcp_harness_tool(tool_name, arguments)?),
        other => Err(invalid_mcp_tool_registry(
            "domain_id",
            format!("{other} has no external MCP dispatcher"),
        )),
    }
}

pub fn validate_mcp_tool_dispatch_surface(
    manifest: &McpDomainToolsetManifest,
    implemented_names: &[String],
) -> Result<(), EngineError> {
    let expected = manifest
        .domains
        .iter()
        .flat_map(|domain| domain.tools.iter().map(|tool| tool.tool_name.clone()))
        .collect::<BTreeSet<_>>();
    let mut actual = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for name in implemented_names {
        if !actual.insert(name.clone()) {
            duplicates.insert(name.clone());
        }
    }
    if !duplicates.is_empty() {
        return Err(invalid_mcp_tool_registry(
            "implemented_names",
            format!(
                "duplicate dispatcher tool names: {}",
                join_names(&duplicates)
            ),
        ));
    }
    let missing = expected
        .difference(&actual)
        .cloned()
        .collect::<BTreeSet<_>>();
    let extra = actual
        .difference(&expected)
        .cloned()
        .collect::<BTreeSet<_>>();
    if !missing.is_empty() || !extra.is_empty() {
        return Err(invalid_mcp_tool_registry(
            "dispatch_surface",
            format!(
                "catalog and dispatcher tool names differ; missing_in_dispatch=[{}]; extra_in_dispatch=[{}]",
                join_names(&missing),
                join_names(&extra)
            ),
        ));
    }
    Ok(())
}

pub fn implemented_mcp_tool_names() -> Vec<String> {
    [
        PAINT_TILES_TOOL,
        BULK_EDIT_TERRAIN_TOOL,
        PLACE_ACTOR_REGION_CAMERA_TOOL,
        SET_START_LOCATIONS_TOOL,
        READ_WORLD_TOOL,
        EVALUATE_REGION_TRANSITIONS_TOOL,
        AUTHOR_TIMELINE_TOOL,
        EVALUATE_TIMELINE_TOOL,
        LIST_TEMPLATES_TOOL,
        CLONE_TEMPLATE_TOOL,
        COPY_BASE_OBJECT_TOOL,
        APPLY_OBJECT_DELTA_TOOL,
        UPSERT_GAMEPLAY_TABLES_TOOL,
        AUTHOR_RACE_TOOL,
        READ_OBJECT_DATA_TOOL,
        LOWER_UNIT_TOOL,
        READ_LOWERED_UNIT_TOOL,
        LOWER_ABILITY_TOOL,
        READ_LOWERED_ABILITY_TOOL,
        LOWER_HERO_TOOL,
        READ_LOWERED_HERO_TOOL,
        LOWER_ITEM_TOOL,
        READ_LOWERED_ITEM_TOOL,
        LOWER_SHOP_TOOL,
        READ_LOWERED_SHOP_TOOL,
        LOWER_TECH_REQUIREMENTS_TOOL,
        READ_LOWERED_TECH_REQUIREMENTS_TOOL,
        LOWER_GAMEPLAY_CONSTANTS_TOOL,
        READ_LOWERED_GAMEPLAY_CONSTANTS_TOOL,
        LOWER_BUFF_TOOL,
        READ_LOWERED_BUFF_TOOL,
        LOWER_UPGRADE_TOOL,
        READ_LOWERED_UPGRADE_TOOL,
        LOWER_DESTRUCTIBLE_DOODAD_TOOL,
        READ_LOWERED_DESTRUCTIBLE_DOODAD_TOOL,
        LOWER_GAME_INTERFACE_TOOL,
        READ_LOWERED_GAME_INTERFACE_TOOL,
        UPSERT_VARIABLE_TOOL,
        UPSERT_ECA_GRAPH_TOOL,
        UPSERT_RUNTIME_CUE_TOOL,
        IMPORT_SOUND_TOOL,
        UPSERT_SOUND_VARIABLE_TOOL,
        PREVIEW_SOUND_TOOL,
        READ_TRIGGER_RUNTIME_TOOL,
        READ_SOUND_EDITOR_TOOL,
        SET_TRIGGER_ORGANIZATION_TOOL,
        QUERY_CROSS_REF_TOOL,
        UPSERT_DIALOGUE_QUEST_TOOL,
        STAGE_CINEMATIC_TOOL,
        UPSERT_LORE_CONTEXT_TOOL,
        UPSERT_LORE_ENTRY_TOOL,
        BUILD_LORE_RELATIONSHIP_GRAPH_TOOL,
        UPSERT_LORE_SURFACE_TOOL,
        QUERY_LORE_CANON_TOOL,
        CO_WRITE_LORE_TOOL,
        GENERATE_LORE_TEMPLATES_TOOL,
        READ_STORY_TOOL,
        DESCRIBE_EFFECT_TOOL,
        SET_PARTICLE_STACK_TOOL,
        ATTACH_EFFECT_TOOL,
        READ_VFX_TOOL,
        SET_PBR_MATERIAL_TOOL,
        AUTHOR_PBR_MATERIAL_TOOL,
        READ_PBR_MATERIAL_TOOL,
        SET_LIGHTING_ANIMATION_POST_TOOL,
        COMPOSE_SCENE_TOOL,
        SET_SHADER_GALLERY_TOOL,
        SET_PRIMITIVE_GALLERY_TOOL,
        SET_SPRITE_BILLBOARD_LIBRARY_TOOL,
        SET_HUD_UI_BUILDER_TOOL,
        READ_RENDER_TOOL,
        AUTHOR_AGENT_TOOL,
        BIND_AGENT_TOOL,
        SET_PROMPT_MODEL_TOOL,
        SET_WARD_ENVELOPE_TOOL,
        READ_AGENTS_TOOL,
        GENERATE_IMPORT_ASSET_TOOL,
        RIG_TEXTURE_ANIMATE_TOOL,
        CONFORM_INGEST_BUDGET_TOOL,
        READ_ASSETS_TOOL,
        APPLY_DOCUMENT_EDIT_TOOL,
        UNDO_TOOL,
        REDO_TOOL,
        AUTOSAVE_TOOL,
        COPY_SELECTION_TOOL,
        PASTE_CLIPBOARD_TOOL,
        EXECUTE_COMMAND_TOOL,
        READ_SESSION_TOOL,
        NEW_OPEN_SAVE_TOOL,
        RUN_HEADLESS_PLAYTEST_TOOL,
        PACKAGE_LITDGAME_TOOL,
        PUBLISH_LITDGAME_TOOL,
        DELIVERY_LITDGAME_TOOL,
        HUB_LAUNCHER_TOOL,
        MULTIPLAYER_SESSION_TOOL,
        SOCIAL_GRAPH_TOOL,
        RANKINGS_REVIEWS_TOOL,
        GAME_UPDATES_TOOL,
        MODERATION_SAFETY_TOOL,
        CONTENT_TRUST_TOOL,
        READ_CONTENT_TRUST_TOOL,
        SERVER_BROWSER_TOOL,
        AUTHOR_CAMPAIGN_TOOL,
        AUTHOR_CAMPAIGN_CHAPTER_TOOL,
        READ_CAMPAIGN_TOOL,
        READ_PROJECT_TOOL,
        READ_WORLD_STATE_TOOL,
        READ_OBJECT_STATE_TOOL,
        READ_TRIGGER_RUNTIME_STATE_TOOL,
        READ_STORY_RENDER_STATE_TOOL,
        READ_PROJECT_ARTIFACTS_TOOL,
        RUN_HEADLESS_TOOL,
        RUN_SANDBOXES_TOOL,
        SNAPSHOT_BRANCH_TOOL,
        ASSERT_STATE_TOOL,
        READ_EVENTS_TOOL,
        RUN_ORACLES_TOOL,
        DRIVE_BOT_TOOL,
        SCREENSHOT_DIFF_TOOL,
        CALYX_SIGNALS_TOOL,
        EMIT_FEEDBACK_TOOL,
        RUN_PEV_TOOL,
        READ_HARNESS_TOOL,
    ]
    .iter()
    .map(|name| (*name).to_string())
    .collect()
}

impl McpToolRegistry {
    pub fn entry(&self, tool_name: &str) -> Result<&McpToolRegistryEntry, EngineError> {
        self.entries
            .iter()
            .find(|entry| entry.tool.tool_name == tool_name)
            .ok_or_else(|| {
                invalid_mcp_tool_registry(
                    "tool_name",
                    format!("{tool_name} is not in the callable MCP registry"),
                )
            })
    }

    pub fn domain_counts(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for entry in &self.entries {
            *counts.entry(entry.domain_id.clone()).or_insert(0) += 1;
        }
        counts
    }
}

fn serialize_dispatch<T: Serialize>(value: T) -> Result<Value, EngineError> {
    serde_json::to_value(value).map_err(|error| EngineError::SerializationFailed {
        format: "mcp-tool-dispatch-json",
        reason: error.to_string(),
    })
}

fn join_names(names: &BTreeSet<String>) -> String {
    names.iter().cloned().collect::<Vec<_>>().join(",")
}

fn invalid_mcp_tool_registry(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let field = format!("editor.mcp_tool_registry.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::mcp_tool_registry",
        field = %field,
        reason = %reason,
        "invalid external MCP tool registry"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

#[cfg(test)]
#[path = "mcp_tool_registry_tests.rs"]
mod tests;
