use egui::Color32;
use litd_editor::asset_scene::{EditorAssetSceneReport, EditorFoliageReport};
use litd_editor::asset_vault::AssetVaultCatalog;
use litd_editor::history::EditorDocument;
use litd_editor::play::{PlayCameraRig, PlaySession};
use litd_editor::shell::{
    DockPanelKind, EditorMode, EditorShellPlan, WorldViewCameraControlInput,
    apply_world_view_camera_control as apply_shell_camera_control, build_editor_shell_plan,
    persist_shell_state, pop_out_panel, reattach_panel, rect_summary, restore_shell_state,
    set_world_view_camera_target_xz as shell_set_world_view_camera_target_xz, switch_editor_mode,
};
use litd_editor::vfx_editor::EditorVfxRuntime;
use litd_editor::world_validation::{WorldValidationReport, WorldValidationUseCase};
use litd_engine::{
    EngineError,
    math::Vec2 as EngineVec2,
    render::{
        editor_world::EditorWorldCameraState,
        ui::{UiRect, UiRenderSettings},
    },
};
use tracing::info;

use crate::main_editor_thumbnails::ThumbnailCache;

#[path = "main_editor_ai_coauthor.rs"]
mod main_editor_ai_coauthor;
#[path = "main_editor_asset_drop.rs"]
mod main_editor_asset_drop;
#[path = "main_editor_asset_entries.rs"]
mod main_editor_asset_entries;
#[path = "main_editor_asset_runtime.rs"]
mod main_editor_asset_runtime;
#[path = "main_editor_build.rs"]
mod main_editor_build;
#[path = "main_editor_camera_navigation.rs"]
mod main_editor_camera_navigation;
#[path = "main_editor_commands.rs"]
mod main_editor_commands;
#[path = "main_editor_content_browser.rs"]
mod main_editor_content_browser;
#[path = "main_editor_contextual_help.rs"]
mod main_editor_contextual_help;
#[path = "main_editor_evidence.rs"]
mod main_editor_evidence;
#[path = "main_editor_evidence_model.rs"]
mod main_editor_evidence_model;
#[path = "main_editor_foliage_authoring.rs"]
mod main_editor_foliage_authoring;
#[path = "main_editor_frame.rs"]
mod main_editor_frame;
#[path = "main_editor_intelligence.rs"]
mod main_editor_intelligence;
#[path = "main_editor_minimap.rs"]
mod main_editor_minimap;
#[path = "main_editor_model_previewer.rs"]
mod main_editor_model_previewer;
#[path = "main_editor_moderation.rs"]
mod main_editor_moderation;
#[path = "main_editor_object_editor.rs"]
mod main_editor_object_editor;
#[path = "main_editor_object_manager.rs"]
mod main_editor_object_manager;
#[path = "main_editor_object_manager_crossref.rs"]
mod main_editor_object_manager_crossref;
#[path = "main_editor_panels.rs"]
mod main_editor_panels;
#[cfg(test)]
#[path = "main_editor_persisted_state.rs"]
mod main_editor_persisted_state;
#[path = "main_editor_persistence.rs"]
mod main_editor_persistence;
#[path = "main_editor_play.rs"]
mod main_editor_play;
#[cfg(test)]
#[path = "main_editor_save_failure_fsv_tests.rs"]
mod main_editor_save_failure_fsv_tests;
#[path = "main_editor_shell_stats.rs"]
mod main_editor_shell_stats;
#[path = "main_editor_status.rs"]
mod main_editor_status;
#[cfg(test)]
#[path = "main_editor_test_scenes.rs"]
mod main_editor_test_scenes;
#[path = "main_editor_top_bar.rs"]
mod main_editor_top_bar;
#[path = "main_editor_triggers.rs"]
mod main_editor_triggers;
#[path = "main_editor_validation.rs"]
mod main_editor_validation;
#[path = "main_editor_vfx.rs"]
mod main_editor_vfx;
#[path = "main_editor_world_creation.rs"]
mod main_editor_world_creation;
#[path = "main_editor_world_layers.rs"]
mod main_editor_world_layers;

use main_editor_ai_coauthor::AiCoAuthorRuntime;
use main_editor_asset_entries::{AssetEntry, build_runtime_asset_entries};
use main_editor_asset_runtime::initialize_asset_runtime;
// validate_pixels_per_point is consumed by the (cfg(not(test))) egui renderer; the
// re-export is unused only in the test build, so silence the test-only warning.
#[cfg_attr(test, allow(unused_imports))]
pub use main_editor_evidence::{logical_surface_size_from_physical, validate_pixels_per_point};
pub use main_editor_evidence_model::EditorShellRuntimeEvidence;
use main_editor_foliage_authoring::FoliageBrushUiState;
use main_editor_intelligence::EditorIntelligenceUiRuntime;
use main_editor_minimap::{EditorMinimapModel, MinimapLayerVisibility};
use main_editor_panels::PlacementMode;
use main_editor_persistence::ShellStateStore;
pub use main_editor_shell_stats::format_shell_stats;
#[cfg(test)]
use main_editor_test_scenes::{
    default_scene_report_for_tests, empty_minimap_scene_report_for_tests,
};
use main_editor_world_creation::WorldCreationRuntime;
use main_editor_world_layers::WorldLayerVisibility;

const PANEL_BG: Color32 = Color32::from_rgb(18, 24, 35);
const PANEL_BG_SOFT: Color32 = Color32::from_rgb(32, 42, 58);
const GOLD: Color32 = Color32::from_rgb(222, 179, 98);
const GOLD_DEEP: Color32 = Color32::from_rgb(123, 91, 45);
const TEXT: Color32 = Color32::from_rgb(236, 232, 214);
const TEXT_DIM: Color32 = Color32::from_rgb(161, 170, 181);
const GOOD: Color32 = Color32::from_rgb(87, 206, 117);
const ERROR: Color32 = Color32::from_rgb(241, 116, 116);

#[derive(Debug, Clone)]
pub struct EditorShellRuntime {
    plan: EditorShellPlan,
    shell_state: ShellStateStore,
    accessibility_text_scale: f32,
    mode_switch_count: u32,
    inspector_popout_count: u32,
    inspector_reattach_count: u32,
    brush_radius: f32,
    falloff: f32,
    scale_min: f32,
    scale_max: f32,
    density: f32,
    placement: PlacementMode,
    ai_coauthor: AiCoAuthorRuntime,
    selected_asset: usize,
    asset_entries: Vec<AssetEntry>,
    asset_drop_preview: String,
    asset_drop_count: u32,
    last_asset_drop: String,
    last_asset_drop_error: String,
    thumbnail_cache: ThumbnailCache,
    play_camera_rig: PlayCameraRig,
    play_session: Option<PlaySession>,
    saved_edit_snapshot: Option<main_editor_play::PlayEditSnapshot>,
    play_transition_count: u32,
    play_rig_switch_count: u32,
    asset_vault_record_count: usize,
    asset_vault_usable_record_count: usize,
    asset_vault_sources: String,
    asset_vault_record_types: String,
    asset_vault_asset_types: String,
    asset_vault_licenses: String,
    asset_vault_semantic_role_counts: String,
    asset_vault_semantic_profile_count: usize,
    asset_vault_semantic_classified_profile_count: usize,
    asset_vault_semantic_unclassified_profile_count: usize,
    asset_vault_semantic_tag_count: usize,
    asset_vault_download_bytes_on_disk: u64,
    asset_vault_extraction_bytes_on_disk: u64,
    asset_vault_issue1242_p0_status: &'static str,
    asset_vault_manifests: String,
    world_scene_primitives: u32,
    world_scene_degenerate_triangle_count: usize,
    foliage: Option<EditorFoliageReport>,
    foliage_brush: FoliageBrushUiState,
    minimap: EditorMinimapModel,
    minimap_layers: MinimapLayerVisibility,
    world_layers: WorldLayerVisibility,
    world_scene_reload_count: u32,
    minimap_navigation_count: u32,
    last_minimap_navigation_xz: Option<[f32; 2]>,
    model_previewer: Option<litd_editor::asset_previewer::ModelPreviewRuntime>,
    model_previewer_error: String,
    model_previewer_load_count: u32,
    map_build_status: String,
    map_build_operation_count: u32,
    world_creation: WorldCreationRuntime,
    world_validation_report: WorldValidationReport,
    world_validation_run_count: u32,
    command_registry: litd_editor::editor_commands::EditorCommandRegistry,
    command_palette_open: bool,
    command_palette_query: String,
    command_palette_focus_pending: bool,
    contextual_help_open: bool,
    contextual_help_query: String,
    contextual_help_answer: Option<litd_editor::contextual_help::ContextualHelpAnswer>,
    contextual_help_error: String,
    contextual_help_action_count: u32,
    contextual_help_last_action: String,
    command_invocation_count: u32,
    last_command_id: String,
    last_command_source: String,
    // Most recent command/action failure, surfaced to the status bar. Empty = no
    // pending error. Cleared on the next successful command (#1489).
    last_command_error: String,
    command_error_count: u32,
    moderation_last_action: String,
    moderation_report_count: usize,
    moderation_state_path: String,
    moderation_state_sha256: String,
    vfx_editor: EditorVfxRuntime,
    editor_intelligence: EditorIntelligenceUiRuntime,
    object_editor_document: EditorDocument,
    object_editor_selected_object: String,
    object_editor_last_action: String,
    // #1485 finding 6: bumped by mark_object_editor_dirty (see main_editor_object_editor.rs)
    // to invalidate the memoized inspector view; rebuilt lazily in object_controls.
    object_editor_generation: u64,
    object_editor_inspector_cache: litd_editor::object_editor_inspector::ObjectEditorInspectorCache,
    object_manager_project: litd_editor::object_manager::ObjectManagerProject,
    // #1461: the Object Manager panel is immediate-mode and redraws every frame;
    // build_object_manager_index is O(project), so it is memoized here and rebuilt only
    // when object_manager_generation advances (bumped by mark_object_manager_dirty on any
    // mutation of object_manager_project).
    object_manager_generation: u64,
    object_manager_index_cache: litd_editor::object_manager::ObjectManagerIndexCache,
    object_manager_query: String,
    object_manager_selected_id: String,
    trigger_editor_project: litd_editor::trigger_editor::TriggerEditorProject,
    trigger_editor_selected_graph: usize,
    trigger_editor_last_action: String,
    trigger_editor_file: main_editor_triggers::TriggerProjectFileState,
    // #1461: the trigger panel showed a `stable graph bytes=N` label by RON-serializing the
    // whole selected graph every frame. Memoize the byte count keyed by
    // (trigger_editor_generation, trigger_editor_selected_graph); the generation is bumped by
    // mark_trigger_editor_dirty on any trigger-graph mutation.
    trigger_editor_generation: u64,
    trigger_editor_ron_cache: litd_editor::trigger_graph_ron_cache::TriggerGraphRonBytesCache,
}

impl EditorShellRuntime {
    #[cfg(test)]
    pub fn new(width: u32, height: u32) -> Result<Self, EngineError> {
        let report = default_scene_report_for_tests()?;
        Self::new_with_world_scene_report(width as f32, height as f32, &report)
    }

    pub fn new_with_world_scene_report(
        width: f32,
        height: f32,
        scene_report: &EditorAssetSceneReport,
    ) -> Result<Self, EngineError> {
        info!("editor shell runtime loading persisted shell state");
        let mut shell_state = ShellStateStore::from_env()?;
        let command_registry = litd_editor::editor_commands::default_editor_command_registry()?;
        let base = build_editor_shell_plan(UiRenderSettings {
            viewport_width: width,
            viewport_height: height,
        })?;
        let plan = shell_state.load_or_initialize(base.settings, base)?;
        let asset_runtime = initialize_asset_runtime(&plan)?;
        info!("editor shell runtime building minimap and editor subsystems");
        let minimap = EditorMinimapModel::from_scene_report(scene_report)?;
        let world_layers = shell_state.world_layers();
        let play_camera_rig = shell_state.play_camera_rig();
        let mut runtime = Self {
            plan,
            shell_state,
            accessibility_text_scale: 1.0,
            mode_switch_count: 0,
            inspector_popout_count: 0,
            inspector_reattach_count: 0,
            brush_radius: 64.0,
            falloff: 0.5,
            scale_min: 0.8,
            scale_max: 1.2,
            density: 50.0,
            placement: PlacementMode::RandomRotation,
            ai_coauthor: AiCoAuthorRuntime::from_env()?,
            selected_asset: asset_runtime.selected_asset,
            asset_entries: asset_runtime.asset_entries,
            asset_drop_preview: "none".to_string(),
            asset_drop_count: 0,
            last_asset_drop: "none".to_string(),
            last_asset_drop_error: String::new(),
            thumbnail_cache: ThumbnailCache::default(),
            play_camera_rig,
            play_session: None,
            saved_edit_snapshot: None,
            play_transition_count: 0,
            play_rig_switch_count: 0,
            asset_vault_record_count: asset_runtime.asset_vault_record_count,
            asset_vault_usable_record_count: asset_runtime.asset_vault_usable_record_count,
            asset_vault_sources: asset_runtime.asset_vault_sources,
            asset_vault_record_types: asset_runtime.asset_vault_record_types,
            asset_vault_asset_types: asset_runtime.asset_vault_asset_types,
            asset_vault_licenses: asset_runtime.asset_vault_licenses,
            asset_vault_semantic_role_counts: asset_runtime.asset_vault_semantic_role_counts,
            asset_vault_semantic_profile_count: asset_runtime.asset_vault_semantic_profile_count,
            asset_vault_semantic_classified_profile_count: asset_runtime
                .asset_vault_semantic_classified_profile_count,
            asset_vault_semantic_unclassified_profile_count: asset_runtime
                .asset_vault_semantic_unclassified_profile_count,
            asset_vault_semantic_tag_count: asset_runtime.asset_vault_semantic_tag_count,
            asset_vault_download_bytes_on_disk: asset_runtime.asset_vault_download_bytes_on_disk,
            asset_vault_extraction_bytes_on_disk: asset_runtime
                .asset_vault_extraction_bytes_on_disk,
            asset_vault_issue1242_p0_status: asset_runtime.asset_vault_issue1242_p0_status,
            asset_vault_manifests: asset_runtime.asset_vault_manifests,
            world_scene_primitives: scene_report.triangle_count as u32,
            world_scene_degenerate_triangle_count: scene_report.degenerate_triangle_count,
            foliage: scene_report.foliage.clone(),
            foliage_brush: FoliageBrushUiState::from_env()?,
            minimap,
            minimap_layers: MinimapLayerVisibility::default(),
            world_layers,
            world_scene_reload_count: 0,
            minimap_navigation_count: 0,
            last_minimap_navigation_xz: None,
            model_previewer: None,
            model_previewer_error: String::new(),
            model_previewer_load_count: 0,
            map_build_status: "idle".to_string(),
            map_build_operation_count: 0,
            world_creation: WorldCreationRuntime::from_env()?,
            world_validation_report: WorldValidationReport::ok(WorldValidationUseCase::Edit),
            world_validation_run_count: 0,
            command_registry,
            command_palette_open: false,
            command_palette_query: String::new(),
            command_palette_focus_pending: false,
            contextual_help_open: false,
            contextual_help_query: String::new(),
            contextual_help_answer: None,
            contextual_help_error: String::new(),
            contextual_help_action_count: 0,
            contextual_help_last_action: "none".to_string(),
            command_invocation_count: 0,
            last_command_id: "none".to_string(),
            last_command_source: "none".to_string(),
            last_command_error: String::new(),
            command_error_count: 0,
            moderation_last_action: "none".to_string(),
            moderation_report_count: 0,
            moderation_state_path: String::new(),
            moderation_state_sha256: String::new(),
            vfx_editor: EditorVfxRuntime::from_env()?,
            editor_intelligence: EditorIntelligenceUiRuntime::from_env()?,
            object_editor_document: litd_editor::object_editor::issue732_base_document(),
            object_editor_selected_object: "base.unit.footman".to_string(),
            object_editor_last_action: "ready".to_string(),
            object_editor_generation: 0,
            object_editor_inspector_cache:
                litd_editor::object_editor_inspector::ObjectEditorInspectorCache::default(),
            object_manager_project: litd_editor::object_manager::issue736_sample_project()?,
            object_manager_generation: 0,
            object_manager_index_cache:
                litd_editor::object_manager::ObjectManagerIndexCache::default(),
            object_manager_query: "guard".to_string(),
            object_manager_selected_id: "unit.issue736.guard".to_string(),
            trigger_editor_project: litd_editor::trigger_editor::issue733_sample_project()?,
            trigger_editor_selected_graph: 0,
            trigger_editor_last_action: "ready".to_string(),
            trigger_editor_file: main_editor_triggers::TriggerProjectFileState::default(),
            trigger_editor_generation: 0,
            trigger_editor_ron_cache:
                litd_editor::trigger_graph_ron_cache::TriggerGraphRonBytesCache::default(),
        };
        if !runtime.asset_entries.is_empty() {
            runtime.refresh_model_previewer();
        }
        Ok(runtime)
    }

    pub fn rebuild_for_size(&mut self, width: f32, height: f32) -> Result<(), EngineError> {
        let mut persisted_plan = self.plan.clone();
        if let Some(snapshot) = self.saved_edit_snapshot {
            persisted_plan.viewport.camera = snapshot.camera;
        }
        let persistence = persist_shell_state(&persisted_plan)?;
        let play_camera = self
            .play_session
            .as_ref()
            .map(|_| self.plan.viewport.camera);
        self.plan = restore_shell_state(
            UiRenderSettings {
                viewport_width: width,
                viewport_height: height,
            },
            &persistence,
        )?;
        if let Some(camera) = play_camera {
            self.plan.viewport.camera = camera;
            self.plan.validate()?;
        } else {
            self.shell_state.save_plan(&self.plan)?;
        }
        Ok(())
    }

    pub fn switch_mode(&mut self, mode: EditorMode) -> Result<(), EngineError> {
        if mode != self.plan.active_mode {
            if mode == EditorMode::Vfx {
                self.vfx_editor.refresh_preview()?;
            }
            self.plan = switch_editor_mode(&self.plan, mode)?;
            self.mode_switch_count += 1;
            self.shell_state.save_plan(&self.plan)?;
        }
        Ok(())
    }

    pub fn accessibility_text_scale(&self) -> f32 {
        self.accessibility_text_scale
    }

    pub fn set_accessibility_text_scale(&mut self, text_scale: f32) -> Result<bool, EngineError> {
        litd_editor::editor_preferences::validate_editor_text_scale(text_scale)?;
        let changed = (self.accessibility_text_scale - text_scale).abs() > f32::EPSILON;
        self.accessibility_text_scale = text_scale;
        Ok(changed)
    }

    pub fn pop_out_inspector(&mut self) -> Result<(), EngineError> {
        if !self.inspector_docked() {
            return Ok(());
        }
        let width = self.plan.settings.viewport_width;
        let height = self.plan.settings.viewport_height;
        let floating = UiRect::new(
            EngineVec2::new((width - 430.0).max(64.0), 92.0),
            EngineVec2::new((width - 38.0).max(460.0), (height - 86.0).max(300.0)),
        )?;
        self.plan = pop_out_panel(&self.plan, DockPanelKind::RightInspector, floating)?;
        self.inspector_popout_count += 1;
        self.shell_state.save_plan(&self.plan)?;
        Ok(())
    }

    pub fn reattach_inspector(&mut self) -> Result<(), EngineError> {
        if self.inspector_docked() {
            return Ok(());
        }
        self.plan = reattach_panel(&self.plan, DockPanelKind::RightInspector)?;
        self.inspector_reattach_count += 1;
        self.shell_state.save_plan(&self.plan)?;
        Ok(())
    }

    pub fn world_camera_state(&self) -> EditorWorldCameraState {
        let camera = self.plan.viewport.camera;
        EditorWorldCameraState {
            target_world: camera.target_world,
            distance: camera.distance,
            yaw_degrees: camera.yaw_degrees,
            pitch_degrees: camera.pitch_degrees,
        }
    }
}

fn invalid_shell_runtime<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.shell.runtime.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "main_editor_ui_interaction_tests.rs"]
mod interaction_tests;
#[cfg(test)]
#[path = "main_editor_ui_mode_tests.rs"]
mod mode_tests;
#[cfg(test)]
#[path = "main_editor_ui_tests.rs"]
mod tests;
