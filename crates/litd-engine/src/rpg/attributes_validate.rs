use std::collections::{BTreeMap, BTreeSet};

use crate::EngineError;

use super::attributes_types::*;

pub(super) fn validate_table(table: &DerivedStatFormulaTable) -> Result<(), EngineError> {
    if table.schema != RPG_STATS_SCHEMA {
        return invalid("table.schema", format!("expected {RPG_STATS_SCHEMA}"));
    }
    for (field, formula) in [
        ("hit_points", &table.hit_points),
        ("mana", &table.mana),
        ("movement", &table.movement),
        ("hitroll", &table.hitroll),
        ("damroll", &table.damroll),
        ("armor_class", &table.armor_class),
        ("carry_weight", &table.carry_weight),
        ("wield_weight", &table.wield_weight),
    ] {
        validate_formula(field, formula)?;
    }
    if table.resistances.len() != DEFAULT_RESISTANCE_IDS.len() {
        return invalid(
            "table.resistances",
            "must cover every default resistance id once",
        );
    }
    let allowed = resistance_allowlist();
    let mut seen = BTreeSet::new();
    for resistance in &table.resistances {
        validate_token("table.resistance_id", &resistance.resistance_id)?;
        if !allowed.contains(resistance.resistance_id.as_str()) {
            return invalid(
                "table.resistance_id",
                format!("unknown resistance {}", resistance.resistance_id),
            );
        }
        if !seen.insert(resistance.resistance_id.as_str()) {
            return invalid(
                "table.resistance_id",
                format!("duplicate resistance {}", resistance.resistance_id),
            );
        }
        validate_formula(&resistance.resistance_id, &resistance.as_formula())?;
    }
    Ok(())
}

pub(super) fn validate_input(input: &AttributeBuildInput) -> Result<AttributeScores, EngineError> {
    validate_token("character_id", &input.character_id)?;
    validate_token("race_id", &input.race_id)?;
    if input.level == 0 || input.level > 100 {
        return invalid("level", "must be inside 1..=100");
    }
    validate_positive_scores("attributes.base", input.base)?;
    let caps = read_race_caps(input)?;
    for (name, base_value) in input.base.pairs() {
        let cap = score_field(caps, name);
        if base_value > cap {
            return invalid(
                format!("attributes.base.{name}"),
                format!("base {base_value} exceeds race cap {cap}"),
            );
        }
    }
    if input.modifiers.len() > 64 {
        return invalid("modifiers", "at most 64 gear/affect modifiers are allowed");
    }
    let mut modifier_ids = BTreeSet::new();
    for modifier in &input.modifiers {
        validate_token("modifier_id", &modifier.modifier_id)?;
        if !modifier_ids.insert(modifier.modifier_id.as_str()) {
            return invalid(
                "modifier_id",
                format!("duplicate modifier {}", modifier.modifier_id),
            );
        }
        validate_modifier_scores("modifiers.attributes", modifier.attributes)?;
    }
    Ok(caps)
}

pub(super) fn validate_modifier_resistances(
    modifier: &StatModifier,
    allowed: &BTreeSet<&'static str>,
) -> Result<(), EngineError> {
    if modifier.resistances.len() > DEFAULT_RESISTANCE_IDS.len() {
        return invalid("modifiers.resistances", "too many resistance modifiers");
    }
    let mut seen = BTreeSet::new();
    for resistance in &modifier.resistances {
        validate_token("modifiers.resistance_id", &resistance.resistance_id)?;
        if !allowed.contains(resistance.resistance_id.as_str()) {
            return invalid(
                "modifiers.resistance_id",
                format!("unknown resistance {}", resistance.resistance_id),
            );
        }
        if !seen.insert(resistance.resistance_id.as_str()) {
            return invalid(
                "modifiers.resistance_id",
                format!(
                    "duplicate resistance {} in modifier",
                    resistance.resistance_id
                ),
            );
        }
        if !(-500..=500).contains(&resistance.amount) {
            return invalid("modifiers.resistance.amount", "must be inside -500..=500");
        }
    }
    Ok(())
}

pub(super) fn resistance_allowlist() -> BTreeSet<&'static str> {
    DEFAULT_RESISTANCE_IDS.into_iter().collect()
}

fn read_race_caps(input: &AttributeBuildInput) -> Result<AttributeScores, EngineError> {
    if input.race_caps.is_empty() {
        return invalid("race_caps", "at least one race cap row is required");
    }
    let mut races = BTreeMap::new();
    for race in &input.race_caps {
        validate_token("race_caps.race_id", &race.race_id)?;
        validate_positive_scores("race_caps.caps", race.caps)?;
        if races.insert(race.race_id.as_str(), race.caps).is_some() {
            return invalid("race_caps.race_id", format!("duplicate {}", race.race_id));
        }
    }
    races
        .get(input.race_id.as_str())
        .copied()
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: "race_id".to_string(),
            reason: format!("missing race cap row for {}", input.race_id),
        })
}

fn validate_formula(field: &str, formula: &StatFormula) -> Result<(), EngineError> {
    if formula.min > formula.max {
        return invalid(field, "min must be <= max");
    }
    if !(-100_000..=100_000).contains(&formula.base)
        || !(-100_000..=100_000).contains(&formula.per_level)
    {
        return invalid(field, "base and per_level must be inside -100000..=100000");
    }
    for (name, value) in formula.coefficients.pairs() {
        if !(-100..=100).contains(&value) {
            return invalid(
                format!("{field}.{name}"),
                "coefficient must be inside -100..=100",
            );
        }
    }
    Ok(())
}

fn validate_positive_scores(field: &str, scores: AttributeScores) -> Result<(), EngineError> {
    for (name, value) in scores.pairs() {
        if !(1..=100).contains(&value) {
            return invalid(format!("{field}.{name}"), "must be inside 1..=100");
        }
    }
    Ok(())
}

fn validate_modifier_scores(field: &str, scores: AttributeScores) -> Result<(), EngineError> {
    for (name, value) in scores.pairs() {
        if !(-100..=100).contains(&value) {
            return invalid(format!("{field}.{name}"), "must be inside -100..=100");
        }
    }
    Ok(())
}

fn score_field(scores: AttributeScores, name: &str) -> i16 {
    match name {
        "strength" => scores.strength,
        "intelligence" => scores.intelligence,
        "wisdom" => scores.wisdom,
        "dexterity" => scores.dexterity,
        "constitution" => scores.constitution,
        "charisma" => scores.charisma,
        _ => unreachable!("fixed attribute field"),
    }
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() {
        return invalid(field, "must be non-empty");
    }
    if value.len() > 96 {
        return invalid(field, "must be 96 bytes or shorter");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.')
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash",
        );
    }
    Ok(())
}

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(EngineError::RpgStatsInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    })
}

trait ResistanceFormulaExt {
    fn as_formula(&self) -> StatFormula;
}

impl ResistanceFormulaExt for ResistanceFormula {
    fn as_formula(&self) -> StatFormula {
        StatFormula {
            base: self.base,
            per_level: 0,
            coefficients: self.coefficients,
            min: self.min,
            max: self.max,
        }
    }
}
