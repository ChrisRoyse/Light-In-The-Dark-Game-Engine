//! Shared editor shell/library code for the `litd-editor` binary and FSV tools.

pub mod adaptive_story_audio;
pub mod agent_latency_budget;
pub mod agent_object_binding;
pub mod agent_skills;
pub mod animation_catalog;
pub mod asset_constellations;
pub mod asset_floor_selection;
pub mod asset_lod;
pub mod asset_manifest;
pub mod asset_placement;
pub mod asset_previewer;
pub mod asset_products;
pub mod asset_scene;
mod asset_scene_composition;
pub mod asset_scene_foliage_brush;
pub mod asset_scene_foliage_config;
mod asset_scene_geometry;
mod asset_scene_texture;
pub mod asset_store;
pub mod asset_vault;
pub mod authoring;
pub mod authoring_dedup_governance;
pub mod calyx_probe_cache;
pub mod campaign;
pub mod capabilities;
pub mod cinematic_authoring;
pub mod clipboard;
pub mod computer_opponents;
pub mod content_trust;
pub mod contextual_help;
pub mod edit_from_play;
pub mod editor_commands;
pub mod editor_intelligence_surfaces;
pub mod editor_min_spec_store;
pub mod editor_performance;
pub mod editor_preferences;
pub mod editor_state_store;
pub mod fsv_support;
// #1825: shared fail-loud provisioning diagnostics for the real-asset-vault fixture tests.
#[cfg(test)]
mod asset_vault_fixture;
// Test-only canonical scratch-dir helper shared across the editor test suite (#1520).
pub mod first_launch_coach;
pub mod gltf_asset_import;
pub mod gltf_validation;
pub mod history;
pub mod hud_ui_builder;
pub mod human_loop;
pub mod immersive_play;
pub mod in_context_feedback;
pub mod instant_play;
pub mod live_hot_reload;
pub mod localization;
pub mod map_build;
pub mod material_editor;
pub mod material_editor_mcp;
pub mod material_editor_panel;
pub mod mcp_agent_tools;
pub mod mcp_asset_tools;
pub mod mcp_cinematic_tools;
pub mod mcp_content_tools;
pub mod mcp_editor_session_tools;
pub mod mcp_fsv_tools;
pub mod mcp_harness_tools;
pub mod mcp_loopback;
pub mod mcp_object_tools;
pub mod mcp_procgen_tools;
pub mod mcp_project_tools;
pub mod mcp_render_tools;
pub mod mcp_story_tools;
pub mod mcp_template_tools;
pub mod mcp_tool_registry;
pub mod mcp_tools;
pub mod mcp_trigger_tools;
pub mod mcp_vfx_tools;
pub mod mcp_world_tools;
pub mod modules;
pub mod object_editor;
pub mod object_editor_inspector;
pub mod object_manager;
pub mod offline_degradation;
pub mod panels;
pub mod placement_brush;
pub mod play;
pub mod play_responsiveness;
pub mod play_session_capture;
pub mod prefabs;
pub mod quick_share;
pub mod region_transitions;
pub mod render_budget_governance;
mod row_safe_ledger_id;
pub mod runtime_datasets_lowering;
pub mod scenario;
pub mod scenario_availability;
pub mod scene_composition;
pub mod schema_forms;
pub mod semantic_authoring_ops;
pub mod shell;
pub mod sound_editor;
pub mod spatial_audio_emitters;
pub mod starter_templates;
pub mod structured_capability_coverage;
pub mod template_gallery;
pub mod temporal_content;
pub mod terrain_advanced_tools;
pub mod terrain_authoring;
pub mod terrain_bulk_ops;
mod terrain_bulk_types;
pub mod terrain_heightmap;
pub mod terrain_lowering;
pub mod terrain_pathing;
pub mod terrain_variation;
#[cfg(test)]
mod test_scratch;
pub mod texture_import;
pub mod tower_defense_fsv;
pub mod trigger_action_library;
pub mod trigger_condition_library;
pub mod trigger_cross_ref;
pub mod trigger_editor;
pub mod trigger_event_catalog;
pub mod trigger_graph_ron_cache;
pub mod trigger_organization_lowering;
pub mod trigger_runtime_lowering;
pub mod trigger_wasm_authoring;
pub mod user_journey_audit;
pub mod vfx_editor;
pub mod vfx_library;
pub mod vfx_text_generation;
pub mod viewport_transform;
pub mod wayfinding_authoring;
pub mod world_content_coverage;
pub mod world_creation;
pub mod world_graph_navigation;
pub mod world_memory;
pub mod world_validation;

#[cfg(test)]
mod mcp_project_social_test_support;

#[cfg(test)]
#[path = "issue1359_calyx_state_tests.rs"]
mod issue1359_calyx_state_tests;

#[cfg(test)]
#[path = "issue1360_calyx_state_tests.rs"]
mod issue1360_calyx_state_tests;

#[cfg(test)]
#[path = "issue1362_calyx_state_tests.rs"]
mod issue1362_calyx_state_tests;

#[cfg(test)]
#[path = "ashen_reach_cartography_tests.rs"]
mod ashen_reach_cartography_tests;

#[cfg(test)]
#[path = "issue1363_side_store_tests.rs"]
mod issue1363_side_store_tests;

#[cfg(test)]
#[path = "issue1429_row_safe_ledger_id_tests.rs"]
mod issue1429_row_safe_ledger_id_tests;

#[cfg(test)]
#[path = "issue1490_project_ledger_append_tests.rs"]
mod issue1490_project_ledger_append_tests;

#[cfg(test)]
#[path = "issue1490_publish_atomicity_tests.rs"]
mod issue1490_publish_atomicity_tests;

#[cfg(test)]
#[path = "issue1521_sibling_ledger_append_tests.rs"]
mod issue1521_sibling_ledger_append_tests;

#[cfg(test)]
#[path = "vfx_end_to_end_tests.rs"]
mod vfx_end_to_end_tests;

#[cfg(test)]
mod trigger_wasm_authoring_tests;
