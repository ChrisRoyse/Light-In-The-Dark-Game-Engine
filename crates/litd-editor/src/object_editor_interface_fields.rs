//! #828 Game Interface field set for the object-data editor, lowered through the #829 valve.
//! A per-project UI customization record carries UI string overrides (keyed message table),
//! player color palette overrides, command-button icon overrides, a command-card layout (grid
//! positions validated: no duplicate cell, cells inside the 4x3 card), and game-message
//! display params. Copy-from-base semantics: defaults ship on the base object, the project's
//! overrides are deltas merged by `copy_base_object`, and lowering emits the RESOLVED set.
//!
//! Variable-count overrides live under authored prefixes (`interface.string.<key>`,
//! `interface.color.<slot>`, `interface.icon.<command>`, `interface.cmd.<command>`), collected
//! deterministically here into sorted maps.

use std::collections::BTreeMap;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::lower::{LoweredArtifactDoc, LoweringProvenance, u32_field};
use super::validation::object_editor_error;
use crate::history::{EditorDocument, EditorObject};

/// Authored key prefixes for the variable-count interface override tables.
pub const INTERFACE_STRING_PREFIX: &str = "interface.string.";
pub const INTERFACE_COLOR_PREFIX: &str = "interface.color.";
pub const INTERFACE_ICON_PREFIX: &str = "interface.icon.";
pub const INTERFACE_CMD_PREFIX: &str = "interface.cmd.";

/// The command card is four columns wide and three rows tall (WC3 parity).
pub const COMMAND_CARD_COLS: u32 = 4;
pub const COMMAND_CARD_ROWS: u32 = 3;
/// Max game-message on-screen duration in seconds.
pub const MESSAGE_DURATION_SECS_MAX: u32 = 3_600;
/// Message font size as a percentage of the base UI font.
pub const MESSAGE_FONT_PCT_MIN: u32 = 25;
pub const MESSAGE_FONT_PCT_MAX: u32 = 400;

/// The overrideable scalar #828 interface property keys.
pub const INTERFACE_SCALAR_PROPERTY_KEYS: &[&str] = &[
    "interface.message_duration_secs",
    "interface.message_font_pct",
];

/// A validated command-card cell (0-indexed col/row inside the 4x3 card).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandCardCell {
    pub col: u32,
    pub row: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterfaceRecord {
    pub ui_strings: BTreeMap<String, String>,
    pub color_overrides: BTreeMap<String, String>,
    pub icon_overrides: BTreeMap<String, String>,
    pub command_cells: BTreeMap<String, CommandCardCell>,
    pub message_duration_secs: u32,
    pub message_font_pct: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredInterfaceDocument {
    pub schema_version: u32,
    pub lowering_fingerprint: String,
    pub object_id: String,
    pub base_object_id: String,
    pub display_name: String,
    pub interface: InterfaceRecord,
    pub provenance: LoweringProvenance,
}

impl LoweredArtifactDoc for ObjectEditorLoweredInterfaceDocument {
    const SCHEMA_VERSION: u32 = 1;
    fn schema_version(&self) -> u32 {
        self.schema_version
    }
    fn fingerprint(&self) -> &str {
        &self.lowering_fingerprint
    }
    fn set_fingerprint(&mut self, fingerprint: String) {
        self.lowering_fingerprint = fingerprint;
    }
    fn provenance(&self) -> &LoweringProvenance {
        &self.provenance
    }
    fn validate_content(&self) -> Result<(), EngineError> {
        if self.object_id.trim().is_empty()
            || self.base_object_id.trim().is_empty()
            || self.display_name.trim().is_empty()
        {
            return Err(object_editor_error(
                "interface.required",
                "lowered interface TOML is missing required identity data",
            ));
        }
        validate_interface_record(&self.interface)
    }
}

/// Deterministically collect the resolved interface record from a persisted object.
pub(crate) fn build_interface_record(
    object: &EditorObject,
) -> Result<InterfaceRecord, EngineError> {
    let record = InterfaceRecord {
        ui_strings: collect_prefixed(object, INTERFACE_STRING_PREFIX),
        color_overrides: collect_colors(object)?,
        icon_overrides: collect_prefixed(object, INTERFACE_ICON_PREFIX),
        command_cells: collect_command_cells(object)?,
        message_duration_secs: u32_field(
            object,
            "interface.message_duration_secs",
            0,
            MESSAGE_DURATION_SECS_MAX,
            10,
        )?,
        message_font_pct: u32_field(
            object,
            "interface.message_font_pct",
            MESSAGE_FONT_PCT_MIN,
            MESSAGE_FONT_PCT_MAX,
            100,
        )?,
    };
    validate_interface_record(&record)?;
    Ok(record)
}

/// Assemble the lowered interface document (pre-fingerprint) from a persisted object.
pub(crate) fn lowered_interface_document(
    document: &EditorDocument,
    object_id: &str,
) -> Result<ObjectEditorLoweredInterfaceDocument, EngineError> {
    let object = document.objects.get(object_id).ok_or_else(|| {
        object_editor_error("interface.object_id", format!("missing object {object_id}"))
    })?;
    if object.kind != "GameInterface" {
        return Err(object_editor_error(
            "interface.kind",
            format!("{object_id} is not a GameInterface object"),
        ));
    }
    Ok(ObjectEditorLoweredInterfaceDocument {
        schema_version: ObjectEditorLoweredInterfaceDocument::SCHEMA_VERSION,
        lowering_fingerprint: String::new(),
        object_id: object.id.clone(),
        base_object_id: required(object, "object.base_id")?,
        display_name: required(object, "object.display_name")?,
        interface: build_interface_record(object)?,
        provenance: LoweringProvenance::new(document, object_id),
    })
}

/// Validate the resolved interface record: cells in the 4x3 card, no duplicate cell, ranges.
pub(crate) fn validate_interface_record(record: &InterfaceRecord) -> Result<(), EngineError> {
    let mut occupied = std::collections::BTreeSet::new();
    for (command, cell) in &record.command_cells {
        if cell.col >= COMMAND_CARD_COLS || cell.row >= COMMAND_CARD_ROWS {
            return Err(object_editor_error(
                "interface.cmd",
                format!(
                    "{command} cell ({},{}) is outside the {COMMAND_CARD_COLS}x{COMMAND_CARD_ROWS} command card",
                    cell.col, cell.row
                ),
            ));
        }
        if !occupied.insert((cell.col, cell.row)) {
            return Err(object_editor_error(
                "interface.cmd",
                format!("duplicate command-card cell ({},{})", cell.col, cell.row),
            ));
        }
    }
    for (slot, value) in &record.color_overrides {
        validate_hex_color(slot, value)?;
    }
    if record.message_duration_secs > MESSAGE_DURATION_SECS_MAX {
        return Err(object_editor_error(
            "interface.message_duration_secs",
            format!("must be at most {MESSAGE_DURATION_SECS_MAX}"),
        ));
    }
    if record.message_font_pct < MESSAGE_FONT_PCT_MIN
        || record.message_font_pct > MESSAGE_FONT_PCT_MAX
    {
        return Err(object_editor_error(
            "interface.message_font_pct",
            format!("must be in {MESSAGE_FONT_PCT_MIN}..={MESSAGE_FONT_PCT_MAX}"),
        ));
    }
    Ok(())
}

fn collect_prefixed(object: &EditorObject, prefix: &str) -> BTreeMap<String, String> {
    object
        .properties
        .iter()
        .filter_map(|(key, value)| {
            key.strip_prefix(prefix)
                .map(|suffix| (suffix.to_string(), value.trim().to_string()))
        })
        .filter(|(_, value)| !value.is_empty())
        .collect()
}

fn collect_colors(object: &EditorObject) -> Result<BTreeMap<String, String>, EngineError> {
    let colors = collect_prefixed(object, INTERFACE_COLOR_PREFIX);
    for (slot, value) in &colors {
        validate_hex_color(slot, value)?;
    }
    Ok(colors)
}

fn collect_command_cells(
    object: &EditorObject,
) -> Result<BTreeMap<String, CommandCardCell>, EngineError> {
    let mut cells = BTreeMap::new();
    for (key, value) in &object.properties {
        let Some(command) = key.strip_prefix(INTERFACE_CMD_PREFIX) else {
            continue;
        };
        cells.insert(command.to_string(), parse_cell(command, value)?);
    }
    Ok(cells)
}

/// Parse a `col,row` command-card cell string. Public so persist-time validation shares it.
pub fn parse_cell(command: &str, raw: &str) -> Result<CommandCardCell, EngineError> {
    let parts = raw.split(',').map(str::trim).collect::<Vec<_>>();
    if parts.len() != 2 {
        return Err(object_editor_error(
            "interface.cmd",
            format!("{command} cell must be 'col,row'; got {raw}"),
        ));
    }
    let col = parts[0].parse::<u32>().map_err(|_| {
        object_editor_error("interface.cmd", format!("{command} col is not an integer"))
    })?;
    let row = parts[1].parse::<u32>().map_err(|_| {
        object_editor_error("interface.cmd", format!("{command} row is not an integer"))
    })?;
    if col >= COMMAND_CARD_COLS || row >= COMMAND_CARD_ROWS {
        return Err(object_editor_error(
            "interface.cmd",
            format!(
                "{command} cell ({col},{row}) is outside the {COMMAND_CARD_COLS}x{COMMAND_CARD_ROWS} command card"
            ),
        ));
    }
    Ok(CommandCardCell { col, row })
}

/// Validate a 6-digit hex RGB color override. Public so persist-time validation shares it.
pub fn validate_hex_color(slot: &str, value: &str) -> Result<(), EngineError> {
    let trimmed = value.trim_start_matches('#');
    if trimmed.len() != 6 || !trimmed.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(object_editor_error(
            "interface.color",
            format!("{slot} must be a 6-digit hex RGB color; got {value}"),
        ));
    }
    Ok(())
}

fn required(object: &EditorObject, key: &str) -> Result<String, EngineError> {
    object
        .properties
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| object_editor_error(key, "required interface property is missing"))
}
