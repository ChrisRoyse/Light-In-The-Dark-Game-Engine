//! #827 Gameplay Constants: the global balance-knob record (singleton per project), lowered
//! through the #829 valve. Base defaults ship in the editor; a project overrides only what it
//! changes and lowering emits the FULL resolved constant set with a fingerprint. Fractional
//! conversion rates are stored as lossless IEEE-754 f64 bit patterns (u64) so the lowered TOML
//! stays integer-only and bit-deterministic (see #862 precedent). Validation: monotonic XP
//! thresholds, positive cycle lengths, and documented ranges. Kept in its own module so the
//! object-editor lowering/validation split stays under the 500-line ratchet.

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::history::EditorObject;

use super::validation::object_editor_error;

// ---- Documented ranges (single source of truth for persist-time + lowering-time checks) ----
/// XP level threshold count bound (level 1 threshold up to the cap).
pub const XP_MAX_LEVELS: usize = 100;
/// Largest cumulative XP threshold (units: XP points).
pub const XP_THRESHOLD_MAX: u32 = 10_000_000;
/// XP gain base and per-level percent bounds (units: XP points; percent).
pub const XP_GAIN_BASE_MAX: u32 = 1_000_000;
pub const XP_GAIN_PCT_MAX: u32 = 10_000;
/// Day / night cycle length bounds (units: seconds; MUST be positive).
pub const CYCLE_LEN_MIN: u32 = 1;
pub const CYCLE_LEN_MAX: u32 = 86_400;
/// Hero revive factor bounds (units: seconds; gold).
pub const REVIVE_SECS_MAX: u32 = 100_000;
pub const REVIVE_GOLD_MAX: u32 = 1_000_000;
/// Food / upkeep threshold bounds (units: food points).
pub const FOOD_CAP_MAX: u32 = 1_000;
/// Hero level cap bound (units: level).
pub const HERO_LEVEL_MAX: u32 = 100;
/// Attribute-to-stat conversion rate bound (units: stat points per attribute point).
pub const CONVERSION_RATE_MAX: f64 = 10_000.0;
/// Item drop-rate percent bound (units: percent, 0..=100).
pub const DROP_RATE_PCT_MAX: f64 = 100.0;

/// The overrideable gameplay-constant property keys (used by tests / FSV / persist validation).
pub const GAMEPLAY_CONSTANT_PROPERTY_KEYS: &[&str] = &[
    "xp.level_thresholds",
    "xp.gain_base",
    "xp.gain_per_level_pct",
    "cycle.day_length_secs",
    "cycle.night_length_secs",
    "hero.revive_time_base_secs",
    "hero.revive_time_per_level_secs",
    "hero.revive_gold_per_level",
    "hero.str_to_hp",
    "hero.str_to_hp_regen",
    "hero.agi_to_armor",
    "hero.agi_to_attack_speed_pct",
    "hero.int_to_mana",
    "hero.int_to_mana_regen",
    "item.drop_rate_pct",
    "item.drop_rate_max_pct",
    "upkeep.no_upkeep_food_cap",
    "upkeep.low_upkeep_food_cap",
    "upkeep.high_upkeep_food_max",
    "misc.max_hero_level",
];

/// The full resolved gameplay-constant set. Fractional knobs are stored as IEEE-754 f64 bit
/// patterns so the lowered TOML is integer-only and lossless.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredGameplayConstants {
    pub xp_level_thresholds: Vec<u32>,
    pub xp_gain_base: u32,
    pub xp_gain_per_level_pct: u32,
    pub day_length_secs: u32,
    pub night_length_secs: u32,
    pub revive_time_base_secs: u32,
    pub revive_time_per_level_secs: u32,
    pub revive_gold_per_level: u32,
    pub str_to_hp_bits: u64,
    pub str_to_hp_regen_bits: u64,
    pub agi_to_armor_bits: u64,
    pub agi_to_attack_speed_pct_bits: u64,
    pub int_to_mana_bits: u64,
    pub int_to_mana_regen_bits: u64,
    pub item_drop_rate_pct_bits: u64,
    pub item_drop_rate_max_pct_bits: u64,
    pub no_upkeep_food_cap: u32,
    pub low_upkeep_food_cap: u32,
    pub high_upkeep_food_max: u32,
    pub max_hero_level: u32,
    /// Constant, documents the fractional-knob encoding used above.
    pub float_encoding: String,
}

/// WC3-authentic base defaults that ship in the editor. A project copies from these and
/// overrides only what it changes; lowering resolves absent fields back to these values.
pub fn gameplay_constants_base_properties() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "xp.level_thresholds",
            "0,200,500,900,1400,2000,2700,3500,4400,5400",
        ),
        ("xp.gain_base", "100"),
        ("xp.gain_per_level_pct", "100"),
        ("cycle.day_length_secs", "480"),
        ("cycle.night_length_secs", "180"),
        ("hero.revive_time_base_secs", "10"),
        ("hero.revive_time_per_level_secs", "9"),
        ("hero.revive_gold_per_level", "85"),
        ("hero.str_to_hp", "25.0"),
        ("hero.str_to_hp_regen", "0.05"),
        ("hero.agi_to_armor", "0.3"),
        ("hero.agi_to_attack_speed_pct", "2.0"),
        ("hero.int_to_mana", "15.0"),
        ("hero.int_to_mana_regen", "0.05"),
        ("item.drop_rate_pct", "12.5"),
        ("item.drop_rate_max_pct", "100.0"),
        ("upkeep.no_upkeep_food_cap", "50"),
        ("upkeep.low_upkeep_food_cap", "80"),
        ("upkeep.high_upkeep_food_max", "100"),
        ("misc.max_hero_level", "10"),
    ]
}

/// The documented fractional-knob encoding tag (mirrors #862).
pub const CONSTANTS_FLOAT_ENCODING: &str = "ieee754_f64_bits";

fn base_default(key: &str) -> &'static str {
    gameplay_constants_base_properties()
        .into_iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value)
        .unwrap_or("0")
}

/// Deterministically extract the FULL resolved constant set from an authored object. Absent
/// fields resolve to the shipped base defaults; present fields are range validated so a
/// lowering can never emit an out-of-band artifact.
pub(crate) fn build_gameplay_constants(
    object: &EditorObject,
) -> Result<LoweredGameplayConstants, EngineError> {
    let constants = LoweredGameplayConstants {
        xp_level_thresholds: thresholds_field(object, "xp.level_thresholds")?,
        xp_gain_base: u32_field(object, "xp.gain_base", 0, XP_GAIN_BASE_MAX)?,
        xp_gain_per_level_pct: u32_field(object, "xp.gain_per_level_pct", 0, XP_GAIN_PCT_MAX)?,
        day_length_secs: u32_field(
            object,
            "cycle.day_length_secs",
            CYCLE_LEN_MIN,
            CYCLE_LEN_MAX,
        )?,
        night_length_secs: u32_field(
            object,
            "cycle.night_length_secs",
            CYCLE_LEN_MIN,
            CYCLE_LEN_MAX,
        )?,
        revive_time_base_secs: u32_field(object, "hero.revive_time_base_secs", 0, REVIVE_SECS_MAX)?,
        revive_time_per_level_secs: u32_field(
            object,
            "hero.revive_time_per_level_secs",
            0,
            REVIVE_SECS_MAX,
        )?,
        revive_gold_per_level: u32_field(object, "hero.revive_gold_per_level", 0, REVIVE_GOLD_MAX)?,
        str_to_hp_bits: f64_bits_field(object, "hero.str_to_hp", CONVERSION_RATE_MAX)?,
        str_to_hp_regen_bits: f64_bits_field(object, "hero.str_to_hp_regen", CONVERSION_RATE_MAX)?,
        agi_to_armor_bits: f64_bits_field(object, "hero.agi_to_armor", CONVERSION_RATE_MAX)?,
        agi_to_attack_speed_pct_bits: f64_bits_field(
            object,
            "hero.agi_to_attack_speed_pct",
            CONVERSION_RATE_MAX,
        )?,
        int_to_mana_bits: f64_bits_field(object, "hero.int_to_mana", CONVERSION_RATE_MAX)?,
        int_to_mana_regen_bits: f64_bits_field(
            object,
            "hero.int_to_mana_regen",
            CONVERSION_RATE_MAX,
        )?,
        item_drop_rate_pct_bits: f64_bits_field(object, "item.drop_rate_pct", DROP_RATE_PCT_MAX)?,
        item_drop_rate_max_pct_bits: f64_bits_field(
            object,
            "item.drop_rate_max_pct",
            DROP_RATE_PCT_MAX,
        )?,
        no_upkeep_food_cap: u32_field(object, "upkeep.no_upkeep_food_cap", 0, FOOD_CAP_MAX)?,
        low_upkeep_food_cap: u32_field(object, "upkeep.low_upkeep_food_cap", 0, FOOD_CAP_MAX)?,
        high_upkeep_food_max: u32_field(object, "upkeep.high_upkeep_food_max", 0, FOOD_CAP_MAX)?,
        max_hero_level: u32_field(object, "misc.max_hero_level", 1, HERO_LEVEL_MAX)?,
        float_encoding: CONSTANTS_FLOAT_ENCODING.to_string(),
    };
    validate_gameplay_constants(&constants)?;
    Ok(constants)
}

/// Validate a resolved constant set: monotonic XP thresholds, positive cycle lengths, ordered
/// upkeep caps, and finite in-range fractional knobs. Called at lowering and on TOML readback.
pub(crate) fn validate_gameplay_constants(
    constants: &LoweredGameplayConstants,
) -> Result<(), EngineError> {
    if constants.float_encoding != CONSTANTS_FLOAT_ENCODING {
        return Err(object_editor_error(
            "constants.float_encoding",
            format!("expected {CONSTANTS_FLOAT_ENCODING}"),
        ));
    }
    validate_thresholds(&constants.xp_level_thresholds)?;
    if constants.day_length_secs < CYCLE_LEN_MIN || constants.night_length_secs < CYCLE_LEN_MIN {
        return Err(object_editor_error(
            "constants.cycle",
            "day/night cycle lengths must be positive",
        ));
    }
    if constants.low_upkeep_food_cap < constants.no_upkeep_food_cap
        || constants.high_upkeep_food_max < constants.low_upkeep_food_cap
    {
        return Err(object_editor_error(
            "constants.upkeep",
            "food/upkeep caps must be non-decreasing: no <= low <= high",
        ));
    }
    for (key, bits, max) in [
        (
            "hero.str_to_hp",
            constants.str_to_hp_bits,
            CONVERSION_RATE_MAX,
        ),
        (
            "hero.str_to_hp_regen",
            constants.str_to_hp_regen_bits,
            CONVERSION_RATE_MAX,
        ),
        (
            "hero.agi_to_armor",
            constants.agi_to_armor_bits,
            CONVERSION_RATE_MAX,
        ),
        (
            "hero.agi_to_attack_speed_pct",
            constants.agi_to_attack_speed_pct_bits,
            CONVERSION_RATE_MAX,
        ),
        (
            "hero.int_to_mana",
            constants.int_to_mana_bits,
            CONVERSION_RATE_MAX,
        ),
        (
            "hero.int_to_mana_regen",
            constants.int_to_mana_regen_bits,
            CONVERSION_RATE_MAX,
        ),
        (
            "item.drop_rate_pct",
            constants.item_drop_rate_pct_bits,
            DROP_RATE_PCT_MAX,
        ),
        (
            "item.drop_rate_max_pct",
            constants.item_drop_rate_max_pct_bits,
            DROP_RATE_PCT_MAX,
        ),
    ] {
        check_f64_bits(key, bits, max)?;
    }
    Ok(())
}

fn validate_thresholds(thresholds: &[u32]) -> Result<(), EngineError> {
    if thresholds.is_empty() || thresholds.len() > XP_MAX_LEVELS {
        return Err(object_editor_error(
            "xp.level_thresholds",
            format!("threshold count must be in 1..={XP_MAX_LEVELS}"),
        ));
    }
    let mut previous: Option<u32> = None;
    for &value in thresholds {
        if value > XP_THRESHOLD_MAX {
            return Err(object_editor_error(
                "xp.level_thresholds",
                format!("threshold {value} exceeds {XP_THRESHOLD_MAX}"),
            ));
        }
        if let Some(prev) = previous
            && value <= prev
        {
            return Err(object_editor_error(
                "xp.level_thresholds",
                format!("thresholds must be strictly increasing; {value} follows {prev}"),
            ));
        }
        previous = Some(value);
    }
    Ok(())
}

fn thresholds_field(object: &EditorObject, key: &str) -> Result<Vec<u32>, EngineError> {
    let raw = object
        .properties
        .get(key)
        .cloned()
        .unwrap_or_else(|| base_default(key).to_string());
    let mut values = Vec::new();
    for token in raw
        .split([',', '|'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        let value = token
            .parse::<u32>()
            .map_err(|_| object_editor_error(key, format!("{token} is not an integer")))?;
        values.push(value);
    }
    validate_thresholds(&values)?;
    Ok(values)
}

fn u32_field(object: &EditorObject, key: &str, min: u32, max: u32) -> Result<u32, EngineError> {
    let raw = object
        .properties
        .get(key)
        .cloned()
        .unwrap_or_else(|| base_default(key).to_string());
    let parsed = raw
        .parse::<u32>()
        .map_err(|_| object_editor_error(key, format!("must be an integer in {min}..={max}")))?;
    if parsed < min || parsed > max {
        return Err(object_editor_error(
            key,
            format!("must be in {min}..={max}; got {parsed}"),
        ));
    }
    Ok(parsed)
}

fn f64_bits_field(object: &EditorObject, key: &str, max: f64) -> Result<u64, EngineError> {
    let raw = object
        .properties
        .get(key)
        .cloned()
        .unwrap_or_else(|| base_default(key).to_string());
    let value = raw
        .parse::<f64>()
        .map_err(|_| object_editor_error(key, format!("must be a finite number in 0..={max}")))?;
    let bits = value.to_bits();
    check_f64_bits(key, bits, max)?;
    Ok(bits)
}

fn check_f64_bits(key: &str, bits: u64, max: f64) -> Result<(), EngineError> {
    let value = f64::from_bits(bits);
    if !value.is_finite() || value < 0.0 || value > max {
        return Err(object_editor_error(
            key,
            format!("must be a finite number in 0..={max}; got {value}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "object_editor_constants_fields_tests.rs"]
mod tests;
