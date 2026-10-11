use crate::EngineError;

use super::{
    ActionProfile, CombatActionCommand, CombatActionKind, CombatArmorType, CombatAttackReadback,
    CombatDefenseAttemptReadback, CombatDefenseKind, CombatInput, CombatMultiAttackReadback,
    CombatRoundReadback, CombatRules, CombatWeaponType, CombatantState, MultiAttackSlot,
    WeaponMatchup,
    combat_validate::{invalid, require_action_profile, skill_rating},
};

pub(super) struct CombatResolution {
    pub attacker: CombatantState,
    pub defender: CombatantState,
    pub rounds: Vec<CombatRoundReadback>,
    pub multi_attack_checks: Vec<CombatMultiAttackReadback>,
    pub total_damage: i32,
    pub rolls_consumed: usize,
}

pub(super) fn resolve_combat(
    input: &CombatInput,
    rules: &CombatRules,
) -> Result<CombatResolution, EngineError> {
    let mut attacker = input.attacker.clone();
    let mut defender = input.defender.clone();
    let mut rolls = RollCursor::new(&input.roll_stream);
    let mut rounds = Vec::new();
    let mut checks = Vec::new();
    let mut attack_index = 0;
    let mut total_damage = 0;
    for round_index in 0..input.rounds {
        let wait_before = attacker.wait_state_ticks;
        let tick_start = u32::from(round_index) * rules.round_ticks;
        if wait_before > 0 {
            attacker.wait_state_ticks = wait_before.saturating_sub(rules.round_ticks);
            rounds.push(CombatRoundReadback {
                round_index,
                tick_start,
                action_kind: "wait_state".to_string(),
                skipped_by_wait_state: true,
                attacker_wait_before: wait_before,
                attacker_wait_after: attacker.wait_state_ticks,
                attacks: Vec::new(),
            });
            continue;
        }
        if defender.hit_points <= 0 {
            rounds.push(CombatRoundReadback {
                round_index,
                tick_start,
                action_kind: "combat_over".to_string(),
                skipped_by_wait_state: false,
                attacker_wait_before: 0,
                attacker_wait_after: 0,
                attacks: Vec::new(),
            });
            continue;
        }
        let action = command_for_round(round_index, &input.commands);
        let profile = require_action_profile(rules, action)?;
        let mut attacks = Vec::new();
        if action == CombatActionKind::AutoAttack {
            let attack = resolve_attack(
                &mut attack_index,
                AttackSpec {
                    slot: 1,
                    action,
                    granted_by_skill: None,
                },
                &attacker,
                &mut defender,
                profile,
                rules,
                &mut rolls,
            )?;
            total_damage += attack.mitigated_damage;
            attacks.push(attack);
            for slot in &rules.multi_attack_slots {
                let check = multi_attack_check(round_index, slot, &attacker, &mut rolls)?;
                let granted = check.granted;
                checks.push(check);
                if granted {
                    let attack = resolve_attack(
                        &mut attack_index,
                        AttackSpec {
                            slot: slot.attack_slot,
                            action,
                            granted_by_skill: Some(slot.skill_id.clone()),
                        },
                        &attacker,
                        &mut defender,
                        profile,
                        rules,
                        &mut rolls,
                    )?;
                    total_damage += attack.mitigated_damage;
                    attacks.push(attack);
                }
            }
        } else {
            let attack = resolve_attack(
                &mut attack_index,
                AttackSpec {
                    slot: 1,
                    action,
                    granted_by_skill: profile.skill_id.clone(),
                },
                &attacker,
                &mut defender,
                profile,
                rules,
                &mut rolls,
            )?;
            total_damage += attack.mitigated_damage;
            attacks.push(attack);
            attacker.wait_state_ticks = attacker.wait_state_ticks.saturating_add(profile.lag_ticks);
        }
        rounds.push(CombatRoundReadback {
            round_index,
            tick_start,
            action_kind: action.label().to_string(),
            skipped_by_wait_state: false,
            attacker_wait_before: wait_before,
            attacker_wait_after: attacker.wait_state_ticks,
            attacks,
        });
    }
    Ok(CombatResolution {
        attacker,
        defender,
        rounds,
        multi_attack_checks: checks,
        total_damage,
        rolls_consumed: rolls.position,
    })
}

struct AttackSpec {
    slot: u8,
    action: CombatActionKind,
    granted_by_skill: Option<String>,
}

fn resolve_attack(
    attack_index: &mut usize,
    spec: AttackSpec,
    attacker: &CombatantState,
    defender: &mut CombatantState,
    profile: &ActionProfile,
    rules: &CombatRules,
    rolls: &mut RollCursor<'_>,
) -> Result<CombatAttackReadback, EngineError> {
    let index = *attack_index;
    *attack_index += 1;
    let attack_roll = rolls.next("attack.hit_roll")?;
    let matchup = weapon_matchup(
        &rules.weapon_matchups,
        attacker.weapon.weapon_type,
        defender.weapon.weapon_type,
    );
    let specialization_bonus = weapon_specialization_bonus(attacker, rules);
    let armor_term = defender.stats.armor_class / rules.armor_class_divisor;
    let hit_total = rules.hit_base
        + i32::from(attack_roll)
        + attacker.stats.hitroll
        + matchup.hit_modifier
        + specialization_bonus
        + profile.hit_modifier
        - armor_term;
    let hit = attack_roll == 100 || (attack_roll != 1 && hit_total >= rules.hit_target);
    let mut defense_attempts = Vec::new();
    let mut defended_by = None;
    if hit {
        for defense in &rules.defense_order {
            let chance = defense_chance(*defense, attacker, defender, matchup, rules);
            if chance == 0 {
                continue;
            }
            let roll = rolls.next(format!("defense.{}", defense.label()))?;
            let success = roll <= chance;
            defense_attempts.push(CombatDefenseAttemptReadback {
                defense: defense.label().to_string(),
                chance,
                roll,
                success,
            });
            if success {
                defended_by = Some(defense.label().to_string());
                break;
            }
        }
    }
    let mut damage_roll = None;
    let mut raw_damage = 0;
    let mut mitigated_damage = 0;
    let hp_before = defender.hit_points;
    if hit && defended_by.is_none() {
        let roll = rolls.next("attack.damage_roll")?;
        damage_roll = Some(roll);
        raw_damage = weapon_damage(attacker, roll) + attacker.stats.damroll + profile.damage_bonus;
        mitigated_damage = mitigate_damage(raw_damage, defender.armor.armor_type, rules);
        defender.hit_points = defender.hit_points.saturating_sub(mitigated_damage);
    }
    Ok(CombatAttackReadback {
        attack_index: index,
        attack_slot: spec.slot,
        action_kind: spec.action.label().to_string(),
        granted_by_skill: spec.granted_by_skill,
        attack_roll,
        hit_total,
        hit_target: rules.hit_target,
        hit,
        defense_attempts,
        defended_by,
        damage_roll,
        raw_damage,
        mitigated_damage,
        defender_hp_before: hp_before,
        defender_hp_after: defender.hit_points,
    })
}

fn command_for_round(round_index: u16, commands: &[CombatActionCommand]) -> CombatActionKind {
    commands
        .iter()
        .find(|command| command.round_index == round_index)
        .map(|command| command.action)
        .unwrap_or(CombatActionKind::AutoAttack)
}

fn multi_attack_check(
    round_index: u16,
    slot: &MultiAttackSlot,
    attacker: &CombatantState,
    rolls: &mut RollCursor<'_>,
) -> Result<CombatMultiAttackReadback, EngineError> {
    let skill = skill_rating(attacker, &slot.skill_id);
    let roll = rolls.next(format!("multi_attack.slot{}", slot.attack_slot))?;
    Ok(CombatMultiAttackReadback {
        round_index,
        attack_slot: slot.attack_slot,
        skill_id: slot.skill_id.clone(),
        skill_rating: skill,
        roll,
        granted: skill >= slot.minimum_rating && roll <= skill,
    })
}

fn defense_chance(
    defense: CombatDefenseKind,
    attacker: &CombatantState,
    defender: &CombatantState,
    matchup: WeaponMatchup,
    rules: &CombatRules,
) -> u16 {
    let skill_id = format!("defense.{}", defense.label());
    let base = i32::from(skill_rating(defender, &skill_id));
    let chance = match defense {
        CombatDefenseKind::Parry => {
            base + matchup.parry_modifier + parry_specialization_bonus(defender, rules)
        }
        CombatDefenseKind::ShieldBlock if !defender.shield_equipped => 0,
        CombatDefenseKind::Distance if attacker.weapon.weapon_type != CombatWeaponType::Polearm => {
            base
        }
        _ => base,
    };
    chance.clamp(0, i32::from(rules.max_defense_chance)) as u16
}

fn weapon_damage(attacker: &CombatantState, roll: u16) -> i32 {
    let span = attacker.weapon.damage_max - attacker.weapon.damage_min + 1;
    attacker.weapon.damage_min + ((i32::from(roll) - 1) % span)
}

fn mitigate_damage(raw_damage: i32, armor: CombatArmorType, rules: &CombatRules) -> i32 {
    let reduction = rules
        .armor_mitigations
        .iter()
        .find(|row| row.armor_type == armor)
        .map(|row| row.damage_reduction_percent)
        .unwrap_or(0);
    let mitigated = raw_damage * i32::from(100 - reduction) / 100;
    mitigated.max(rules.min_damage)
}

fn weapon_specialization_bonus(attacker: &CombatantState, rules: &CombatRules) -> i32 {
    let skill_id = attacker.weapon.weapon_type.skill_id();
    if skill_rating(attacker, &skill_id) >= rules.weapon_specialization_min {
        rules.weapon_specialization_hit_bonus
    } else {
        0
    }
}

fn parry_specialization_bonus(defender: &CombatantState, rules: &CombatRules) -> i32 {
    let skill_id = defender.weapon.weapon_type.skill_id();
    if skill_rating(defender, &skill_id) >= rules.weapon_specialization_min {
        rules.parry_specialization_bonus
    } else {
        0
    }
}

fn weapon_matchup(
    rows: &[WeaponMatchup],
    attacker: CombatWeaponType,
    defender: CombatWeaponType,
) -> WeaponMatchup {
    // Sparse-table design (#1464): an unlisted pairing is intentionally neutral,
    // not a swallowed error. `validate_weapon_matchups` accepts a non-exhaustive
    // table on purpose; the explicit `neutral` constructor documents the default.
    rows.iter()
        .find(|row| row.attacker == attacker && row.defender == defender)
        .cloned()
        .unwrap_or_else(|| WeaponMatchup::neutral(attacker, defender))
}

struct RollCursor<'a> {
    rolls: &'a [u16],
    position: usize,
}

impl<'a> RollCursor<'a> {
    const fn new(rolls: &'a [u16]) -> Self {
        Self { rolls, position: 0 }
    }

    fn next(&mut self, field: impl std::fmt::Display) -> Result<u16, EngineError> {
        let roll = self.rolls.get(self.position).copied().ok_or_else(|| {
            EngineError::RpgCombatInvalidConfig {
                field: "roll_stream".to_string(),
                reason: format!("insufficient deterministic rolls for {field}"),
            }
        })?;
        self.position += 1;
        if !(1..=100).contains(&roll) {
            return invalid("roll_stream", "rolls must be inside 1..=100");
        }
        Ok(roll)
    }
}
