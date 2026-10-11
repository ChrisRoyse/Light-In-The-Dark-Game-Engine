use std::collections::BTreeMap;

use crate::EngineError;

use super::{
    AbilityDefinition, AbilityLearningState, AdvancementAction, AdvancementActionReadback,
    AdvancementAttribute, AdvancementAwardReadback, AdvancementInput, AdvancementPools,
    AdvancementRules, AdvancementState, AdvancementVital, AdvancementVitals, AttributeScores,
    advancement_readback::{practice_gain, study_cost, study_cost_optional},
    advancement_validate::{invalid, read_race_caps},
};

pub(super) fn grant_advancement_awards(
    state: &mut AdvancementState,
    to_level: u16,
    awards: &AdvancementAwardReadback,
) -> Result<(), EngineError> {
    state.level = to_level;
    state.pools.practices =
        checked_pool(state.pools.practices, awards.practice_awarded, "practices")?;
    state.pools.trains = checked_pool(state.pools.trains, awards.trains_awarded, "trains")?;
    state.pools.study_points = checked_pool(
        state.pools.study_points,
        awards.study_points_awarded,
        "study_points",
    )?;
    Ok(())
}

pub(super) fn apply_advancement_action(
    index: usize,
    action: &AdvancementAction,
    state: &mut AdvancementState,
    input: &AdvancementInput,
    rules: &AdvancementRules,
    ability_index: &BTreeMap<&str, &AbilityDefinition>,
    cap: u16,
) -> Result<AdvancementActionReadback, EngineError> {
    match action {
        AdvancementAction::Study { ability_id } => apply_study(
            index,
            ability_id,
            state,
            rules,
            ability_index,
            input.to_level,
        ),
        AdvancementAction::Practice {
            ability_id,
            sessions,
        } => apply_practice(ApplyPracticeArgs {
            index,
            ability_id,
            sessions: *sessions,
            state,
            input,
            rules,
            ability_index,
            cap,
        }),
        AdvancementAction::TrainAttribute {
            attribute,
            sessions,
        } => apply_train_attribute(index, *attribute, *sessions, state, input, rules),
        AdvancementAction::TrainVital { vital, sessions } => {
            apply_train_vital(index, *vital, *sessions, state, rules)
        }
        AdvancementAction::UseAbility {
            ability_id,
            successful_uses,
        } => apply_use(ApplyUseArgs {
            index,
            ability_id,
            successful_uses: *successful_uses,
            state,
            input,
            rules,
            ability_index,
            cap,
        }),
    }
}

fn apply_study(
    index: usize,
    ability_id: &str,
    state: &mut AdvancementState,
    rules: &AdvancementRules,
    ability_index: &BTreeMap<&str, &AbilityDefinition>,
    level: u16,
) -> Result<AdvancementActionReadback, EngineError> {
    require_ability_available(ability_id, ability_index, level)?;
    require_prerequisites(ability_id, state, ability_index)?;
    let cost = study_cost(ability_id, rules)?;
    let before_pool = state.pools.clone();
    spend(&mut state.pools.study_points, cost, "study_points")?;
    let entry = learning_entry(state, ability_id);
    let before = i32::from(entry.proficiency);
    entry.studied = true;
    action_readback(ActionReadbackArgs {
        action_index: index,
        action_kind: "study",
        target_id: ability_id,
        before,
        after: before,
        before_pool,
        after_pool: &state.pools,
        cap_applied: false,
    })
}

struct ApplyPracticeArgs<'a> {
    index: usize,
    ability_id: &'a str,
    sessions: u16,
    state: &'a mut AdvancementState,
    input: &'a AdvancementInput,
    rules: &'a AdvancementRules,
    ability_index: &'a BTreeMap<&'a str, &'a AbilityDefinition>,
    cap: u16,
}

fn apply_practice(args: ApplyPracticeArgs<'_>) -> Result<AdvancementActionReadback, EngineError> {
    let ApplyPracticeArgs {
        index,
        ability_id,
        sessions,
        state,
        input,
        rules,
        ability_index,
        cap,
    } = args;
    require_ability_available(ability_id, ability_index, input.to_level)?;
    require_prerequisites(ability_id, state, ability_index)?;
    require_studied_if_needed(ability_id, state, rules)?;
    let before_pool = state.pools.clone();
    spend(&mut state.pools.practices, sessions, "practices")?;
    let gain = sessions * practice_gain(input.attributes.intelligence, rules);
    let practice_cap = rules.practice_cap.min(cap);
    let entry = learning_entry(state, ability_id);
    let before = entry.proficiency;
    let raw = before.saturating_add(gain);
    entry.proficiency = raw.min(practice_cap);
    entry.practiced = true;
    action_readback(ActionReadbackArgs {
        action_index: index,
        action_kind: "practice",
        target_id: ability_id,
        before: i32::from(before),
        after: i32::from(entry.proficiency),
        before_pool,
        after_pool: &state.pools,
        cap_applied: raw > practice_cap,
    })
}

struct ApplyUseArgs<'a> {
    index: usize,
    ability_id: &'a str,
    successful_uses: u16,
    state: &'a mut AdvancementState,
    input: &'a AdvancementInput,
    rules: &'a AdvancementRules,
    ability_index: &'a BTreeMap<&'a str, &'a AbilityDefinition>,
    cap: u16,
}

fn apply_use(args: ApplyUseArgs<'_>) -> Result<AdvancementActionReadback, EngineError> {
    let ApplyUseArgs {
        index,
        ability_id,
        successful_uses,
        state,
        input,
        rules,
        ability_index,
        cap,
    } = args;
    if successful_uses == 0 || successful_uses > 10_000 {
        return invalid("action.use.successful_uses", "must be inside 1..=10000");
    }
    require_ability_available(ability_id, ability_index, input.to_level)?;
    require_prerequisites(ability_id, state, ability_index)?;
    let before_pool = state.pools.clone();
    let focus_bonus = if input.focused_ability_id.as_deref() == Some(ability_id) {
        rules.focused_skill_learn_bonus
    } else {
        0
    };
    let gain = successful_uses * (rules.use_gain_per_success + focus_bonus);
    let entry = learning_entry(state, ability_id);
    if !entry.practiced && !entry.studied && entry.proficiency == 0 {
        return invalid(
            "action.use.ability_id",
            "ability must be learned before use improvement",
        );
    }
    let before = entry.proficiency;
    let raw = before.saturating_add(gain);
    entry.proficiency = raw.min(cap).min(100);
    action_readback(ActionReadbackArgs {
        action_index: index,
        action_kind: "use_ability",
        target_id: ability_id,
        before: i32::from(before),
        after: i32::from(entry.proficiency),
        before_pool,
        after_pool: &state.pools,
        cap_applied: raw > cap,
    })
}

fn apply_train_attribute(
    index: usize,
    attribute: AdvancementAttribute,
    sessions: u16,
    state: &mut AdvancementState,
    input: &AdvancementInput,
    rules: &AdvancementRules,
) -> Result<AdvancementActionReadback, EngineError> {
    let caps = read_race_caps(&input.race_id, &input.race_caps)?;
    let before_pool = state.pools.clone();
    spend(&mut state.pools.trains, sessions, "trains")?;
    let before = attribute_value(state.attributes, attribute);
    let after = before + rules.train_stat_increment * sessions as i16;
    if after > attribute_value(caps, attribute) {
        return invalid("action.train_attribute.cap", "train would exceed race cap");
    }
    set_attribute(&mut state.attributes, attribute, after);
    action_readback(ActionReadbackArgs {
        action_index: index,
        action_kind: "train_attribute",
        target_id: attribute.label(),
        before: i32::from(before),
        after: i32::from(after),
        before_pool,
        after_pool: &state.pools,
        cap_applied: false,
    })
}

fn apply_train_vital(
    index: usize,
    vital: AdvancementVital,
    sessions: u16,
    state: &mut AdvancementState,
    rules: &AdvancementRules,
) -> Result<AdvancementActionReadback, EngineError> {
    let before_pool = state.pools.clone();
    spend(&mut state.pools.trains, sessions, "trains")?;
    let before = vital_value(&state.vitals, vital);
    let delta = vital_increment(vital, rules) * i32::from(sessions);
    set_vital(&mut state.vitals, vital, before + delta)?;
    action_readback(ActionReadbackArgs {
        action_index: index,
        action_kind: "train_vital",
        target_id: vital.label(),
        before,
        after: before + delta,
        before_pool,
        after_pool: &state.pools,
        cap_applied: false,
    })
}

fn require_ability_available(
    ability_id: &str,
    ability_index: &BTreeMap<&str, &AbilityDefinition>,
    current_level: u16,
) -> Result<(), EngineError> {
    let ability =
        ability_index
            .get(ability_id)
            .ok_or_else(|| EngineError::RpgAdvancementInvalidConfig {
                field: "ability_id".to_string(),
                reason: format!("unknown ability {ability_id}"),
            })?;
    if ability.requirements.min_level > current_level {
        return invalid("ability.level", "ability is above current level");
    }
    Ok(())
}

fn require_prerequisites(
    ability_id: &str,
    state: &AdvancementState,
    ability_index: &BTreeMap<&str, &AbilityDefinition>,
) -> Result<(), EngineError> {
    let ability =
        ability_index
            .get(ability_id)
            .ok_or_else(|| EngineError::RpgAdvancementInvalidConfig {
                field: "ability_id".to_string(),
                reason: format!("unknown ability {ability_id}"),
            })?;
    for prereq in &ability.requirements.prerequisites {
        let learned = state
            .abilities
            .iter()
            .find(|item| item.ability_id == prereq.ability_id)
            .map(|item| item.proficiency >= prereq.min_rank)
            .unwrap_or(false);
        if !learned {
            return invalid(
                "ability.prerequisite",
                format!("missing {}", prereq.ability_id),
            );
        }
    }
    Ok(())
}

fn require_studied_if_needed(
    ability_id: &str,
    state: &AdvancementState,
    rules: &AdvancementRules,
) -> Result<(), EngineError> {
    if study_cost_optional(ability_id, rules).is_some()
        && !state
            .abilities
            .iter()
            .any(|item| item.ability_id == ability_id && item.studied)
    {
        return invalid("ability.study", "ability must be studied before practice");
    }
    Ok(())
}

fn learning_entry<'a>(
    state: &'a mut AdvancementState,
    ability_id: &str,
) -> &'a mut AbilityLearningState {
    if let Some(index) = state
        .abilities
        .iter()
        .position(|item| item.ability_id == ability_id)
    {
        return &mut state.abilities[index];
    }
    let index = state.abilities.len();
    state.abilities.push(AbilityLearningState {
        ability_id: ability_id.to_string(),
        proficiency: 0,
        studied: false,
        practiced: false,
    });
    &mut state.abilities[index]
}

struct ActionReadbackArgs<'a> {
    action_index: usize,
    action_kind: &'a str,
    target_id: &'a str,
    before: i32,
    after: i32,
    before_pool: AdvancementPools,
    after_pool: &'a AdvancementPools,
    cap_applied: bool,
}

fn action_readback(args: ActionReadbackArgs<'_>) -> Result<AdvancementActionReadback, EngineError> {
    let ActionReadbackArgs {
        action_index,
        action_kind,
        target_id,
        before,
        after,
        before_pool,
        after_pool,
        cap_applied,
    } = args;
    Ok(AdvancementActionReadback {
        action_index,
        action_kind: action_kind.to_string(),
        target_id: target_id.to_string(),
        before_value: before,
        after_value: after,
        delta: after - before,
        before_pool,
        after_pool: after_pool.clone(),
        cap_applied,
    })
}

fn spend(pool: &mut u16, amount: u16, field: &str) -> Result<(), EngineError> {
    if *pool < amount {
        return invalid(field, "insufficient advancement pool");
    }
    *pool -= amount;
    Ok(())
}

fn checked_pool(lhs: u16, rhs: u16, field: &str) -> Result<u16, EngineError> {
    lhs.checked_add(rhs)
        .ok_or_else(|| EngineError::RpgAdvancementInvalidConfig {
            field: field.to_string(),
            reason: "advancement pool overflowed".to_string(),
        })
}

fn attribute_value(scores: AttributeScores, attribute: AdvancementAttribute) -> i16 {
    match attribute {
        AdvancementAttribute::Strength => scores.strength,
        AdvancementAttribute::Intelligence => scores.intelligence,
        AdvancementAttribute::Wisdom => scores.wisdom,
        AdvancementAttribute::Dexterity => scores.dexterity,
        AdvancementAttribute::Constitution => scores.constitution,
        AdvancementAttribute::Charisma => scores.charisma,
    }
}

fn set_attribute(scores: &mut AttributeScores, attribute: AdvancementAttribute, value: i16) {
    match attribute {
        AdvancementAttribute::Strength => scores.strength = value,
        AdvancementAttribute::Intelligence => scores.intelligence = value,
        AdvancementAttribute::Wisdom => scores.wisdom = value,
        AdvancementAttribute::Dexterity => scores.dexterity = value,
        AdvancementAttribute::Constitution => scores.constitution = value,
        AdvancementAttribute::Charisma => scores.charisma = value,
    }
}

fn vital_value(vitals: &AdvancementVitals, vital: AdvancementVital) -> i32 {
    match vital {
        AdvancementVital::HitPoints => vitals.hit_points,
        AdvancementVital::Mana => vitals.mana,
        AdvancementVital::Movement => vitals.movement,
    }
}

fn vital_increment(vital: AdvancementVital, rules: &AdvancementRules) -> i32 {
    match vital {
        AdvancementVital::HitPoints => rules.train_hit_points,
        AdvancementVital::Mana => rules.train_mana,
        AdvancementVital::Movement => rules.train_movement,
    }
}

fn set_vital(
    vitals: &mut AdvancementVitals,
    vital: AdvancementVital,
    value: i32,
) -> Result<(), EngineError> {
    if value < 0 {
        return invalid("vitals", "vital value cannot become negative");
    }
    match vital {
        AdvancementVital::HitPoints => vitals.hit_points = value,
        AdvancementVital::Mana => vitals.mana = value,
        AdvancementVital::Movement => vitals.movement = value,
    }
    Ok(())
}
