use std::collections::BTreeMap;

use super::serialize_sha256;

use crate::EngineError;

use super::{
    attributes_types::*,
    attributes_validate::{
        resistance_allowlist, validate_input, validate_modifier_resistances, validate_table,
    },
};

pub fn derive_character_stats(
    input: &AttributeBuildInput,
    table: &DerivedStatFormulaTable,
) -> Result<AttributeDerivedReadback, EngineError> {
    validate_table(table)?;
    let caps = validate_input(input)?;
    let mut attribute_mods = AttributeScores::ZERO;
    let mut derived_mods = DerivedStats::ZERO;
    let mut resistance_mods: BTreeMap<String, i32> = BTreeMap::new();
    let allowed = resistance_allowlist();
    for modifier in &input.modifiers {
        attribute_mods =
            checked_add_scores(attribute_mods, modifier.attributes, "attributes.modifier")?;
        checked_add_derived(&mut derived_mods, &modifier.derived, "derived.modifier")?;
        validate_modifier_resistances(modifier, &allowed)?;
        for resistance in &modifier.resistances {
            let entry = resistance_mods
                .entry(resistance.resistance_id.clone())
                .or_default();
            *entry = add_i32(
                *entry,
                resistance.amount,
                "resistance.modifier",
                resistance.resistance_id.as_str(),
            )?;
        }
    }
    let uncapped = checked_add_scores(input.base, attribute_mods, "attributes.effective")?;
    let (effective, cap_clamps) = apply_caps(uncapped, caps);
    let derived = DerivedStats {
        hit_points: eval_formula(
            &table.hit_points,
            input.level,
            effective,
            derived_mods.hit_points,
        )?,
        mana: eval_formula(&table.mana, input.level, effective, derived_mods.mana)?,
        movement: eval_formula(
            &table.movement,
            input.level,
            effective,
            derived_mods.movement,
        )?,
        hitroll: eval_formula(&table.hitroll, input.level, effective, derived_mods.hitroll)?,
        damroll: eval_formula(&table.damroll, input.level, effective, derived_mods.damroll)?,
        armor_class: eval_formula(
            &table.armor_class,
            input.level,
            effective,
            derived_mods.armor_class,
        )?,
        carry_weight: eval_formula(
            &table.carry_weight,
            input.level,
            effective,
            derived_mods.carry_weight,
        )?,
        wield_weight: eval_formula(
            &table.wield_weight,
            input.level,
            effective,
            derived_mods.wield_weight,
        )?,
    };
    let mut resistances = Vec::with_capacity(table.resistances.len());
    for formula in &table.resistances {
        let modifier = resistance_mods
            .get(&formula.resistance_id)
            .copied()
            .unwrap_or_default();
        resistances.push(ResistanceReadback {
            resistance_id: formula.resistance_id.clone(),
            value: eval_resistance(formula, effective, modifier)?,
        });
    }
    let gear_modifier_count = input
        .modifiers
        .iter()
        .filter(|modifier| modifier.source == StatModifierSource::Gear)
        .count();
    let input_sha256 = serialize_sha256(input)?;
    let table_sha256 = serialize_sha256(table)?;
    let mut readback = AttributeDerivedReadback {
        schema: RPG_STATS_SCHEMA.to_string(),
        character_id: input.character_id.clone(),
        race_id: input.race_id.clone(),
        level: input.level,
        base_attributes: input.base,
        race_caps: caps,
        uncapped_attributes: uncapped,
        effective_attributes: effective,
        cap_clamps,
        derived,
        resistances,
        modifier_count: input.modifiers.len(),
        gear_modifier_count,
        affect_modifier_count: input.modifiers.len() - gear_modifier_count,
        input_sha256,
        table_sha256,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&result_fingerprint_payload(&readback))?;
    Ok(readback)
}

fn checked_add_scores(
    lhs: AttributeScores,
    rhs: AttributeScores,
    field: &str,
) -> Result<AttributeScores, EngineError> {
    Ok(AttributeScores {
        strength: add_i16(lhs.strength, rhs.strength, field, "strength")?,
        intelligence: add_i16(lhs.intelligence, rhs.intelligence, field, "intelligence")?,
        wisdom: add_i16(lhs.wisdom, rhs.wisdom, field, "wisdom")?,
        dexterity: add_i16(lhs.dexterity, rhs.dexterity, field, "dexterity")?,
        constitution: add_i16(lhs.constitution, rhs.constitution, field, "constitution")?,
        charisma: add_i16(lhs.charisma, rhs.charisma, field, "charisma")?,
    })
}

fn checked_add_derived(
    lhs: &mut DerivedStats,
    rhs: &DerivedStats,
    field: &str,
) -> Result<(), EngineError> {
    lhs.hit_points = add_i32(lhs.hit_points, rhs.hit_points, field, "hit_points")?;
    lhs.mana = add_i32(lhs.mana, rhs.mana, field, "mana")?;
    lhs.movement = add_i32(lhs.movement, rhs.movement, field, "movement")?;
    lhs.hitroll = add_i32(lhs.hitroll, rhs.hitroll, field, "hitroll")?;
    lhs.damroll = add_i32(lhs.damroll, rhs.damroll, field, "damroll")?;
    lhs.armor_class = add_i32(lhs.armor_class, rhs.armor_class, field, "armor_class")?;
    lhs.carry_weight = add_i32(lhs.carry_weight, rhs.carry_weight, field, "carry_weight")?;
    lhs.wield_weight = add_i32(lhs.wield_weight, rhs.wield_weight, field, "wield_weight")?;
    Ok(())
}

fn apply_caps(
    uncapped: AttributeScores,
    caps: AttributeScores,
) -> (AttributeScores, Vec<AttributeCapClamp>) {
    let mut clamps = Vec::new();
    let effective = AttributeScores {
        strength: cap_score("strength", uncapped.strength, caps.strength, &mut clamps),
        intelligence: cap_score(
            "intelligence",
            uncapped.intelligence,
            caps.intelligence,
            &mut clamps,
        ),
        wisdom: cap_score("wisdom", uncapped.wisdom, caps.wisdom, &mut clamps),
        dexterity: cap_score("dexterity", uncapped.dexterity, caps.dexterity, &mut clamps),
        constitution: cap_score(
            "constitution",
            uncapped.constitution,
            caps.constitution,
            &mut clamps,
        ),
        charisma: cap_score("charisma", uncapped.charisma, caps.charisma, &mut clamps),
    };
    (effective, clamps)
}

fn cap_score(name: &str, value: i16, cap: i16, clamps: &mut Vec<AttributeCapClamp>) -> i16 {
    let capped = value.clamp(1, cap);
    if capped != value {
        clamps.push(AttributeCapClamp {
            attribute: name.to_string(),
            uncapped: value,
            capped,
            cap,
        });
    }
    capped
}

fn eval_formula(
    formula: &StatFormula,
    level: u16,
    effective: AttributeScores,
    modifier: i32,
) -> Result<i32, EngineError> {
    let value = formula
        .base
        .checked_add(i32::from(level) * formula.per_level)
        .and_then(|value| value.checked_add(effective.dot(formula.coefficients)))
        .and_then(|value| value.checked_add(modifier))
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: "formula.value".to_string(),
            reason: "derived stat overflowed i32".to_string(),
        })?;
    Ok(value.clamp(formula.min, formula.max))
}

fn eval_resistance(
    formula: &ResistanceFormula,
    effective: AttributeScores,
    modifier: i32,
) -> Result<i32, EngineError> {
    let value = formula
        .base
        .checked_add(effective.dot(formula.coefficients))
        .and_then(|value| value.checked_add(modifier))
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: "resistance.value".to_string(),
            reason: "resistance stat overflowed i32".to_string(),
        })?;
    Ok(value.clamp(formula.min, formula.max))
}

fn add_i16(lhs: i16, rhs: i16, field: &str, name: &str) -> Result<i16, EngineError> {
    lhs.checked_add(rhs)
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: format!("{field}.{name}"),
            reason: "attribute value overflowed i16".to_string(),
        })
}

fn add_i32(lhs: i32, rhs: i32, field: &str, name: &str) -> Result<i32, EngineError> {
    lhs.checked_add(rhs)
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: format!("{field}.{name}"),
            reason: "derived modifier overflowed i32".to_string(),
        })
}

fn result_fingerprint_payload(readback: &AttributeDerivedReadback) -> serde_json::Value {
    serde_json::json!({
        "schema": readback.schema,
        "character_id": readback.character_id,
        "race_id": readback.race_id,
        "level": readback.level,
        "effective_attributes": readback.effective_attributes,
        "derived": readback.derived,
        "resistances": readback.resistances,
        "input_sha256": readback.input_sha256,
        "table_sha256": readback.table_sha256,
    })
}
