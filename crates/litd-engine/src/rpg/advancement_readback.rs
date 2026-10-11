use super::serialize_sha256;

use crate::EngineError;

use super::{
    AbilityCatalogReadback, AdvancementActionReadback, AdvancementAwardReadback, AdvancementInput,
    AdvancementReadback, AdvancementRules, AdvancementState, RPG_ADVANCEMENT_SCHEMA,
};

pub(super) fn advancement_awards(
    input: &AdvancementInput,
    rules: &AdvancementRules,
) -> AdvancementAwardReadback {
    let levels_gained = input.to_level - input.from_level;
    let practice_per_level = practice_per_level(input.attributes.wisdom, rules);
    let practice_awarded = practice_per_level * levels_gained;
    let trains_awarded = rules.train_sessions_per_level * levels_gained;
    let study_points_awarded = ((input.from_level + 1)..=input.to_level)
        .map(|level| study_points_for_level(level, &input.class_id, rules))
        .sum();
    AdvancementAwardReadback {
        levels_gained,
        practice_per_level,
        practice_awarded,
        trains_awarded,
        study_points_awarded,
    }
}

pub(super) fn practice_gain(intelligence: i16, rules: &AdvancementRules) -> u16 {
    rules.practice_gain_base + (intelligence.max(0) as u16 / rules.practice_intelligence_divisor)
}

pub(super) fn study_cost(ability_id: &str, rules: &AdvancementRules) -> Result<u16, EngineError> {
    study_cost_optional(ability_id, rules).ok_or_else(|| EngineError::RpgAdvancementInvalidConfig {
        field: "study_cost".to_string(),
        reason: format!("missing study cost for {ability_id}"),
    })
}

pub(super) fn study_cost_optional(ability_id: &str, rules: &AdvancementRules) -> Option<u16> {
    rules
        .study_costs
        .iter()
        .find(|cost| cost.ability_id == ability_id)
        .map(|cost| cost.points)
}

pub(super) fn level_cap(level: u16, rules: &AdvancementRules) -> Result<u16, EngineError> {
    rules
        .level_cap_base
        .checked_add(level * rules.level_cap_per_level)
        .map(|value| value.min(100))
        .ok_or_else(|| EngineError::RpgAdvancementInvalidConfig {
            field: "level_cap".to_string(),
            reason: "level proficiency cap overflowed".to_string(),
        })
}

pub(super) fn build_advancement_readback(
    input: &AdvancementInput,
    rules: &AdvancementRules,
    catalog: &AbilityCatalogReadback,
    final_state: AdvancementState,
    awards: AdvancementAwardReadback,
    level_cap: u16,
    actions: Vec<AdvancementActionReadback>,
) -> Result<AdvancementReadback, EngineError> {
    let input_sha256 = serialize_sha256(input)?;
    let rules_sha256 = serialize_sha256(rules)?;
    let catalog_sha256 = catalog.input_sha256.clone();
    let mut readback = AdvancementReadback {
        schema: RPG_ADVANCEMENT_SCHEMA.to_string(),
        character_id: input.character_id.clone(),
        class_id: input.class_id.clone(),
        race_id: input.race_id.clone(),
        from_level: input.from_level,
        to_level: input.to_level,
        awards,
        level_proficiency_cap: level_cap,
        focused_ability_id: input.focused_ability_id.clone(),
        focused_skill_learn_bonus: rules.focused_skill_learn_bonus,
        final_state,
        actions,
        input_sha256,
        rules_sha256,
        catalog_sha256,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&result_payload(&readback))?;
    Ok(readback)
}

fn practice_per_level(wisdom: i16, rules: &AdvancementRules) -> u16 {
    rules
        .practice_bands
        .iter()
        .filter(|band| wisdom >= band.min_wisdom)
        .map(|band| band.practices_per_level)
        .next_back()
        .unwrap_or(0)
}

fn study_points_for_level(level: u16, class_id: &str, rules: &AdvancementRules) -> u16 {
    rules
        .study_grants
        .iter()
        .filter(|grant| grant.class_id == class_id)
        .map(|grant| {
            let per_level = if level <= grant.max_level {
                grant.points_per_level
            } else {
                0
            };
            let milestone = if level == grant.milestone_level {
                grant.milestone_points
            } else {
                0
            };
            per_level + milestone
        })
        .sum()
}

fn result_payload(readback: &AdvancementReadback) -> serde_json::Value {
    serde_json::json!({
        "schema": readback.schema,
        "character_id": readback.character_id,
        "class_id": readback.class_id,
        "race_id": readback.race_id,
        "to_level": readback.to_level,
        "awards": readback.awards,
        "level_proficiency_cap": readback.level_proficiency_cap,
        "focused_ability_id": readback.focused_ability_id,
        "focused_skill_learn_bonus": readback.focused_skill_learn_bonus,
        "final_state": readback.final_state,
        "actions": readback.actions,
        "input_sha256": readback.input_sha256,
        "rules_sha256": readback.rules_sha256,
        "catalog_sha256": readback.catalog_sha256,
    })
}
