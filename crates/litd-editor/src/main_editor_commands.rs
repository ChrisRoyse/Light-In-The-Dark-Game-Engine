use egui::{RichText, Stroke, StrokeKind};
use litd_editor::{
    editor_commands::{
        COMMAND_CREATE_WORLD, COMMAND_FRAME_SELECTED, COMMAND_INSPECTOR_POP_OUT,
        COMMAND_INSPECTOR_REATTACH, COMMAND_MODE_AI_COAUTHOR, COMMAND_MODE_CINEMATICS,
        COMMAND_MODE_OBJECTS, COMMAND_MODE_TERRAIN, COMMAND_MODE_TRIGGERS, COMMAND_MODE_VFX,
        COMMAND_MODERATION_SUBMIT_REPORT, COMMAND_OPEN_PALETTE, COMMAND_RESTORE_BOOKMARK_1,
        COMMAND_RESTORE_BOOKMARK_2, COMMAND_RESTORE_BOOKMARK_3, COMMAND_SAVE_BOOKMARK_1,
        COMMAND_SAVE_BOOKMARK_2, COMMAND_SAVE_BOOKMARK_3, COMMAND_SNAP_GAME_CAMERA,
        COMMAND_TOGGLE_PLAYTEST, COMMAND_VALIDATE_WORLD, EditorCommandEntry, EditorCommandRegistry,
        EditorKeyChord,
    },
    shell::EditorMode,
};

use super::main_editor_world_layers::WorldLayerToggle;
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorCommandOutcome {
    pub known: bool,
    pub command_id: String,
    pub source: String,
}

impl EditorShellRuntime {
    pub(super) fn handle_editor_command_shortcuts(&mut self, ctx: &egui::Context) {
        let bindings = self.shell_state.command_keybindings().dispatch_order();
        for binding in bindings {
            let shortcut = match binding.key_chord.to_egui_shortcut() {
                Ok(shortcut) => shortcut,
                Err(err) => {
                    tracing::error!(error = %err, command_id = %binding.command_id, "editor keybinding conversion failed");
                    continue;
                }
            };
            let pressed = ctx.input_mut(|input| input.consume_shortcut(&shortcut));
            if pressed
                && let Err(err) = self.invoke_editor_command_by_id(&binding.command_id, "hotkey")
            {
                tracing::error!(error = %err, command_id = %binding.command_id, "editor command hotkey failed");
            }
        }
    }

    pub(super) fn show_command_palette(&mut self, ctx: &egui::Context) {
        if !self.command_palette_open {
            return;
        }
        let matches = self
            .command_registry
            .search(&self.command_palette_query)
            .into_iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>();
        let mut selected = None;
        let mut submit_query = false;
        let mut close_palette = false;
        let mut open = self.command_palette_open;
        egui::Window::new("Command Palette")
            .id(egui::Id::new("editor.command_palette"))
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL_BG)
                    .stroke(Stroke::new(1.0, GOLD_DEEP))
                    .corner_radius(4.0)
                    .inner_margin(10.0),
            )
            .open(&mut open)
            .show(ctx, |ui| {
                let search_id = egui::Id::new("editor.command_palette.search");
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.command_palette_query)
                        .id(search_id)
                        .desired_width(f32::INFINITY),
                );
                if self.command_palette_focus_pending {
                    search.request_focus();
                    self.command_palette_focus_pending = false;
                    tracing::debug!(
                        query = %self.command_palette_query,
                        "command palette search focus requested"
                    );
                }
                if search.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    submit_query = true;
                }
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    close_palette = true;
                }
                ui.separator();
                for command in &matches {
                    if command_row(ui, command, &self.command_registry).clicked() {
                        selected = Some(command.command_id.clone());
                    }
                }
                if matches.is_empty() {
                    ui.label(RichText::new("No commands").color(TEXT_DIM));
                }
            });
        self.command_palette_open = open && !close_palette;
        if !self.command_palette_open {
            self.command_palette_focus_pending = false;
        }
        if let Some(command_id) = selected {
            self.command_palette_open = false;
            self.command_palette_focus_pending = false;
            if let Err(err) = self.invoke_editor_command_by_id(&command_id, "palette") {
                tracing::error!(error = %err, command_id, "editor command palette invocation failed");
            }
        } else if submit_query && let Err(err) = self.submit_command_palette_query("palette_enter")
        {
            tracing::error!(
                error = %err,
                query = %self.command_palette_query,
                "editor command palette enter invocation failed"
            );
        }
    }

    pub(crate) fn open_command_palette(&mut self) {
        self.command_palette_open = true;
        self.command_palette_query.clear();
        self.command_palette_focus_pending = true;
    }

    pub(crate) fn submit_command_palette_query(
        &mut self,
        source: &'static str,
    ) -> Result<EditorCommandOutcome, EngineError> {
        let query = self.command_palette_query.trim();
        if query.is_empty() {
            tracing::warn!(source, "command palette submit rejected empty query");
            return Ok(EditorCommandOutcome {
                known: false,
                command_id: String::new(),
                source: source.to_string(),
            });
        }
        let Some(command_id) = self
            .command_registry
            .search(query)
            .first()
            .map(|command| command.command_id.clone())
        else {
            tracing::warn!(query, source, "command palette submit found no command");
            return Ok(EditorCommandOutcome {
                known: false,
                command_id: query.to_string(),
                source: source.to_string(),
            });
        };
        self.command_palette_open = false;
        self.command_palette_focus_pending = false;
        tracing::info!(
            query,
            command_id,
            source,
            "command palette submit dispatching command"
        );
        self.invoke_editor_command_by_id(&command_id, source)
    }

    #[cfg(test)]
    pub(crate) fn invoke_editor_command_search(
        &mut self,
        query: &str,
        source: &'static str,
    ) -> Result<EditorCommandOutcome, EngineError> {
        let Some(command_id) = self
            .command_registry
            .search(query)
            .first()
            .map(|command| command.command_id.clone())
        else {
            tracing::warn!(
                query,
                source,
                "editor command palette search returned no command"
            );
            return Ok(EditorCommandOutcome {
                known: false,
                command_id: query.to_string(),
                source: source.to_string(),
            });
        };
        self.invoke_editor_command_by_id(&command_id, source)
    }

    pub(crate) fn invoke_editor_command_by_id(
        &mut self,
        command_id: &str,
        source: &'static str,
    ) -> Result<EditorCommandOutcome, EngineError> {
        let Some(command) = self.command_registry.get(command_id).cloned() else {
            // A hotkey/palette binding that resolves to no registered command is a real
            // misconfiguration, not user typo noise: surface it so a stale binding does
            // not silently no-op (#1489). The `known: false` return is preserved for
            // callers that branch on it.
            tracing::warn!(command_id, source, "unknown editor command ignored");
            self.record_command_error(
                command_id,
                source,
                format_args!("no command is registered for id '{command_id}'"),
            );
            return Ok(EditorCommandOutcome {
                known: false,
                command_id: command_id.to_string(),
                source: source.to_string(),
            });
        };
        if let Err(error) = self.execute_known_command(command_id) {
            self.record_command_error(command_id, source, format_args!("{error}"));
            return Err(error);
        }
        self.command_invocation_count = self.command_invocation_count.saturating_add(1);
        self.last_command_id = command.command_id;
        self.last_command_source = source.to_string();
        self.clear_command_error();
        Ok(EditorCommandOutcome {
            known: true,
            command_id: command_id.to_string(),
            source: source.to_string(),
        })
    }

    /// Record a command/action failure for the status bar. `source` names the entry
    /// point (hotkey, palette, top_bar, panel, ...). Kept alongside `tracing::error!`
    /// so the failure is both logged and user-visible (#1489, #800 §4).
    pub(crate) fn record_command_error(
        &mut self,
        command_id: &str,
        source: &str,
        reason: std::fmt::Arguments<'_>,
    ) {
        self.last_command_error = format!("{command_id} ({source}): {reason}");
        self.command_error_count = self.command_error_count.saturating_add(1);
    }

    pub(crate) fn clear_command_error(&mut self) {
        self.last_command_error.clear();
    }

    pub(crate) fn last_command_error_message(&self) -> Option<&str> {
        let trimmed = self.last_command_error.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(self.last_command_error.as_str())
        }
    }

    #[cfg(test)]
    pub(crate) fn invoke_editor_keybinding(
        &mut self,
        key_chord: &EditorKeyChord,
    ) -> Result<EditorCommandOutcome, EngineError> {
        let Some(binding) = self
            .shell_state
            .command_keybindings()
            .bindings
            .iter()
            .find(|binding| &binding.key_chord == key_chord)
        else {
            return Ok(EditorCommandOutcome {
                known: false,
                command_id: key_chord.format(),
                source: "hotkey".to_string(),
            });
        };
        let command_id = binding.command_id.clone();
        self.invoke_editor_command_by_id(&command_id, "hotkey")
    }

    #[cfg(test)]
    pub(crate) fn rebind_editor_command(
        &mut self,
        command_id: &str,
        key_chord: EditorKeyChord,
    ) -> Result<(), EngineError> {
        self.shell_state.replace_command_keybinding(
            &self.command_registry,
            command_id,
            key_chord,
        )?;
        self.shell_state.save_plan(&self.plan)
    }

    fn execute_known_command(&mut self, command_id: &str) -> Result<(), EngineError> {
        match command_id {
            COMMAND_OPEN_PALETTE => {
                self.open_command_palette();
                Ok(())
            }
            COMMAND_MODE_TERRAIN => self.switch_mode(EditorMode::Terrain),
            COMMAND_MODE_OBJECTS => self.switch_mode(EditorMode::Objects),
            COMMAND_MODE_TRIGGERS => self.switch_mode(EditorMode::Triggers),
            COMMAND_MODE_CINEMATICS => self.switch_mode(EditorMode::Cinematics),
            COMMAND_MODE_VFX => self.switch_mode(EditorMode::Vfx),
            COMMAND_MODE_AI_COAUTHOR => self.switch_mode(EditorMode::AiCoAuthor),
            COMMAND_CREATE_WORLD => self.create_world_from_draft_command(),
            COMMAND_VALIDATE_WORLD => self.validate_active_world_command(),
            COMMAND_MODERATION_SUBMIT_REPORT => self.submit_moderation_report_command(),
            COMMAND_TOGGLE_PLAYTEST => self.toggle_playtest(),
            COMMAND_FRAME_SELECTED => self.frame_selected_world_view_camera().map(|_| ()),
            COMMAND_SNAP_GAME_CAMERA => self.snap_to_game_camera(),
            COMMAND_INSPECTOR_POP_OUT => self.pop_out_inspector(),
            COMMAND_INSPECTOR_REATTACH => self.reattach_inspector(),
            COMMAND_SAVE_BOOKMARK_1 => self.save_world_view_camera_bookmark(1),
            COMMAND_SAVE_BOOKMARK_2 => self.save_world_view_camera_bookmark(2),
            COMMAND_SAVE_BOOKMARK_3 => self.save_world_view_camera_bookmark(3),
            COMMAND_RESTORE_BOOKMARK_1 => self.restore_world_view_camera_bookmark(1).map(|_| ()),
            COMMAND_RESTORE_BOOKMARK_2 => self.restore_world_view_camera_bookmark(2).map(|_| ()),
            COMMAND_RESTORE_BOOKMARK_3 => self.restore_world_view_camera_bookmark(3).map(|_| ()),
            other => self.execute_world_layer_command(other),
        }
    }

    fn execute_world_layer_command(&mut self, command_id: &str) -> Result<(), EngineError> {
        let toggle = match command_id.strip_prefix("editor.view.toggle_") {
            Some("terrain") => WorldLayerToggle::Terrain,
            Some("foliage") => WorldLayerToggle::Foliage,
            Some("doodads") => WorldLayerToggle::Doodads,
            Some("units") => WorldLayerToggle::Units,
            Some("water") => WorldLayerToggle::Water,
            Some("blight") => WorldLayerToggle::Blight,
            Some("shadows") => WorldLayerToggle::Shadows,
            Some("lighting") => WorldLayerToggle::Lighting,
            Some("weather") => WorldLayerToggle::Weather,
            Some("sky") => WorldLayerToggle::Sky,
            Some("fog_effects") => WorldLayerToggle::FogEffects,
            Some("grid") => WorldLayerToggle::Grid,
            Some("camera_bounds") => WorldLayerToggle::CameraBounds,
            Some("pathing_ground") => WorldLayerToggle::PathingGround,
            Some("pathing_naval") => WorldLayerToggle::PathingNaval,
            Some("regions") => WorldLayerToggle::Regions,
            Some("camera_objects") => WorldLayerToggle::CameraObjects,
            Some("info_display") => WorldLayerToggle::InfoDisplay,
            Some("letterbox_mode") => WorldLayerToggle::LetterboxMode,
            Some("lock_to_game_camera") => WorldLayerToggle::LockToGameCamera,
            _ => {
                return invalid_shell_runtime(
                    "commands.dispatch",
                    format!("registered command {command_id} has no runtime handler"),
                );
            }
        };
        let next = !self.world_layers.is_enabled(toggle);
        self.set_world_layer_visible(toggle, next)
    }
}

fn command_row(
    ui: &mut egui::Ui,
    command: &EditorCommandEntry,
    registry: &EditorCommandRegistry,
) -> egui::Response {
    let shortcut = command
        .default_keybinding
        .as_ref()
        .map(EditorKeyChord::format)
        .unwrap_or_default();
    let row_size = egui::vec2(ui.available_width().max(220.0), 28.0);
    let (rect, response) = ui.allocate_exact_size(row_size, egui::Sense::click());
    let fill = if response.hovered() {
        Color32::from_rgb(43, 55, 75)
    } else {
        PANEL_BG_SOFT
    };
    let stroke = if response.hovered() {
        Stroke::new(1.0, GOLD)
    } else {
        Stroke::new(1.0, GOLD_DEEP)
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter()
        .rect_stroke(rect, 4.0, stroke, StrokeKind::Inside);
    let title_pos = egui::pos2(rect.left() + 10.0, rect.center().y);
    ui.painter().text(
        title_pos,
        egui::Align2::LEFT_CENTER,
        &command.title,
        egui::FontId::proportional(14.0),
        TEXT,
    );
    if registry.get(&command.command_id).is_none() {
        ui.painter().text(
            egui::pos2(rect.right() - 10.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            "invalid",
            egui::FontId::proportional(12.0),
            ERROR,
        );
    } else if !shortcut.is_empty() {
        ui.painter().text(
            egui::pos2(rect.right() - 10.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            shortcut,
            egui::FontId::proportional(12.0),
            TEXT_DIM,
        );
    }
    response
}

pub(super) fn mode_command_id(mode: EditorMode) -> &'static str {
    match mode {
        EditorMode::Terrain => COMMAND_MODE_TERRAIN,
        EditorMode::Objects => COMMAND_MODE_OBJECTS,
        EditorMode::Triggers => COMMAND_MODE_TRIGGERS,
        EditorMode::Cinematics => COMMAND_MODE_CINEMATICS,
        EditorMode::Vfx => COMMAND_MODE_VFX,
        EditorMode::AiCoAuthor => COMMAND_MODE_AI_COAUTHOR,
    }
}

pub(super) fn save_bookmark_command_id(slot: u8) -> Option<&'static str> {
    match slot {
        1 => Some(COMMAND_SAVE_BOOKMARK_1),
        2 => Some(COMMAND_SAVE_BOOKMARK_2),
        3 => Some(COMMAND_SAVE_BOOKMARK_3),
        _ => None,
    }
}

pub(super) fn restore_bookmark_command_id(slot: u8) -> Option<&'static str> {
    match slot {
        1 => Some(COMMAND_RESTORE_BOOKMARK_1),
        2 => Some(COMMAND_RESTORE_BOOKMARK_2),
        3 => Some(COMMAND_RESTORE_BOOKMARK_3),
        _ => None,
    }
}

pub(super) fn world_layer_command_id(toggle: WorldLayerToggle) -> &'static str {
    match toggle {
        WorldLayerToggle::Terrain => "editor.view.toggle_terrain",
        WorldLayerToggle::Foliage => "editor.view.toggle_foliage",
        WorldLayerToggle::Doodads => "editor.view.toggle_doodads",
        WorldLayerToggle::Units => "editor.view.toggle_units",
        WorldLayerToggle::Water => "editor.view.toggle_water",
        WorldLayerToggle::Blight => "editor.view.toggle_blight",
        WorldLayerToggle::Shadows => "editor.view.toggle_shadows",
        WorldLayerToggle::Lighting => "editor.view.toggle_lighting",
        WorldLayerToggle::Weather => "editor.view.toggle_weather",
        WorldLayerToggle::Sky => "editor.view.toggle_sky",
        WorldLayerToggle::FogEffects => "editor.view.toggle_fog_effects",
        WorldLayerToggle::Grid => "editor.view.toggle_grid",
        WorldLayerToggle::CameraBounds => "editor.view.toggle_camera_bounds",
        WorldLayerToggle::PathingGround => "editor.view.toggle_pathing_ground",
        WorldLayerToggle::PathingNaval => "editor.view.toggle_pathing_naval",
        WorldLayerToggle::Regions => "editor.view.toggle_regions",
        WorldLayerToggle::CameraObjects => "editor.view.toggle_camera_objects",
        WorldLayerToggle::InfoDisplay => "editor.view.toggle_info_display",
        WorldLayerToggle::LetterboxMode => "editor.view.toggle_letterbox_mode",
        WorldLayerToggle::LockToGameCamera => "editor.view.toggle_lock_to_game_camera",
    }
}

#[cfg(test)]
#[path = "main_editor_commands_tests.rs"]
mod tests;
