use crate::{
    EngineError,
    rpg::{
        CombatActionCommand, CombatActionKind, CombatArmorSet, CombatArmorType, CombatInput,
        CombatRules, CombatSkillRating, CombatWeapon, CombatWeaponType, CombatantState,
        DerivedStats, resolve_combat_plan,
    },
};

pub(crate) fn happy_rules() -> CombatRules {
    CombatRules::mud_defaults()
}

pub(crate) fn happy_input() -> CombatInput {
    CombatInput {
        encounter_id: "encounter.issue963.combat".to_string(),
        rounds: 3,
        attacker: CombatantState {
            combatant_id: "char.ashen.vanguard".to_string(),
            level: 30,
            stats: DerivedStats {
                hit_points: 220,
                mana: 40,
                movement: 180,
                hitroll: 18,
                damroll: 12,
                armor_class: 10,
                carry_weight: 120,
                wield_weight: 38,
            },
            hit_points: 220,
            wait_state_ticks: 0,
            weapon: CombatWeapon {
                weapon_id: "weapon.ember_sabre".to_string(),
                weapon_type: CombatWeaponType::Sword,
                damage_min: 8,
                damage_max: 14,
                weight: 6,
            },
            armor: CombatArmorSet {
                armor_id: "armor.scaled_mail".to_string(),
                armor_type: CombatArmorType::MetalArmor,
            },
            shield_equipped: false,
            skills: skills(&[
                ("weapon.sword", 88),
                ("skill.second_attack", 92),
                ("skill.third_attack", 81),
                ("skill.fourth_attack", 60),
                ("skill.bash", 70),
            ]),
        },
        defender: CombatantState {
            combatant_id: "mob.iron_sentinel".to_string(),
            level: 28,
            stats: DerivedStats {
                hit_points: 180,
                mana: 0,
                movement: 120,
                hitroll: 12,
                damroll: 9,
                armor_class: 20,
                carry_weight: 160,
                wield_weight: 30,
            },
            hit_points: 180,
            wait_state_ticks: 0,
            weapon: CombatWeapon {
                weapon_id: "weapon.hook_dagger".to_string(),
                weapon_type: CombatWeaponType::Dagger,
                damage_min: 5,
                damage_max: 9,
                weight: 3,
            },
            armor: CombatArmorSet {
                armor_id: "armor.iron_mail".to_string(),
                armor_type: CombatArmorType::HeavyMail,
            },
            shield_equipped: true,
            skills: skills(&[
                ("weapon.dagger", 85),
                ("defense.distance", 15),
                ("defense.spin", 45),
                ("defense.evade", 50),
                ("defense.parry", 80),
                ("defense.shield_block", 65),
                ("defense.dodge", 40),
            ]),
        },
        commands: vec![CombatActionCommand {
            round_index: 1,
            action: CombatActionKind::Bash,
        }],
        roll_stream: vec![
            75, 20, 40, 30, 80, 90, 90, 84, 78, 20, 25, 85, 90, 90, 70, 78, 80, 50, 6, 65, 70, 90,
            90, 90, 90, 90, 90, 5,
        ],
    }
}

pub(crate) fn duplicate_defender_skill() -> Result<(), EngineError> {
    let mut input = happy_input();
    input.defender.skills.push(CombatSkillRating {
        skill_id: "defense.dodge".to_string(),
        rating: 10,
    });
    resolve_combat_plan(&input, &happy_rules()).map(|_| ())
}

pub(crate) fn exhausted_roll_stream() -> Result<(), EngineError> {
    let mut input = happy_input();
    input.roll_stream = vec![75];
    resolve_combat_plan(&input, &happy_rules()).map(|_| ())
}

pub(crate) fn duplicate_defense_order() -> Result<(), EngineError> {
    let input = happy_input();
    let mut rules = happy_rules();
    rules.defense_order.push(rules.defense_order[0]);
    resolve_combat_plan(&input, &rules).map(|_| ())
}

pub(crate) fn invalid_weapon_damage() -> Result<(), EngineError> {
    let mut input = happy_input();
    input.attacker.weapon.damage_min = 20;
    input.attacker.weapon.damage_max = 5;
    resolve_combat_plan(&input, &happy_rules()).map(|_| ())
}

pub(crate) fn missing_weapon_matchup() -> Result<(), EngineError> {
    let input = happy_input();
    let mut rules = happy_rules();
    rules.weapon_matchups.clear();
    resolve_combat_plan(&input, &rules).map(|_| ())
}

pub(crate) fn missing_armor_mitigation() -> Result<(), EngineError> {
    let input = happy_input();
    let mut rules = happy_rules();
    rules
        .armor_mitigations
        .retain(|row| row.armor_type != input.defender.armor.armor_type);
    resolve_combat_plan(&input, &rules).map(|_| ())
}

pub(crate) fn command_out_of_range() -> Result<(), EngineError> {
    let mut input = happy_input();
    input.commands[0].round_index = input.rounds;
    resolve_combat_plan(&input, &happy_rules()).map(|_| ())
}

fn skills(rows: &[(&str, u16)]) -> Vec<CombatSkillRating> {
    rows.iter()
        .map(|(skill_id, rating)| CombatSkillRating {
            skill_id: (*skill_id).to_string(),
            rating: *rating,
        })
        .collect()
}
