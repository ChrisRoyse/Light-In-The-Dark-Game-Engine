use crate::EngineError;

use super::{
    AdvancementAction, AdvancementInput, AdvancementReadback, AdvancementRules, ClassSpec,
    StudyAbilityCost,
    advancement_apply::{apply_advancement_action, grant_advancement_awards},
    advancement_readback::{advancement_awards, build_advancement_readback, level_cap},
    advancement_validate::{
        index_ability_catalog, validate_advancement_input, validate_advancement_rules,
    },
    build_ability_catalog_readback,
    validate_class_spec,
};

/// Apply an advancement plan against one authored [`ClassSpec`].
///
/// The legacy entry point accepts a generic ability catalog because it predates the
/// ClassSpec acquisition contract. This entry point is the production boundary for
/// class-authorized point-buy: every ability action must be present in the selected
/// class, and every authored acquisition cost replaces the generic study-cost table.
/// It deliberately delegates the actual state transition to the existing deterministic
/// engine after this admission pass, so there is one mutation path and one readback shape.
pub fn apply_class_spec_advancement_plan(
    input: &AdvancementInput,
    rules: &AdvancementRules,
    class_spec: &ClassSpec,
) -> Result<AdvancementReadback, EngineError> {
    validate_class_spec(class_spec)?;
    if input.class_id != class_spec.class_id {
        return Err(EngineError::RpgAdvancementInvalidConfig {
            field: "class_id".to_string(),
            reason: format!(
                "advancement selected {} but ClassSpec is {}",
                input.class_id, class_spec.class_id
            ),
        });
    }
    if input.to_level > class_spec.max_level {
        return Err(EngineError::RpgAdvancementInvalidConfig {
            field: "class_spec.max_level".to_string(),
            reason: format!(
                "class {} stops at level {}, requested {}",
                class_spec.class_id, class_spec.max_level, input.to_level
            ),
        });
    }

    let authored = class_spec
        .leveled_abilities
        .iter()
        .map(|ability| (ability.ability_id.as_str(), ability))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut class_rules = rules.clone();
    class_rules.study_costs = class_spec
        .leveled_abilities
        .iter()
        .filter_map(|ability| {
            ability.acquisition.as_ref().map(|acquisition| StudyAbilityCost {
                ability_id: ability.ability_id.clone(),
                points: acquisition.cost,
            })
        })
        .collect();

    for action in &input.actions {
        let ability_id = match action {
            AdvancementAction::Study { ability_id }
            | AdvancementAction::Practice { ability_id, .. }
            | AdvancementAction::UseAbility { ability_id, .. } => Some(ability_id),
            AdvancementAction::TrainAttribute { .. } | AdvancementAction::TrainVital { .. } => None,
        };
        let Some(ability_id) = ability_id else { continue };
        let Some(class_ability) = authored.get(ability_id.as_str()) else {
            return Err(EngineError::RpgAdvancementInvalidConfig {
                field: "class_spec.ability_id".to_string(),
                reason: format!(
                    "ability {} is not authored for class {}",
                    ability_id, class_spec.class_id
                ),
            });
        };
        if matches!(action, AdvancementAction::Study { .. })
            && class_ability.acquisition.is_none()
        {
            return Err(EngineError::RpgAdvancementInvalidConfig {
                field: "class_spec.acquisition".to_string(),
                reason: format!(
                    "ability {} is level-granted and has no point-buy acquisition cost",
                    ability_id
                ),
            });
        }
    }

    apply_advancement_plan(input, &class_rules)
}

pub fn apply_advancement_plan(
    input: &AdvancementInput,
    rules: &AdvancementRules,
) -> Result<AdvancementReadback, EngineError> {
    validate_advancement_rules(rules)?;
    let catalog = build_ability_catalog_readback(&input.ability_catalog)?;
    let ability_index = index_ability_catalog(input);
    validate_advancement_input(input, rules, &ability_index)?;
    let awards = advancement_awards(input, rules);
    let level_cap = level_cap(input.to_level, rules)?;
    let mut state = input.starting.clone();
    grant_advancement_awards(&mut state, input.to_level, &awards)?;
    let mut actions = Vec::with_capacity(input.actions.len());
    for (index, action) in input.actions.iter().enumerate() {
        actions.push(apply_advancement_action(
            index,
            action,
            &mut state,
            input,
            rules,
            &ability_index,
            level_cap,
        )?);
    }
    build_advancement_readback(input, rules, &catalog, state, awards, level_cap, actions)
}
