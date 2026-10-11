//! Racial movement-noise & concealment innate profile table (#1603).
//!
//! One authoritative data table giving every authored race a stealth profile:
//! its movement-noise class (size/weight derived), footstep-radius scale, and the
//! innate stealth/detection/reveal traits it is born with. CF-factual innates are
//! transcribed from the wiki race pages (see per-profile comments in
//! `racial_stealth.rs`); where CF is silent on movement noise, the mapping is
//! defined by issue #1603 (size class -> noise class) and applied uniformly.
//!
//! Referential integrity: every `RacialStealthTrait.ability_id` MUST resolve in
//! the shared stealth ability catalog (#1604), and every `race_id` MUST match an
//! authored `RaceSpec` id (#999/#1000-#1018). Validation fails loud on any
//! dangling reference (see `racial_stealth::validate_racial_stealth_table`).

use serde::{Deserialize, Serialize};

use super::race_specs_types::RaceSize;

pub const RPG_RACIAL_STEALTH_SCHEMA: &str = "litd.rpg.racial_stealth_profile.v1";
pub const RACIAL_STEALTH_TABLE_ID: &str = "table.ashen_reach.racial_stealth";

/// Movement-noise class a race broadcasts while moving. Size/weight derived; the
/// two elven deep-lines override to `Silent` via their innate perma-sneak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementNoiseClass {
    Silent,
    Quiet,
    Normal,
    Heavy,
    Thunderous,
}

impl MovementNoiseClass {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Silent => "silent",
            Self::Quiet => "quiet",
            Self::Normal => "normal",
            Self::Heavy => "heavy",
            Self::Thunderous => "thunderous",
        }
    }

    /// Footstep audio radius as a percent of the medium-race baseline (100).
    /// Issue #1603 mapping: silent broadcasts nothing, giants roughly 2.5x.
    pub const fn footstep_radius_scale_percent(self) -> u16 {
        match self {
            Self::Silent => 0,
            Self::Quiet => 50,
            Self::Normal => 100,
            Self::Heavy => 160,
            Self::Thunderous => 250,
        }
    }
}

/// What role an innate trait plays in the stealth loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StealthTraitRole {
    /// Move without being noticed (sneak / quiet movement).
    SilentMove,
    /// Stay unseen while stationary or camouflaged.
    Conceal,
    /// See through concealment (detect hidden / detect invis).
    Detect,
    /// Force-reveal or lock out an enemy's concealment (faerie fire).
    Reveal,
}

impl StealthTraitRole {
    pub const fn label(self) -> &'static str {
        match self {
            Self::SilentMove => "silent_move",
            Self::Conceal => "conceal",
            Self::Detect => "detect",
            Self::Reveal => "reveal",
        }
    }
}

/// Terrain scope in which an innate trait is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StealthTerrainScope {
    /// Everywhere, permanently.
    Always,
    /// Natural (non-city, non-indoor) terrain only.
    NaturalTerrain,
    /// Wilderness tags only (a strict subset of natural terrain).
    Wilderness,
    /// Forest tags only.
    Forest,
    /// Underground and city tags (the duergar detect-hidden profile).
    UndergroundAndCity,
}

impl StealthTerrainScope {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::NaturalTerrain => "natural_terrain",
            Self::Wilderness => "wilderness",
            Self::Forest => "forest",
            Self::UndergroundAndCity => "underground_and_city",
        }
    }
}

/// A single innate stealth/detection/reveal trait a race is born with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RacialStealthTrait {
    pub trait_id: String,
    pub role: StealthTraitRole,
    /// References an ability id in the #1604 stealth catalog (referential integrity).
    pub ability_id: String,
    pub scope: StealthTerrainScope,
    /// Innate traits are `false`; a `true` here means the race merely trains it
    /// faster and would not belong in this innate table (guarded by validation).
    pub requires_training: bool,
    pub description: String,
}

/// A race's full movement-noise & concealment profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RacialStealthProfile {
    /// Matches an authored `RaceSpec.race_id`.
    pub race_id: String,
    pub display_name: String,
    pub source_issue: u32,
    pub mechanics_source_url: String,
    pub size: RaceSize,
    pub movement_noise: MovementNoiseClass,
    pub footstep_radius_scale_percent: u16,
    /// `true` when the race's size penalizes the creep skill (large/giant frames).
    pub creep_size_penalty: bool,
    pub innate_traits: Vec<RacialStealthTrait>,
    pub notes: Vec<String>,
}

/// The whole 20-race stealth profile table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RacialStealthProfileTable {
    pub schema: String,
    pub table_id: String,
    pub profiles: Vec<RacialStealthProfile>,
}

/// Deterministic readback proving what the table contains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RacialStealthTableReadback {
    pub schema: String,
    pub table_id: String,
    pub profile_count: usize,
    pub noise_class_counts: std::collections::BTreeMap<String, usize>,
    pub trait_role_counts: std::collections::BTreeMap<String, usize>,
    pub innate_trait_total: usize,
    pub races_with_innate_stealth: usize,
    pub creep_penalized_races: usize,
    pub referenced_ability_ids: Vec<String>,
    pub race_ids: Vec<String>,
    pub table_sha256: String,
}
