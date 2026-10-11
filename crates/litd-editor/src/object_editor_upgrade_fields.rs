//! #824 upgrades field set for the object-data editor, lowered through the #829 valve.
//! A leveled upgrade record carries its level count, per-level typed stat deltas
//! (attack/armor/hp/mana/speed/custom), per-level gold/lumber cost + research time, tech
//! requirement refs, and applies-to unit lists. Per-level vectors are length-checked against
//! the level count; tech + applies-to refs are validated at lowering (validated-id contract).
//! The effect-kind enum here is the single source of truth shared with persist-time validation.

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::lower::{
    LoweredArtifactDoc, LoweringProvenance, check_enum, enum_field, i32_list_field, id_list_field,
    u32_field, u32_list_field,
};
use super::validation::object_editor_error;
use crate::history::{EditorDocument, EditorObject};

/// Typed upgrade effect kinds (which stat the per-level deltas modify).
pub const WC3_UPGRADE_EFFECT_KINDS: &[&str] = &["attack", "armor", "hp", "mana", "speed", "custom"];

/// Max number of upgrade levels.
pub const UPGRADE_LEVEL_COUNT_MAX: u32 = 20;
/// Max per-level gold/lumber cost.
pub const UPGRADE_COST_MAX: u32 = 1_000_000;
/// Max per-level research time in seconds.
pub const UPGRADE_RESEARCH_SECS_MAX: u32 = 100_000;

/// The overrideable #824 upgrade property keys (used by tests/FSV to exercise the full set).
pub const WC3_UPGRADE_PROPERTY_KEYS: &[&str] = &[
    "upgrade.effect_kind",
    "upgrade.level_count",
    "upgrade.level_deltas",
    "upgrade.gold_costs",
    "upgrade.lumber_costs",
    "upgrade.research_times",
    "upgrade.tech_requires",
    "upgrade.applies_to",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradeRecord {
    pub effect_kind: String,
    pub level_count: u32,
    /// Per-level stat delta (signed; a custom kind may still push negative deltas).
    pub level_deltas: Vec<i32>,
    pub gold_costs: Vec<u32>,
    pub lumber_costs: Vec<u32>,
    pub research_times: Vec<u32>,
    pub tech_requires: Vec<String>,
    pub applies_to: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredUpgradeDocument {
    pub schema_version: u32,
    pub lowering_fingerprint: String,
    pub object_id: String,
    pub rawcode: String,
    pub base_object_id: String,
    pub display_name: String,
    pub upgrade: UpgradeRecord,
    pub provenance: LoweringProvenance,
}

impl LoweredArtifactDoc for ObjectEditorLoweredUpgradeDocument {
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
                "upgrade.required",
                "lowered upgrade TOML is missing required identity data",
            ));
        }
        validate_upgrade_record(&self.upgrade)
    }
}

/// Deterministically extract the upgrade record from an authored upgrade object.
pub(crate) fn build_upgrade_record(object: &EditorObject) -> Result<UpgradeRecord, EngineError> {
    let record = UpgradeRecord {
        effect_kind: enum_field(
            object,
            "upgrade.effect_kind",
            WC3_UPGRADE_EFFECT_KINDS,
            "attack",
        )?,
        level_count: u32_field(object, "upgrade.level_count", 1, UPGRADE_LEVEL_COUNT_MAX, 1)?,
        level_deltas: i32_list_field(object, "upgrade.level_deltas")?,
        gold_costs: u32_list_field(object, "upgrade.gold_costs")?,
        lumber_costs: u32_list_field(object, "upgrade.lumber_costs")?,
        research_times: u32_list_field(object, "upgrade.research_times")?,
        tech_requires: id_list_field(object, "upgrade.tech_requires")?,
        applies_to: id_list_field(object, "upgrade.applies_to")?,
    };
    validate_upgrade_record(&record)?;
    Ok(record)
}

/// Assemble the lowered upgrade document (pre-fingerprint) from a persisted object.
pub(crate) fn lowered_upgrade_document(
    document: &EditorDocument,
    object_id: &str,
) -> Result<ObjectEditorLoweredUpgradeDocument, EngineError> {
    let object = document.objects.get(object_id).ok_or_else(|| {
        object_editor_error("upgrade.object_id", format!("missing object {object_id}"))
    })?;
    if object.kind != "Upgrade" {
        return Err(object_editor_error(
            "upgrade.kind",
            format!("{object_id} is not an Upgrade object"),
        ));
    }
    let upgrade = build_upgrade_record(object)?;
    // Cross-record existence (#1837 item 2): tech_requires / applies_to must name persisted
    // Tech / Unit objects (same contract as #820 learnable and #822 commerce).
    validate_upgrade_cross_record(document, &upgrade)?;
    Ok(ObjectEditorLoweredUpgradeDocument {
        schema_version: ObjectEditorLoweredUpgradeDocument::SCHEMA_VERSION,
        lowering_fingerprint: String::new(),
        object_id: object.id.clone(),
        rawcode: required(object, "object.rawcode")?,
        base_object_id: required(object, "object.base_id")?,
        display_name: required(object, "object.display_name")?,
        upgrade,
        provenance: LoweringProvenance::new(document, object_id),
    })
}

/// Every `tech_requires` id must be a persisted `Tech` object; every `applies_to` id must be a
/// persisted `Unit`. Empty lists are allowed (no prereqs / global apply).
pub(crate) fn validate_upgrade_cross_record(
    document: &EditorDocument,
    upgrade: &UpgradeRecord,
) -> Result<(), EngineError> {
    for tech_id in &upgrade.tech_requires {
        let referenced = document.objects.get(tech_id).ok_or_else(|| {
            object_editor_error(
                "upgrade.tech_requires",
                format!("tech requirement {tech_id} is not a persisted object"),
            )
        })?;
        if referenced.kind != "Tech" {
            return Err(object_editor_error(
                "upgrade.tech_requires",
                format!(
                    "tech requirement {tech_id} is kind {} not Tech",
                    referenced.kind
                ),
            ));
        }
    }
    for unit_id in &upgrade.applies_to {
        let referenced = document.objects.get(unit_id).ok_or_else(|| {
            object_editor_error(
                "upgrade.applies_to",
                format!("applies-to unit {unit_id} is not a persisted object"),
            )
        })?;
        if referenced.kind != "Unit" {
            return Err(object_editor_error(
                "upgrade.applies_to",
                format!(
                    "applies-to ref {unit_id} is kind {} not Unit",
                    referenced.kind
                ),
            ));
        }
    }
    Ok(())
}

/// Validate a lowered upgrade record: enum, level count, and per-level vector lengths.
pub(crate) fn validate_upgrade_record(upgrade: &UpgradeRecord) -> Result<(), EngineError> {
    check_enum(
        "upgrade.effect_kind",
        &upgrade.effect_kind,
        WC3_UPGRADE_EFFECT_KINDS,
    )?;
    if upgrade.level_count < 1 || upgrade.level_count > UPGRADE_LEVEL_COUNT_MAX {
        return Err(object_editor_error(
            "upgrade.level_count",
            format!(
                "must be in 1..={UPGRADE_LEVEL_COUNT_MAX}; got {}",
                upgrade.level_count
            ),
        ));
    }
    let levels = upgrade.level_count as usize;
    check_len("upgrade.level_deltas", upgrade.level_deltas.len(), levels)?;
    check_len("upgrade.gold_costs", upgrade.gold_costs.len(), levels)?;
    check_len("upgrade.lumber_costs", upgrade.lumber_costs.len(), levels)?;
    check_len(
        "upgrade.research_times",
        upgrade.research_times.len(),
        levels,
    )?;
    for cost in upgrade.gold_costs.iter().chain(&upgrade.lumber_costs) {
        if *cost > UPGRADE_COST_MAX {
            return Err(object_editor_error(
                "upgrade.cost",
                format!("per-level cost must be at most {UPGRADE_COST_MAX}; got {cost}"),
            ));
        }
    }
    for secs in &upgrade.research_times {
        if *secs > UPGRADE_RESEARCH_SECS_MAX {
            return Err(object_editor_error(
                "upgrade.research_times",
                format!("research time must be at most {UPGRADE_RESEARCH_SECS_MAX}; got {secs}"),
            ));
        }
    }
    Ok(())
}

fn check_len(field: &str, actual: usize, expected: usize) -> Result<(), EngineError> {
    if actual != expected {
        return Err(object_editor_error(
            field,
            format!("must carry exactly one value per level ({expected}); got {actual}"),
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
        .ok_or_else(|| object_editor_error(key, "required upgrade property is missing"))
}
