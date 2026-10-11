use serde::{Deserialize, Serialize};

pub const RPG_STATS_SCHEMA: &str = "litd.rpg.attributes.v1";
pub const DEFAULT_RESISTANCE_IDS: [&str; 9] = [
    "physical",
    "fire",
    "cold",
    "lightning",
    "poison",
    "disease",
    "mental",
    "holy",
    "shadow",
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeScores {
    pub strength: i16,
    pub intelligence: i16,
    pub wisdom: i16,
    pub dexterity: i16,
    pub constitution: i16,
    pub charisma: i16,
}

impl AttributeScores {
    pub const ZERO: Self = Self {
        strength: 0,
        intelligence: 0,
        wisdom: 0,
        dexterity: 0,
        constitution: 0,
        charisma: 0,
    };

    pub const fn new(
        strength: i16,
        intelligence: i16,
        wisdom: i16,
        dexterity: i16,
        constitution: i16,
        charisma: i16,
    ) -> Self {
        Self {
            strength,
            intelligence,
            wisdom,
            dexterity,
            constitution,
            charisma,
        }
    }

    pub(super) fn dot(self, coefficients: Self) -> i32 {
        i32::from(self.strength) * i32::from(coefficients.strength)
            + i32::from(self.intelligence) * i32::from(coefficients.intelligence)
            + i32::from(self.wisdom) * i32::from(coefficients.wisdom)
            + i32::from(self.dexterity) * i32::from(coefficients.dexterity)
            + i32::from(self.constitution) * i32::from(coefficients.constitution)
            + i32::from(self.charisma) * i32::from(coefficients.charisma)
    }

    pub(super) fn pairs(self) -> [(&'static str, i16); 6] {
        [
            ("strength", self.strength),
            ("intelligence", self.intelligence),
            ("wisdom", self.wisdom),
            ("dexterity", self.dexterity),
            ("constitution", self.constitution),
            ("charisma", self.charisma),
        ]
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedStats {
    pub hit_points: i32,
    pub mana: i32,
    pub movement: i32,
    pub hitroll: i32,
    pub damroll: i32,
    pub armor_class: i32,
    pub carry_weight: i32,
    pub wield_weight: i32,
}

impl DerivedStats {
    pub const ZERO: Self = Self {
        hit_points: 0,
        mana: 0,
        movement: 0,
        hitroll: 0,
        damroll: 0,
        armor_class: 0,
        carry_weight: 0,
        wield_weight: 0,
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceAttributeCaps {
    pub race_id: String,
    pub caps: AttributeScores,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatModifierSource {
    Gear,
    Affect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResistanceModifier {
    pub resistance_id: String,
    pub amount: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatModifier {
    pub modifier_id: String,
    pub source: StatModifierSource,
    pub attributes: AttributeScores,
    pub derived: DerivedStats,
    pub resistances: Vec<ResistanceModifier>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatFormula {
    pub base: i32,
    pub per_level: i32,
    pub coefficients: AttributeScores,
    pub min: i32,
    pub max: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResistanceFormula {
    pub resistance_id: String,
    pub base: i32,
    pub coefficients: AttributeScores,
    pub min: i32,
    pub max: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedStatFormulaTable {
    pub schema: String,
    pub hit_points: StatFormula,
    pub mana: StatFormula,
    pub movement: StatFormula,
    pub hitroll: StatFormula,
    pub damroll: StatFormula,
    pub armor_class: StatFormula,
    pub carry_weight: StatFormula,
    pub wield_weight: StatFormula,
    pub resistances: Vec<ResistanceFormula>,
}

impl DerivedStatFormulaTable {
    pub fn mud_defaults() -> Self {
        Self {
            schema: RPG_STATS_SCHEMA.to_string(),
            hit_points: formula(20, 8, AttributeScores::new(2, 0, 0, 0, 9, 0), 1, 5000),
            mana: formula(10, 5, AttributeScores::new(0, 6, 6, 0, 0, 1), 0, 5000),
            movement: formula(30, 2, AttributeScores::new(1, 0, 0, 5, 3, 0), 0, 5000),
            hitroll: formula(-5, 1, AttributeScores::new(1, 0, 0, 1, 0, 0), -100, 300),
            damroll: formula(-4, 1, AttributeScores::new(2, 0, 0, 0, 0, 0), -100, 300),
            armor_class: formula(100, -1, AttributeScores::new(0, 0, 0, -3, -1, 0), -300, 300),
            carry_weight: formula(25, 0, AttributeScores::new(12, 0, 0, 0, 3, 0), 0, 2000),
            wield_weight: formula(0, 0, AttributeScores::new(3, 0, 0, 1, 0, 0), 1, 500),
            resistances: vec![
                resist("physical", AttributeScores::new(1, 0, 0, 0, 1, 0)),
                resist("fire", AttributeScores::new(0, 0, 1, 0, 1, 0)),
                resist("cold", AttributeScores::new(0, 1, 0, 0, 1, 0)),
                resist("lightning", AttributeScores::new(0, 0, 1, 1, 0, 0)),
                resist("poison", AttributeScores::new(0, 0, 0, 0, 2, 0)),
                resist("disease", AttributeScores::new(0, 0, 1, 0, 1, 0)),
                resist("mental", AttributeScores::new(0, 1, 1, 0, 0, 1)),
                resist("holy", AttributeScores::new(0, 0, 1, 0, 0, 1)),
                resist("shadow", AttributeScores::new(0, 0, 1, 0, 0, 1)),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeBuildInput {
    pub character_id: String,
    pub race_id: String,
    pub level: u16,
    pub base: AttributeScores,
    pub race_caps: Vec<RaceAttributeCaps>,
    pub modifiers: Vec<StatModifier>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeCapClamp {
    pub attribute: String,
    pub uncapped: i16,
    pub capped: i16,
    pub cap: i16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResistanceReadback {
    pub resistance_id: String,
    pub value: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeDerivedReadback {
    pub schema: String,
    pub character_id: String,
    pub race_id: String,
    pub level: u16,
    pub base_attributes: AttributeScores,
    pub race_caps: AttributeScores,
    pub uncapped_attributes: AttributeScores,
    pub effective_attributes: AttributeScores,
    pub cap_clamps: Vec<AttributeCapClamp>,
    pub derived: DerivedStats,
    pub resistances: Vec<ResistanceReadback>,
    pub modifier_count: usize,
    pub gear_modifier_count: usize,
    pub affect_modifier_count: usize,
    pub input_sha256: String,
    pub table_sha256: String,
    pub result_sha256: String,
}

fn formula(
    base: i32,
    per_level: i32,
    coefficients: AttributeScores,
    min: i32,
    max: i32,
) -> StatFormula {
    StatFormula {
        base,
        per_level,
        coefficients,
        min,
        max,
    }
}

fn resist(resistance_id: &str, coefficients: AttributeScores) -> ResistanceFormula {
    ResistanceFormula {
        resistance_id: resistance_id.to_string(),
        base: 0,
        coefficients,
        min: -100,
        max: 100,
    }
}
