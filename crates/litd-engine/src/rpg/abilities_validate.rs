use std::collections::{BTreeMap, BTreeSet};

use crate::EngineError;

use super::{
    abilities_support::{
        invalid, validate_option_token, validate_text, validate_token, validate_token_list,
    },
    abilities_types::*,
};

pub(super) struct AbilityCatalogValidation {
    pub max_prerequisite_depth: usize,
}

pub(super) fn validate_ability_catalog(
    catalog: &AbilityCatalog,
) -> Result<AbilityCatalogValidation, EngineError> {
    validate_catalog_shape(catalog)?;
    let by_id = index_abilities(catalog)?;
    for ability in &catalog.abilities {
        validate_ability(ability, &by_id)?;
    }
    detect_prerequisite_cycles(&by_id)?;
    Ok(AbilityCatalogValidation {
        max_prerequisite_depth: max_prerequisite_depth(&by_id)?,
    })
}

fn validate_catalog_shape(catalog: &AbilityCatalog) -> Result<(), EngineError> {
    if catalog.schema != RPG_ABILITY_SCHEMA {
        return invalid("catalog.schema", format!("expected {RPG_ABILITY_SCHEMA}"));
    }
    validate_token("catalog_id", &catalog.catalog_id)?;
    if catalog.abilities.is_empty() || catalog.abilities.len() > 100_000 {
        return invalid("abilities", "catalog must contain 1..=100000 abilities");
    }
    Ok(())
}

fn index_abilities(
    catalog: &AbilityCatalog,
) -> Result<BTreeMap<&str, &AbilityDefinition>, EngineError> {
    let mut by_id = BTreeMap::new();
    for ability in &catalog.abilities {
        validate_token("ability_id", &ability.ability_id)?;
        if by_id.insert(ability.ability_id.as_str(), ability).is_some() {
            return invalid(
                "ability_id",
                format!("duplicate ability {}", ability.ability_id),
            );
        }
    }
    Ok(by_id)
}

fn validate_ability(
    ability: &AbilityDefinition,
    by_id: &BTreeMap<&str, &AbilityDefinition>,
) -> Result<(), EngineError> {
    validate_text("display_name", &ability.display_name, 96)?;
    validate_text("description", &ability.description, 4096)?;
    validate_category(&ability.category)?;
    validate_requirements(ability, by_id)?;
    validate_costs(&ability.costs)?;
    validate_timing(&ability.timing)?;
    validate_targeting(&ability.targeting)?;
    validate_effects(&ability.effects)?;
    validate_token_list("tags", &ability.tags, 64)?;
    validate_token_list("grants_tags", &ability.grants_tags, 64)?;
    validate_token_list("blocks_tags", &ability.blocks_tags, 64)?;
    validate_token_list("trigger_tags", &ability.trigger_tags, 32)?;
    if let Some(slot) = &ability.ui_slot {
        validate_token("ui_slot", slot)?;
    }
    validate_kind_requirements(ability)?;
    validate_activation_contract(ability)
}

fn validate_category(category: &AbilityCategory) -> Result<(), EngineError> {
    validate_token("category.family_id", &category.family_id)?;
    validate_option_token("category.school_id", category.school_id.as_deref())?;
    validate_option_token("category.path_id", category.path_id.as_deref())?;
    validate_option_token("category.branch_id", category.branch_id.as_deref())?;
    if category.rank == 0 || category.max_rank == 0 || category.rank > category.max_rank {
        return invalid("category.rank", "rank must be inside 1..=max_rank");
    }
    if category.max_rank > 100 {
        return invalid("category.max_rank", "max_rank must be <= 100");
    }
    Ok(())
}

fn validate_requirements(
    ability: &AbilityDefinition,
    by_id: &BTreeMap<&str, &AbilityDefinition>,
) -> Result<(), EngineError> {
    let requirements = &ability.requirements;
    if requirements.min_level == 0 || requirements.min_level > 100 {
        return invalid("requirements.min_level", "must be inside 1..=100");
    }
    validate_token_list("requirements.class_ids", &requirements.class_ids, 32)?;
    validate_token_list("requirements.race_ids", &requirements.race_ids, 32)?;
    validate_token_list("requirements.cabal_ids", &requirements.cabal_ids, 32)?;
    validate_token_list("requirements.alignments", &requirements.alignments, 9)?;
    validate_token_list("requirements.ethos", &requirements.ethos, 9)?;
    validate_token_list(
        "requirements.deity_domains",
        &requirements.deity_domains,
        32,
    )?;
    if requirements.prerequisites.len() > 32 {
        return invalid("requirements.prerequisites", "at most 32 prerequisites");
    }
    let mut seen = BTreeSet::new();
    for prereq in &requirements.prerequisites {
        validate_prerequisite(ability, prereq, requirements, by_id, &mut seen)?;
    }
    Ok(())
}

fn validate_prerequisite<'a>(
    ability: &AbilityDefinition,
    prereq: &'a AbilityPrerequisite,
    requirements: &AbilityRequirements,
    by_id: &BTreeMap<&str, &AbilityDefinition>,
    seen: &mut BTreeSet<&'a str>,
) -> Result<(), EngineError> {
    validate_token("requirements.prerequisite.ability_id", &prereq.ability_id)?;
    if prereq.ability_id == ability.ability_id {
        return invalid("requirements.prerequisite", "ability cannot require itself");
    }
    if !seen.insert(prereq.ability_id.as_str()) {
        return invalid(
            "requirements.prerequisite",
            format!("duplicate prerequisite {}", prereq.ability_id),
        );
    }
    if prereq.min_rank == 0 {
        return invalid("requirements.prerequisite.min_rank", "must be >= 1");
    }
    let Some(parent) = by_id.get(prereq.ability_id.as_str()) else {
        return invalid(
            "requirements.prerequisite.ability_id",
            format!("unknown ability {}", prereq.ability_id),
        );
    };
    if prereq.min_rank > parent.category.max_rank {
        return invalid(
            "requirements.prerequisite.min_rank",
            format!(
                "requires rank {} but {} max_rank is {}",
                prereq.min_rank, prereq.ability_id, parent.category.max_rank
            ),
        );
    }
    if parent.requirements.min_level > requirements.min_level {
        return invalid(
            "requirements.prerequisite.min_level",
            format!(
                "prerequisite {} unlocks at level {} after child level {}",
                prereq.ability_id, parent.requirements.min_level, requirements.min_level
            ),
        );
    }
    Ok(())
}

fn validate_kind_requirements(ability: &AbilityDefinition) -> Result<(), EngineError> {
    let req = &ability.requirements;
    match ability.kind {
        AbilityKind::Skill | AbilityKind::Spell | AbilityKind::Song => {
            if req.class_ids.is_empty() {
                return invalid(
                    "requirements.class_ids",
                    "class abilities require a class gate",
                );
            }
        }
        AbilityKind::Supplication => {
            if req.deity_domains.is_empty() {
                return invalid(
                    "requirements.deity_domains",
                    "supplications require deity domain gates",
                );
            }
        }
        AbilityKind::CabalPower => {
            if req.cabal_ids.is_empty() {
                return invalid("requirements.cabal_ids", "cabal powers require cabal gates");
            }
        }
        AbilityKind::RacialInnate => {
            if req.race_ids.is_empty() {
                return invalid(
                    "requirements.race_ids",
                    "racial/innate abilities require races",
                );
            }
        }
    }
    Ok(())
}

fn validate_costs(costs: &[AbilityCost]) -> Result<(), EngineError> {
    if costs.len() > 8 {
        return invalid("costs", "at most 8 resource costs");
    }
    let mut seen = BTreeSet::new();
    for cost in costs {
        validate_token("cost.resource_id", &cost.resource_id)?;
        if !seen.insert(cost.resource_id.as_str()) {
            return invalid(
                "cost.resource_id",
                format!("duplicate {}", cost.resource_id),
            );
        }
        if !(1..=100_000).contains(&cost.amount) {
            return invalid("cost.amount", "must be inside 1..=100000");
        }
    }
    Ok(())
}

fn validate_timing(timing: &AbilityTiming) -> Result<(), EngineError> {
    if timing.cast_ticks > 600 || timing.round_ticks > 600 || timing.lag_ticks > 600 {
        return invalid("timing", "cast/round/lag ticks must be <= 600");
    }
    if timing.cooldown_ticks > 216_000 {
        return invalid("timing.cooldown_ticks", "must be <= 216000");
    }
    if let Some(group) = &timing.cooldown_group {
        validate_token("timing.cooldown_group", group)?;
        if timing.cooldown_ticks == 0 {
            return invalid(
                "timing.cooldown_group",
                "cooldown_group requires cooldown_ticks > 0",
            );
        }
    }
    Ok(())
}

fn validate_targeting(targeting: &AbilityTargeting) -> Result<(), EngineError> {
    if targeting.mode == AbilityTargetMode::None {
        if targeting.max_targets != 0 || targeting.range_tiles != 0 {
            return invalid(
                "targeting.none",
                "none target mode must have zero max_targets and range",
            );
        }
        return Ok(());
    }
    if targeting.max_targets == 0 || targeting.max_targets > 200 {
        return invalid("targeting.max_targets", "must be inside 1..=200");
    }
    if targeting.range_tiles > 200 {
        return invalid("targeting.range_tiles", "must be <= 200");
    }
    Ok(())
}

fn validate_effects(effects: &[AbilityEffect]) -> Result<(), EngineError> {
    if effects.is_empty() || effects.len() > 32 {
        return invalid("effects", "ability must define 1..=32 effects");
    }
    let mut seen = BTreeSet::new();
    for effect in effects {
        validate_effect(effect, &mut seen)?;
    }
    Ok(())
}

fn validate_effect<'a>(
    effect: &'a AbilityEffect,
    seen: &mut BTreeSet<&'a str>,
) -> Result<(), EngineError> {
    validate_token("effect.effect_id", &effect.effect_id)?;
    if !seen.insert(effect.effect_id.as_str()) {
        return invalid(
            "effect.effect_id",
            format!("duplicate {}", effect.effect_id),
        );
    }
    validate_token("effect.channel", &effect.channel)?;
    if effect.magnitude_min > effect.magnitude_max {
        return invalid("effect.magnitude", "min must be <= max");
    }
    if !(-1_000_000..=1_000_000).contains(&effect.magnitude_min)
        || !(-1_000_000..=1_000_000).contains(&effect.magnitude_max)
    {
        return invalid("effect.magnitude", "must be inside +/-1000000");
    }
    if effect.duration_ticks > 216_000 {
        return invalid("effect.duration_ticks", "must be <= 216000");
    }
    if effect.tick_interval > effect.duration_ticks && effect.duration_ticks != 0 {
        return invalid(
            "effect.tick_interval",
            "tick interval cannot exceed duration",
        );
    }
    if effect.tick_interval > 0 && effect.duration_ticks == 0 {
        return invalid("effect.tick_interval", "tick interval requires duration");
    }
    validate_stacking(effect)?;
    if numeric_effect_is_zero(effect) {
        return invalid(
            "effect.magnitude",
            "numeric effect magnitude cannot be zero",
        );
    }
    Ok(())
}

fn numeric_effect_is_zero(effect: &AbilityEffect) -> bool {
    matches!(
        effect.kind,
        AbilityEffectKind::Damage | AbilityEffectKind::Heal | AbilityEffectKind::ResourceDelta
    ) && effect.magnitude_min == 0
        && effect.magnitude_max == 0
}

fn validate_stacking(effect: &AbilityEffect) -> Result<(), EngineError> {
    match effect.stacking_policy {
        AbilityStackingPolicy::StackUpToCap if (effect.stack_cap < 2 || effect.stack_cap > 100) => {
            return invalid("effect.stack_cap", "stacking cap must be inside 2..=100");
        }
        _ if effect.stack_cap != 0 => {
            return invalid(
                "effect.stack_cap",
                "non-stacking effects must use stack_cap=0",
            );
        }
        _ => {}
    }
    Ok(())
}

fn validate_activation_contract(ability: &AbilityDefinition) -> Result<(), EngineError> {
    match ability.activation {
        AbilityActivation::Active => validate_active(ability),
        AbilityActivation::Passive => validate_passive(ability),
        AbilityActivation::Automatic => validate_automatic(ability),
    }
}

fn validate_active(ability: &AbilityDefinition) -> Result<(), EngineError> {
    if ability.targeting.mode == AbilityTargetMode::None {
        return invalid(
            "activation.active.targeting",
            "active abilities require a target",
        );
    }
    if ability.costs.is_empty()
        && ability.timing.lag_ticks == 0
        && ability.timing.cooldown_ticks == 0
    {
        return invalid(
            "activation.active.cost_or_wait",
            "active abilities require cost, lag, or cooldown",
        );
    }
    Ok(())
}

fn validate_passive(ability: &AbilityDefinition) -> Result<(), EngineError> {
    if !ability.costs.is_empty()
        || ability.timing.cast_ticks != 0
        || ability.timing.round_ticks != 0
        || ability.timing.lag_ticks != 0
        || ability.timing.cooldown_ticks != 0
        || ability.targeting.mode != AbilityTargetMode::None
        || !ability.trigger_tags.is_empty()
    {
        return invalid(
            "activation.passive",
            "passive abilities cannot define costs, timing, targets, or triggers",
        );
    }
    Ok(())
}

fn validate_automatic(ability: &AbilityDefinition) -> Result<(), EngineError> {
    if !ability.costs.is_empty()
        || ability.timing.cast_ticks != 0
        || ability.timing.round_ticks != 0
        || ability.timing.lag_ticks != 0
    {
        return invalid(
            "activation.automatic",
            "automatic abilities cannot define costs or manual wait timing",
        );
    }
    if ability.trigger_tags.is_empty() {
        return invalid(
            "activation.automatic.trigger_tags",
            "automatic abilities require trigger tags",
        );
    }
    Ok(())
}

fn detect_prerequisite_cycles(
    by_id: &BTreeMap<&str, &AbilityDefinition>,
) -> Result<(), EngineError> {
    let mut states = BTreeMap::<&str, u8>::new();
    for ability_id in by_id.keys().copied() {
        visit_prereq(ability_id, by_id, &mut states, &mut Vec::new())?;
    }
    Ok(())
}

fn visit_prereq<'a>(
    ability_id: &'a str,
    by_id: &BTreeMap<&'a str, &'a AbilityDefinition>,
    states: &mut BTreeMap<&'a str, u8>,
    path: &mut Vec<&'a str>,
) -> Result<(), EngineError> {
    match states.get(ability_id).copied() {
        Some(2) => return Ok(()),
        Some(1) => {
            path.push(ability_id);
            return invalid("requirements.prerequisite.cycle", path.join(" -> "));
        }
        _ => {}
    }
    states.insert(ability_id, 1);
    path.push(ability_id);
    let ability = by_id
        .get(ability_id)
        .expect("ability id comes from indexed catalog");
    for prereq in &ability.requirements.prerequisites {
        visit_prereq(prereq.ability_id.as_str(), by_id, states, path)?;
    }
    path.pop();
    states.insert(ability_id, 2);
    Ok(())
}

fn max_prerequisite_depth(
    by_id: &BTreeMap<&str, &AbilityDefinition>,
) -> Result<usize, EngineError> {
    let mut memo = BTreeMap::<&str, usize>::new();
    let mut max_depth = 0;
    for ability_id in by_id.keys().copied() {
        max_depth = max_depth.max(prereq_depth(ability_id, by_id, &mut memo)?);
    }
    Ok(max_depth)
}

fn prereq_depth<'a>(
    ability_id: &'a str,
    by_id: &BTreeMap<&'a str, &'a AbilityDefinition>,
    memo: &mut BTreeMap<&'a str, usize>,
) -> Result<usize, EngineError> {
    if let Some(depth) = memo.get(ability_id).copied() {
        return Ok(depth);
    }
    let ability = by_id
        .get(ability_id)
        .ok_or_else(|| EngineError::RpgAbilityInvalidConfig {
            field: "requirements.prerequisite.ability_id".to_string(),
            reason: format!("unknown ability {ability_id}"),
        })?;
    let mut depth = 0;
    for prereq in &ability.requirements.prerequisites {
        depth = depth.max(prereq_depth(prereq.ability_id.as_str(), by_id, memo)? + 1);
    }
    memo.insert(ability_id, depth);
    Ok(depth)
}
