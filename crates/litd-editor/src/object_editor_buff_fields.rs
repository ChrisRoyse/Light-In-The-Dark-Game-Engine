//! #823 buffs/effects field set for the object-data editor, lowered through the #829 valve.
//! A buff record carries its status label (name/icon/tooltip), polarity, aura params
//! (radius + affected-target flags via the canonical `WC3_TARGET_FLAGS`), looping effect art
//! ref, stacking rule, and duration semantics. The enum/flag constant sets here are the
//! SINGLE source of truth used by BOTH persist-time validation (`validate_known_property_value`)
//! and this module's lowering-time `validate_content`.

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::lower::{
    LoweredArtifactDoc, LoweringProvenance, check_enum, check_flags, check_range, enum_field,
    flag_list_field, string_field, u32_field,
};
use super::unit_fields::WC3_TARGET_FLAGS;
use super::validation::object_editor_error;
use crate::history::{EditorDocument, EditorObject};

/// Buff polarity: whether the status helps (positive) or harms (negative) its bearer.
pub const WC3_BUFF_POLARITIES: &[&str] = &["positive", "negative"];
/// Stacking rule enum: how a re-applied buff of the same kind combines with the existing one.
pub const WC3_BUFF_STACK_RULES: &[&str] = &["none", "refresh", "stack", "replace"];
/// Duration semantics: how the buff's lifetime is resolved at runtime.
pub const WC3_BUFF_DURATION_SEMANTICS: &[&str] = &["timed", "permanent", "hero_reduced"];

/// Max aura radius in world units.
pub const BUFF_AURA_RADIUS_MAX: u32 = 100_000;
/// Max buff duration in seconds.
pub const BUFF_DURATION_SECS_MAX: u32 = 100_000;

/// The overrideable #823 buff property keys (used by tests/FSV to exercise the full set).
pub const WC3_BUFF_PROPERTY_KEYS: &[&str] = &[
    "buff.status_name",
    "buff.status_icon",
    "buff.status_tooltip",
    "buff.polarity",
    "buff.aura_radius",
    "buff.aura_targets",
    "buff.effect_art",
    "buff.stacking_rule",
    "buff.duration_semantics",
    "buff.duration_secs",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuffRecord {
    pub status_name: String,
    pub status_icon: String,
    pub status_tooltip: String,
    pub polarity: String,
    pub aura_radius: u32,
    pub aura_targets: Vec<String>,
    pub effect_art: String,
    pub stacking_rule: String,
    pub duration_semantics: String,
    pub duration_secs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredBuffDocument {
    pub schema_version: u32,
    pub lowering_fingerprint: String,
    pub object_id: String,
    pub rawcode: String,
    pub base_object_id: String,
    pub display_name: String,
    pub buff: BuffRecord,
    pub provenance: LoweringProvenance,
}

impl LoweredArtifactDoc for ObjectEditorLoweredBuffDocument {
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
                "buff.required",
                "lowered buff TOML is missing required identity data",
            ));
        }
        validate_buff_record(&self.buff)
    }
}

/// Deterministically extract the buff record from an authored buff object.
pub(crate) fn build_buff_record(object: &EditorObject) -> Result<BuffRecord, EngineError> {
    Ok(BuffRecord {
        status_name: string_field(object, "buff.status_name", ""),
        status_icon: string_field(object, "buff.status_icon", ""),
        status_tooltip: string_field(object, "buff.status_tooltip", ""),
        polarity: enum_field(object, "buff.polarity", WC3_BUFF_POLARITIES, "positive")?,
        aura_radius: u32_field(object, "buff.aura_radius", 0, BUFF_AURA_RADIUS_MAX, 0)?,
        aura_targets: flag_list_field(object, "buff.aura_targets", WC3_TARGET_FLAGS, "ground")?,
        effect_art: string_field(object, "buff.effect_art", ""),
        stacking_rule: enum_field(
            object,
            "buff.stacking_rule",
            WC3_BUFF_STACK_RULES,
            "refresh",
        )?,
        duration_semantics: enum_field(
            object,
            "buff.duration_semantics",
            WC3_BUFF_DURATION_SEMANTICS,
            "timed",
        )?,
        duration_secs: u32_field(object, "buff.duration_secs", 0, BUFF_DURATION_SECS_MAX, 0)?,
    })
}

/// Assemble the lowered buff document (pre-fingerprint) from a persisted object.
pub(crate) fn lowered_buff_document(
    document: &EditorDocument,
    object_id: &str,
) -> Result<ObjectEditorLoweredBuffDocument, EngineError> {
    let object = document.objects.get(object_id).ok_or_else(|| {
        object_editor_error("buff.object_id", format!("missing object {object_id}"))
    })?;
    if object.kind != "Buff" {
        return Err(object_editor_error(
            "buff.kind",
            format!("{object_id} is not a Buff object"),
        ));
    }
    Ok(ObjectEditorLoweredBuffDocument {
        schema_version: ObjectEditorLoweredBuffDocument::SCHEMA_VERSION,
        lowering_fingerprint: String::new(),
        object_id: object.id.clone(),
        rawcode: required(object, "object.rawcode")?,
        base_object_id: required(object, "object.base_id")?,
        display_name: required(object, "object.display_name")?,
        buff: build_buff_record(object)?,
        provenance: LoweringProvenance::new(document, object_id),
    })
}

/// Validate a lowered buff record against every canonical enum/flag/range bound.
pub(crate) fn validate_buff_record(buff: &BuffRecord) -> Result<(), EngineError> {
    check_enum("buff.polarity", &buff.polarity, WC3_BUFF_POLARITIES)?;
    check_range(
        "buff.aura_radius",
        buff.aura_radius,
        0,
        BUFF_AURA_RADIUS_MAX,
    )?;
    check_flags("buff.aura_targets", &buff.aura_targets, WC3_TARGET_FLAGS)?;
    check_enum(
        "buff.stacking_rule",
        &buff.stacking_rule,
        WC3_BUFF_STACK_RULES,
    )?;
    check_enum(
        "buff.duration_semantics",
        &buff.duration_semantics,
        WC3_BUFF_DURATION_SEMANTICS,
    )?;
    check_range(
        "buff.duration_secs",
        buff.duration_secs,
        0,
        BUFF_DURATION_SECS_MAX,
    )?;
    // A timed buff with a zero duration would never expire: reject the contradiction.
    if buff.duration_semantics == "timed" && buff.duration_secs == 0 {
        return Err(object_editor_error(
            "buff.duration_secs",
            "a timed buff must declare a positive duration",
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
        .ok_or_else(|| object_editor_error(key, "required buff property is missing"))
}
