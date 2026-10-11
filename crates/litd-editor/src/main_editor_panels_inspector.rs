//! Inspector panel body, per-mode tool controls and the compact slider/placement/foliage
//! widgets for the editor shell (#1466 split of `main_editor_panels.rs`).
//!
//! A continuation `impl EditorShellRuntime` plus the module-private widget helpers. Nested so
//! it reaches `EditorShellRuntime`'s private fields and the crate's `pub(super)` control
//! methods; `inspector_contents` is `pub(super)` so the panel scaffolding calls it unchanged.

use std::ops::RangeInclusive;

use egui::RichText;
use litd_editor::editor_commands::{COMMAND_INSPECTOR_POP_OUT, COMMAND_INSPECTOR_REATTACH};

use super::super::main_editor_asset_entries::compact_asset_label;
use super::super::main_editor_frame::panel_width;
use super::super::*;
use crate::main_editor_assets::draw_asset_icon;

impl PlacementMode {
    fn label(self) -> &'static str {
        match self {
            Self::RandomRotation => "Random Rotation",
            Self::AlignToNormal => "Align To Normal",
            Self::GridSnap => "Grid Snap",
        }
    }
}

impl EditorShellRuntime {
    pub(super) fn inspector_contents(&mut self, ui: &mut egui::Ui, docked: bool) {
        let compact = panel_width(ui) < 260.0;
        let dense = panel_width(ui) < 235.0;
        let title = if compact {
            if dense {
                self.plan.active_mode.compact_label().to_string()
            } else {
                format!("{} Brush", self.plan.active_mode.label())
            }
        } else {
            format!(
                "{}  /  {}",
                self.plan.active_mode.label(),
                self.plan.active_inspector.title
            )
        };
        let dock_label = match (compact, docked) {
            (true, true) => "Pop",
            (_, true) => "Pop Out",
            _ => "Dock",
        };
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(title)
                    .size(if compact { 13.0 } else { 14.0 })
                    .strong()
                    .color(TEXT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(dock_label).clicked() {
                    let command_id = if docked {
                        COMMAND_INSPECTOR_POP_OUT
                    } else {
                        COMMAND_INSPECTOR_REATTACH
                    };
                    if let Err(err) = self.invoke_editor_command_by_id(command_id, "panel") {
                        tracing::error!(error = %err, "inspector dock toggle failed");
                    }
                }
            });
        });
        ui.separator();
        self.intelligence_surfaces_panel(ui);
        ui.separator();
        match self.plan.active_mode {
            EditorMode::Terrain => self.terrain_controls(ui),
            EditorMode::Objects => self.object_controls(ui),
            EditorMode::Triggers => self.trigger_controls(ui),
            EditorMode::AiCoAuthor => self.ai_controls(ui),
            EditorMode::Vfx => self.vfx_controls(ui),
            mode => self.generic_tool_controls(ui, mode),
        }
    }

    fn terrain_controls(&mut self, ui: &mut egui::Ui) {
        let compact = panel_width(ui) < 260.0;
        let short = ui.available_height() < 470.0;
        self.world_creation_controls(ui);
        ui.separator();
        compact_slider(ui, "Brush Radius", &mut self.brush_radius, 4.0..=128.0);
        compact_slider(ui, "Falloff", &mut self.falloff, 0.0..=1.0);
        placement_control(ui, &mut self.placement);
        compact_slider(ui, "Scale Min", &mut self.scale_min, 0.2..=2.0);
        compact_slider(ui, "Scale Max", &mut self.scale_max, 0.2..=2.5);
        compact_slider(ui, "Density", &mut self.density, 0.0..=100.0);
        foliage_brush_buttons(ui, self);
        self.camera_navigation_controls(ui);
        self.selected_asset_provenance(ui);
        ui.separator();
        let columns = ((panel_width(ui) + 8.0) / 58.0).floor().clamp(2.0, 4.0) as usize;
        let icon_size = if short {
            38.0
        } else if compact {
            44.0
        } else {
            50.0
        };
        let palette_limit = if short {
            columns
        } else if compact {
            4
        } else {
            8
        };
        egui::Grid::new("doodad_palette")
            .num_columns(columns)
            .spacing([if short { 6.0 } else { 8.0 }, if short { 6.0 } else { 8.0 }])
            .show(ui, |ui| {
                for index in 0..self.asset_entries.len().min(palette_limit) {
                    let asset = self.asset_entries[index].clone();
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(icon_size, icon_size),
                        egui::Sense::click(),
                    );
                    draw_asset_icon(ui.painter(), rect, asset.icon, self.selected_asset == index);
                    let response = response
                        .on_hover_text(format!("{}\n{}", asset.source, asset.relative_path));
                    if response.clicked() {
                        self.select_asset(index);
                    }
                    if (index + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
    }

    fn selected_asset_provenance(&mut self, ui: &mut egui::Ui) {
        let Some(asset) = self.selected_asset_entry() else {
            ui.separator();
            ui.label(
                RichText::new("Selected: asset vault not loaded for Objects mode startup")
                    .size(12.0)
                    .strong()
                    .color(GOLD),
            );
            return;
        };
        let short_sha = asset.sha256.chars().take(12).collect::<String>();
        ui.separator();
        ui.label(
            RichText::new(format!("Selected: {}", compact_asset_label(&asset.label)))
                .size(12.0)
                .strong()
                .color(GOLD),
        );
        ui.label(
            RichText::new(format!(
                "{} / {} / {}",
                asset.source, asset.asset_type, asset.license
            ))
            .size(11.0)
            .color(TEXT_DIM),
        );
        ui.label(
            RichText::new(format!("{} / sha {}", asset.status, short_sha))
                .size(11.0)
                .color(TEXT_DIM),
        );
        self.model_previewer_panel(ui);
    }

    fn generic_tool_controls(&mut self, ui: &mut egui::Ui, mode: EditorMode) {
        ui.label(RichText::new(self.plan.active_inspector.tool_id.clone()).color(TEXT_DIM));
        for control in self.plan.active_inspector.primary_controls.clone() {
            ui.add_sized(
                [panel_width(ui), 28.0],
                egui::Button::new(format!("{}: {}", mode.label(), control)),
            );
        }
    }
}

fn compact_slider(ui: &mut egui::Ui, label: &str, value: &mut f32, range: RangeInclusive<f32>) {
    ui.horizontal(|ui| {
        let available = panel_width(ui);
        let value_text = compact_slider_value(label, *value);
        if available < 230.0 {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [(available - 52.0).max(96.0), 18.0],
                        egui::Label::new(RichText::new(label).color(TEXT_DIM).size(12.0)),
                    );
                    ui.add_sized(
                        [44.0, 18.0],
                        egui::Label::new(RichText::new(value_text).color(TEXT).size(12.0)),
                    );
                });
                ui.add_sized(
                    [available, 18.0],
                    egui::Slider::new(value, range).show_value(false),
                );
            });
            return;
        }
        let label_width = 74.0_f32.min(available * 0.42);
        let value_width = if available < 210.0 { 34.0 } else { 44.0 };
        let slider_width = (available - label_width - value_width - 14.0).max(42.0);
        ui.add_sized(
            [label_width, 18.0],
            egui::Label::new(RichText::new(label).color(TEXT_DIM).size(12.0)),
        );
        ui.add_sized(
            [slider_width, 18.0],
            egui::Slider::new(value, range).show_value(false),
        );
        ui.add_sized(
            [value_width, 18.0],
            egui::Label::new(RichText::new(value_text).color(TEXT).size(12.0)),
        );
    });
}

fn placement_control(ui: &mut egui::Ui, placement: &mut PlacementMode) {
    let available = panel_width(ui);
    let add_combo = |ui: &mut egui::Ui, width: f32, placement: &mut PlacementMode| {
        egui::ComboBox::from_id_salt("terrain_placement")
            .width(width)
            .selected_text(placement.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(placement, PlacementMode::RandomRotation, "Random Rotation");
                ui.selectable_value(placement, PlacementMode::AlignToNormal, "Align");
                ui.selectable_value(placement, PlacementMode::GridSnap, "Grid Snap");
            });
    };
    if available < 230.0 {
        ui.vertical(|ui| {
            ui.label(RichText::new("Placement").color(TEXT_DIM).size(12.0));
            add_combo(ui, available, placement);
        });
    } else {
        ui.horizontal(|ui| {
            ui.add_sized(
                [74.0, 18.0],
                egui::Label::new(RichText::new("Placement").color(TEXT_DIM).size(12.0)),
            );
            add_combo(ui, (available - 82.0).max(92.0), placement);
        });
    }
}

fn foliage_brush_buttons(ui: &mut egui::Ui, runtime: &mut EditorShellRuntime) {
    let available = panel_width(ui);
    let button_width = ((available - 8.0) * 0.5).max(68.0);
    ui.horizontal(|ui| {
        if ui
            .add_sized([button_width, 24.0], egui::Button::new("Add Foliage"))
            .clicked()
            && let Err(err) = runtime.add_foliage_density_brush()
        {
            tracing::error!(error = %err, "foliage density brush add failed");
            runtime.record_command_error(
                "editor.foliage.add_brush",
                "panel",
                format_args!("{err}"),
            );
        }
        if ui
            .add_sized([button_width, 24.0], egui::Button::new("Erase"))
            .clicked()
            && let Err(err) = runtime.erase_foliage_density_brush()
        {
            tracing::error!(error = %err, "foliage density brush erase failed");
            runtime.record_command_error(
                "editor.foliage.erase_brush",
                "panel",
                format_args!("{err}"),
            );
        }
    });
}

fn compact_slider_value(label: &str, value: f32) -> String {
    if label == "Falloff" || label.starts_with("Scale") {
        format!("{value:.2}")
    } else {
        format!("{value:.0}")
    }
}
