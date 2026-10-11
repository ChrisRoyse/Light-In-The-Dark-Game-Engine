//! #826 destructibles & doodads DATA field set for the object-data editor, lowered through
//! the #829 valve. Two record families share one lowered artifact (tagged by `record_kind`):
//!
//! * Destructible types: kind, hp, armor + armor type, targeted-as flags, death art ref,
//!   pathing footprint ref. Lowers into the fields `litd_engine::world_destructibles`
//!   (`DestructibleSpec`) consumes at scatter time (#1817): `kind`/`max_hp`/`armor`/`model_ref`.
//! * Doodad types: variations count, art ref, scale range (thousandths), pathing footprint.
//!   Lowers into the fields `litd_engine::world_scatter` (`DoodadScatterBrush`) consumes:
//!   `variant`/`min_scale_milli`/`max_scale_milli`.
//!
//! The enum/flag constant sets are the single source of truth shared with persist-time
//! validation.

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::lower::{
    LoweredArtifactDoc, LoweringProvenance, check_enum, check_flags, check_range, enum_field,
    flag_list_field, string_field, u32_field,
};
use super::unit_fields::WC3_TARGET_FLAGS;
use super::validation::object_editor_error;
use crate::history::{EditorDocument, EditorObject};

/// Destructible families, matching `litd_engine::world_destructibles::DestructibleKind`.
pub const WC3_DESTRUCTIBLE_KINDS: &[&str] = &["tree", "gate", "wall", "breakable"];
/// Destructible armor classes (defense family the flat armor value belongs to).
pub const WC3_DESTRUCTIBLE_ARMOR_TYPES: &[&str] =
    &["none", "wood", "stone", "metal", "flesh", "ethereal"];

/// Max destructible hit points, aligned with the runtime `MAX_HP` bound.
pub const DESTRUCTIBLE_HP_MAX: u32 = 1_000_000;
/// Max doodad variations, aligned with the runtime scatter `MAX_VARIANTS` bound.
pub const DOODAD_VARIATIONS_MAX: u32 = 256;
/// Max doodad scale in thousandths, aligned with the runtime scatter `MAX_SCALE_MILLI` bound.
pub const DOODAD_SCALE_MILLI_MAX: u32 = 100_000;

/// The overrideable #826 destructible property keys.
pub const WC3_DESTRUCTIBLE_PROPERTY_KEYS: &[&str] = &[
    "destructible.kind",
    "destructible.max_hp",
    "destructible.armor",
    "destructible.armor_type",
    "destructible.targeted_as",
    "destructible.death_art",
    "destructible.pathing_footprint",
];
/// The overrideable #826 doodad property keys.
pub const WC3_DOODAD_PROPERTY_KEYS: &[&str] = &[
    "doodad.variations",
    "doodad.art_ref",
    "doodad.min_scale_milli",
    "doodad.max_scale_milli",
    "doodad.pathing_footprint",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DestructibleTypeRecord {
    pub kind: String,
    pub max_hp: u32,
    pub armor: u32,
    pub armor_type: String,
    pub targeted_as: Vec<String>,
    pub death_art: String,
    pub pathing_footprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoodadTypeRecord {
    pub variations: u32,
    pub art_ref: String,
    pub min_scale_milli: u32,
    pub max_scale_milli: u32,
    pub pathing_footprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredScatterDocument {
    pub schema_version: u32,
    pub lowering_fingerprint: String,
    /// Either "destructible" or "doodad"; exactly one record table is present.
    pub record_kind: String,
    pub object_id: String,
    pub rawcode: String,
    pub base_object_id: String,
    pub display_name: String,
    pub destructible: Option<DestructibleTypeRecord>,
    pub doodad: Option<DoodadTypeRecord>,
    pub provenance: LoweringProvenance,
}

impl LoweredArtifactDoc for ObjectEditorLoweredScatterDocument {
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
            || self.rawcode.len() != 4
            || self.base_object_id.trim().is_empty()
            || self.display_name.trim().is_empty()
        {
            return Err(object_editor_error(
                "scatter.required",
                "lowered scatter TOML is missing required identity data",
            ));
        }
        match (self.record_kind.as_str(), &self.destructible, &self.doodad) {
            ("destructible", Some(record), None) => validate_destructible_record(record),
            ("doodad", None, Some(record)) => validate_doodad_record(record),
            _ => Err(object_editor_error(
                "scatter.record_kind",
                "record_kind must be 'destructible' or 'doodad' with exactly its own record table",
            )),
        }
    }
}

/// Assemble the lowered scatter document (pre-fingerprint) from a persisted object, dispatching
/// on the object kind so one MCP tool covers both destructible and doodad type records.
pub(crate) fn lowered_scatter_document(
    document: &EditorDocument,
    object_id: &str,
) -> Result<ObjectEditorLoweredScatterDocument, EngineError> {
    let object = document.objects.get(object_id).ok_or_else(|| {
        object_editor_error("scatter.object_id", format!("missing object {object_id}"))
    })?;
    let (record_kind, destructible, doodad) = match object.kind.as_str() {
        "Destructible" => (
            "destructible".to_string(),
            Some(build_destructible_record(object)?),
            None,
        ),
        "Doodad" => (
            "doodad".to_string(),
            None,
            Some(build_doodad_record(object)?),
        ),
        other => {
            return Err(object_editor_error(
                "scatter.kind",
                format!("{object_id} kind {other} is neither Destructible nor Doodad"),
            ));
        }
    };
    Ok(ObjectEditorLoweredScatterDocument {
        schema_version: ObjectEditorLoweredScatterDocument::SCHEMA_VERSION,
        lowering_fingerprint: String::new(),
        record_kind,
        object_id: object.id.clone(),
        rawcode: required(object, "object.rawcode")?,
        base_object_id: required(object, "object.base_id")?,
        display_name: required(object, "object.display_name")?,
        destructible,
        doodad,
        provenance: LoweringProvenance::new(document, object_id),
    })
}

/// Deterministically extract the destructible type record from an authored object.
pub(crate) fn build_destructible_record(
    object: &EditorObject,
) -> Result<DestructibleTypeRecord, EngineError> {
    let record = DestructibleTypeRecord {
        kind: enum_field(object, "destructible.kind", WC3_DESTRUCTIBLE_KINDS, "tree")?,
        max_hp: u32_field(object, "destructible.max_hp", 1, DESTRUCTIBLE_HP_MAX, 50)?,
        armor: u32_field(object, "destructible.armor", 0, DESTRUCTIBLE_HP_MAX, 0)?,
        armor_type: enum_field(
            object,
            "destructible.armor_type",
            WC3_DESTRUCTIBLE_ARMOR_TYPES,
            "wood",
        )?,
        targeted_as: flag_list_field(object, "destructible.targeted_as", WC3_TARGET_FLAGS, "tree")?,
        death_art: string_field(object, "destructible.death_art", ""),
        pathing_footprint: string_field(object, "destructible.pathing_footprint", ""),
    };
    validate_destructible_record(&record)?;
    Ok(record)
}

/// Deterministically extract the doodad type record from an authored object.
pub(crate) fn build_doodad_record(object: &EditorObject) -> Result<DoodadTypeRecord, EngineError> {
    let record = DoodadTypeRecord {
        variations: u32_field(object, "doodad.variations", 1, DOODAD_VARIATIONS_MAX, 1)?,
        art_ref: string_field(object, "doodad.art_ref", ""),
        min_scale_milli: u32_field(
            object,
            "doodad.min_scale_milli",
            1,
            DOODAD_SCALE_MILLI_MAX,
            1_000,
        )?,
        max_scale_milli: u32_field(
            object,
            "doodad.max_scale_milli",
            1,
            DOODAD_SCALE_MILLI_MAX,
            1_000,
        )?,
        pathing_footprint: string_field(object, "doodad.pathing_footprint", ""),
    };
    validate_doodad_record(&record)?;
    Ok(record)
}

/// Validate a destructible type record; enforces the runtime `armor < max_hp` breakability rule.
pub(crate) fn validate_destructible_record(
    record: &DestructibleTypeRecord,
) -> Result<(), EngineError> {
    check_enum("destructible.kind", &record.kind, WC3_DESTRUCTIBLE_KINDS)?;
    check_range("destructible.max_hp", record.max_hp, 1, DESTRUCTIBLE_HP_MAX)?;
    check_enum(
        "destructible.armor_type",
        &record.armor_type,
        WC3_DESTRUCTIBLE_ARMOR_TYPES,
    )?;
    check_flags(
        "destructible.targeted_as",
        &record.targeted_as,
        WC3_TARGET_FLAGS,
    )?;
    if record.armor >= record.max_hp {
        return Err(object_editor_error(
            "destructible.armor",
            "armor must be strictly less than max_hp so the object stays breakable",
        ));
    }
    Ok(())
}

/// Validate a doodad type record; enforces `min_scale_milli <= max_scale_milli`.
pub(crate) fn validate_doodad_record(record: &DoodadTypeRecord) -> Result<(), EngineError> {
    check_range(
        "doodad.variations",
        record.variations,
        1,
        DOODAD_VARIATIONS_MAX,
    )?;
    check_range(
        "doodad.min_scale_milli",
        record.min_scale_milli,
        1,
        DOODAD_SCALE_MILLI_MAX,
    )?;
    check_range(
        "doodad.max_scale_milli",
        record.max_scale_milli,
        1,
        DOODAD_SCALE_MILLI_MAX,
    )?;
    if record.min_scale_milli > record.max_scale_milli {
        return Err(object_editor_error(
            "doodad.max_scale_milli",
            "min_scale_milli must be less than or equal to max_scale_milli",
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
        .ok_or_else(|| object_editor_error(key, "required scatter property is missing"))
}
