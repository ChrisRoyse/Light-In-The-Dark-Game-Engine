use egui::{Color32, RichText, Stroke};
use litd_editor::editor_commands::{
    COMMAND_OPEN_PALETTE, COMMAND_SNAP_GAME_CAMERA, COMMAND_TOGGLE_PLAYTEST,
};

use super::main_editor_commands::{mode_command_id, world_layer_command_id};
use super::main_editor_frame::{fixed_panel, panel_width};
use super::main_editor_play::{PLAY_CAMERA_RIGS, play_camera_rig_label};
use super::main_editor_world_layers::WorldLayerToggle;
use super::*;

impl EditorShellRuntime {
    pub(crate) fn show_top_bar(&mut self, ctx: &egui::Context) {
        let Some(rect) = self.panel_rect(DockPanelKind::TopModeBar) else {
            return;
        };
        fixed_panel(ctx, "editor.top_bar", rect, 0, |ui| {
            ui.horizontal_centered(|ui| {
                let available = panel_width(ui);
                let row_height = ui.available_height().clamp(20.0, 42.0);
                let modes = self.plan.mode_tabs.clone();
                let layout = TopBarLayout::new(available, row_height, &modes, self.play_running());
                ui.spacing_mut().item_spacing.x = layout.item_spacing;

                ui.add_sized(
                    [layout.title_width, row_height],
                    egui::Label::new(
                        RichText::new(layout.title_label)
                            .size(layout.title_font)
                            .color(GOLD),
                    ),
                );
                ui.add_space(layout.leading_gap);
                self.show_command_palette_button(ui, layout.command_width, row_height);
                ui.add_space(6.0);
                self.show_contextual_help_button(ui, layout.help_width, row_height);
                ui.add_space(6.0);
                for mode in modes {
                    self.show_mode_button(
                        ui,
                        mode,
                        layout.mode_width(mode),
                        row_height,
                        layout.mode_font,
                        layout.compact_labels,
                    );
                }
                ui.add_space(layout.trailing_fill(ui.available_width()));
                self.show_map_build_menu(ui, layout.build_width);
                ui.add_space(6.0);
                self.show_view_layers_menu(ui, layout.view_width);
                ui.add_space(6.0);
                if layout.show_rig {
                    self.show_play_camera_rig_combo(ui, layout.rig_width);
                    ui.add_space(6.0);
                }
                self.show_play_toggle(ui, layout.play_width, row_height, layout.play_label);
            });
        });
    }

    fn show_mode_button(
        &mut self,
        ui: &mut egui::Ui,
        mode: EditorMode,
        width: f32,
        height: f32,
        font: f32,
        compact_labels: bool,
    ) {
        let selected = self.plan.active_mode == mode;
        let label = if compact_labels {
            mode.compact_label()
        } else {
            mode.label()
        };
        let button = egui::Button::new(RichText::new(label).size(font).color(if selected {
            TEXT
        } else {
            TEXT_DIM
        }))
        .fill(if selected { GOLD_DEEP } else { PANEL_BG_SOFT })
        .stroke(Stroke::new(
            1.0,
            if selected { GOLD } else { Color32::BLACK },
        ));
        if ui.add_sized([width, height], button).clicked()
            && let Err(err) = self.invoke_editor_command_by_id(mode_command_id(mode), "top_bar")
        {
            tracing::error!(error = %err, "mode switch failed");
        }
    }

    fn show_command_palette_button(&mut self, ui: &mut egui::Ui, width: f32, height: f32) {
        let button = egui::Button::new(RichText::new("Cmd").size(13.0).color(TEXT_DIM))
            .fill(PANEL_BG_SOFT)
            .stroke(Stroke::new(1.0, GOLD_DEEP));
        if ui.add_sized([width, height], button).clicked()
            && let Err(err) = self.invoke_editor_command_by_id(COMMAND_OPEN_PALETTE, "top_bar")
        {
            tracing::error!(error = %err, "command palette open failed");
        }
    }

    fn show_contextual_help_button(&mut self, ui: &mut egui::Ui, width: f32, height: f32) {
        let button = egui::Button::new(RichText::new("Help").size(13.0).color(TEXT_DIM))
            .fill(PANEL_BG_SOFT)
            .stroke(Stroke::new(1.0, GOLD_DEEP));
        if ui.add_sized([width, height], button).clicked() {
            self.open_contextual_help();
        }
    }

    fn show_play_camera_rig_combo(&mut self, ui: &mut egui::Ui, width: f32) {
        egui::ComboBox::from_id_salt("play_camera_rig")
            .width(width)
            .selected_text(play_camera_rig_label(self.play_camera_rig()))
            .show_ui(ui, |ui| {
                for rig in PLAY_CAMERA_RIGS {
                    let changed = ui.selectable_label(
                        self.play_camera_rig() == rig,
                        play_camera_rig_label(rig),
                    );
                    if changed.clicked()
                        && let Err(err) = self.set_play_camera_rig(rig)
                    {
                        tracing::error!(error = %err, rig = ?rig, "play camera rig switch failed");
                    }
                }
            });
    }

    fn show_view_layers_menu(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.allocate_ui_with_layout(
            egui::vec2(width, ui.available_height()),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                egui::ComboBox::from_id_salt("editor_view_layers")
                    .width(width)
                    .selected_text("View")
                    .show_ui(ui, |ui| {
                        ui.set_min_width(260.0);
                        self.show_view_toggle_group(ui, "Scene", SCENE_VIEW_TOGGLES);
                        ui.separator();
                        self.show_view_toggle_group(ui, "Overlays", OVERLAY_VIEW_TOGGLES);
                        ui.separator();
                        self.show_view_toggle_group(ui, "Display", DISPLAY_VIEW_TOGGLES);
                        ui.separator();
                        self.show_accessibility_text_scale(ui);
                        ui.separator();
                        if ui.button("Snap to Game Camera").clicked()
                            && let Err(err) = self
                                .invoke_editor_command_by_id(COMMAND_SNAP_GAME_CAMERA, "top_bar")
                        {
                            tracing::error!(error = %err, "game camera snap failed");
                        }
                    });
            },
        );
    }

    fn show_accessibility_text_scale(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Accessibility").color(GOLD).strong());
        let current = self.accessibility_text_scale();
        egui::ComboBox::from_id_salt("editor_accessibility_text_scale")
            .selected_text(format!("Text {}%", (current * 100.0).round() as u32))
            .show_ui(ui, |ui| {
                for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
                    let label = format!("{}%", (scale * 100.0) as u32);
                    if ui
                        .selectable_label((current - scale).abs() < f32::EPSILON, label)
                        .clicked()
                        && let Err(error) = self.set_accessibility_text_scale(scale)
                    {
                        tracing::error!(
                            %error,
                            text_scale = scale,
                            "editor accessibility text scale rejected"
                        );
                    }
                }
            });
    }

    fn show_view_toggle_group(
        &mut self,
        ui: &mut egui::Ui,
        heading: &'static str,
        toggles: &[WorldLayerToggle],
    ) {
        ui.label(RichText::new(heading).color(GOLD).strong());
        for toggle in toggles {
            let mut checked = self.world_layers.is_enabled(*toggle);
            if ui.checkbox(&mut checked, toggle.label()).changed()
                && let Err(err) =
                    self.invoke_editor_command_by_id(world_layer_command_id(*toggle), "top_bar")
            {
                tracing::error!(
                    error = %err,
                    toggle = ?toggle,
                    "world view layer visibility save failed"
                );
            }
        }
    }

    fn show_play_toggle(
        &mut self,
        ui: &mut egui::Ui,
        width: f32,
        height: f32,
        label: &'static str,
    ) {
        let running = self.play_running();
        let button = egui::Button::new(
            RichText::new(label)
                .size(if height < 30.0 { 13.0 } else { 16.0 })
                .color(TEXT),
        )
        .fill(if running {
            Color32::from_rgb(153, 63, 34)
        } else {
            Color32::from_rgb(143, 107, 48)
        })
        .stroke(Stroke::new(1.0, GOLD));
        if ui.add_sized([width, height], button).clicked()
            && let Err(err) = self.invoke_editor_command_by_id(COMMAND_TOGGLE_PLAYTEST, "top_bar")
        {
            tracing::error!(error = %err, "playtest toggle failed");
        }
    }
}

const SCENE_VIEW_TOGGLES: &[WorldLayerToggle] = &[
    WorldLayerToggle::Terrain,
    WorldLayerToggle::Foliage,
    WorldLayerToggle::Doodads,
    WorldLayerToggle::Units,
    WorldLayerToggle::Water,
    WorldLayerToggle::Blight,
];

const OVERLAY_VIEW_TOGGLES: &[WorldLayerToggle] = &[
    WorldLayerToggle::Grid,
    WorldLayerToggle::CameraBounds,
    WorldLayerToggle::PathingGround,
    WorldLayerToggle::PathingNaval,
    WorldLayerToggle::Regions,
    WorldLayerToggle::CameraObjects,
    WorldLayerToggle::InfoDisplay,
];

const DISPLAY_VIEW_TOGGLES: &[WorldLayerToggle] = &[
    WorldLayerToggle::Shadows,
    WorldLayerToggle::Lighting,
    WorldLayerToggle::Weather,
    WorldLayerToggle::Sky,
    WorldLayerToggle::FogEffects,
    WorldLayerToggle::LetterboxMode,
    WorldLayerToggle::LockToGameCamera,
];

#[derive(Debug, Clone, Copy)]
struct TopBarLayout {
    compact_labels: bool,
    show_rig: bool,
    item_spacing: f32,
    leading_gap: f32,
    title_label: &'static str,
    title_width: f32,
    title_font: f32,
    mode_font: f32,
    command_width: f32,
    help_width: f32,
    rig_width: f32,
    build_width: f32,
    view_width: f32,
    play_width: f32,
    play_label: &'static str,
    #[cfg(test)]
    available_width: f32,
    #[cfg(test)]
    reserved_width: f32,
}

impl TopBarLayout {
    fn new(available: f32, row_height: f32, _modes: &[EditorMode], running: bool) -> Self {
        let compact_labels = available < 1240.0 || row_height < 24.0;
        let show_rig = available >= 1268.0 && row_height >= 30.0;
        let full_title = available >= 1120.0 && row_height >= 28.0;
        let full_play = available >= 1268.0 && row_height >= 30.0;
        let title_width = if full_title {
            if show_rig { 198.0 } else { 170.0 }
        } else {
            64.0
        };
        let play_width = if running {
            82.0
        } else if full_play {
            142.0
        } else {
            96.0
        };
        let rig_width = if show_rig { 128.0 } else { 0.0 };
        let command_width = 48.0;
        let help_width = 48.0;
        let build_width = 78.0;
        let view_width = 74.0;
        #[cfg(test)]
        let mode_width_total: f32 = _modes
            .iter()
            .map(|mode| mode_width(*mode, compact_labels))
            .sum();
        #[cfg(test)]
        let reserved_width = title_width
            + if full_title { 8.0 } else { 4.0 }
            + command_width
            + help_width
            + mode_width_total
            + rig_width
            + if show_rig { 6.0 } else { 0.0 }
            + build_width
            + view_width
            + play_width
            + 6.0 * 9.0;
        Self {
            compact_labels,
            show_rig,
            item_spacing: 6.0,
            leading_gap: if full_title { 8.0 } else { 4.0 },
            title_label: if full_title {
                "Light in the Dark"
            } else {
                "LitD"
            },
            title_width,
            title_font: if full_title { 21.0 } else { 16.0 },
            mode_font: if row_height < 30.0 { 12.0 } else { 15.0 },
            command_width,
            help_width,
            rig_width,
            build_width,
            view_width,
            play_width,
            play_label: play_button_label(running, full_play),
            #[cfg(test)]
            available_width: available,
            #[cfg(test)]
            reserved_width,
        }
    }

    fn mode_width(self, mode: EditorMode) -> f32 {
        mode_width(mode, self.compact_labels)
    }

    fn trailing_fill(self, remaining_after_modes: f32) -> f32 {
        let trailing_controls = self.play_width
            + self.view_width
            + self.build_width
            + 6.0
            + 6.0
            + if self.show_rig {
                self.rig_width + 6.0
            } else {
                0.0
            };
        (remaining_after_modes - trailing_controls).max(0.0)
    }

    #[cfg(test)]
    fn fits(self) -> bool {
        self.reserved_width <= self.available_width + 0.5
    }
}

fn mode_width(mode: EditorMode, compact_labels: bool) -> f32 {
    if compact_labels {
        return match mode {
            EditorMode::Terrain => 74.0,
            EditorMode::Objects => 56.0,
            EditorMode::Triggers => 60.0,
            EditorMode::Cinematics => 58.0,
            EditorMode::Vfx => 54.0,
            EditorMode::AiCoAuthor => 50.0,
        };
    }
    match mode {
        EditorMode::Terrain => 94.0,
        EditorMode::Objects => 102.0,
        EditorMode::Triggers => 110.0,
        EditorMode::Cinematics => 132.0,
        EditorMode::Vfx => 66.0,
        EditorMode::AiCoAuthor => 126.0,
    }
}

fn play_button_label(running: bool, full_play: bool) -> &'static str {
    match (running, full_play) {
        (true, _) => "Stop",
        (false, true) => "Playtest >",
        (false, false) => "Play >",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODES: [EditorMode; 6] = [
        EditorMode::Terrain,
        EditorMode::Objects,
        EditorMode::Triggers,
        EditorMode::Cinematics,
        EditorMode::Vfx,
        EditorMode::AiCoAuthor,
    ];

    #[test]
    fn top_bar_keeps_full_labels_at_wide_fsv_size() {
        let layout = TopBarLayout::new(1252.0, 33.0, &MODES, false);

        assert!(!layout.compact_labels);
        assert!(!layout.show_rig);
        assert_eq!(layout.play_label, "Play >");
        assert!(layout.fits());
        assert!(layout.mode_width(EditorMode::Cinematics) >= 132.0);
        assert!(layout.mode_width(EditorMode::AiCoAuthor) >= 126.0);
    }

    #[test]
    fn top_bar_keeps_primary_controls_inside_enforced_minimum() {
        let layout = TopBarLayout::new(950.0, 27.0, &MODES, false);

        assert!(layout.compact_labels);
        assert!(!layout.show_rig);
        assert_eq!(layout.title_label, "LitD");
        assert_eq!(layout.play_label, "Play >");
        assert!(layout.fits());
    }
}
