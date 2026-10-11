//! `litd-engine` — the Light in the Dark engine core (built on Calyx).
//!
//! This is the reusable engine library: rendering, sim, assets, etc. The editor and
//! the game runner are thin binaries on top of it (dependency direction never reverses).

// Let this crate refer to itself as `litd_engine`, so a single authoritative RPG test
// fixture can `use litd_engine::rpg::...` and resolve identically whether it is compiled
// as part of the lib's test build OR textually `include!`d into an external FSV example.
// This is what lets `examples/fixtures/*` be one-line `include!` shims of the one source
// in `src/rpg/*_test_fixture.rs` instead of hand-synced byte-for-byte copies (#1502).
extern crate self as litd_engine;

macro_rules! full_modules {
    ($($module:item)*) => { $(#[cfg(feature = "full")] $module)* };
}

full_modules! {
    pub mod agent_response;
    pub mod agent_response_corpus;
    mod agent_response_corpus_support;
    pub mod agent_response_gate;
    mod agent_response_gate_support;
    mod agent_response_gate_types;
    pub mod anneal_optimization;
    pub mod calyx_agent_runtime;
    mod calyx_agent_runtime_support;
    pub mod calyx_ai_addon_gate;
    pub mod calyx_asset_intelligence;
    pub mod calyx_asset_pipeline;
    pub mod calyx_content;
    mod calyx_content_a37_admission;
    mod calyx_content_a37_identity;
    mod calyx_content_a37_multi_anchor;
    mod calyx_content_assay;
    pub mod calyx_content_backfill;
    mod calyx_content_measurement;
    pub mod calyx_content_measurement_absent;
    mod calyx_content_measurement_exec;
    mod calyx_content_measurement_panel;
    pub mod calyx_content_runtime;
    mod calyx_content_vectors;
    pub mod calyx_editor_intelligence;
    mod calyx_editor_intelligence_types;
    pub mod calyx_lowering;
    pub mod calyx_replay_analytics;
    pub mod calyx_replay_analytics_assay;
    pub mod calyx_replay_analytics_desync;
    pub mod calyx_replay_analytics_oracle;
    pub mod calyx_semantic;
    mod calyx_semantic_support;
    pub mod calyx_ward_gate;
    mod calyx_ward_gate_support;
    pub mod calyx_ward_subjects;
    pub mod live_world_forecast;
    pub mod rpg;
    pub mod what_if_preview;
    pub mod world_authoring;
    pub mod world_environment;
    pub mod world_scatter;
}

pub mod ai;
pub mod animation;
pub mod app;
pub mod asset_vault_root;
pub mod audio;
pub mod calyx_backend;
pub mod calyx_json_store;
pub mod calyx_observability;
mod calyx_observability_types;
pub mod calyx_paths;
pub mod calyx_persistence;
pub mod calyx_runtime;
pub mod calyx_world_activation;
mod calyx_world_activation_hash;
mod calyx_world_activation_types;
mod calyx_world_activation_validate;
pub mod cinematic_timeline;
pub mod determinism_boundary;
pub mod diagnostics;
pub mod ecs;
pub mod editor_min_spec;
pub mod error;
pub mod forge_compute;
pub mod frame_alloc;
pub mod game_package;
pub mod game_runtime;
pub mod gpu;
pub mod gpu_adapter;
pub mod input;
pub mod math;
pub mod nav;
pub mod net_authority;
pub mod performance_min_spec;
pub mod performance_quality;
pub mod physics;
pub mod presentation;
pub mod procgen;
pub mod profile;
pub mod reference_scene_quality;
pub mod reflection;
pub mod render;
pub mod scene;
pub mod sim;
pub mod sim_order;
pub mod sim_replay;
pub mod stream;
pub mod two_d_pipeline;
pub mod visual_target_acceptance;
#[cfg(feature = "wasm-script")]
pub mod wasm_script;
#[cfg(feature = "wasm-script")]
mod wasm_script_runtime;
#[cfg(feature = "wasm-script")]
mod wasm_script_validation;
#[cfg(feature = "full")]
pub mod world_destructibles;
#[cfg(feature = "full")]
pub mod world_destructibles_sim;

pub mod fsv_artifacts;
pub mod fsv_deadline;
pub mod fsv_quality_harness;
pub mod test_temp;

#[cfg(test)]
mod agent_response_corpus_tests;
#[cfg(test)]
mod agent_response_gate_tests;
#[cfg(test)]
mod agent_response_tests;
#[cfg(test)]
mod anneal_optimization_tests;
#[cfg(test)]
mod audio_spatial_tests;
#[cfg(test)]
mod audio_tests;
#[cfg(test)]
mod calyx_agent_runtime_tests;
#[cfg(test)]
mod calyx_asset_pipeline_tests;
#[cfg(test)]
mod calyx_content_backfill_tests;
#[cfg(test)]
mod calyx_content_issue1479_tests;
#[cfg(test)]
mod calyx_content_issue1511_tests;
#[cfg(test)]
mod calyx_content_tests;
#[cfg(test)]
mod calyx_editor_intelligence_tests;
#[cfg(test)]
mod calyx_lowering_tests;
#[cfg(test)]
mod calyx_replay_analytics_tests;
#[cfg(test)]
mod calyx_semantic_tests;
#[cfg(test)]
mod calyx_ward_gate_tests;
#[cfg(test)]
mod calyx_world_activation_tests;
#[cfg(test)]
mod determinism_boundary_tests;
#[cfg(test)]
mod ecs_slot_freelist_tests;
#[cfg(test)]
mod forge_compute_tests;
#[cfg(test)]
mod fsv_quality_harness_tests;
#[cfg(test)]
mod game_package_tests;
#[cfg(test)]
#[path = "game_runtime/game_runtime_tests.rs"]
mod game_runtime_tests;
#[cfg(test)]
mod gpu_context_tests;
#[cfg(test)]
mod live_world_forecast_tests;
#[cfg(test)]
mod net_authority_tests;
#[cfg(test)]
mod performance_min_spec_tests;
#[cfg(test)]
mod performance_quality_tests;
#[cfg(test)]
mod physics_tests;
#[cfg(test)]
mod profile_tests;
#[cfg(test)]
mod reference_scene_quality_tests;
#[cfg(test)]
#[path = "rpg/abilities_stealth_tests.rs"]
mod rpg_abilities_stealth_tests;
#[cfg(test)]
#[path = "rpg/abilities_test_fixture.rs"]
mod rpg_abilities_test_fixture;
#[cfg(test)]
#[path = "rpg/abilities_tests.rs"]
mod rpg_abilities_tests;
#[cfg(test)]
#[path = "rpg/advancement_test_fixture.rs"]
mod rpg_advancement_test_fixture;
#[cfg(test)]
#[path = "rpg/advancement_tests.rs"]
mod rpg_advancement_tests;
#[cfg(test)]
#[path = "rpg/area_registry_corpus_tests.rs"]
mod rpg_area_registry_corpus_tests;
#[cfg(test)]
#[path = "rpg/area_reset_repop_corpus_tests.rs"]
mod rpg_area_reset_repop_corpus_tests;
#[cfg(test)]
#[path = "rpg/area_specs_galadon_satellites.rs"]
mod rpg_area_specs_galadon_satellites;
#[cfg(test)]
#[path = "rpg/area_specs_test_fixture.rs"]
mod rpg_area_specs_test_fixture;
#[cfg(test)]
#[path = "rpg/area_specs_tests.rs"]
mod rpg_area_specs_tests;
#[cfg(test)]
#[path = "rpg/area_terrain_acoustics_corpus_tests.rs"]
mod rpg_area_terrain_acoustics_corpus_tests;
#[cfg(test)]
#[path = "rpg/attributes_tests.rs"]
mod rpg_attributes_tests;
#[cfg(test)]
#[path = "rpg/authored_content_tests.rs"]
mod rpg_authored_content_tests;
#[cfg(test)]
#[path = "rpg/cities_thera_batch_1637_1643.rs"]
mod rpg_cities_thera_batch_1637_1643;
#[cfg(test)]
#[path = "rpg/class_access_corpus_tests.rs"]
mod rpg_class_access_corpus_tests;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_anti_paladin.rs"]
mod rpg_class_specs_fixture_anti_paladin;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_assassin.rs"]
mod rpg_class_specs_fixture_assassin;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_bard.rs"]
mod rpg_class_specs_fixture_bard;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_berserker.rs"]
mod rpg_class_specs_fixture_berserker;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_conjurer.rs"]
mod rpg_class_specs_fixture_conjurer;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_druid.rs"]
mod rpg_class_specs_fixture_druid;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_healer.rs"]
mod rpg_class_specs_fixture_healer;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_invoker.rs"]
mod rpg_class_specs_fixture_invoker;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_necromancer.rs"]
mod rpg_class_specs_fixture_necromancer;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_paladin.rs"]
mod rpg_class_specs_fixture_paladin;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_raider.rs"]
mod rpg_class_specs_fixture_raider;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_ranger.rs"]
mod rpg_class_specs_fixture_ranger;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_shaman.rs"]
mod rpg_class_specs_fixture_shaman;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_shapeshifter.rs"]
mod rpg_class_specs_fixture_shapeshifter;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_thief.rs"]
mod rpg_class_specs_fixture_thief;
#[cfg(test)]
#[path = "rpg/class_specs_fixture_transmuter.rs"]
mod rpg_class_specs_fixture_transmuter;
#[cfg(test)]
#[path = "rpg/class_specs_test_fixture.rs"]
mod rpg_class_specs_test_fixture;
#[cfg(test)]
#[path = "rpg/class_specs_tests.rs"]
mod rpg_class_specs_tests;
#[cfg(test)]
#[path = "rpg/class_specs_tests_batch1.rs"]
mod rpg_class_specs_tests_batch1;
#[cfg(test)]
#[path = "rpg/class_specs_tests_batch2.rs"]
mod rpg_class_specs_tests_batch2;
#[cfg(test)]
#[path = "rpg/class_specs_tests_batch3.rs"]
mod rpg_class_specs_tests_batch3;
#[cfg(test)]
#[path = "rpg/class_specs_tests_batch4.rs"]
mod rpg_class_specs_tests_batch4;
#[cfg(test)]
#[path = "rpg/class_specs_tests_support.rs"]
mod rpg_class_specs_tests_support;
#[cfg(test)]
#[path = "rpg/combat_test_fixture.rs"]
mod rpg_combat_test_fixture;
#[cfg(test)]
#[path = "rpg/combat_tests.rs"]
mod rpg_combat_tests;
#[cfg(test)]
#[path = "rpg/dungeons_thera_batch_1657_1659.rs"]
mod rpg_dungeons_thera_batch_1657_1659;
#[cfg(test)]
#[path = "rpg/faction_leaders_tests.rs"]
mod rpg_faction_leaders_tests;
#[cfg(test)]
#[path = "rpg/hall_of_remembered_test_fixture.rs"]
mod rpg_hall_of_remembered_test_fixture;
#[cfg(test)]
#[path = "rpg/hall_of_remembered_tests.rs"]
mod rpg_hall_of_remembered_tests;
#[cfg(test)]
#[path = "rpg/human_immortal_avatar_test_fixture.rs"]
mod rpg_human_immortal_avatar_test_fixture;
#[cfg(test)]
#[path = "rpg/human_immortal_avatar_tests.rs"]
mod rpg_human_immortal_avatar_tests;
#[cfg(test)]
#[path = "rpg/immortal_coembodiment_test_fixture.rs"]
mod rpg_immortal_coembodiment_test_fixture;
#[cfg(test)]
#[path = "rpg/immortal_coembodiment_tests.rs"]
mod rpg_immortal_coembodiment_tests;
#[cfg(test)]
#[path = "rpg/immortals_tests.rs"]
mod rpg_immortals_tests;
#[cfg(test)]
#[path = "rpg/infernal_access_test_fixture.rs"]
mod rpg_infernal_access_test_fixture;
#[cfg(test)]
#[path = "rpg/infernal_access_tests.rs"]
mod rpg_infernal_access_tests;
#[cfg(test)]
#[path = "rpg/infernal_attrition_test_fixture.rs"]
mod rpg_infernal_attrition_test_fixture;
#[cfg(test)]
#[path = "rpg/infernal_attrition_tests.rs"]
mod rpg_infernal_attrition_tests;
#[cfg(test)]
#[path = "rpg/infernal_boss_agents_tests.rs"]
mod rpg_infernal_boss_agents_tests;
#[cfg(test)]
#[path = "rpg/infernal_deeps_tests.rs"]
mod rpg_infernal_deeps_tests;
#[cfg(test)]
#[path = "rpg/infernal_relic_vaults_tests.rs"]
mod rpg_infernal_relic_vaults_tests;
#[cfg(test)]
#[path = "rpg/infernal_zones_test_fixture.rs"]
mod rpg_infernal_zones_test_fixture;
#[cfg(test)]
#[path = "rpg/infernal_zones_tests.rs"]
mod rpg_infernal_zones_tests;
#[cfg(test)]
#[path = "rpg/item_flags_materials_tests.rs"]
mod rpg_item_flags_materials_tests;
#[cfg(test)]
#[path = "rpg/item_ingest_tests.rs"]
mod rpg_item_ingest_tests;
#[cfg(test)]
#[path = "rpg/item_limited_test_fixture.rs"]
mod rpg_item_limited_test_fixture;
#[cfg(test)]
#[path = "rpg/item_limited_tests.rs"]
mod rpg_item_limited_tests;
#[cfg(test)]
#[path = "rpg/item_specs_taxonomy_tests.rs"]
mod rpg_item_specs_taxonomy_tests;
#[cfg(test)]
#[path = "rpg/item_specs_test_fixture.rs"]
mod rpg_item_specs_test_fixture;
#[cfg(test)]
#[path = "rpg/item_specs_tests.rs"]
mod rpg_item_specs_tests;
#[cfg(test)]
#[path = "rpg/map_access_tests.rs"]
mod rpg_map_access_tests;
#[cfg(test)]
#[path = "rpg/mortality_test_fixture.rs"]
mod rpg_mortality_test_fixture;
#[cfg(test)]
#[path = "rpg/mortality_tests.rs"]
mod rpg_mortality_tests;
#[cfg(test)]
#[path = "rpg/outfit_shop_test_fixture.rs"]
mod rpg_outfit_shop_test_fixture;
#[cfg(test)]
#[path = "rpg/outfit_shop_tests.rs"]
mod rpg_outfit_shop_tests;
#[cfg(test)]
#[path = "rpg/overworld_roads_thera_tests.rs"]
mod rpg_overworld_roads_thera_tests;
#[cfg(test)]
#[path = "rpg/overworld_topology_thera_fixture.rs"]
mod rpg_overworld_topology_thera_fixture;
#[cfg(test)]
#[path = "rpg/race_specs_batch_1001_1018.rs"]
mod rpg_race_specs_batch_1001_1018;
#[cfg(test)]
#[path = "rpg/race_specs_test_fixture.rs"]
mod rpg_race_specs_test_fixture;
#[cfg(test)]
#[path = "rpg/race_specs_tests.rs"]
mod rpg_race_specs_tests;
#[cfg(test)]
#[path = "rpg/racial_stealth_tests.rs"]
mod rpg_racial_stealth_tests;
#[cfg(test)]
#[path = "rpg/regions_thera_batch_1649_1660.rs"]
mod rpg_regions_thera_batch_1649_1660;
#[cfg(test)]
#[path = "rpg/road_network_thera_fixture.rs"]
mod rpg_road_network_thera_fixture;
#[cfg(test)]
#[path = "rpg/skilled_rewards_test_fixture.rs"]
mod rpg_skilled_rewards_test_fixture;
#[cfg(test)]
#[path = "rpg/skilled_rewards_tests.rs"]
mod rpg_skilled_rewards_tests;
#[cfg(test)]
#[path = "rpg/stealth_acquisition_tests.rs"]
mod rpg_stealth_acquisition_tests;
#[cfg(test)]
mod rpg_tests;
#[cfg(test)]
#[path = "rpg/themes_test_fixture.rs"]
mod rpg_themes_test_fixture;
#[cfg(test)]
#[path = "rpg/themes_tests.rs"]
mod rpg_themes_tests;
#[cfg(test)]
#[path = "rpg/towns_thera_batch_1644_1647.rs"]
mod rpg_towns_thera_batch_1644_1647;
#[cfg(test)]
#[path = "rpg/undeath_test_fixture.rs"]
mod rpg_undeath_test_fixture;
#[cfg(test)]
#[path = "rpg/undeath_tests.rs"]
mod rpg_undeath_tests;
#[cfg(test)]
#[path = "rpg/unhallowed_knight_test_fixture.rs"]
mod rpg_unhallowed_knight_test_fixture;
#[cfg(test)]
#[path = "rpg/unhallowed_knight_tests.rs"]
mod rpg_unhallowed_knight_tests;
#[cfg(test)]
#[path = "rpg/withering_raid_path_tests.rs"]
mod rpg_withering_raid_path_tests;
#[cfg(test)]
mod sim_attack_retention_tests;
#[cfg(test)]
mod sim_order_pruning_tests;
#[cfg(test)]
mod sim_order_tests;
#[cfg(test)]
mod sim_replay_tests;
#[cfg(test)]
mod sim_tests;
#[cfg(test)]
mod two_d_pipeline_tests;
#[cfg(test)]
mod visual_target_acceptance_tests;
#[cfg(all(test, feature = "wasm-script"))]
mod wasm_script_tests;
#[cfg(test)]
mod what_if_preview_tests;

pub use error::EngineError;
mod logging;
pub use logging::{init_file_logging, init_logging};

#[cfg(test)]
mod presentation_tests;

/// Engine version, sourced from this crate's Cargo manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
