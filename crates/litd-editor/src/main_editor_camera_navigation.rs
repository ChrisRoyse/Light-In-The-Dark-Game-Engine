use egui::RichText;
use litd_editor::editor_commands::COMMAND_FRAME_SELECTED;
use litd_editor::shell::{
    WORLD_VIEW_CAMERA_BOOKMARK_SLOT_COUNT, WorldViewFrameSelection,
    clamp_world_view_camera_to_profile, frame_world_view_camera_on_selection,
};

use super::main_editor_commands::{restore_bookmark_command_id, save_bookmark_command_id};
use super::main_editor_frame::panel_width;
use super::*;

impl EditorShellRuntime {
    pub(super) fn apply_world_view_camera_control(
        &mut self,
        input: WorldViewCameraControlInput,
    ) -> Result<bool, EngineError> {
        if !input.has_motion() {
            return Ok(false);
        }
        let before = self.plan.viewport.camera;
        let next = apply_shell_camera_control(before, self.plan.viewport.world_bounds, input)?;
        if next == before {
            return Ok(false);
        }
        self.plan.viewport.camera = next;
        self.plan.validate()?;
        self.shell_state.save_plan(&self.plan)?;
        Ok(true)
    }

    pub(super) fn set_world_view_camera_target_xz(
        &mut self,
        target_xz: [f32; 2],
    ) -> Result<(), EngineError> {
        let next = shell_set_world_view_camera_target_xz(
            self.plan.viewport.camera,
            self.plan.viewport.world_bounds,
            target_xz,
        )?;
        self.plan.viewport.camera = next;
        self.plan.validate()?;
        self.shell_state.save_plan(&self.plan)
    }
}

const FRAME_SELECTED_RADIUS: f32 = 24.0;
const NAV_ORBIT_STEP_PX: f32 = 120.0;
const NAV_PAN_STEP_PX: f32 = 96.0;
const NAV_ZOOM_STEP_PX: f32 = 300.0;

impl EditorShellRuntime {
    pub(super) fn camera_navigation_controls(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("View").strong().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(self.shell_state.camera_bookmark_summary())
                        .size(10.5)
                        .color(TEXT_DIM),
                );
            });
        });
        ui.horizontal_wrapped(|ui| {
            self.camera_control_button(
                ui,
                "Orbit L",
                WorldViewCameraControlInput {
                    orbit_delta_px: EngineVec2::new(-NAV_ORBIT_STEP_PX, 0.0),
                    ..WorldViewCameraControlInput::ZERO
                },
            );
            self.camera_control_button(
                ui,
                "Orbit R",
                WorldViewCameraControlInput {
                    orbit_delta_px: EngineVec2::new(NAV_ORBIT_STEP_PX, 0.0),
                    ..WorldViewCameraControlInput::ZERO
                },
            );
            self.camera_control_button(
                ui,
                "Pan L",
                WorldViewCameraControlInput {
                    pan_delta_px: EngineVec2::new(NAV_PAN_STEP_PX, 0.0),
                    ..WorldViewCameraControlInput::ZERO
                },
            );
            self.camera_control_button(
                ui,
                "Pan R",
                WorldViewCameraControlInput {
                    pan_delta_px: EngineVec2::new(-NAV_PAN_STEP_PX, 0.0),
                    ..WorldViewCameraControlInput::ZERO
                },
            );
            self.camera_control_button(
                ui,
                "Zoom +",
                WorldViewCameraControlInput {
                    zoom_delta_px: NAV_ZOOM_STEP_PX,
                    ..WorldViewCameraControlInput::ZERO
                },
            );
            self.camera_control_button(
                ui,
                "Zoom -",
                WorldViewCameraControlInput {
                    zoom_delta_px: -NAV_ZOOM_STEP_PX,
                    ..WorldViewCameraControlInput::ZERO
                },
            );
            if ui.button("Frame").clicked()
                && let Err(err) = self.invoke_editor_command_by_id(COMMAND_FRAME_SELECTED, "panel")
            {
                tracing::error!(error = %err, "world-view frame selected failed");
            }
            for slot in 1..=WORLD_VIEW_CAMERA_BOOKMARK_SLOT_COUNT {
                let save = format!("Save {slot}");
                if ui.button(save).clicked()
                    && let Some(command_id) = save_bookmark_command_id(slot)
                    && let Err(err) = self.invoke_editor_command_by_id(command_id, "panel")
                {
                    tracing::error!(error = %err, slot, "world-view camera bookmark save failed");
                }
                let restore = format!("Go {slot}");
                if ui.button(restore).clicked()
                    && let Some(command_id) = restore_bookmark_command_id(slot)
                    && let Err(err) = self.invoke_editor_command_by_id(command_id, "panel")
                {
                    tracing::error!(error = %err, slot, "world-view camera bookmark restore failed");
                }
            }
        });
        if panel_width(ui) >= 260.0 {
            ui.label(
                RichText::new(format!(
                    "{} saved view{}",
                    self.shell_state.camera_bookmark_count(),
                    if self.shell_state.camera_bookmark_count() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ))
                .size(11.0)
                .color(TEXT_DIM),
            );
        }
    }

    fn camera_control_button(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        input: WorldViewCameraControlInput,
    ) {
        if ui.button(label).clicked()
            && let Err(err) = self.apply_world_view_camera_control(input)
        {
            tracing::error!(error = %err, control = label, "world-view camera navigation failed");
        }
    }

    pub(super) fn frame_selected_world_view_camera(&mut self) -> Result<bool, EngineError> {
        if self.world_layers.lock_to_game_camera {
            return invalid_shell_runtime(
                "camera.frame_selected",
                "world view camera is locked to the game camera",
            );
        }
        let before = self.plan.viewport.camera;
        let selection = WorldViewFrameSelection {
            selected_entities: self.plan.gizmo.selected_entities,
            pivot_world: self.plan.gizmo.pivot_world,
            radius: FRAME_SELECTED_RADIUS,
        };
        let next = frame_world_view_camera_on_selection(
            before,
            self.plan.viewport.world_bounds,
            selection,
        )?;
        if next == before {
            return Ok(false);
        }
        self.plan.viewport.camera = next;
        if let Err(error) = self.plan.validate() {
            self.plan.viewport.camera = before;
            return Err(error);
        }
        if let Err(error) = self.shell_state.save_plan(&self.plan) {
            self.plan.viewport.camera = before;
            return Err(error);
        }
        Ok(true)
    }

    pub(super) fn save_world_view_camera_bookmark(&mut self, slot: u8) -> Result<(), EngineError> {
        // Optimistically insert the bookmark (save_plan serializes the in-memory store),
        // then roll it back if the persist fails so a "saved" bookmark can't linger in
        // memory while disk keeps the old value and vanish on restart (#1489).
        let snapshot = self.shell_state.camera_bookmarks_snapshot();
        self.shell_state
            .save_camera_bookmark(slot, self.plan.viewport.camera)?;
        if let Err(error) = self.shell_state.save_plan(&self.plan) {
            self.shell_state.restore_camera_bookmarks(snapshot);
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn restore_world_view_camera_bookmark(
        &mut self,
        slot: u8,
    ) -> Result<bool, EngineError> {
        if self.world_layers.lock_to_game_camera {
            return invalid_shell_runtime(
                "camera.bookmark.restore",
                "world view camera is locked to the game camera",
            );
        }
        let bookmarked = self.shell_state.restore_camera_bookmark(slot)?;
        let current_profile =
            clamp_world_view_camera_to_profile(bookmarked, self.plan.viewport.world_bounds)?;
        if current_profile != bookmarked {
            return invalid_shell_runtime(
                "camera.bookmark.restore",
                "saved bookmark is outside the current world camera profile",
            );
        }
        let before = self.plan.viewport.camera;
        if before == bookmarked {
            return Ok(false);
        }
        self.plan.viewport.camera = bookmarked;
        if let Err(error) = self.plan.validate() {
            self.plan.viewport.camera = before;
            return Err(error);
        }
        if let Err(error) = self.shell_state.save_plan(&self.plan) {
            self.plan.viewport.camera = before;
            return Err(error);
        }
        Ok(true)
    }
}

#[cfg(test)]
#[path = "main_editor_camera_navigation_tests.rs"]
mod tests;
