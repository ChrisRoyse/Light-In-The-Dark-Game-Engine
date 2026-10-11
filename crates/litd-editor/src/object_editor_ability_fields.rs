//! #819 ability data model + canonical enum/flag constant sets, lowered through the #829
//! object-data valve. Pure serde model + deterministic extraction from the authored
//! `EditorObject` property bag: ability kind (passive/active/autocast/hero-learnable),
//! mana/cooldown/range, targeting (self/unit/point/none + reused `WC3_TARGET_FLAGS`),
//! duration, area of effect, per-level scaling arrays, effect refs, icon/art refs, and
//! requirements. The constant sets here are the SINGLE source of truth shared by BOTH
//! persist-time validation (`mcp_object_tools_validation`) and lowering-time validation.

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::field_helpers::{
    check_enum, check_flags, check_range, enum_field, flag_list_field, id_list_field, string_field,
    u32_field, u32_list_field,
};
use super::unit_fields::WC3_TARGET_FLAGS;
use super::validation::object_editor_error;

/// Allowed ability kinds. `hero-learnable` abilities are the ones a hero may learn (#820).
pub const ABILITY_KINDS: &[&str] = &["passive", "active", "autocast", "hero-learnable"];
/// Allowed ability targeting modes.
pub const ABILITY_TARGET_TYPES: &[&str] = &["self", "unit", "point", "none"];

/// Overrideable ability property keys (used by tests/FSV to exercise the full set).
pub const ABILITY_PROPERTY_KEYS: &[&str] = &[
    "ability.kind",
    "ability.mana_cost",
    "ability.cooldown_ms",
    "ability.cast_range",
    "ability.targeting",
    "ability.targets_allowed",
    "ability.duration_ms",
    "ability.area_of_effect",
    "ability.levels",
    "ability.scaling.base_damage",
    "ability.scaling.mana_cost",
    "ability.scaling.duration_ms",
    "ability.scaling.area_of_effect",
    "ability.effects",
    "ability.icon",
    "ability.art",
    "ability.requirements",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityFields {
    pub kind: String,
    pub mana_cost: u32,
    pub cooldown_ms: u32,
    pub cast_range: u32,
    pub targeting: String,
    pub targets_allowed: Vec<String>,
    pub duration_ms: u32,
    pub area_of_effect: u32,
    pub levels: u32,
    pub scaling: AbilityScaling,
    pub effects: Vec<String>,
    pub icon: String,
    pub art: String,
    pub requirements: Vec<String>,
}

/// Per-level scaling arrays for a leveled ability. Each array is either empty (the field does
/// not scale with level) or exactly `levels` long (one value per learned level).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityScaling {
    pub base_damage: Vec<u32>,
    pub mana_cost: Vec<u32>,
    pub duration_ms: Vec<u32>,
    pub area_of_effect: Vec<u32>,
}

/// Single source of truth for ability numeric bounds (#1837): shared with persist-time MCP
/// validation so lowering and persist cannot drift.
pub const ABILITY_MANA_MAX: u32 = 1_000_000;
pub const ABILITY_COOLDOWN_MAX: u32 = 600_000;
pub const ABILITY_RANGE_MAX: u32 = 100_000;
pub const ABILITY_DURATION_MAX: u32 = 600_000;
pub const ABILITY_AOE_MAX: u32 = 100_000;
pub const ABILITY_LEVELS_MIN: u32 = 1;
pub const ABILITY_LEVELS_MAX: u32 = 100;
pub const ABILITY_SCALE_MAX: u32 = 1_000_000;

/// Deterministically extract the ability field set from an authored ability object.
/// Absent optional fields resolve to defaults; present fields are range/enum validated so a
/// lowering can never emit an out-of-band artifact. Per-level scaling arrays are cross-checked
/// against the declared level count.
pub(crate) fn build_ability_fields(
    object: &crate::history::EditorObject,
) -> Result<AbilityFields, EngineError> {
    let levels = u32_field(
        object,
        "ability.levels",
        ABILITY_LEVELS_MIN,
        ABILITY_LEVELS_MAX,
        1,
    )?;
    let scaling = AbilityScaling {
        base_damage: scaling_array(object, "ability.scaling.base_damage", levels)?,
        mana_cost: scaling_array(object, "ability.scaling.mana_cost", levels)?,
        duration_ms: scaling_array(object, "ability.scaling.duration_ms", levels)?,
        area_of_effect: scaling_array(object, "ability.scaling.area_of_effect", levels)?,
    };
    Ok(AbilityFields {
        kind: enum_field(object, "ability.kind", ABILITY_KINDS, "passive")?,
        mana_cost: u32_field(object, "ability.mana_cost", 0, ABILITY_MANA_MAX, 0)?,
        cooldown_ms: u32_field(object, "ability.cooldown_ms", 0, ABILITY_COOLDOWN_MAX, 0)?,
        cast_range: u32_field(object, "ability.cast_range", 0, ABILITY_RANGE_MAX, 0)?,
        targeting: enum_field(object, "ability.targeting", ABILITY_TARGET_TYPES, "none")?,
        targets_allowed: flag_list_field(object, "ability.targets_allowed", WC3_TARGET_FLAGS)?,
        duration_ms: u32_field(object, "ability.duration_ms", 0, ABILITY_DURATION_MAX, 0)?,
        area_of_effect: u32_field(object, "ability.area_of_effect", 0, ABILITY_AOE_MAX, 0)?,
        levels,
        scaling,
        effects: id_list_field(object, "ability.effects")?,
        icon: string_field(object, "ability.icon", ""),
        art: string_field(object, "ability.art", ""),
        requirements: id_list_field(object, "ability.requirements")?,
    })
}

fn scaling_array(
    object: &crate::history::EditorObject,
    key: &str,
    levels: u32,
) -> Result<Vec<u32>, EngineError> {
    let values = u32_list_field(object, key, 0, ABILITY_SCALE_MAX)?;
    if !values.is_empty() && values.len() as u32 != levels {
        return Err(object_editor_error(
            key,
            format!(
                "per-level scaling array has {} entries but ability declares {levels} levels",
                values.len()
            ),
        ));
    }
    Ok(values)
}

/// Re-validate a lowered `AbilityFields` against every range/enum/flag/scaling bound. Called on
/// TOML readback so a corrupted or hand-edited artifact fails loud.
pub(crate) fn validate_ability_fields(fields: &AbilityFields) -> Result<(), EngineError> {
    check_enum("ability.kind", &fields.kind, ABILITY_KINDS)?;
    check_range("ability.mana_cost", fields.mana_cost, 0, ABILITY_MANA_MAX)?;
    check_range(
        "ability.cooldown_ms",
        fields.cooldown_ms,
        0,
        ABILITY_COOLDOWN_MAX,
    )?;
    check_range(
        "ability.cast_range",
        fields.cast_range,
        0,
        ABILITY_RANGE_MAX,
    )?;
    check_enum("ability.targeting", &fields.targeting, ABILITY_TARGET_TYPES)?;
    check_flags(
        "ability.targets_allowed",
        &fields.targets_allowed,
        WC3_TARGET_FLAGS,
    )?;
    check_range(
        "ability.duration_ms",
        fields.duration_ms,
        0,
        ABILITY_DURATION_MAX,
    )?;
    check_range(
        "ability.area_of_effect",
        fields.area_of_effect,
        0,
        ABILITY_AOE_MAX,
    )?;
    check_range(
        "ability.levels",
        fields.levels,
        ABILITY_LEVELS_MIN,
        ABILITY_LEVELS_MAX,
    )?;
    check_scaling(
        "ability.scaling.base_damage",
        &fields.scaling.base_damage,
        fields.levels,
    )?;
    check_scaling(
        "ability.scaling.mana_cost",
        &fields.scaling.mana_cost,
        fields.levels,
    )?;
    check_scaling(
        "ability.scaling.duration_ms",
        &fields.scaling.duration_ms,
        fields.levels,
    )?;
    check_scaling(
        "ability.scaling.area_of_effect",
        &fields.scaling.area_of_effect,
        fields.levels,
    )?;
    Ok(())
}

fn check_scaling(key: &str, values: &[u32], levels: u32) -> Result<(), EngineError> {
    if !values.is_empty() && values.len() as u32 != levels {
        return Err(object_editor_error(
            key,
            format!(
                "per-level scaling array has {} entries but ability declares {levels} levels",
                values.len()
            ),
        ));
    }
    for value in values {
        check_range(key, *value, 0, ABILITY_SCALE_MAX)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "object_editor_ability_fields_tests.rs"]
mod tests;
