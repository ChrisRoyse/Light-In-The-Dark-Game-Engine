//! Full WC3-style unit field set for the #818 unit data editor, lowered through the
//! #829 object-data valve. Pure serde data model + deterministic extraction from the
//! authored `EditorObject` property bag. Grouped into stats/art/sound/combat/movement/
//! techtree/economy so the lowered TOML mirrors the World Editor's Unit tabs. Kept in
//! its own module so `object_editor` and its lowering/validation split stay under the
//! 500-line ratchet.

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::history::EditorObject;

use super::validation::object_editor_error;

/// Single source of truth for unit WC3 numeric bounds (#1837): shared with persist-time MCP
/// validation so lowering and persist cannot drift.
pub const UNIT_MANA_MAX: u32 = 1_000_000;
pub const UNIT_ARMOR_MAX: u32 = 1_000;
pub const UNIT_SCALE_PCT_MIN: u32 = 1;
pub const UNIT_SCALE_PCT_MAX: u32 = 1_000;
pub const UNIT_DAMAGE_DICE_MIN: u32 = 1;
pub const UNIT_DAMAGE_DICE_MAX: u32 = 100;
pub const UNIT_DAMAGE_BASE_MAX: u32 = 100_000;
pub const UNIT_ATTACK_COOLDOWN_MS_MIN: u32 = 1;
pub const UNIT_ATTACK_COOLDOWN_MS_MAX: u32 = 600_000;
pub const UNIT_ATTACK_RANGE_MAX: u32 = 100_000;
pub const UNIT_TURN_RATE_PCT_MAX: u32 = 100;
pub const UNIT_GOLD_COST_MAX: u32 = 1_000_000;
pub const UNIT_LUMBER_COST_MAX: u32 = 1_000_000;
pub const UNIT_FOOD_COST_MAX: u32 = 200;
pub const UNIT_BUILD_TIME_SECS_MAX: u32 = 100_000;

/// Allowed WC3 attack types (damage table rows).
pub const WC3_ATTACK_TYPES: &[&str] = &[
    "Normal", "Pierce", "Siege", "Magic", "Chaos", "Spells", "Hero",
];
/// Allowed WC3 armor types (defense table rows).
pub const WC3_ARMOR_TYPES: &[&str] = &[
    "Unarmored",
    "Light",
    "Medium",
    "Heavy",
    "Fortified",
    "Hero",
    "Divine",
];
/// Allowed WC3 movement types.
pub const WC3_MOVE_TYPES: &[&str] = &["Foot", "Horse", "Fly", "Hover", "Amphibious", "Float"];
/// Allowed unit targeting flags (a unit may attack these classes).
pub const WC3_TARGET_FLAGS: &[&str] = &[
    "ground",
    "air",
    "structure",
    "ward",
    "item",
    "tree",
    "wall",
    "debris",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitWc3Fields {
    pub stats: UnitStats,
    pub art: UnitArt,
    pub sound: UnitSound,
    pub combat: UnitCombat,
    pub movement: UnitMovement,
    pub techtree: UnitTechtree,
    pub economy: UnitEconomy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitStats {
    pub mana: u32,
    pub armor: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitArt {
    pub icon: String,
    pub scale_pct: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitSound {
    pub move_set: String,
    pub ready: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitCombat {
    pub damage_dice: u32,
    pub damage_sides: u32,
    pub damage_base: u32,
    pub attack_cooldown_ms: u32,
    pub attack_range: u32,
    pub attack_type: String,
    pub armor_type: String,
    pub targets_allowed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitMovement {
    pub move_type: String,
    pub height: u32,
    pub turn_rate_pct: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitTechtree {
    pub requires: Vec<String>,
    pub trains: Vec<String>,
    pub upgrades_to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitEconomy {
    pub gold_cost: u32,
    pub lumber_cost: u32,
    pub food_cost: u32,
    pub build_time_secs: u32,
}

/// The overrideable WC3 unit property keys (used by tests/FSV to exercise the full set).
pub const WC3_UNIT_PROPERTY_KEYS: &[&str] = &[
    "stats.mana",
    "stats.armor",
    "art.icon",
    "art.scale_pct",
    "sound.move_set",
    "sound.ready",
    "combat.damage_dice",
    "combat.damage_sides",
    "combat.damage_base",
    "combat.attack_cooldown_ms",
    "combat.attack_range",
    "combat.attack_type",
    "combat.armor_type",
    "combat.targets_allowed",
    "movement.type",
    "movement.height",
    "movement.turn_rate_pct",
    "techtree.requires",
    "techtree.trains",
    "techtree.upgrades_to",
    "economy.gold_cost",
    "economy.lumber_cost",
    "economy.food_cost",
    "economy.build_time_secs",
];

/// Deterministically extract the full WC3 field set from an authored unit object.
/// Absent optional fields resolve to WC3-authentic defaults; present fields are
/// range/enum validated so a lowering can never emit an out-of-band artifact.
pub(crate) fn build_wc3_fields(object: &EditorObject) -> Result<UnitWc3Fields, EngineError> {
    Ok(UnitWc3Fields {
        stats: UnitStats {
            mana: u32_field(object, "stats.mana", 0, UNIT_MANA_MAX, 0)?,
            armor: u32_field(object, "stats.armor", 0, UNIT_ARMOR_MAX, 0)?,
        },
        art: UnitArt {
            icon: string_field(object, "art.icon", ""),
            scale_pct: u32_field(
                object,
                "art.scale_pct",
                UNIT_SCALE_PCT_MIN,
                UNIT_SCALE_PCT_MAX,
                100,
            )?,
        },
        sound: UnitSound {
            move_set: string_field(object, "sound.move_set", ""),
            ready: string_field(object, "sound.ready", ""),
        },
        combat: UnitCombat {
            damage_dice: u32_field(
                object,
                "combat.damage_dice",
                UNIT_DAMAGE_DICE_MIN,
                UNIT_DAMAGE_DICE_MAX,
                1,
            )?,
            damage_sides: u32_field(
                object,
                "combat.damage_sides",
                UNIT_DAMAGE_DICE_MIN,
                UNIT_DAMAGE_DICE_MAX,
                1,
            )?,
            damage_base: u32_field(object, "combat.damage_base", 0, UNIT_DAMAGE_BASE_MAX, 0)?,
            attack_cooldown_ms: u32_field(
                object,
                "combat.attack_cooldown_ms",
                UNIT_ATTACK_COOLDOWN_MS_MIN,
                UNIT_ATTACK_COOLDOWN_MS_MAX,
                1_500,
            )?,
            attack_range: u32_field(object, "combat.attack_range", 0, UNIT_ATTACK_RANGE_MAX, 90)?,
            attack_type: enum_field(object, "combat.attack_type", WC3_ATTACK_TYPES, "Normal")?,
            armor_type: enum_field(object, "combat.armor_type", WC3_ARMOR_TYPES, "Medium")?,
            targets_allowed: flag_list_field(object, "combat.targets_allowed", "ground")?,
        },
        movement: UnitMovement {
            move_type: enum_field(object, "movement.type", WC3_MOVE_TYPES, "Foot")?,
            height: u32_field(object, "movement.height", 0, UNIT_ATTACK_RANGE_MAX, 0)?,
            turn_rate_pct: u32_field(
                object,
                "movement.turn_rate_pct",
                0,
                UNIT_TURN_RATE_PCT_MAX,
                60,
            )?,
        },
        techtree: UnitTechtree {
            requires: id_list_field(object, "techtree.requires")?,
            trains: id_list_field(object, "techtree.trains")?,
            upgrades_to: string_field(object, "techtree.upgrades_to", ""),
        },
        economy: UnitEconomy {
            gold_cost: u32_field(object, "economy.gold_cost", 0, UNIT_GOLD_COST_MAX, 0)?,
            lumber_cost: u32_field(object, "economy.lumber_cost", 0, UNIT_LUMBER_COST_MAX, 0)?,
            food_cost: u32_field(object, "economy.food_cost", 0, UNIT_FOOD_COST_MAX, 0)?,
            build_time_secs: u32_field(
                object,
                "economy.build_time_secs",
                0,
                UNIT_BUILD_TIME_SECS_MAX,
                0,
            )?,
        },
    })
}

/// Validate that a lowered `UnitWc3Fields` still satisfies every range/enum bound.
/// Called on TOML readback so a corrupted or hand-edited artifact fails loud.
pub(crate) fn validate_wc3_fields(fields: &UnitWc3Fields) -> Result<(), EngineError> {
    check_range("stats.mana", fields.stats.mana, 0, UNIT_MANA_MAX)?;
    check_range("stats.armor", fields.stats.armor, 0, UNIT_ARMOR_MAX)?;
    check_range(
        "art.scale_pct",
        fields.art.scale_pct,
        UNIT_SCALE_PCT_MIN,
        UNIT_SCALE_PCT_MAX,
    )?;
    check_range(
        "combat.damage_dice",
        fields.combat.damage_dice,
        UNIT_DAMAGE_DICE_MIN,
        UNIT_DAMAGE_DICE_MAX,
    )?;
    check_range(
        "combat.damage_sides",
        fields.combat.damage_sides,
        UNIT_DAMAGE_DICE_MIN,
        UNIT_DAMAGE_DICE_MAX,
    )?;
    check_range(
        "combat.damage_base",
        fields.combat.damage_base,
        0,
        UNIT_DAMAGE_BASE_MAX,
    )?;
    check_range(
        "combat.attack_cooldown_ms",
        fields.combat.attack_cooldown_ms,
        UNIT_ATTACK_COOLDOWN_MS_MIN,
        UNIT_ATTACK_COOLDOWN_MS_MAX,
    )?;
    check_range(
        "combat.attack_range",
        fields.combat.attack_range,
        0,
        UNIT_ATTACK_RANGE_MAX,
    )?;
    check_enum(
        "combat.attack_type",
        &fields.combat.attack_type,
        WC3_ATTACK_TYPES,
    )?;
    check_enum(
        "combat.armor_type",
        &fields.combat.armor_type,
        WC3_ARMOR_TYPES,
    )?;
    check_flags("combat.targets_allowed", &fields.combat.targets_allowed)?;
    check_enum("movement.type", &fields.movement.move_type, WC3_MOVE_TYPES)?;
    check_range(
        "movement.height",
        fields.movement.height,
        0,
        UNIT_ATTACK_RANGE_MAX,
    )?;
    check_range(
        "movement.turn_rate_pct",
        fields.movement.turn_rate_pct,
        0,
        UNIT_TURN_RATE_PCT_MAX,
    )?;
    check_range(
        "economy.gold_cost",
        fields.economy.gold_cost,
        0,
        UNIT_GOLD_COST_MAX,
    )?;
    check_range(
        "economy.lumber_cost",
        fields.economy.lumber_cost,
        0,
        UNIT_LUMBER_COST_MAX,
    )?;
    check_range(
        "economy.food_cost",
        fields.economy.food_cost,
        0,
        UNIT_FOOD_COST_MAX,
    )?;
    check_range(
        "economy.build_time_secs",
        fields.economy.build_time_secs,
        0,
        UNIT_BUILD_TIME_SECS_MAX,
    )?;
    Ok(())
}

fn u32_field(
    object: &EditorObject,
    key: &str,
    min: u32,
    max: u32,
    default: u32,
) -> Result<u32, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(default);
    };
    let parsed = raw
        .parse::<u32>()
        .map_err(|_| object_editor_error(key, format!("must be an integer in {min}..={max}")))?;
    check_range(key, parsed, min, max)?;
    Ok(parsed)
}

fn string_field(object: &EditorObject, key: &str, default: &str) -> String {
    object
        .properties
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn enum_field(
    object: &EditorObject,
    key: &str,
    allowed: &[&str],
    default: &str,
) -> Result<String, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(default.to_string());
    };
    let value = raw.trim().to_string();
    check_enum(key, &value, allowed)?;
    Ok(value)
}

fn flag_list_field(
    object: &EditorObject,
    key: &str,
    default: &str,
) -> Result<Vec<String>, EngineError> {
    let raw = object
        .properties
        .get(key)
        .map(String::as_str)
        .unwrap_or(default);
    let flags = split_list(raw);
    if flags.is_empty() {
        return Err(object_editor_error(key, "at least one flag is required"));
    }
    check_flags(key, &flags)?;
    Ok(flags)
}

fn id_list_field(object: &EditorObject, key: &str) -> Result<Vec<String>, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(Vec::new());
    };
    let ids = split_list(raw);
    let mut seen = std::collections::BTreeSet::new();
    for id in &ids {
        if !seen.insert(id.as_str()) {
            return Err(object_editor_error(key, format!("duplicate id {id}")));
        }
    }
    Ok(ids)
}

fn split_list(raw: &str) -> Vec<String> {
    raw.split([',', '|'])
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn check_range(key: &str, value: u32, min: u32, max: u32) -> Result<(), EngineError> {
    if value < min || value > max {
        return Err(object_editor_error(
            key,
            format!("must be in {min}..={max}; got {value}"),
        ));
    }
    Ok(())
}

fn check_enum(key: &str, value: &str, allowed: &[&str]) -> Result<(), EngineError> {
    if !allowed.contains(&value) {
        return Err(object_editor_error(
            key,
            format!("{value} is not one of {allowed:?}"),
        ));
    }
    Ok(())
}

fn check_flags(key: &str, flags: &[String]) -> Result<(), EngineError> {
    let mut seen = std::collections::BTreeSet::new();
    for flag in flags {
        if !WC3_TARGET_FLAGS.contains(&flag.as_str()) {
            return Err(object_editor_error(
                key,
                format!("{flag} is not one of {WC3_TARGET_FLAGS:?}"),
            ));
        }
        if !seen.insert(flag.as_str()) {
            return Err(object_editor_error(key, format!("duplicate flag {flag}")));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "object_editor_unit_fields_tests.rs"]
mod tests;
