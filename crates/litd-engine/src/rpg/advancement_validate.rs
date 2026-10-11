use std::collections::BTreeMap;

use crate::EngineError;

use super::{
    AbilityDefinition, AbilityLearningState, AdvancementAction, AdvancementInput, AdvancementRules,
    AttributeScores, RPG_ADVANCEMENT_SCHEMA, RaceAttributeCaps,
};

pub(super) fn validate_advancement_rules(rules: &AdvancementRules) -> Result<(), EngineError> {
    if rules.schema != RPG_ADVANCEMENT_SCHEMA {
        return invalid("rules.schema", format!("expected {RPG_ADVANCEMENT_SCHEMA}"));
    }
    if rules.max_level == 0 || rules.max_level > 100 {
        return invalid("rules.max_level", "must be inside 1..=100");
    }
    if rules.practice_bands.is_empty() || rules.practice_bands.len() > 16 {
        return invalid("rules.practice_bands", "must contain 1..=16 rows");
    }
    let mut previous = 0;
    for band in &rules.practice_bands {
        if band.min_wisdom <= previous || band.practices_per_level > 20 {
            return invalid(
                "rules.practice_bands",
                "wisdom bands must ascend with <=20 practices",
            );
        }
        previous = band.min_wisdom;
    }
    if rules.practice_intelligence_divisor == 0
        || rules.practice_cap == 0
        || rules.practice_cap > 100
        || rules.level_cap_per_level == 0
    {
        return invalid(
            "rules.practice",
            "practice and level-cap values must be positive",
        );
    }
    if rules.train_stat_increment <= 0
        || rules.train_sessions_per_level > 20
        || rules.train_hit_points <= 0
        || rules.train_mana <= 0
        || rules.train_movement <= 0
    {
        return invalid(
            "rules.train",
            "train awards and increments must be positive",
        );
    }
    validate_study_rules(rules)
}

fn validate_study_rules(rules: &AdvancementRules) -> Result<(), EngineError> {
    let mut costs = BTreeMap::new();
    for grant in &rules.study_grants {
        validate_token("rules.study_grants.class_id", &grant.class_id)?;
        if grant.max_level == 0 || grant.max_level > rules.max_level {
            return invalid(
                "rules.study_grants.max_level",
                "must be inside 1..=max_level",
            );
        }
        if grant.points_per_level == 0 && grant.milestone_points == 0 {
            return invalid("rules.study_grants", "grant row must award points");
        }
    }
    for cost in &rules.study_costs {
        validate_token("rules.study_costs.ability_id", &cost.ability_id)?;
        if cost.points == 0 {
            return invalid("rules.study_costs.points", "must be positive");
        }
        if costs
            .insert(cost.ability_id.as_str(), cost.points)
            .is_some()
        {
            return invalid("rules.study_costs.ability_id", "duplicate study cost");
        }
    }
    Ok(())
}

pub(super) fn validate_advancement_input(
    input: &AdvancementInput,
    rules: &AdvancementRules,
    ability_index: &BTreeMap<&str, &AbilityDefinition>,
) -> Result<(), EngineError> {
    validate_token("character_id", &input.character_id)?;
    validate_token("class_id", &input.class_id)?;
    validate_token("race_id", &input.race_id)?;
    if input.from_level == 0
        || input.to_level <= input.from_level
        || input.to_level > rules.max_level
    {
        return invalid(
            "level",
            "from/to levels must be an increasing range inside rules.max_level",
        );
    }
    if input.starting.level != input.from_level {
        return invalid("starting.level", "must equal from_level");
    }
    validate_positive_scores("attributes", input.attributes)?;
    let caps = read_race_caps(&input.race_id, &input.race_caps)?;
    validate_scores_against_caps(input.attributes, caps)?;
    validate_learning_states(&input.starting.abilities, ability_index)?;
    for action in &input.actions {
        validate_action_shape(action)?;
    }
    if let Some(focus) = &input.focused_ability_id {
        validate_token("focused_ability_id", focus)?;
        if !ability_index.contains_key(focus.as_str()) {
            return invalid("focused_ability_id", "unknown focused ability");
        }
    }
    Ok(())
}

fn validate_learning_states(
    states: &[AbilityLearningState],
    ability_index: &BTreeMap<&str, &AbilityDefinition>,
) -> Result<(), EngineError> {
    let mut seen = BTreeMap::new();
    for state in states {
        validate_token("learning.ability_id", &state.ability_id)?;
        if !ability_index.contains_key(state.ability_id.as_str()) {
            return invalid("learning.ability_id", "unknown ability in learning state");
        }
        if state.proficiency > 100 {
            return invalid("learning.proficiency", "must be <= 100");
        }
        if seen.insert(state.ability_id.as_str(), ()).is_some() {
            return invalid("learning.ability_id", "duplicate learning state");
        }
    }
    Ok(())
}

fn validate_action_shape(action: &AdvancementAction) -> Result<(), EngineError> {
    match action {
        AdvancementAction::Study { ability_id }
        | AdvancementAction::UseAbility { ability_id, .. } => {
            validate_token("action.ability_id", ability_id)
        }
        AdvancementAction::Practice {
            ability_id,
            sessions,
        } => {
            validate_token("action.ability_id", ability_id)?;
            validate_sessions("action.practice.sessions", *sessions)
        }
        AdvancementAction::TrainAttribute { sessions, .. }
        | AdvancementAction::TrainVital { sessions, .. } => {
            validate_sessions("action.train.sessions", *sessions)
        }
    }
}

fn validate_sessions(field: &str, sessions: u16) -> Result<(), EngineError> {
    if sessions == 0 || sessions > 20 {
        return invalid(field, "must be inside 1..=20");
    }
    Ok(())
}

pub(super) fn index_ability_catalog(
    input: &AdvancementInput,
) -> BTreeMap<&str, &AbilityDefinition> {
    input
        .ability_catalog
        .abilities
        .iter()
        .map(|ability| (ability.ability_id.as_str(), ability))
        .collect()
}

pub(super) fn read_race_caps(
    race_id: &str,
    rows: &[RaceAttributeCaps],
) -> Result<AttributeScores, EngineError> {
    rows.iter()
        .find(|row| row.race_id == race_id)
        .map(|row| row.caps)
        .ok_or_else(|| EngineError::RpgAdvancementInvalidConfig {
            field: "race_caps".to_string(),
            reason: format!("missing caps for {race_id}"),
        })
}

fn validate_scores_against_caps(
    scores: AttributeScores,
    caps: AttributeScores,
) -> Result<(), EngineError> {
    for (name, value) in score_pairs(scores) {
        if value > attribute_by_name(caps, name) {
            return invalid("attributes.cap", format!("{name} exceeds race cap"));
        }
    }
    Ok(())
}

fn validate_positive_scores(field: &str, scores: AttributeScores) -> Result<(), EngineError> {
    for (name, value) in score_pairs(scores) {
        if !(1..=100).contains(&value) {
            return invalid(format!("{field}.{name}"), "must be inside 1..=100");
        }
    }
    Ok(())
}

pub(super) fn score_pairs(scores: AttributeScores) -> [(&'static str, i16); 6] {
    [
        ("strength", scores.strength),
        ("intelligence", scores.intelligence),
        ("wisdom", scores.wisdom),
        ("dexterity", scores.dexterity),
        ("constitution", scores.constitution),
        ("charisma", scores.charisma),
    ]
}

fn attribute_by_name(scores: AttributeScores, name: &str) -> i16 {
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

pub(super) fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid(field, "must be 1..=96 bytes");
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

pub(super) fn invalid<T>(
    field: impl Into<String>,
    reason: impl std::fmt::Display,
) -> Result<T, EngineError> {
    Err(EngineError::RpgAdvancementInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    })
}
