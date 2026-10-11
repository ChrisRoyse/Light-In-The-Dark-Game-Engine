//! Stealth acquisition matrix types (#1623).
//!
//! One authoritative data table mapping every stealth/detection ability to who
//! grants it (class or race), at what level, and how (trained / innate / a class
//! path). CF-factual levels are transcribed from the helpfiles cited in issue
//! #1623. Referential integrity is enforced by
//! `stealth_acquisition::validate_stealth_acquisition_matrix`: every `ability_id`
//! must resolve in the #1604 catalog and every `grantor_id` must match an
//! authored class/race spec id (or be flagged `provisional`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const RPG_STEALTH_ACQUISITION_SCHEMA: &str = "litd.rpg.stealth_acquisition_matrix.v1";
pub const STEALTH_ACQUISITION_TABLE_ID: &str = "table.ashen_reach.stealth_acquisition";

/// Whether a row is granted by a class or by a race.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantorKind {
    Class,
    Race,
}

impl GrantorKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Race => "race",
        }
    }
}

/// How an owner comes to have the ability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionKind {
    /// Practiced/learned at the row's level.
    Trained,
    /// Born with it (racial innate, level 1).
    Innate,
    /// Only available through a specific class path/specialization.
    PathOnly,
    /// Castable by the class on other players (utility grant).
    CastableOnOthers,
}

impl AcquisitionKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Trained => "trained",
            Self::Innate => "innate",
            Self::PathOnly => "path_only",
            Self::CastableOnOthers => "castable_on_others",
        }
    }
}

/// One (ability, owner, level) cell of the matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StealthAcquisitionEntry {
    /// References an ability id in the #1604 stealth catalog.
    pub ability_id: String,
    pub grantor_kind: GrantorKind,
    /// A class spec id (#1020-#1036) or race spec id (#999/#1000-#1018).
    pub grantor_id: String,
    /// Level the owner gains it (1 for innate). CF cap is 51.
    pub level: u16,
    pub acquisition: AcquisitionKind,
    /// Class path/specialization that gates the row, when `acquisition` is path-only.
    pub path_id: Option<String>,
    /// Thief practice point cost, where the helpfile lists one.
    pub point_cost: Option<u16>,
    /// Terrain profile for terrain-gated skills (hide).
    pub terrain_profile: Option<String>,
    /// `true` when the grantor spec is not yet authored in the repo; validation
    /// then skips the id-resolution check for this row.
    pub provisional: bool,
    /// Helpfile id / note documenting the CF source of the level.
    pub source_note: String,
}

/// The whole acquisition matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StealthAcquisitionMatrix {
    pub schema: String,
    pub table_id: String,
    pub entries: Vec<StealthAcquisitionEntry>,
}

/// Deterministic readback proving what the matrix contains, plus the exact set of
/// provisional grantors and dangling-reference audit counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StealthAcquisitionReadback {
    pub schema: String,
    pub table_id: String,
    pub entry_count: usize,
    pub class_entry_count: usize,
    pub race_entry_count: usize,
    pub acquisition_counts: BTreeMap<String, usize>,
    pub distinct_ability_count: usize,
    pub distinct_grantor_count: usize,
    pub provisional_entry_count: usize,
    pub provisional_grantors: Vec<String>,
    pub abilities_validated: usize,
    pub grantors_validated: usize,
    pub referenced_ability_ids: Vec<String>,
    pub owners_per_ability: BTreeMap<String, usize>,
    pub matrix_sha256: String,
}
