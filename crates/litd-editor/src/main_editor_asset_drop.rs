use litd_engine::{EngineError, math::Vec3 as EngineVec3};

use super::*;

impl EditorShellRuntime {
    fn content_browser_asset_refs(
        &self,
        asset_indices: &[usize],
    ) -> Result<Vec<litd_editor::asset_placement::ContentBrowserAssetRef>, EngineError> {
        if asset_indices.is_empty() {
            return invalid_shell_runtime("asset_drop.indices", "drop payload contained no assets");
        }
        asset_indices
            .iter()
            .map(|index| {
                self.asset_entries
                    .get(*index)
                    .map(AssetEntry::placement_ref)
                    .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                        field: "editor.shell.runtime.asset_drop.indices".to_string(),
                        reason: format!("asset index {index} is outside the content browser"),
                    })
            })
            .collect()
    }

    fn viewport_screen_to_world(&self, pos: egui::Pos2) -> Result<EngineVec3, EngineError> {
        let rect = self.plan.viewport.rect_px;
        let size = rect.size();
        if size.x <= 0.0 || size.y <= 0.0 {
            return invalid_shell_runtime("asset_drop.viewport", "viewport rect is degenerate");
        }
        let bounds = self.plan.viewport.world_bounds;
        let u = ((pos.x - rect.min.x) / size.x).clamp(0.0, 1.0);
        let v = ((pos.y - rect.min.y) / size.y).clamp(0.0, 1.0);
        Ok(EngineVec3::new(
            bounds.min.x + (bounds.max.x - bounds.min.x) * u,
            0.0,
            bounds.min.z + (bounds.max.z - bounds.min.z) * v,
        ))
    }

    pub(super) fn preview_content_browser_drop(
        &mut self,
        asset_indices: &[usize],
        pos: egui::Pos2,
    ) {
        if let Ok(world) = self.viewport_screen_to_world(pos) {
            self.asset_drop_preview = format!(
                "assets={};world={:.1},0.0,{:.1}",
                asset_indices.len(),
                world.x,
                world.z
            );
        }
    }

    pub(super) fn drop_content_browser_assets(
        &mut self,
        asset_indices: &[usize],
        pos: egui::Pos2,
    ) -> Result<(), EngineError> {
        let assets = self.content_browser_asset_refs(asset_indices)?;
        let world = self.viewport_screen_to_world(pos)?;
        let report = self.place_content_browser_assets_command(assets, world)?;
        self.asset_drop_count = self.asset_drop_count.saturating_add(1);
        self.last_asset_drop = format!(
            "objects={};ids={}",
            report.placed_object_ids.len(),
            report.placed_object_ids.join("|")
        );
        self.last_asset_drop_error.clear();
        Ok(())
    }
}
