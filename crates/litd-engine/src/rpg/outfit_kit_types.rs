//! The Carrion Fields newbie OUTFIT kit data model (#1687).
//!
//! CF-exact from helpfile 314 (OUTFIT GEAR): the `outfit` command equips a
//! character from empty equipment slots. There are two bands:
//!
//! - Levels 1-10 (`LowbieAnywhere`): usable ANYWHERE. Grants soft leather gear
//!   (light source, weapon, helmet, shield, vest) from the Mayor's warehouses,
//!   plus MAPS of Thera, Galadon, and hometown (the acquired-maps loop
//!   #944/#1633), a basic drink container, and food. Only empty slots are filled.
//! - Levels 11+ (`GuildOnly`): usable only within your own guild. Grants a set
//!   of simple equipment and provisions; the gear is shoddy and prone to
//!   eventual breakage.
//!
//! This is the full-loot recovery floor (#976): a player can ALWAYS re-arm.
//! Names/lore are originalized per #984; the mechanics (slot set, band gating,
//! map grants, breakage) are factual.

use serde::{Deserialize, Serialize};

pub const RPG_OUTFIT_KIT_SCHEMA: &str = "litd.rpg.outfit_kit.v1";

/// Which outfit band a kit models (helpfile 314's two paragraphs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutfitBand {
    /// Levels 1-10: outfit ANYWHERE from the Mayor's warehouses; includes maps.
    LowbieAnywhere,
    /// Levels 11+: outfit only within your own guild; shoddy, breakage-prone.
    GuildOnly,
}

impl OutfitBand {
    pub const fn as_id(self) -> &'static str {
        match self {
            OutfitBand::LowbieAnywhere => "lowbie_anywhere",
            OutfitBand::GuildOnly => "guild_only",
        }
    }

    /// True when this band may only be invoked inside the player's own guild.
    pub const fn guild_gated(self) -> bool {
        matches!(self, OutfitBand::GuildOnly)
    }

    /// The inclusive CF level window this band serves (helpfile 314).
    pub const fn level_window(self) -> (u16, u16) {
        match self {
            OutfitBand::LowbieAnywhere => (1, 10),
            OutfitBand::GuildOnly => (11, u16::MAX),
        }
    }
}

/// One granted gear piece. The wear-slot id is a token from the #1683 wear-slot
/// vocabulary (`light`, `wield`, `head`, `shield`, `body`); `material` is a token
/// from the #1685 material vocabulary (the kit is soft `leather`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutfitGearPiece {
    pub wear_slot_id: String,
    pub material_id: String,
    /// A human label for the piece (helpfile 314 names: light source, weapon,
    /// helmet, shield, vest).
    pub label: String,
}

/// A granted provision (drink container / food) -- the survival floor (#953).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutfitProvision {
    /// `drink_container` or `food` (the two provisions helpfile 314 names).
    pub provision_kind: String,
    pub label: String,
}

/// A granted map (the acquired-maps loop #944/#1633). Only the lowbie band
/// grants maps (helpfile 314); the guild band re-gears without re-mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutfitMapGrant {
    /// A region/city id the map covers (`thera`, `galadon`, `hometown`).
    pub map_region_id: String,
    pub label: String,
}

/// The CF newbie outfit kit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutfitKitSpec {
    pub schema: String,
    pub outfit_id: String,
    pub display_name: String,
    pub band: OutfitBand,
    /// Gear granted (soft leather: light, weapon, helmet, shield, vest).
    pub gear: Vec<OutfitGearPiece>,
    /// Provisions granted (drink container + food).
    pub provisions: Vec<OutfitProvision>,
    /// Maps granted (lowbie band only: Thera, Galadon, hometown).
    pub maps: Vec<OutfitMapGrant>,
    /// helpfile 314: "Only empty equipment slots are affected." Always true.
    pub only_fills_empty_slots: bool,
    /// helpfile 314: the guild-band gear is "fairly shoddy and prone to eventual
    /// breakage." True for `GuildOnly`, false for the lowbie band.
    pub breakage_prone: bool,
    /// The full-loot recovery floor this kit backstops (#976).
    pub recovery_floor_issue: u32,
    pub source_issue: u32,
    pub canon_issues: Vec<u32>,
    pub mechanics_source_url: String,
    pub implementation_notes: Vec<String>,
}

/// The FSV readback surface: band + counts + the deterministic spec hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutfitKitReadback {
    pub schema: String,
    pub outfit_id: String,
    pub band: String,
    pub guild_gated: bool,
    pub gear_count: usize,
    pub provision_count: usize,
    pub map_count: usize,
    pub breakage_prone: bool,
    pub level_window_low: u16,
    pub level_window_high: u16,
    pub outfit_kit_sha256: String,
}
