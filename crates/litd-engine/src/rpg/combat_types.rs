use serde::{Deserialize, Serialize};

use super::DerivedStats;

pub const RPG_COMBAT_SCHEMA: &str = "litd.rpg.combat.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatActionKind {
    AutoAttack,
    Bash,
    Trip,
    Disarm,
    Charge,
}

impl CombatActionKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::AutoAttack => "auto_attack",
            Self::Bash => "bash",
            Self::Trip => "trip",
            Self::Disarm => "disarm",
            Self::Charge => "charge",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatDefenseKind {
    Distance,
    Spin,
    Evade,
    Parry,
    ShieldBlock,
    Dodge,
}

impl CombatDefenseKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Distance => "distance",
            Self::Spin => "spin",
            Self::Evade => "evade",
            Self::Parry => "parry",
            Self::ShieldBlock => "shield_block",
            Self::Dodge => "dodge",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatWeaponType {
    HandToHand,
    Axe,
    Dagger,
    Flail,
    Mace,
    Polearm,
    Spear,
    Sword,
    Staff,
    Whip,
    Bow,
}

impl CombatWeaponType {
    pub const fn label(self) -> &'static str {
        match self {
            Self::HandToHand => "hand_to_hand",
            Self::Axe => "axe",
            Self::Dagger => "dagger",
            Self::Flail => "flail",
            Self::Mace => "mace",
            Self::Polearm => "polearm",
            Self::Spear => "spear",
            Self::Sword => "sword",
            Self::Staff => "staff",
            Self::Whip => "whip",
            Self::Bow => "bow",
        }
    }

    pub fn skill_id(self) -> String {
        format!("weapon.{}", self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatArmorType {
    Unarmored,
    Cloth,
    Leather,
    LightArmor,
    MetalArmor,
    HeavyMail,
    Plate,
    Bone,
}

impl CombatArmorType {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unarmored => "unarmored",
            Self::Cloth => "cloth",
            Self::Leather => "leather",
            Self::LightArmor => "light_armor",
            Self::MetalArmor => "metal_armor",
            Self::HeavyMail => "heavy_mail",
            Self::Plate => "plate",
            Self::Bone => "bone",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatRules {
    pub schema: String,
    pub tick_millis: u32,
    pub round_ticks: u32,
    pub max_rounds: u16,
    pub hit_base: i32,
    pub hit_target: i32,
    pub armor_class_divisor: i32,
    pub min_damage: i32,
    pub max_defense_chance: u16,
    pub weapon_specialization_min: u16,
    pub weapon_specialization_hit_bonus: i32,
    pub parry_specialization_bonus: i32,
    pub defense_order: Vec<CombatDefenseKind>,
    pub multi_attack_slots: Vec<MultiAttackSlot>,
    pub weapon_matchups: Vec<WeaponMatchup>,
    pub armor_mitigations: Vec<ArmorMitigation>,
    pub action_profiles: Vec<ActionProfile>,
}

impl CombatRules {
    pub fn mud_defaults() -> Self {
        Self {
            schema: RPG_COMBAT_SCHEMA.to_string(),
            tick_millis: 500,
            round_ticks: 3,
            max_rounds: 30,
            hit_base: 50,
            hit_target: 100,
            armor_class_divisor: 5,
            min_damage: 1,
            max_defense_chance: 95,
            weapon_specialization_min: 75,
            weapon_specialization_hit_bonus: 8,
            parry_specialization_bonus: 5,
            defense_order: vec![
                CombatDefenseKind::Distance,
                CombatDefenseKind::Spin,
                CombatDefenseKind::Evade,
                CombatDefenseKind::Parry,
                CombatDefenseKind::ShieldBlock,
                CombatDefenseKind::Dodge,
            ],
            multi_attack_slots: vec![
                MultiAttackSlot::new(2, "skill.second_attack", 1),
                MultiAttackSlot::new(3, "skill.third_attack", 1),
                MultiAttackSlot::new(4, "skill.fourth_attack", 1),
            ],
            weapon_matchups: vec![
                WeaponMatchup::new(CombatWeaponType::Sword, CombatWeaponType::Dagger, 5, -10),
                WeaponMatchup::new(CombatWeaponType::Polearm, CombatWeaponType::Sword, 8, 5),
                WeaponMatchup::new(CombatWeaponType::Axe, CombatWeaponType::Dagger, 6, -12),
            ],
            armor_mitigations: vec![
                ArmorMitigation::new(CombatArmorType::Unarmored, 0),
                ArmorMitigation::new(CombatArmorType::Cloth, 0),
                ArmorMitigation::new(CombatArmorType::Leather, 8),
                ArmorMitigation::new(CombatArmorType::LightArmor, 12),
                ArmorMitigation::new(CombatArmorType::MetalArmor, 16),
                ArmorMitigation::new(CombatArmorType::HeavyMail, 20),
                ArmorMitigation::new(CombatArmorType::Plate, 30),
                ArmorMitigation::new(CombatArmorType::Bone, 14),
            ],
            action_profiles: vec![
                ActionProfile::new(CombatActionKind::AutoAttack, None, 0, 0, 0),
                ActionProfile::new(CombatActionKind::Bash, Some("skill.bash"), -5, 5, 3),
                ActionProfile::new(CombatActionKind::Trip, Some("skill.trip"), -3, 2, 2),
                ActionProfile::new(CombatActionKind::Disarm, Some("skill.disarm"), -4, 0, 2),
                ActionProfile::new(CombatActionKind::Charge, Some("skill.charge"), -8, 10, 4),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiAttackSlot {
    pub attack_slot: u8,
    pub skill_id: String,
    pub minimum_rating: u16,
}

impl MultiAttackSlot {
    pub fn new(attack_slot: u8, skill_id: &str, minimum_rating: u16) -> Self {
        Self {
            attack_slot,
            skill_id: skill_id.to_string(),
            minimum_rating,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeaponMatchup {
    pub attacker: CombatWeaponType,
    pub defender: CombatWeaponType,
    pub hit_modifier: i32,
    pub parry_modifier: i32,
}

impl WeaponMatchup {
    pub const fn new(
        attacker: CombatWeaponType,
        defender: CombatWeaponType,
        hit_modifier: i32,
        parry_modifier: i32,
    ) -> Self {
        Self {
            attacker,
            defender,
            hit_modifier,
            parry_modifier,
        }
    }

    /// The neutral (zero-modifier) matchup for a weapon pairing. The weapon
    /// matchup table is **sparse by design** (#1464): authors list only the
    /// pairings that grant an advantage or penalty, and every unlisted pairing
    /// is intentionally neutral. `validate_weapon_matchups` therefore checks the
    /// listed rows for duplicates/ranges but does not require the table to be
    /// exhaustive. This named constructor makes that default explicit rather
    /// than an anonymous fallback.
    pub const fn neutral(attacker: CombatWeaponType, defender: CombatWeaponType) -> Self {
        Self::new(attacker, defender, 0, 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmorMitigation {
    pub armor_type: CombatArmorType,
    pub damage_reduction_percent: u16,
}

impl ArmorMitigation {
    pub const fn new(armor_type: CombatArmorType, damage_reduction_percent: u16) -> Self {
        Self {
            armor_type,
            damage_reduction_percent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionProfile {
    pub action: CombatActionKind,
    pub skill_id: Option<String>,
    pub hit_modifier: i32,
    pub damage_bonus: i32,
    pub lag_ticks: u32,
}

impl ActionProfile {
    pub fn new(
        action: CombatActionKind,
        skill_id: Option<&str>,
        hit_modifier: i32,
        damage_bonus: i32,
        lag_ticks: u32,
    ) -> Self {
        Self {
            action,
            skill_id: skill_id.map(str::to_string),
            hit_modifier,
            damage_bonus,
            lag_ticks,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatInput {
    pub encounter_id: String,
    pub rounds: u16,
    pub attacker: CombatantState,
    pub defender: CombatantState,
    pub commands: Vec<CombatActionCommand>,
    pub roll_stream: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatActionCommand {
    pub round_index: u16,
    pub action: CombatActionKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatantState {
    pub combatant_id: String,
    pub level: u16,
    pub stats: DerivedStats,
    pub hit_points: i32,
    pub wait_state_ticks: u32,
    pub weapon: CombatWeapon,
    pub armor: CombatArmorSet,
    pub shield_equipped: bool,
    pub skills: Vec<CombatSkillRating>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatWeapon {
    pub weapon_id: String,
    pub weapon_type: CombatWeaponType,
    pub damage_min: i32,
    pub damage_max: i32,
    pub weight: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatArmorSet {
    pub armor_id: String,
    pub armor_type: CombatArmorType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatSkillRating {
    pub skill_id: String,
    pub rating: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatReadback {
    pub schema: String,
    pub encounter_id: String,
    pub tick_millis: u32,
    pub round_ticks: u32,
    pub round_millis: u32,
    pub requested_rounds: u16,
    pub resolved_rounds: u16,
    pub total_damage: i32,
    pub defense_order: Vec<String>,
    pub multi_attack_checks: Vec<CombatMultiAttackReadback>,
    pub rounds: Vec<CombatRoundReadback>,
    pub final_attacker: CombatantState,
    pub final_defender: CombatantState,
    pub rolls_consumed: usize,
    pub input_sha256: String,
    pub rules_sha256: String,
    pub result_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatRoundReadback {
    pub round_index: u16,
    pub tick_start: u32,
    pub action_kind: String,
    pub skipped_by_wait_state: bool,
    pub attacker_wait_before: u32,
    pub attacker_wait_after: u32,
    pub attacks: Vec<CombatAttackReadback>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatMultiAttackReadback {
    pub round_index: u16,
    pub attack_slot: u8,
    pub skill_id: String,
    pub skill_rating: u16,
    pub roll: u16,
    pub granted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatAttackReadback {
    pub attack_index: usize,
    pub attack_slot: u8,
    pub action_kind: String,
    pub granted_by_skill: Option<String>,
    pub attack_roll: u16,
    pub hit_total: i32,
    pub hit_target: i32,
    pub hit: bool,
    pub defense_attempts: Vec<CombatDefenseAttemptReadback>,
    pub defended_by: Option<String>,
    pub damage_roll: Option<u16>,
    pub raw_damage: i32,
    pub mitigated_damage: i32,
    pub defender_hp_before: i32,
    pub defender_hp_after: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatDefenseAttemptReadback {
    pub defense: String,
    pub chance: u16,
    pub roll: u16,
    pub success: bool,
}
