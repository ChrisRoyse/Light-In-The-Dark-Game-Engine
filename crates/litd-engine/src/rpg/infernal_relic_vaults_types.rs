use serde::{Deserialize, Serialize};

use super::InfernalCircleId;

pub const RPG_INFERNAL_RELIC_VAULTS_SCHEMA: &str = "litd.rpg.infernal_relic_vaults.v1";
pub const ASHEN_REACH_INFERNAL_RELIC_VAULTS_POLICY_ID: &str =
    "server.ashen_reach.infernal_relic_vaults.default";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfernalRelicVaultKind {
    MoltenThroneRoad,
    ObsidianThrone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfernalRelicSlot {
    Weapon,
    Crown,
    Armor,
    Amulet,
    Ring,
    Lantern,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfernalRelicRarity {
    HellforgedLegendary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfernalRelicPerkKind {
    TyrantBane,
    AttritionResistance,
    SoulWard,
    LegionCommand,
    VaultSeal,
    NameDefense,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalRelicVaultPolicy {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub built_in_map_required: bool,
    pub world_finite_enforced: bool,
    pub vaults: Vec<InfernalRelicVault>,
    pub item_set: Vec<InfernalRelicItem>,
    pub set_bonuses: Vec<InfernalRelicSetBonus>,
}

impl InfernalRelicVaultPolicy {
    pub fn ashen_reach_defaults() -> Self {
        Self {
            schema: RPG_INFERNAL_RELIC_VAULTS_SCHEMA.to_string(),
            server_policy_id: ASHEN_REACH_INFERNAL_RELIC_VAULTS_POLICY_ID.to_string(),
            source_issue: 1069,
            built_in_map_required: false,
            world_finite_enforced: true,
            vaults: vec![
                vault(VaultArgs {
                    kind: InfernalRelicVaultKind::MoltenThroneRoad,
                    vault_id: "vault.infernal.molten_throne_road.crown_lock",
                    display_name: "Crown-lock Relic Vault",
                    circle_id: InfernalCircleId::MoltenThroneRoad,
                    zone_id: "zone.infernal.molten_throne_road.relic_vault",
                    guarded_by_ruler_ref: "lord.infernal.molten_throne_road.throne_warden",
                    guarded_by_agent_id: "agent.infernal.molten_throne_road.throne_warden",
                    access_lock_ref: "gate.infernal.obsidian_crown",
                    world_finite_capacity: 4,
                }),
                vault(VaultArgs {
                    kind: InfernalRelicVaultKind::ObsidianThrone,
                    vault_id: "vault.infernal.obsidian_throne.tyrant_cache",
                    display_name: "Tyrant's Obsidian Cache",
                    circle_id: InfernalCircleId::ObsidianThrone,
                    zone_id: "zone.infernal.obsidian_throne.crown_chamber",
                    guarded_by_ruler_ref: "lord.infernal.obsidian_throne.maldrithax",
                    guarded_by_agent_id: "agent.infernal.obsidian_throne.maldrithax",
                    access_lock_ref: "seal.infernal.maldrithax_namebrand",
                    world_finite_capacity: 2,
                }),
            ],
            item_set: vec![
                item(
                    "item.infernal.hellforged.brandblade",
                    "Brandblade of the Deeps",
                    InfernalRelicSlot::Weapon,
                    "vault.infernal.molten_throne_road.crown_lock",
                    "lord.infernal.molten_throne_road.throne_warden",
                    980,
                    &[
                        perk(
                            "perk.infernal.brandblade.tyrant_bane",
                            InfernalRelicPerkKind::TyrantBane,
                            990,
                        ),
                        perk(
                            "perk.infernal.brandblade.crown_cleave",
                            InfernalRelicPerkKind::LegionCommand,
                            970,
                        ),
                    ],
                ),
                item(
                    "item.infernal.hellforged.ashen_crown",
                    "Ashen Tyrant's Crown",
                    InfernalRelicSlot::Crown,
                    "vault.infernal.obsidian_throne.tyrant_cache",
                    "lord.infernal.obsidian_throne.maldrithax",
                    1000,
                    &[
                        perk(
                            "perk.infernal.ashen_crown.sovereign_decree",
                            InfernalRelicPerkKind::LegionCommand,
                            1000,
                        ),
                        perk(
                            "perk.infernal.ashen_crown.name_defense",
                            InfernalRelicPerkKind::NameDefense,
                            995,
                        ),
                    ],
                ),
                item(
                    "item.infernal.hellforged.vault_lantern",
                    "Lantern of the Locked Flame",
                    InfernalRelicSlot::Lantern,
                    "vault.infernal.molten_throne_road.crown_lock",
                    "lord.infernal.molten_throne_road.throne_warden",
                    960,
                    &[
                        perk(
                            "perk.infernal.vault_lantern.soul_ward",
                            InfernalRelicPerkKind::SoulWard,
                            970,
                        ),
                        perk(
                            "perk.infernal.vault_lantern.dark_pressure_guard",
                            InfernalRelicPerkKind::AttritionResistance,
                            965,
                        ),
                    ],
                ),
                item(
                    "item.infernal.hellforged.obsidian_heart",
                    "Obsidian Heart Reliquary",
                    InfernalRelicSlot::Amulet,
                    "vault.infernal.obsidian_throne.tyrant_cache",
                    "lord.infernal.obsidian_throne.maldrithax",
                    990,
                    &[
                        perk(
                            "perk.infernal.obsidian_heart.constitution_anchor",
                            InfernalRelicPerkKind::SoulWard,
                            990,
                        ),
                        perk(
                            "perk.infernal.obsidian_heart.soul_drain_guard",
                            InfernalRelicPerkKind::AttritionResistance,
                            985,
                        ),
                    ],
                ),
                item(
                    "item.infernal.hellforged.chainmantle",
                    "Chainmantle of Furnace Law",
                    InfernalRelicSlot::Armor,
                    "vault.infernal.molten_throne_road.crown_lock",
                    "lord.infernal.molten_throne_road.throne_warden",
                    970,
                    &[
                        perk(
                            "perk.infernal.chainmantle.heat_dampening",
                            InfernalRelicPerkKind::AttritionResistance,
                            975,
                        ),
                        perk(
                            "perk.infernal.chainmantle.vault_seal",
                            InfernalRelicPerkKind::VaultSeal,
                            970,
                        ),
                    ],
                ),
                item(
                    "item.infernal.hellforged.ember_signet",
                    "Ember Signet of the Crown-road",
                    InfernalRelicSlot::Ring,
                    "vault.infernal.molten_throne_road.crown_lock",
                    "lord.infernal.molten_throne_road.throne_warden",
                    965,
                    &[
                        perk(
                            "perk.infernal.ember_signet.pact_break",
                            InfernalRelicPerkKind::NameDefense,
                            965,
                        ),
                        perk(
                            "perk.infernal.ember_signet.cinder_resolve",
                            InfernalRelicPerkKind::SoulWard,
                            960,
                        ),
                    ],
                ),
            ],
            set_bonuses: vec![
                set_bonus(
                    "set.infernal.hellforged.2.soulward",
                    2,
                    "Two pieces harden the bearer against soul-drain attrition.",
                    960,
                ),
                set_bonus(
                    "set.infernal.hellforged.4.crownroad",
                    4,
                    "Four pieces turn Infernal ruler pressure into bounded resolve.",
                    980,
                ),
                set_bonus(
                    "set.infernal.hellforged.6.tyrant_oath",
                    6,
                    "Six pieces unlock the full anti-tyrant set oath without bypassing recall locks.",
                    1000,
                ),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalRelicVault {
    pub kind: InfernalRelicVaultKind,
    pub vault_id: String,
    pub display_name: String,
    pub circle_id: InfernalCircleId,
    pub zone_id: String,
    pub guarded_by_ruler_ref: String,
    pub guarded_by_agent_id: String,
    pub access_lock_ref: String,
    pub world_finite_capacity: u16,
    pub respawns_world_finite_items: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalRelicItem {
    pub item_id: String,
    pub display_name: String,
    pub slot: InfernalRelicSlot,
    pub rarity: InfernalRelicRarity,
    pub source_vault_id: String,
    pub guarded_by_ruler_ref: String,
    pub world_finite_count: u16,
    pub power_rating_milli: u16,
    pub grants_map_or_minimap: bool,
    pub bypasses_infernal_no_recall: bool,
    pub perks: Vec<InfernalRelicPerk>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalRelicPerk {
    pub perk_id: String,
    pub kind: InfernalRelicPerkKind,
    pub power_rating_milli: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalRelicSetBonus {
    pub bonus_id: String,
    pub pieces_required: u8,
    pub effect_summary: String,
    pub power_rating_milli: u16,
    pub grants_map_or_minimap: bool,
    pub bypasses_infernal_no_recall: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalRelicVaultReadback {
    pub schema: String,
    pub server_policy_id: String,
    pub source_issue: u32,
    pub built_in_map_required: bool,
    pub world_finite_enforced: bool,
    pub vault_count: usize,
    pub item_count: usize,
    pub set_bonus_count: usize,
    pub world_finite_item_total: u16,
    pub highest_power_rating_milli: u16,
    pub molten_vault_guard: String,
    pub obsidian_vault_guard: String,
    pub item_rows: Vec<InfernalRelicItemReadback>,
    pub policy_sha256: String,
    pub zone_policy_sha256: String,
    pub boss_policy_sha256: String,
    pub result_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfernalRelicItemReadback {
    pub item_id: String,
    pub display_name: String,
    pub slot: InfernalRelicSlot,
    pub source_vault_id: String,
    pub world_finite_count: u16,
    pub power_rating_milli: u16,
    pub perk_count: usize,
    pub guarded_by_ruler_ref: String,
}

struct VaultArgs<'a> {
    kind: InfernalRelicVaultKind,
    vault_id: &'a str,
    display_name: &'a str,
    circle_id: InfernalCircleId,
    zone_id: &'a str,
    guarded_by_ruler_ref: &'a str,
    guarded_by_agent_id: &'a str,
    access_lock_ref: &'a str,
    world_finite_capacity: u16,
}

fn vault(args: VaultArgs<'_>) -> InfernalRelicVault {
    let VaultArgs {
        kind,
        vault_id,
        display_name,
        circle_id,
        zone_id,
        guarded_by_ruler_ref,
        guarded_by_agent_id,
        access_lock_ref,
        world_finite_capacity,
    } = args;
    InfernalRelicVault {
        kind,
        vault_id: vault_id.to_string(),
        display_name: display_name.to_string(),
        circle_id,
        zone_id: zone_id.to_string(),
        guarded_by_ruler_ref: guarded_by_ruler_ref.to_string(),
        guarded_by_agent_id: guarded_by_agent_id.to_string(),
        access_lock_ref: access_lock_ref.to_string(),
        world_finite_capacity,
        respawns_world_finite_items: false,
    }
}

fn item(
    item_id: &str,
    display_name: &str,
    slot: InfernalRelicSlot,
    source_vault_id: &str,
    guarded_by_ruler_ref: &str,
    power_rating_milli: u16,
    perks: &[InfernalRelicPerk],
) -> InfernalRelicItem {
    InfernalRelicItem {
        item_id: item_id.to_string(),
        display_name: display_name.to_string(),
        slot,
        rarity: InfernalRelicRarity::HellforgedLegendary,
        source_vault_id: source_vault_id.to_string(),
        guarded_by_ruler_ref: guarded_by_ruler_ref.to_string(),
        world_finite_count: 1,
        power_rating_milli,
        grants_map_or_minimap: false,
        bypasses_infernal_no_recall: false,
        perks: perks.to_vec(),
    }
}

fn perk(perk_id: &str, kind: InfernalRelicPerkKind, power_rating_milli: u16) -> InfernalRelicPerk {
    InfernalRelicPerk {
        perk_id: perk_id.to_string(),
        kind,
        power_rating_milli,
    }
}

fn set_bonus(
    bonus_id: &str,
    pieces_required: u8,
    effect_summary: &str,
    power_rating_milli: u16,
) -> InfernalRelicSetBonus {
    InfernalRelicSetBonus {
        bonus_id: bonus_id.to_string(),
        pieces_required,
        effect_summary: effect_summary.to_string(),
        power_rating_milli,
        grants_map_or_minimap: false,
        bypasses_infernal_no_recall: false,
    }
}
