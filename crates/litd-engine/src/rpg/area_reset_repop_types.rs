//! Reset/repop model schema for the authored world (#1668).
//!
//! CF-style timed zone resets ("the zone repops"). The reset CADENCE is CF-
//! documented and GLOBAL: helpfile 98 (TICK/TICKS/HOURS/REPOPULATE) states that
//! "Area resetting, otherwise known as repopulation, happens roughly every 3
//! area_minutes if no one is in the area; less often (15 area_minutes) if some one
//! is. Also note that objects lying on the ground will not be regenerated if
//! anyone is in the area when it resets." Those two documented invariants -- the
//! 3/15 area_minute cadence and object-regen-only-when-empty -- apply to EVERY
//! area, so every area's cadence is marked `CfDocumented` (source helpfile 98).
//!
//! What differs per area is the REPOP POLICY: the mob/object respawn semantics a
//! player experiences. That is DERIVED from each area's structural `AreaKind`
//! (cities repop dense civilians, dungeons re-arm doors/locks/traps, boss lairs run
//! boss timers per #1662), and is honestly marked `DerivedFromKind` -- exactly the
//! #1667 `TerrainDerivation` pattern. A CF-documented per-area override table
//! (`area_reset_repop_table.rs`) can promote a specific area's policy to
//! `CfDocumented` where CF documents it.
//!
//! Scarcity is honored per the limited-item ledger (helpfile 61 + #973/#974):
//! capped items do NOT respawn while held. Every profile asserts that invariant;
//! a profile that claims capped items respawn while held fails validation loud.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const RPG_AREA_RESET_REPOP_SCHEMA: &str = "litd.rpg.area_reset_repop.v1";
pub const AREA_RESET_REPOP_TABLE_ID: &str = "table.thera.area_reset_repop";

/// The CF-documented reset cadence, in game `area_minutes` (helpfile 98). These are
/// game-time area_minutes, NOT wall-clock minutes -- CF does not document a fixed
/// real-time conversion, so the model keeps the documented unit rather than
/// inventing one.
pub const CF_EMPTY_RESET_AREA_MINUTES: u16 = 3;
pub const CF_OCCUPIED_RESET_AREA_MINUTES: u16 = 15;

/// The mob/object respawn policy a player experiences in an area. Derived from the
/// area's structural `AreaKind`: the five families below cover every authored kind.
/// The reset CADENCE is uniform (helpfile 98); the POLICY is what makes a city feel
/// different from a boss lair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepopPolicy {
    /// Cities/towns/cabal halls: dense civilian + guard repop, doors and locks
    /// re-arm on reset (shops relock), no traps, no boss timer.
    CivilizedDense,
    /// Roads: sparse wandering patrols/travellers repop; nothing to re-lock, no
    /// traps, no boss timer.
    RoadPatrol,
    /// Open wilderness/regions: roaming fauna and bandits repop; no doors/locks to
    /// re-arm, no boss timer.
    WildRoaming,
    /// Dungeons: guarded interiors -- doors and locks re-arm AND traps re-arm
    /// (trapper parity, #1668), no single boss timer.
    GuardedInterior,
    /// Lairs: boss-anchored -- the lair boss runs a boss timer (#1662) and its
    /// limited loot honors the scarcity ledger; doors/locks re-arm, traps re-arm.
    BossLair,
}

impl RepopPolicy {
    pub const fn label(self) -> &'static str {
        match self {
            Self::CivilizedDense => "civilized_dense",
            Self::RoadPatrol => "road_patrol",
            Self::WildRoaming => "wild_roaming",
            Self::GuardedInterior => "guarded_interior",
            Self::BossLair => "boss_lair",
        }
    }

    /// Every policy, for histogram initialisation (an unrepresented policy still
    /// appears in the readback as `0`, so a blind spot is visible).
    pub const fn all() -> [RepopPolicy; 5] {
        [
            Self::CivilizedDense,
            Self::RoadPatrol,
            Self::WildRoaming,
            Self::GuardedInterior,
            Self::BossLair,
        ]
    }

    /// The respawn affordances this policy confers. Encoded once here so the derive
    /// path and the validator agree on what each policy means.
    pub const fn affordances(self) -> RepopAffordances {
        match self {
            Self::CivilizedDense => RepopAffordances {
                rearms_doors_locks: true,
                rearms_traps: false,
                has_boss_timer: false,
                population_density_tier: 3,
            },
            Self::RoadPatrol => RepopAffordances {
                rearms_doors_locks: false,
                rearms_traps: false,
                has_boss_timer: false,
                population_density_tier: 1,
            },
            Self::WildRoaming => RepopAffordances {
                rearms_doors_locks: false,
                rearms_traps: false,
                has_boss_timer: false,
                population_density_tier: 2,
            },
            Self::GuardedInterior => RepopAffordances {
                rearms_doors_locks: true,
                rearms_traps: true,
                has_boss_timer: false,
                population_density_tier: 2,
            },
            Self::BossLair => RepopAffordances {
                rearms_doors_locks: true,
                rearms_traps: true,
                has_boss_timer: true,
                population_density_tier: 2,
            },
        }
    }
}

/// The respawn affordances a `RepopPolicy` confers (see `RepopPolicy::affordances`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepopAffordances {
    pub rearms_doors_locks: bool,
    pub rearms_traps: bool,
    pub has_boss_timer: bool,
    pub population_density_tier: u8,
}

/// How an area's repop policy was determined, for auditability (#984: derive, do
/// not invent). Mirrors #1667 `TerrainDerivation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetDerivation {
    /// A CF page / helpfile documents the specific behavior (the global cadence,
    /// helpfile 98; or a per-area override in `area_reset_repop_table.rs`).
    CfDocumented,
    /// Derived from the area's structural `AreaKind` (City -> civilized dense,
    /// Dungeon -> guarded interior, Lair -> boss lair, ...).
    DerivedFromKind,
}

impl ResetDerivation {
    pub const fn label(self) -> &'static str {
        match self {
            Self::CfDocumented => "cf_documented",
            Self::DerivedFromKind => "derived_from_kind",
        }
    }
}

/// The full reset+repop profile for one authored area.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaResetRepopProfile {
    pub area_id: String,
    pub repop_policy: RepopPolicy,
    /// CF-documented cadence (helpfile 98), in game area_minutes: reset when empty.
    pub empty_reset_area_minutes: u16,
    /// CF-documented cadence (helpfile 98): the slower reset while occupied.
    pub occupied_reset_area_minutes: u16,
    /// CF helpfile 98: objects on the ground are NOT regenerated if anyone is in the
    /// area when it resets. Always true (a CF invariant); a false value fails loud.
    pub regen_objects_only_when_empty: bool,
    /// CF scarcity ledger (helpfile 61 + #973/#974): capped/limited items do NOT
    /// respawn while held. Always false (a CF invariant); a true value fails loud.
    pub capped_items_respawn_while_held: bool,
    pub rearms_doors_locks: bool,
    pub rearms_traps: bool,
    pub has_boss_timer: bool,
    /// Relative repop density (1 sparse .. 3 dense), from the policy affordances.
    pub population_density_tier: u8,
    /// How the CADENCE was determined (always `CfDocumented`, helpfile 98).
    pub cadence_derivation: ResetDerivation,
    /// How the POLICY was determined (`DerivedFromKind`, or `CfDocumented` when a
    /// per-area override documents it).
    pub policy_derivation: ResetDerivation,
    pub note: String,
}

/// The whole authored reset+repop table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaResetRepopTable {
    pub schema: String,
    pub table_id: String,
    pub entries: Vec<AreaResetRepopProfile>,
}

/// Deterministic readback with a by-policy histogram for blind-spot detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaResetRepopReadback {
    pub schema: String,
    pub table_id: String,
    pub area_count: usize,
    /// Count per repop policy; every policy appears (0 for an unrepresented one).
    pub policy_histogram: BTreeMap<String, usize>,
    pub boss_timer_count: usize,
    pub rearms_doors_locks_count: usize,
    pub rearms_traps_count: usize,
    pub cf_documented_policy_count: usize,
    pub derived_from_kind_policy_count: usize,
    /// Smallest empty-reset cadence across the table (proves the CF 3 area_minute
    /// floor holds).
    pub min_empty_reset_area_minutes: u16,
    /// Largest occupied-reset cadence across the table (proves the CF 15
    /// area_minute ceiling holds).
    pub max_occupied_reset_area_minutes: u16,
    pub table_sha256: String,
}

/// Coverage readback proving every authored area carries a reset+repop profile
/// (#1668 corpus lint), with the by-policy histogram.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaResetRepopCoverageReadback {
    pub registry_area_count: usize,
    pub profiled_area_count: usize,
    pub policy_histogram: BTreeMap<String, usize>,
    /// How many of the five repop policies are represented (blind-spot guard).
    pub distinct_policy_count: usize,
    pub table_sha256: String,
}
