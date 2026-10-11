//! Object-editor lowered-unit data model (#1466 split from `object_editor.rs`).
//!
//! Pure serde data types + version constants for the unit lowering / TOML readback / sim readback
//! surface. Re-exported by `object_editor` via `pub use document::*` so every existing
//! `crate::object_editor::ObjectEditor*` path is unchanged.

use litd_engine::calyx_lowering::{EngineLoweredSimPackage, EngineLoweredSimReadback};
use serde::{Deserialize, Serialize};

pub use super::unit_fields::{
    UnitArt, UnitCombat, UnitEconomy, UnitMovement, UnitSound, UnitStats, UnitTechtree,
    UnitWc3Fields, WC3_ARMOR_TYPES, WC3_ATTACK_TYPES, WC3_MOVE_TYPES, WC3_TARGET_FLAGS,
    WC3_UNIT_PROPERTY_KEYS,
};

pub const OBJECT_EDITOR_UNIT_TOML_SCHEMA_VERSION: u32 = 1;
pub const OBJECT_EDITOR_LOWERING_VERSION: &str = "litd-editor.object_editor.v1";
pub const UNIT_MOVE_SPEED_MIN: u32 = 1;
pub const UNIT_MOVE_SPEED_MAX: u32 = 522;
/// Core unit hit-points bounds (#1837): shared with persist-time MCP validation and
/// lowering/readback validators.
pub const UNIT_HIT_POINTS_MIN: u32 = 1;
pub const UNIT_HIT_POINTS_MAX: u32 = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredUnitDocument {
    pub schema_version: u32,
    pub lowering_fingerprint: String,
    pub unit: ObjectEditorLoweredUnit,
    pub validation: ObjectEditorLoweringValidation,
    pub provenance: ObjectEditorLoweringProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredUnit {
    pub object_id: String,
    pub rawcode: String,
    pub base_object_id: String,
    pub display_name: String,
    pub hit_points: u32,
    pub move_speed: u32,
    pub model: String,
    pub abilities: Vec<String>,
    pub wc3: UnitWc3Fields,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweringValidation {
    pub lower_unit_valid: bool,
    pub range_checked: bool,
    pub ability_count: usize,
    pub field_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweringProvenance {
    pub source_document_id: String,
    pub source_revision_id: String,
    pub source_object_id: String,
    pub lowering_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredUnitToml {
    pub canonical_toml: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorFieldValidation {
    pub field: String,
    pub value: String,
    pub valid: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLiveValidation {
    pub object_id: String,
    pub valid: bool,
    pub fields: Vec<ObjectEditorFieldValidation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorTomlReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub fingerprint: String,
    pub document: ObjectEditorLoweredUnitDocument,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectEditorSimReadback {
    pub runtime_vault_dir: String,
    pub unit_object_id: String,
    pub move_speed: u32,
    pub move_speed_fp: i64,
    pub abilities: Vec<String>,
    pub content_cx_id: String,
    pub content_input_sha256: String,
    pub lowered_package: EngineLoweredSimPackage,
    pub lowered_readback: EngineLoweredSimReadback,
    pub sim_body_id: u32,
    pub sim_body_count: usize,
    pub sim_velocity_x_raw: i64,
    pub sim_state_hash: u64,
}
