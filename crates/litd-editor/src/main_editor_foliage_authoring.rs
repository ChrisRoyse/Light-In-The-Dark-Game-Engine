use std::path::Path;

use litd_editor::{
    asset_scene::EditorAssetSceneReport, asset_scene_foliage_brush::FoliageBrushStore,
    asset_scene_foliage_config::load_default_foliage_config,
};
use litd_engine::EngineError;

use super::*;

#[derive(Debug, Clone)]
pub(super) struct FoliageBrushUiState {
    store: FoliageBrushStore,
    reload_requested: bool,
    action_count: u32,
    last_action: String,
}

impl FoliageBrushUiState {
    pub(super) fn from_env() -> Result<Self, EngineError> {
        Ok(Self {
            store: FoliageBrushStore::from_env()?,
            reload_requested: false,
            action_count: 0,
            last_action: "none".to_string(),
        })
    }
}

impl EditorShellRuntime {
    pub(crate) fn foliage_brush_state_path(&self) -> &Path {
        self.foliage_brush.store.path()
    }

    pub(crate) fn take_world_scene_reload_requested(&mut self) -> bool {
        let requested = self.foliage_brush.reload_requested;
        self.foliage_brush.reload_requested = false;
        requested
    }

    pub(crate) fn replace_world_scene_report(
        &mut self,
        report: &EditorAssetSceneReport,
    ) -> Result<(), EngineError> {
        self.world_scene_primitives = report.triangle_count as u32;
        self.world_scene_degenerate_triangle_count = report.degenerate_triangle_count;
        self.foliage = report.foliage.clone();
        self.minimap = EditorMinimapModel::from_scene_report(report)?;
        self.world_scene_reload_count = self.world_scene_reload_count.saturating_add(1);
        Ok(())
    }

    pub(crate) fn add_foliage_density_brush(&mut self) -> Result<(), EngineError> {
        let config = load_default_foliage_config()?;
        let center = self.brush_center_xz();
        let state = self.foliage_brush.store.add_density_brush(
            &config,
            center,
            self.brush_radius,
            self.density,
        )?;
        self.record_foliage_brush_action(format!(
            "add@{:.1},{:.1};r={:.0};density={:.0};stamps={}",
            center[0],
            center[1],
            self.brush_radius,
            self.density,
            state.stamps.len()
        ));
        Ok(())
    }

    pub(crate) fn erase_foliage_density_brush(&mut self) -> Result<(), EngineError> {
        let center = self.brush_center_xz();
        let state = self
            .foliage_brush
            .store
            .erase_density_brush(center, self.brush_radius)?;
        self.record_foliage_brush_action(format!(
            "erase@{:.1},{:.1};r={:.0};stamps={}",
            center[0],
            center[1],
            self.brush_radius,
            state.stamps.len()
        ));
        Ok(())
    }

    pub(crate) fn foliage_brush_action_count(&self) -> u32 {
        self.foliage_brush.action_count
    }

    pub(crate) fn foliage_brush_last_action(&self) -> &str {
        &self.foliage_brush.last_action
    }

    fn brush_center_xz(&self) -> [f32; 2] {
        [
            self.plan.viewport.camera.target_world.x,
            self.plan.viewport.camera.target_world.z,
        ]
    }

    fn record_foliage_brush_action(&mut self, action: String) {
        self.foliage_brush.action_count = self.foliage_brush.action_count.saturating_add(1);
        self.foliage_brush.last_action = action;
        self.foliage_brush.reload_requested = true;
    }
}
