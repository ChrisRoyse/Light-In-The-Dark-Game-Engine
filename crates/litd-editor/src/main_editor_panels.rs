use egui::RichText;
use litd_editor::editor_commands::COMMAND_INSPECTOR_REATTACH;

use super::main_editor_frame::{fixed_panel, fixed_vscroll_panel, panel_width, pos, size};
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlacementMode {
    RandomRotation,
    AlignToNormal,
    GridSnap,
}

impl EditorShellRuntime {
    pub(super) fn inspector_docked(&self) -> bool {
        self.plan
            .panel(DockPanelKind::RightInspector)
            .map(|panel| panel.docked)
            .unwrap_or(false)
    }

    pub(super) fn panel_rect(&self, kind: DockPanelKind) -> Option<UiRect> {
        self.plan.panel(kind).map(|panel| panel.rect_px)
    }

    pub(super) fn operational_control_count(&self) -> u32 {
        7 + self.plan.mode_tabs.len() as u32
            + self.plan.active_inspector.primary_controls.len() as u32
    }
}

#[path = "main_editor_panels_inspector.rs"]
mod main_editor_panels_inspector;

#[path = "main_editor_world_hierarchy.rs"]
mod main_editor_world_hierarchy;

impl EditorShellRuntime {
    pub fn show(&mut self, ctx: &egui::Context) {
        self.handle_editor_command_shortcuts(ctx);
        self.show_top_bar(ctx);
        self.show_command_palette(ctx);
        self.show_contextual_help(ctx);
        self.show_left_hierarchy(ctx);
        self.show_viewport_input_layer(ctx);
        self.show_letterbox_overlay(ctx);
        if self.inspector_docked() {
            self.show_right_inspector(ctx);
        } else {
            self.show_popped_placeholder(ctx);
            self.show_floating_inspector(ctx);
        }
        self.show_minimap(ctx);
        self.show_content_browser(ctx);
        self.show_status(ctx);
    }

    fn show_left_hierarchy(&mut self, ctx: &egui::Context) {
        let Some(rect) = self.panel_rect(DockPanelKind::LeftHierarchy) else {
            return;
        };
        fixed_vscroll_panel(ctx, "editor.left_hierarchy", rect, 4, |ui| {
            let compact = panel_width(ui) < 240.0 || rect.size().y < 520.0;
            ui.label(RichText::new("World").strong().color(TEXT));
            ui.separator();
            ui.label(RichText::new("Environment").size(11.0).color(GOLD));
            for item in main_editor_world_hierarchy::ENVIRONMENT_ITEMS {
                main_editor_world_hierarchy::hierarchy_row(ui, item);
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Placed items").size(11.0).color(GOLD));
            for item in main_editor_world_hierarchy::PLACED_ITEMS {
                main_editor_world_hierarchy::hierarchy_row(ui, item);
            }
            let ai_block_height = if compact { 150.0 } else { 154.0 };
            ui.add_space((ui.available_height() - ai_block_height).max(8.0));
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("AI Co-Author").strong().color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("x").color(GOLD));
                });
            });
            self.ai_coauthor_compact_prompt(ui, compact);
        });
    }

    fn show_viewport_input_layer(&mut self, ctx: &egui::Context) {
        let rect = self.plan.viewport.rect_px;
        egui::Area::new(egui::Id::new("editor.world_view"))
            .fixed_pos(pos(rect.min))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                let (_view_rect, response) =
                    ui.allocate_exact_size(size(rect), egui::Sense::click_and_drag());
                if self.world_layers.lock_to_game_camera {
                    return;
                }
                if let Some(payload) = response
                    .dnd_hover_payload::<main_editor_content_browser::ContentBrowserDragPayload>(
                ) && let Some(pos) = ui.input(|input| input.pointer.hover_pos())
                {
                    self.preview_content_browser_drop(&payload.asset_indices, pos);
                }
                if let Some(payload) = response
                    .dnd_release_payload::<main_editor_content_browser::ContentBrowserDragPayload>(
                ) {
                    if let Some(pos) = ui.input(|input| input.pointer.latest_pos())
                        && let Err(err) =
                            self.drop_content_browser_assets(&payload.asset_indices, pos)
                    {
                        self.last_asset_drop_error = err.to_string();
                        tracing::error!(error = %err, "content-browser asset drop failed");
                    }
                    return;
                }
                let mut control = WorldViewCameraControlInput::ZERO;
                if response.hovered() {
                    let scroll = ui.input(|input| input.raw_scroll_delta.y);
                    if scroll != 0.0 {
                        control.zoom_delta_px = scroll;
                    }
                }
                if response.dragged() {
                    let (delta, pan_mode) = ui.input(|input| {
                        (
                            input.pointer.delta(),
                            input.pointer.button_down(egui::PointerButton::Middle)
                                || input.modifiers.shift,
                        )
                    });
                    let delta = EngineVec2::new(delta.x, delta.y);
                    if pan_mode {
                        control.pan_delta_px = delta;
                    } else {
                        control.orbit_delta_px = delta;
                    }
                }
                if control.has_motion()
                    && let Err(err) = self.apply_world_view_camera_control(control)
                {
                    tracing::error!(error = %err, "world-view camera control failed");
                }
            });
    }

    fn show_letterbox_overlay(&self, ctx: &egui::Context) {
        if !self.world_layers.letterbox_mode {
            return;
        }
        let rect = self.plan.viewport.rect_px;
        let bar_height = (rect.size().y * 0.105).clamp(18.0, 72.0);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("editor.letterbox_overlay"),
        ));
        let top = UiRect::new(
            rect.min,
            EngineVec2::new(rect.max.x, rect.min.y + bar_height),
        );
        let bottom = UiRect::new(
            EngineVec2::new(rect.min.x, rect.max.y - bar_height),
            rect.max,
        );
        if let (Ok(top), Ok(bottom)) = (top, bottom) {
            painter.rect_filled(
                egui::Rect::from_min_max(pos(top.min), pos(top.max)),
                0.0,
                Color32::BLACK,
            );
            painter.rect_filled(
                egui::Rect::from_min_max(pos(bottom.min), pos(bottom.max)),
                0.0,
                Color32::BLACK,
            );
        }
    }

    fn show_right_inspector(&mut self, ctx: &egui::Context) {
        let Some(rect) = self.panel_rect(DockPanelKind::RightInspector) else {
            return;
        };
        fixed_vscroll_panel(ctx, "editor.right_inspector", rect, 4, |ui| {
            self.inspector_contents(ui, true);
        });
    }

    fn show_popped_placeholder(&mut self, ctx: &egui::Context) {
        let Some(rect) = self.panel_rect(DockPanelKind::RightInspector) else {
            return;
        };
        fixed_panel(ctx, "editor.right_placeholder", rect, 4, |ui| {
            ui.label(RichText::new("Tool Inspector").strong().color(TEXT));
            ui.label(RichText::new("Popped out").color(TEXT_DIM));
            if ui.button("Dock").clicked()
                && let Err(err) =
                    self.invoke_editor_command_by_id(COMMAND_INSPECTOR_REATTACH, "panel")
            {
                tracing::error!(error = %err, "inspector reattach failed");
            }
        });
    }

    fn show_floating_inspector(&mut self, ctx: &egui::Context) {
        let Some(panel) = self
            .plan
            .panel(DockPanelKind::RightInspector)
            .and_then(|panel| panel.floating_rect_px)
        else {
            return;
        };
        let mut open = true;
        egui::Window::new(self.plan.active_inspector.title.clone())
            .id(egui::Id::new("editor.inspector.floating"))
            .default_pos(pos(panel.min))
            .default_size(size(panel))
            .scroll([true, true])
            .open(&mut open)
            .show(ctx, |ui| {
                self.inspector_contents(ui, false);
            });
        if !open
            && let Err(err) = self.invoke_editor_command_by_id(COMMAND_INSPECTOR_REATTACH, "panel")
        {
            tracing::error!(error = %err, "inspector close reattach failed");
        }
    }

    pub(super) fn duplicate_object_editor_selection(&mut self) {
        let Some(source) = self
            .object_editor_document
            .objects
            .get(&self.object_editor_selected_object)
            .cloned()
        else {
            self.object_editor_last_action = "missing selection".to_string();
            return;
        };
        if source.kind != "Unit" {
            self.object_editor_last_action = "select a Unit".to_string();
            return;
        }
        let new_id = "unit.issue732.guardian_footman";
        if self.object_editor_document.objects.contains_key(new_id) {
            self.object_editor_selected_object = new_id.to_string();
            self.object_editor_last_action = "collision handled".to_string();
            self.mark_object_editor_dirty();
            return;
        }
        let mut duplicate = source.clone();
        duplicate.id = new_id.to_string();
        duplicate.parent = None;
        duplicate
            .properties
            .insert("object.base_id".to_string(), source.id.clone());
        duplicate
            .properties
            .insert("object.rawcode".to_string(), "u732".to_string());
        duplicate.properties.insert(
            "object.display_name".to_string(),
            "Guardian Footman".to_string(),
        );
        duplicate
            .properties
            .insert("object.category".to_string(), "Unit".to_string());
        duplicate
            .properties
            .insert("stats.move_speed".to_string(), "310".to_string());
        duplicate.properties.insert(
            "abilities.normal".to_string(),
            "ability.issue732.defend,ability.issue732.guardian_leap".to_string(),
        );
        match self.object_editor_document.upsert_object(duplicate) {
            Ok(()) => {
                self.object_editor_selected_object = new_id.to_string();
                self.object_editor_last_action = "duplicated".to_string();
                self.mark_object_editor_dirty();
            }
            Err(error) => {
                self.object_editor_last_action = error.to_string();
            }
        }
    }

    pub(super) fn set_object_editor_property(&mut self, key: &str, value: &str) {
        let Some(object) = self
            .object_editor_document
            .objects
            .get_mut(&self.object_editor_selected_object)
        else {
            self.object_editor_last_action = "missing selection".to_string();
            return;
        };
        object.properties.insert(key.to_string(), value.to_string());
        self.object_editor_last_action = format!("{key} updated");
        self.mark_object_editor_dirty();
    }

    pub(super) fn revert_object_editor_selection(&mut self) {
        if self.object_editor_selected_object == "unit.issue732.guardian_footman" {
            self.object_editor_document
                .objects
                .remove("unit.issue732.guardian_footman");
            self.object_editor_selected_object = "base.unit.footman".to_string();
            self.object_editor_last_action = "reverted".to_string();
            self.mark_object_editor_dirty();
        } else {
            self.object_editor_last_action = "base unchanged".to_string();
        }
    }

    fn show_status(&mut self, ctx: &egui::Context) {
        let Some(rect) = self.panel_rect(DockPanelKind::StatusStrip) else {
            return;
        };
        fixed_panel(ctx, "editor.status", rect, 0, |ui| {
            let compact = ui.available_width() < 760.0 || ui.available_height() < 24.0;
            let font = if compact { 12.0 } else { 14.0 };
            let status = self.status_summary();
            let feedback_emphasis = if status.error_count == 0 {
                self.status_feedback_emphasis(ctx)
            } else {
                0.0
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(if compact {
                        "Saved: auto"
                    } else {
                        "Saved State: Auto-saved 2 mins ago"
                    })
                    .size(font)
                    .color(TEXT),
                );
                ui.separator();
                let status_color = if status.error_count == 0 { GOOD } else { ERROR };
                ui.label(
                    RichText::new(status.label(compact))
                        .size(font + feedback_emphasis * 1.5)
                        .color(if feedback_emphasis > 0.0 {
                            GOLD
                        } else {
                            status_color
                        }),
                )
                .on_hover_text(if status.error_count == 0 {
                    "Most recent successful editor action"
                } else {
                    "Editor action requires attention"
                });
                ui.separator();
                if self.play_running() {
                    ui.label(RichText::new("Play Mode Active").size(font).color(GOLD));
                    ui.separator();
                }
                ui.label(
                    RichText::new(if compact {
                        "AI: 1250/5000"
                    } else {
                        "AI Tokens: 1250/5000"
                    })
                    .size(font)
                    .color(TEXT_DIM),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("FPS: 60").size(font).color(TEXT));
                });
            });
        });
    }
}
