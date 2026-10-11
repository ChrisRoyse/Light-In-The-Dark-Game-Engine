use std::collections::BTreeSet;

use crate::EngineError;

use super::{
    ActionProfile, CombatActionCommand, CombatActionKind, CombatArmorSet, CombatInput, CombatRules,
    CombatSkillRating, CombatWeapon, CombatantState, RPG_COMBAT_SCHEMA,
};

pub(super) fn validate_combat_rules(rules: &CombatRules) -> Result<(), EngineError> {
    if rules.schema != RPG_COMBAT_SCHEMA {
        return invalid("rules.schema", format!("expected {RPG_COMBAT_SCHEMA}"));
    }
    if rules.tick_millis == 0 || rules.round_ticks == 0 || rules.max_rounds == 0 {
        return invalid(
            "rules.timing",
            "tick, round, and max_rounds must be positive",
        );
    }
    if rules.round_ticks > 60 || rules.max_rounds > 120 {
        return invalid(
            "rules.timing",
            "round_ticks/max_rounds exceed bounded sim limits",
        );
    }
    if rules.hit_target <= 0 || rules.armor_class_divisor <= 0 || rules.min_damage <= 0 {
        return invalid(
            "rules.hit",
            "hit target, armor divisor, and min damage must be positive",
        );
    }
    if rules.max_defense_chance == 0 || rules.max_defense_chance > 100 {
        return invalid("rules.max_defense_chance", "must be inside 1..=100");
    }
    validate_defense_order(rules)?;
    validate_multi_attack_slots(rules)?;
    validate_weapon_matchups(rules)?;
    validate_armor_mitigations(rules)?;
    validate_action_profiles(rules)
}

pub(super) fn validate_combat_input(
    input: &CombatInput,
    rules: &CombatRules,
) -> Result<(), EngineError> {
    validate_token("encounter_id", &input.encounter_id)?;
    if input.rounds == 0 || input.rounds > rules.max_rounds {
        return invalid("rounds", "must be inside 1..=rules.max_rounds");
    }
    validate_combatant("attacker", &input.attacker)?;
    validate_combatant("defender", &input.defender)?;
    validate_combat_coverage(input, rules)?;
    validate_commands(&input.commands, input.rounds, rules)?;
    if input.roll_stream.is_empty() || input.roll_stream.len() > 512 {
        return invalid("roll_stream", "must contain 1..=512 deterministic rolls");
    }
    for roll in &input.roll_stream {
        if !(1..=100).contains(roll) {
            return invalid("roll_stream", "rolls must be inside 1..=100");
        }
    }
    Ok(())
}

fn validate_defense_order(rules: &CombatRules) -> Result<(), EngineError> {
    if rules.defense_order.is_empty() || rules.defense_order.len() > 8 {
        return invalid("rules.defense_order", "must contain 1..=8 defenses");
    }
    let mut seen = BTreeSet::new();
    for defense in &rules.defense_order {
        if !seen.insert(*defense) {
            return invalid("rules.defense_order", "duplicate defense");
        }
    }
    Ok(())
}

fn validate_multi_attack_slots(rules: &CombatRules) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for slot in &rules.multi_attack_slots {
        validate_token("rules.multi_attack_slots.skill_id", &slot.skill_id)?;
        if !(2..=8).contains(&slot.attack_slot) || slot.minimum_rating > 100 {
            return invalid("rules.multi_attack_slots", "slot/rating out of range");
        }
        if !seen.insert(slot.attack_slot) {
            return invalid("rules.multi_attack_slots.attack_slot", "duplicate slot");
        }
    }
    Ok(())
}

// The weapon matchup table is sparse by design (#1464): authors list only the
// pairings with a non-neutral modifier, and every unlisted pairing resolves to
// `WeaponMatchup::neutral`. Completeness is intentionally NOT required here; we
// validate only that listed rows are unique and in range.
fn validate_weapon_matchups(rules: &CombatRules) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for row in &rules.weapon_matchups {
        if !seen.insert((row.attacker, row.defender)) {
            return invalid("rules.weapon_matchups", "duplicate weapon matchup");
        }
        if !(-100..=100).contains(&row.hit_modifier) || !(-100..=100).contains(&row.parry_modifier)
        {
            return invalid(
                "rules.weapon_matchups",
                "modifiers must be inside -100..=100",
            );
        }
    }
    Ok(())
}

fn validate_armor_mitigations(rules: &CombatRules) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for row in &rules.armor_mitigations {
        if !seen.insert(row.armor_type) {
            return invalid("rules.armor_mitigations", "duplicate armor type");
        }
        if row.damage_reduction_percent > 95 {
            return invalid("rules.armor_mitigations", "reduction must be <= 95");
        }
    }
    Ok(())
}

fn validate_action_profiles(rules: &CombatRules) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for profile in &rules.action_profiles {
        if !seen.insert(profile.action) {
            return invalid("rules.action_profiles.action", "duplicate action");
        }
        if let Some(skill_id) = &profile.skill_id {
            validate_token("rules.action_profiles.skill_id", skill_id)?;
        }
        if profile.lag_ticks > rules.round_ticks * 4 {
            return invalid(
                "rules.action_profiles.lag_ticks",
                "lag exceeds bounded limit",
            );
        }
    }
    require_action_profile(rules, CombatActionKind::AutoAttack)?;
    Ok(())
}

pub(super) fn require_action_profile(
    rules: &CombatRules,
    action: CombatActionKind,
) -> Result<&ActionProfile, EngineError> {
    rules
        .action_profiles
        .iter()
        .find(|profile| profile.action == action)
        .ok_or_else(|| EngineError::RpgCombatInvalidConfig {
            field: "rules.action_profiles".to_string(),
            reason: format!("missing action profile {}", action.label()),
        })
}

fn validate_commands(
    commands: &[CombatActionCommand],
    rounds: u16,
    rules: &CombatRules,
) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for command in commands {
        if command.round_index >= rounds {
            return invalid(
                "commands.round_index",
                "command round is outside combat range",
            );
        }
        if !seen.insert(command.round_index) {
            return invalid("commands.round_index", "duplicate command round");
        }
        require_action_profile(rules, command.action)?;
    }
    Ok(())
}

fn validate_combatant(field: &str, combatant: &CombatantState) -> Result<(), EngineError> {
    validate_token(format!("{field}.combatant_id"), &combatant.combatant_id)?;
    if combatant.level == 0 || combatant.level > 100 {
        return invalid(format!("{field}.level"), "must be inside 1..=100");
    }
    if combatant.hit_points <= 0 || combatant.hit_points > 100_000 {
        return invalid(format!("{field}.hit_points"), "must be inside 1..=100000");
    }
    validate_weapon(field, &combatant.weapon)?;
    validate_armor(field, &combatant.armor)?;
    validate_skill_rows(field, &combatant.skills)
}

fn validate_combat_coverage(input: &CombatInput, rules: &CombatRules) -> Result<(), EngineError> {
    if !rules.weapon_matchups.iter().any(|row| {
        row.attacker == input.attacker.weapon.weapon_type
            && row.defender == input.defender.weapon.weapon_type
    }) {
        return invalid(
            "rules.weapon_matchups",
            format!(
                "missing matchup for attacker {} vs defender {}",
                input.attacker.weapon.weapon_type.label(),
                input.defender.weapon.weapon_type.label()
            ),
        );
    }
    for (field, armor_type) in [
        ("attacker.armor.armor_type", input.attacker.armor.armor_type),
        ("defender.armor.armor_type", input.defender.armor.armor_type),
    ] {
        if !rules
            .armor_mitigations
            .iter()
            .any(|row| row.armor_type == armor_type)
        {
            return invalid(
                "rules.armor_mitigations",
                format!("missing mitigation row for {field} {}", armor_type.label()),
            );
        }
    }
    Ok(())
}

fn validate_weapon(field: &str, weapon: &CombatWeapon) -> Result<(), EngineError> {
    validate_token(format!("{field}.weapon.weapon_id"), &weapon.weapon_id)?;
    if weapon.damage_min <= 0 || weapon.damage_max < weapon.damage_min || weapon.damage_max > 1000 {
        return invalid(format!("{field}.weapon.damage"), "invalid damage range");
    }
    if weapon.weight <= 0 || weapon.weight > 1000 {
        return invalid(format!("{field}.weapon.weight"), "must be inside 1..=1000");
    }
    Ok(())
}

fn validate_armor(field: &str, armor: &CombatArmorSet) -> Result<(), EngineError> {
    validate_token(format!("{field}.armor.armor_id"), &armor.armor_id)?;
    Ok(())
}

fn validate_skill_rows(field: &str, skills: &[CombatSkillRating]) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for skill in skills {
        validate_token(format!("{field}.skills.skill_id"), &skill.skill_id)?;
        if skill.rating > 100 {
            return invalid(format!("{field}.skills.rating"), "must be <= 100");
        }
        if !seen.insert(skill.skill_id.as_str()) {
            return invalid(format!("{field}.skills.skill_id"), "duplicate skill");
        }
    }
    Ok(())
}

pub(super) fn skill_rating(combatant: &CombatantState, skill_id: &str) -> u16 {
    combatant
        .skills
        .iter()
        .find(|skill| skill.skill_id == skill_id)
        .map(|skill| skill.rating)
        .unwrap_or(0)
}

fn validate_token(field: impl Into<String>, value: &str) -> Result<(), EngineError> {
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
    Err(EngineError::RpgCombatInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    })
}
