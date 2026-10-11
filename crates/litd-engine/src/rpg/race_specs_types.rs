use serde::{Deserialize, Serialize};

use super::attributes_types::AttributeScores;

pub const RPG_RACE_SPEC_SCHEMA: &str = "litd.rpg.race_spec.v1";
pub const ASHEN_HUMAN_RACE_ID: &str = "race.ashen_human";
pub const ELF_RACE_ID: &str = "race.elf";
// #1001-#1018 race ids (Carrion Fields factual mechanics; names/lore originalized per #984).
pub const HALF_ELF_RACE_ID: &str = "race.half_elf";
pub const ASHKIN_MIXED_RACE_ID: &str = "race.ashkin_mixed";
pub const ASHKIN_DEEP_RACE_ID: &str = "race.ashkin_deep";
pub const WOOD_ELF_RACE_ID: &str = "race.wood_elf";
pub const DWARF_RACE_ID: &str = "race.dwarf";
pub const ONYX_KIN_RACE_ID: &str = "race.onyx_kin";
pub const GNOME_RACE_ID: &str = "race.gnome";
pub const DEEP_KIN_RACE_ID: &str = "race.deep_kin";
pub const ORC_RACE_ID: &str = "race.orc";
pub const GOBLIN_RACE_ID: &str = "race.goblin";
pub const MINOTAUR_RACE_ID: &str = "race.minotaur";
pub const STORM_GIANT_RACE_ID: &str = "race.storm_giant";
pub const FIRE_GIANT_RACE_ID: &str = "race.fire_giant";
pub const FROST_GIANT_RACE_ID: &str = "race.frost_giant";
pub const CLOUD_GIANT_RACE_ID: &str = "race.cloud_giant";
pub const SYLPHKIN_RACE_ID: &str = "race.sylphkin";
pub const FELISAR_RACE_ID: &str = "race.felisar";
pub const SAUR_RACE_ID: &str = "race.saur";
// #1019 Centaur (Carrion Fields factual mechanics; names/lore originalized per #984).
pub const CENTAUR_RACE_ID: &str = "race.centaur";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceSpec {
    pub schema: String,
    pub race_id: String,
    pub display_name: String,
    pub player_facing_canon_name: String,
    pub source_issue: u32,
    pub canon_issues: Vec<u32>,
    pub mechanics_source_url: String,
    pub source_history_label: String,
    pub culture_summary: String,
    pub stat_caps: AttributeScores,
    pub alignment: RaceAccessRule,
    pub ethos: RaceAccessRule,
    pub size: RaceSize,
    pub resistances: Vec<RaceResistanceProfile>,
    pub vulnerabilities: Vec<RaceResistanceProfile>,
    pub innate_abilities: Vec<RaceInnateAbility>,
    pub vision: RaceVision,
    pub class_access: RaceClassAccess,
    pub experience_penalty_percent: u16,
    pub hometowns: Vec<RaceHometown>,
    pub visual: RaceVisualContract,
    pub implementation_notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaceAccessRule {
    Any,
    Restricted { values: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaceSize {
    Small,
    Medium,
    Large,
    Giant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceResistanceProfile {
    pub resistance_id: String,
    pub amount: i32,
}

/// The controlled vocabulary of resistance / vulnerability channels a race may
/// carry in [`RaceResistanceProfile::resistance_id`].
///
/// This is decoupled from [`super::attributes_types::DEFAULT_RESISTANCE_IDS`]
/// -- the nine combat damage channels wired length-invariantly into the
/// derived-stat formula table (see `attributes_validate.rs`, which asserts the
/// table has exactly `DEFAULT_RESISTANCE_IDS.len()` rows). Growing that shared
/// array to hold non-combat affinities would ripple into every derived-stat
/// table, so #1631 instead makes a race's affinity vocabulary its own
/// independently-maintained enum. The nine combat channels are re-listed here
/// on purpose: this enum -- not the derived-stat array -- is the single source
/// of truth for what a race resistance/vulnerability id may be.
///
/// Facts are transcribed from Carrion Fields, never invented (#984/#959): the
/// damage-type taxonomy in helpfile 2304
/// (`carrionfields.net/helpsearch.php?id=2304`: PHYSICAL bash/pierce/slash;
/// MAGIC energy/light/holy/negative/poison/mental; ELEMENTAL
/// acid/lightning/fire/cold/water) plus the per-race "Attributes"
/// (Resistant/Vulnerable) sections on `wiki.carrionfields.net/race/<slug>`,
/// which add the material and meta affinities (iron, silver, mithril,
/// whitesteel, wood, curse, charm, magic, holy-water) that no damage channel
/// can express -- e.g. the High Elf's `Resistant: Curse` / `Vulnerable: Iron`
/// (`wiki.carrionfields.net/race/elf`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaceAffinity {
    // Combat damage channels (overlap DEFAULT_RESISTANCE_IDS by design).
    Physical,
    Fire,
    Cold,
    Lightning,
    Poison,
    Disease,
    Mental,
    Holy,
    Shadow,
    // Extended CF damage/attack types (helpfile 2304) with no combat-channel slot.
    Bash,
    Pierce,
    Slash,
    Energy,
    Light,
    Negative,
    Acid,
    Water,
    Drowning,
    // Material and meta affinities from the per-race "Attributes" sections.
    Iron,
    Silver,
    Mithril,
    Whitesteel,
    Wood,
    Curse,
    Charm,
    Magic,
    HolyWater,
}

impl RaceAffinity {
    /// Every affinity in the vocabulary, in declaration order.
    pub const ALL: [RaceAffinity; 27] = [
        RaceAffinity::Physical,
        RaceAffinity::Fire,
        RaceAffinity::Cold,
        RaceAffinity::Lightning,
        RaceAffinity::Poison,
        RaceAffinity::Disease,
        RaceAffinity::Mental,
        RaceAffinity::Holy,
        RaceAffinity::Shadow,
        RaceAffinity::Bash,
        RaceAffinity::Pierce,
        RaceAffinity::Slash,
        RaceAffinity::Energy,
        RaceAffinity::Light,
        RaceAffinity::Negative,
        RaceAffinity::Acid,
        RaceAffinity::Water,
        RaceAffinity::Drowning,
        RaceAffinity::Iron,
        RaceAffinity::Silver,
        RaceAffinity::Mithril,
        RaceAffinity::Whitesteel,
        RaceAffinity::Wood,
        RaceAffinity::Curse,
        RaceAffinity::Charm,
        RaceAffinity::Magic,
        RaceAffinity::HolyWater,
    ];

    /// The canonical snake_case id stored in `resistance_id`.
    pub const fn as_id(self) -> &'static str {
        match self {
            RaceAffinity::Physical => "physical",
            RaceAffinity::Fire => "fire",
            RaceAffinity::Cold => "cold",
            RaceAffinity::Lightning => "lightning",
            RaceAffinity::Poison => "poison",
            RaceAffinity::Disease => "disease",
            RaceAffinity::Mental => "mental",
            RaceAffinity::Holy => "holy",
            RaceAffinity::Shadow => "shadow",
            RaceAffinity::Bash => "bash",
            RaceAffinity::Pierce => "pierce",
            RaceAffinity::Slash => "slash",
            RaceAffinity::Energy => "energy",
            RaceAffinity::Light => "light",
            RaceAffinity::Negative => "negative",
            RaceAffinity::Acid => "acid",
            RaceAffinity::Water => "water",
            RaceAffinity::Drowning => "drowning",
            RaceAffinity::Iron => "iron",
            RaceAffinity::Silver => "silver",
            RaceAffinity::Mithril => "mithril",
            RaceAffinity::Whitesteel => "whitesteel",
            RaceAffinity::Wood => "wood",
            RaceAffinity::Curse => "curse",
            RaceAffinity::Charm => "charm",
            RaceAffinity::Magic => "magic",
            RaceAffinity::HolyWater => "holy_water",
        }
    }

    /// Resolve a stored `resistance_id` back to its affinity, or `None` if the
    /// id is not in the vocabulary (the loud-fail path for `validate_race_spec`).
    pub fn from_id(id: &str) -> Option<Self> {
        RaceAffinity::ALL.into_iter().find(|a| a.as_id() == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceInnateAbility {
    pub ability_id: String,
    pub display_name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceVision {
    pub infravision: bool,
    pub infravision_tiles: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaceClassAccess {
    All,
    Restricted { class_ids: Vec<String> },
    // #1000: CF races are frequently authored as "any profession EXCEPT <list>"
    // (e.g. the Elf is barred from Druid/Anti-Paladin/Necromancer/Assassin/Shaman).
    // `Restricted` is an allowlist and cannot express that without enumerating an
    // as-yet-unbuilt class roster, so a denylist variant records the factual mechanic
    // exactly as the source states it.
    AllExcept { class_ids: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceHometown {
    pub origin_id: String,
    pub display_name: String,
    pub canon_place: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceVisualContract {
    pub renderer: String,
    pub model_id: String,
    pub skeleton_id: String,
    pub required_animations: Vec<String>,
    pub silhouette: String,
    pub material_notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceSpecReadback {
    pub schema: String,
    pub race_id: String,
    pub display_name: String,
    pub player_facing_canon_name: String,
    pub source_issue: u32,
    pub stat_caps: AttributeScores,
    pub alignment: RaceAccessRule,
    pub ethos: RaceAccessRule,
    pub size: RaceSize,
    pub resistance_count: usize,
    pub vulnerability_count: usize,
    pub innate_ability_count: usize,
    pub infravision: bool,
    pub class_access: RaceClassAccess,
    pub experience_penalty_percent: u16,
    pub hometown_count: usize,
    pub visual_renderer: String,
    pub animation_count: usize,
    pub race_spec_sha256: String,
}
