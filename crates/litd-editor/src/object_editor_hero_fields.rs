//! #820 hero data model + canonical constant sets: a hero-specific extension over the unit
//! field set. Primary attribute (STR/AGI/INT), per-attribute base + per-level gain (gains are
//! integer milli-units, thousandths of a point, so fractional WC3 gains stay lossless without
//! floats through the integer-only #829 valve), XP/level curve REFERENCES (the curves live in
//! gameplay constants), learnable hero-ability slots referencing #819 ability ids with level
//! requirements, revive parameters, and hero flags. The constant sets are the SINGLE source of
//! truth shared by persist-time and lowering-time validation.

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::field_helpers::{
    check_enum, check_flags, check_range, enum_field, flag_list_field, string_field, u32_field,
};
use super::validation::object_editor_error;
use crate::history::EditorObject;

/// Allowed hero primary attributes.
pub const HERO_PRIMARY_ATTRS: &[&str] = &["STR", "AGI", "INT"];
/// Allowed hero-specific flags.
pub const HERO_FLAGS: &[&str] = &["hero_glow", "revivable", "can_flee", "auto_attack"];

/// Overrideable hero-specific property keys (used by tests/FSV to exercise the full set).
pub const HERO_PROPERTY_KEYS: &[&str] = &[
    "hero.primary_attribute",
    "hero.str_base",
    "hero.str_gain_milli",
    "hero.agi_base",
    "hero.agi_gain_milli",
    "hero.int_base",
    "hero.int_gain_milli",
    "hero.xp_curve_ref",
    "hero.level_curve_ref",
    "hero.revive_base_time_secs",
    "hero.revive_time_per_level_secs",
    "hero.revive_base_cost_gold",
    "hero.revive_cost_per_level_gold",
    "hero.learnable",
    "hero.flags",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroFields {
    pub primary_attribute: String,
    pub str_base: u32,
    pub str_gain_milli: u32,
    pub agi_base: u32,
    pub agi_gain_milli: u32,
    pub int_base: u32,
    pub int_gain_milli: u32,
    pub xp_curve_ref: String,
    pub level_curve_ref: String,
    pub revive_base_time_secs: u32,
    pub revive_time_per_level_secs: u32,
    pub revive_base_cost_gold: u32,
    pub revive_cost_per_level_gold: u32,
    pub learnable: Vec<HeroLearnableSlot>,
    pub flags: Vec<String>,
}

/// A learnable hero-ability slot: a reference to a #819 ability id and the hero level at which
/// it may first be learned. Cross-validated at lowering time against persisted ability records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroLearnableSlot {
    pub ability_id: String,
    pub level_requirement: u32,
}

/// Single source of truth for hero numeric bounds (#1837): shared with persist-time MCP
/// validation. `HERO_LEVEL_MAX` for learnable slots reuses `constants_fields::HERO_LEVEL_MAX`
/// (same value 100) via the local alias below so the hero module stays self-contained.
pub const HERO_ATTR_BASE_MAX: u32 = 1_000;
pub const HERO_ATTR_GAIN_MILLI_MAX: u32 = 100_000;
pub const HERO_REVIVE_TIME_MAX: u32 = 100_000;
pub const HERO_REVIVE_COST_MAX: u32 = 1_000_000;
pub const HERO_LEVEL_MIN: u32 = 1;
/// Learnable ability level ceiling; must stay equal to `constants_fields::HERO_LEVEL_MAX`.
pub(crate) const HERO_LEVEL_MAX: u32 = 100;

/// Deterministically extract the hero field set from an authored hero object. Absent optional
/// fields resolve to defaults; present fields are range/enum validated. Learnable slots are
/// parsed and intra-record validated (well-formed `<ability_id>@<level>` with a level in range,
/// no duplicate ability); the cross-record kind check happens at lowering time.
pub(crate) fn build_hero_fields(object: &EditorObject) -> Result<HeroFields, EngineError> {
    Ok(HeroFields {
        primary_attribute: enum_field(object, "hero.primary_attribute", HERO_PRIMARY_ATTRS, "STR")?,
        str_base: u32_field(object, "hero.str_base", 0, HERO_ATTR_BASE_MAX, 0)?,
        str_gain_milli: u32_field(
            object,
            "hero.str_gain_milli",
            0,
            HERO_ATTR_GAIN_MILLI_MAX,
            0,
        )?,
        agi_base: u32_field(object, "hero.agi_base", 0, HERO_ATTR_BASE_MAX, 0)?,
        agi_gain_milli: u32_field(
            object,
            "hero.agi_gain_milli",
            0,
            HERO_ATTR_GAIN_MILLI_MAX,
            0,
        )?,
        int_base: u32_field(object, "hero.int_base", 0, HERO_ATTR_BASE_MAX, 0)?,
        int_gain_milli: u32_field(
            object,
            "hero.int_gain_milli",
            0,
            HERO_ATTR_GAIN_MILLI_MAX,
            0,
        )?,
        xp_curve_ref: string_field(object, "hero.xp_curve_ref", ""),
        level_curve_ref: string_field(object, "hero.level_curve_ref", ""),
        revive_base_time_secs: u32_field(
            object,
            "hero.revive_base_time_secs",
            0,
            HERO_REVIVE_TIME_MAX,
            0,
        )?,
        revive_time_per_level_secs: u32_field(
            object,
            "hero.revive_time_per_level_secs",
            0,
            HERO_REVIVE_TIME_MAX,
            0,
        )?,
        revive_base_cost_gold: u32_field(
            object,
            "hero.revive_base_cost_gold",
            0,
            HERO_REVIVE_COST_MAX,
            0,
        )?,
        revive_cost_per_level_gold: u32_field(
            object,
            "hero.revive_cost_per_level_gold",
            0,
            HERO_REVIVE_COST_MAX,
            0,
        )?,
        learnable: parse_learnable(object, "hero.learnable")?,
        flags: flag_list_field(object, "hero.flags", HERO_FLAGS)?,
    })
}

/// Parse the compound `hero.learnable` list: `<ability_id>@<level>` tokens, comma/pipe delimited.
pub(crate) fn parse_learnable(
    object: &EditorObject,
    key: &str,
) -> Result<Vec<HeroLearnableSlot>, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(Vec::new());
    };
    let mut slots = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for token in raw
        .split([',', '|'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        let (ability_id, level) = token.split_once('@').ok_or_else(|| {
            object_editor_error(key, format!("slot {token} must be <ability_id>@<level>"))
        })?;
        let ability_id = ability_id.trim();
        if ability_id.is_empty() {
            return Err(object_editor_error(
                key,
                format!("slot {token} has empty ability id"),
            ));
        }
        let level_requirement = level.trim().parse::<u32>().map_err(|_| {
            object_editor_error(key, format!("slot {token} level must be an integer"))
        })?;
        check_range(
            &format!("{key}.level"),
            level_requirement,
            HERO_LEVEL_MIN,
            HERO_LEVEL_MAX,
        )?;
        if !seen.insert(ability_id.to_string()) {
            return Err(object_editor_error(
                key,
                format!("duplicate learnable ability {ability_id}"),
            ));
        }
        slots.push(HeroLearnableSlot {
            ability_id: ability_id.to_string(),
            level_requirement,
        });
    }
    Ok(slots)
}

/// Re-validate a lowered `HeroFields` against every range/enum/flag bound. Called on TOML
/// readback so a corrupted or hand-edited artifact fails loud. The cross-record learnable-kind
/// check is a lowering-time concern (needs the document) and is not repeated here.
pub(crate) fn validate_hero_fields(fields: &HeroFields) -> Result<(), EngineError> {
    check_enum(
        "hero.primary_attribute",
        &fields.primary_attribute,
        HERO_PRIMARY_ATTRS,
    )?;
    check_range("hero.str_base", fields.str_base, 0, HERO_ATTR_BASE_MAX)?;
    check_range(
        "hero.str_gain_milli",
        fields.str_gain_milli,
        0,
        HERO_ATTR_GAIN_MILLI_MAX,
    )?;
    check_range("hero.agi_base", fields.agi_base, 0, HERO_ATTR_BASE_MAX)?;
    check_range(
        "hero.agi_gain_milli",
        fields.agi_gain_milli,
        0,
        HERO_ATTR_GAIN_MILLI_MAX,
    )?;
    check_range("hero.int_base", fields.int_base, 0, HERO_ATTR_BASE_MAX)?;
    check_range(
        "hero.int_gain_milli",
        fields.int_gain_milli,
        0,
        HERO_ATTR_GAIN_MILLI_MAX,
    )?;
    check_range(
        "hero.revive_base_time_secs",
        fields.revive_base_time_secs,
        0,
        HERO_REVIVE_TIME_MAX,
    )?;
    check_range(
        "hero.revive_time_per_level_secs",
        fields.revive_time_per_level_secs,
        0,
        HERO_REVIVE_TIME_MAX,
    )?;
    check_range(
        "hero.revive_base_cost_gold",
        fields.revive_base_cost_gold,
        0,
        HERO_REVIVE_COST_MAX,
    )?;
    check_range(
        "hero.revive_cost_per_level_gold",
        fields.revive_cost_per_level_gold,
        0,
        HERO_REVIVE_COST_MAX,
    )?;
    check_flags("hero.flags", &fields.flags, HERO_FLAGS)?;
    let mut seen = std::collections::BTreeSet::new();
    for slot in &fields.learnable {
        check_range(
            "hero.learnable.level",
            slot.level_requirement,
            HERO_LEVEL_MIN,
            HERO_LEVEL_MAX,
        )?;
        if slot.ability_id.trim().is_empty() {
            return Err(object_editor_error(
                "hero.learnable",
                "slot ability id must be non-empty",
            ));
        }
        if !seen.insert(slot.ability_id.as_str()) {
            return Err(object_editor_error(
                "hero.learnable",
                format!("duplicate learnable ability {}", slot.ability_id),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "object_editor_hero_fields_tests.rs"]
mod tests;
