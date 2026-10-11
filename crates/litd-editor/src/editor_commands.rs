//! Shared editor command registry for GUI palette, hotkeys, and MCP routing.

use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

#[path = "editor_commands_defaults.rs"]
mod editor_commands_defaults;

pub use editor_commands_defaults::{
    default_editor_command_bindings, default_editor_command_registry,
};

pub const COMMAND_OPEN_PALETTE: &str = "editor.command_palette.open";
pub const COMMAND_MODE_TERRAIN: &str = "editor.mode.terrain";
pub const COMMAND_MODE_OBJECTS: &str = "editor.mode.objects";
pub const COMMAND_MODE_TRIGGERS: &str = "editor.mode.triggers";
pub const COMMAND_MODE_CINEMATICS: &str = "editor.mode.cinematics";
pub const COMMAND_MODE_VFX: &str = "editor.mode.vfx";
pub const COMMAND_MODE_AI_COAUTHOR: &str = "editor.mode.ai_coauthor";
pub const COMMAND_CREATE_WORLD: &str = "editor.world.create";
pub const COMMAND_VALIDATE_WORLD: &str = "editor.world.validate";
pub const COMMAND_MODERATION_SUBMIT_REPORT: &str = "editor.project.moderation.submit_report";
pub const COMMAND_TOGGLE_PLAYTEST: &str = "editor.playtest.toggle";
pub const COMMAND_FRAME_SELECTED: &str = "editor.camera.frame_selected";
pub const COMMAND_SNAP_GAME_CAMERA: &str = "editor.camera.snap_to_game_camera";
pub const COMMAND_INSPECTOR_POP_OUT: &str = "editor.inspector.pop_out";
pub const COMMAND_INSPECTOR_REATTACH: &str = "editor.inspector.reattach";
pub const COMMAND_SAVE_BOOKMARK_1: &str = "editor.camera.bookmark.save.1";
pub const COMMAND_SAVE_BOOKMARK_2: &str = "editor.camera.bookmark.save.2";
pub const COMMAND_SAVE_BOOKMARK_3: &str = "editor.camera.bookmark.save.3";
pub const COMMAND_RESTORE_BOOKMARK_1: &str = "editor.camera.bookmark.restore.1";
pub const COMMAND_RESTORE_BOOKMARK_2: &str = "editor.camera.bookmark.restore.2";
pub const COMMAND_RESTORE_BOOKMARK_3: &str = "editor.camera.bookmark.restore.3";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorCommandRegistry {
    pub schema_version: u32,
    pub parent_issue: u32,
    pub shared_registry_id: String,
    pub commands: Vec<EditorCommandEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorCommandEntry {
    pub command_id: String,
    pub title: String,
    pub search_terms: Vec<String>,
    pub operation_id: String,
    pub gui_route: String,
    pub mcp_tool_name: String,
    pub mutates_shell_state: bool,
    pub default_keybinding: Option<EditorKeyChord>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EditorKeyChord {
    pub key: String,
    #[serde(default)]
    pub command: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorCommandBindings {
    pub bindings: Vec<EditorCommandBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorCommandBinding {
    pub command_id: String,
    pub key_chord: EditorKeyChord,
}

impl EditorKeyChord {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            command: false,
            shift: false,
            alt: false,
        }
    }

    pub fn command(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            command: true,
            shift: false,
            alt: false,
        }
    }

    pub fn alt(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            command: false,
            shift: false,
            alt: true,
        }
    }

    pub fn command_shift(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            command: true,
            shift: true,
            alt: false,
        }
    }

    pub fn format(&self) -> String {
        let mut parts = Vec::new();
        if self.command {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        parts.push(self.key.as_str());
        parts.join("+")
    }

    pub fn to_egui_shortcut(&self) -> Result<egui::KeyboardShortcut, EngineError> {
        Ok(egui::KeyboardShortcut::new(
            egui::Modifiers {
                alt: self.alt,
                ctrl: self.command,
                shift: self.shift,
                mac_cmd: false,
                command: self.command,
            },
            self.to_egui_key()?,
        ))
    }

    fn validate(&self) -> Result<(), EngineError> {
        let _ = self.to_egui_key()?;
        Ok(())
    }

    fn to_egui_key(&self) -> Result<egui::Key, EngineError> {
        match self.key.trim().to_ascii_uppercase().as_str() {
            "A" => Ok(egui::Key::A),
            "B" => Ok(egui::Key::B),
            "C" => Ok(egui::Key::C),
            "D" => Ok(egui::Key::D),
            "E" => Ok(egui::Key::E),
            "F" => Ok(egui::Key::F),
            "G" => Ok(egui::Key::G),
            "H" => Ok(egui::Key::H),
            "I" => Ok(egui::Key::I),
            "J" => Ok(egui::Key::J),
            "K" => Ok(egui::Key::K),
            "L" => Ok(egui::Key::L),
            "M" => Ok(egui::Key::M),
            "N" => Ok(egui::Key::N),
            "O" => Ok(egui::Key::O),
            "P" => Ok(egui::Key::P),
            "Q" => Ok(egui::Key::Q),
            "R" => Ok(egui::Key::R),
            "S" => Ok(egui::Key::S),
            "T" => Ok(egui::Key::T),
            "U" => Ok(egui::Key::U),
            "V" => Ok(egui::Key::V),
            "W" => Ok(egui::Key::W),
            "X" => Ok(egui::Key::X),
            "Y" => Ok(egui::Key::Y),
            "Z" => Ok(egui::Key::Z),
            "0" | "NUM0" => Ok(egui::Key::Num0),
            "1" | "NUM1" => Ok(egui::Key::Num1),
            "2" | "NUM2" => Ok(egui::Key::Num2),
            "3" | "NUM3" => Ok(egui::Key::Num3),
            "4" | "NUM4" => Ok(egui::Key::Num4),
            "5" | "NUM5" => Ok(egui::Key::Num5),
            "6" | "NUM6" => Ok(egui::Key::Num6),
            "ENTER" => Ok(egui::Key::Enter),
            "ESCAPE" => Ok(egui::Key::Escape),
            "SPACE" => Ok(egui::Key::Space),
            key => {
                invalid_editor_command("keybinding.key", format!("unsupported editor key '{key}'"))
            }
        }
    }
}

impl EditorCommandRegistry {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema_version != 1 || self.parent_issue != 1217 {
            return invalid_editor_command(
                "registry.issue_binding",
                "command registry must bind schema 1 to #1217",
            );
        }
        let mut ids = BTreeSet::new();
        for command in &self.commands {
            if !ids.insert(command.command_id.as_str()) {
                return invalid_editor_command(
                    "registry.command_id",
                    format!("duplicate command {}", command.command_id),
                );
            }
            if command.operation_id != command.command_id
                || !command.gui_route.starts_with("egui://editor.commands/")
                || !command.mcp_tool_name.starts_with("litd.editor.commands.")
            {
                return invalid_editor_command(
                    "registry.route",
                    format!("command {} has inconsistent routes", command.command_id),
                );
            }
            if let Some(chord) = &command.default_keybinding {
                chord.validate()?;
            }
        }
        Ok(())
    }

    pub fn contains(&self, command_id: &str) -> bool {
        self.commands
            .iter()
            .any(|command| command.command_id == command_id)
    }

    pub fn get(&self, command_id: &str) -> Option<&EditorCommandEntry> {
        self.commands
            .iter()
            .find(|command| command.command_id == command_id)
    }

    pub fn search(&self, query: &str) -> Vec<&EditorCommandEntry> {
        let needle = query.trim().to_ascii_lowercase();
        self.commands
            .iter()
            .filter(|command| {
                needle.is_empty()
                    || command.command_id.contains(&needle)
                    || command.title.to_ascii_lowercase().contains(&needle)
                    || command
                        .search_terms
                        .iter()
                        .any(|term| term.to_ascii_lowercase().contains(&needle))
            })
            .collect()
    }
}

impl EditorCommandBindings {
    pub fn defaults(registry: &EditorCommandRegistry) -> Self {
        let mut bindings = Self {
            bindings: registry
                .commands
                .iter()
                .filter_map(|command| {
                    command
                        .default_keybinding
                        .clone()
                        .map(|key_chord| EditorCommandBinding {
                            command_id: command.command_id.clone(),
                            key_chord,
                        })
                })
                .collect(),
        };
        bindings.sort_for_dispatch();
        bindings
    }

    pub fn validate(&self, registry: &EditorCommandRegistry) -> Result<(), EngineError> {
        registry.validate()?;
        let mut command_ids = BTreeSet::new();
        let mut chords = BTreeMap::<EditorKeyChord, &str>::new();
        for binding in &self.bindings {
            if !registry.contains(&binding.command_id) {
                return invalid_editor_command(
                    "keybinding.command_id",
                    format!("unknown command {}", binding.command_id),
                );
            }
            if !command_ids.insert(binding.command_id.as_str()) {
                return invalid_editor_command(
                    "keybinding.command_id",
                    format!("duplicate binding for {}", binding.command_id),
                );
            }
            binding.key_chord.validate()?;
            if let Some(existing) = chords.insert(binding.key_chord.clone(), &binding.command_id) {
                return invalid_editor_command(
                    "keybinding.conflict",
                    format!(
                        "{} is bound to both {} and {}",
                        binding.key_chord.format(),
                        existing,
                        binding.command_id
                    ),
                );
            }
        }
        Ok(())
    }

    pub fn replace_binding(
        &mut self,
        registry: &EditorCommandRegistry,
        command_id: &str,
        key_chord: EditorKeyChord,
    ) -> Result<(), EngineError> {
        if !registry.contains(command_id) {
            return invalid_editor_command(
                "keybinding.command_id",
                format!("unknown command {command_id}"),
            );
        }
        key_chord.validate()?;
        if let Some(binding) = self
            .bindings
            .iter_mut()
            .find(|binding| binding.command_id == command_id)
        {
            binding.key_chord = key_chord;
        } else {
            self.bindings.push(EditorCommandBinding {
                command_id: command_id.to_string(),
                key_chord,
            });
        }
        self.sort_for_dispatch();
        self.validate(registry)
    }

    pub fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    pub fn dispatch_order(&self) -> Vec<EditorCommandBinding> {
        let mut bindings = self.bindings.clone();
        bindings.sort_by(binding_dispatch_order);
        bindings
    }

    fn sort_for_dispatch(&mut self) {
        self.bindings.sort_by(binding_dispatch_order);
    }
}

fn binding_dispatch_order(
    left: &EditorCommandBinding,
    right: &EditorCommandBinding,
) -> std::cmp::Ordering {
    shortcut_specificity(&right.key_chord)
        .cmp(&shortcut_specificity(&left.key_chord))
        .then(left.key_chord.cmp(&right.key_chord))
        .then(left.command_id.cmp(&right.command_id))
}

fn shortcut_specificity(chord: &EditorKeyChord) -> u8 {
    chord.command as u8 + chord.shift as u8 + chord.alt as u8
}

fn invalid_editor_command<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.commands.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "editor_commands_tests.rs"]
mod tests;
