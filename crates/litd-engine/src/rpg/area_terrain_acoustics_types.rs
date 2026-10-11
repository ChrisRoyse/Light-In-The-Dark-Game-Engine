//! Terrain-tag + acoustics substrate schema for the authored world (#1667).
//!
//! Every navigable area is tagged with a terrain family, a light level, and an
//! acoustic profile (surface material, reverb, occlusion), plus the stealth-
//! legality affordances the CF stealth doctrine (#1629) calls out: whether hide/
//! camouflage is legal, whether the ground is exposed, and whether the area
//! contains no-sneak interior floors. This is the per-area data the hide/
//! camouflage legality and footstep detection profiles (#1601/#1602/#1603/#1623)
//! read to decide, room by room, what is possible.
//!
//! Terrain is DERIVED from each area's CF-factual sector: a City/Town is urban, a
//! Road is a road, a Dungeon is an underground interior, and every open natural
//! area (Wilderness/Region/Lair) carries an explicitly authored terrain tag
//! transcribed from its CF page / authored sector (see
//! `area_terrain_acoustics_table.rs`). Acoustics are then a deterministic function
//! of the terrain family (`terrain_acoustic_profile`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const RPG_AREA_TERRAIN_ACOUSTICS_SCHEMA: &str = "litd.rpg.area_terrain_acoustics.v1";
pub const AREA_TERRAIN_ACOUSTICS_TABLE_ID: &str = "table.thera.area_terrain_acoustics";

/// The nine terrain families the stealth substrate recognises (#1601/#1667). This
/// is the CF race/terrain matrix demand: forest, plains, mountain, water, desert,
/// swamp, and cavern (underground) must all exist so every ranger build has a
/// homeland, plus the civilized urban and the exposed road families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerrainTag {
    Forest,
    Plains,
    Mountain,
    Water,
    Desert,
    Swamp,
    Underground,
    Urban,
    Road,
}

impl TerrainTag {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Forest => "forest",
            Self::Plains => "plains",
            Self::Mountain => "mountain",
            Self::Water => "water",
            Self::Desert => "desert",
            Self::Swamp => "swamp",
            Self::Underground => "underground",
            Self::Urban => "urban",
            Self::Road => "road",
        }
    }

    /// Every terrain family, for histogram initialisation (a family with zero
    /// tagged areas still appears in the readback as `0`, so a blind spot is
    /// visible rather than silently absent).
    pub const fn all() -> [TerrainTag; 9] {
        [
            Self::Forest,
            Self::Plains,
            Self::Mountain,
            Self::Water,
            Self::Desert,
            Self::Swamp,
            Self::Underground,
            Self::Urban,
            Self::Road,
        ]
    }
}

/// Ambient light level, gating vision-based detection (dark rooms defeat sight,
/// caves are dark, plains are bright).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightLevel {
    Bright,
    Dim,
    Dark,
}

impl LightLevel {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bright => "bright",
            Self::Dim => "dim",
            Self::Dark => "dark",
        }
    }
}

/// The surface material a footstep lands on. Drives footstep-audio timbre and the
/// mud-source rule (swamp/water zones leave tracks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceMaterialClass {
    Grass,
    Dirt,
    Stone,
    Sand,
    Mud,
    Water,
    Cobblestone,
    Wood,
}

impl SurfaceMaterialClass {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Grass => "grass",
            Self::Dirt => "dirt",
            Self::Stone => "stone",
            Self::Sand => "sand",
            Self::Mud => "mud",
            Self::Water => "water",
            Self::Cobblestone => "cobblestone",
            Self::Wood => "wood",
        }
    }
}

/// How sound reverberates in the space (drives the audible radius of movement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReverbClass {
    /// Sand/soft ground swallows sound.
    Dead,
    /// Open sky, sound dissipates.
    Open,
    /// Walls and roofs contain and reflect sound (indoor/urban).
    Enclosed,
    /// Hard cave/mountain rock echoes a long time.
    Echoing,
}

impl ReverbClass {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dead => "dead",
            Self::Open => "open",
            Self::Enclosed => "enclosed",
            Self::Echoing => "echoing",
        }
    }
}

/// How much line-of-sight cover the terrain provides (drives concealment).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcclusionClass {
    /// Nothing to hide behind (open plains, open water, roads).
    Open,
    /// Some cover (trees, buildings, boulders).
    Partial,
    /// Dense cover / winding walls (caves, deep forest interiors).
    Heavy,
}

impl OcclusionClass {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Partial => "partial",
            Self::Heavy => "heavy",
        }
    }
}

/// How an area's terrain tag was determined, for auditability (#984: derive, do
/// not invent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerrainDerivation {
    /// The CF page / authored fixture documents the specific terrain (a named
    /// forest, swamp, sea, cavern, ...).
    CfDocumented,
    /// Derived from the area's structural `AreaKind` (City/Town -> urban, Road ->
    /// road, Dungeon -> underground interior).
    DerivedFromKind,
}

impl TerrainDerivation {
    pub const fn label(self) -> &'static str {
        match self {
            Self::CfDocumented => "cf_documented",
            Self::DerivedFromKind => "derived_from_kind",
        }
    }
}

/// The full terrain + acoustics profile for one authored area.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaTerrainAcoustics {
    pub area_id: String,
    pub primary_terrain: TerrainTag,
    /// Additional terrain families present at the edges (e.g. a forest that a road
    /// cuts through -> waylay corridors, #1629). Never repeats the primary.
    pub secondary_terrain: Vec<TerrainTag>,
    pub light_level: LightLevel,
    pub surface_material: SurfaceMaterialClass,
    pub reverb: ReverbClass,
    pub occlusion: OcclusionClass,
    /// Hide / camouflage is legal here (forests, cities civilized-hide, caves,
    /// swamps, mountains).
    pub hide_legal: bool,
    /// Exposed ground: no natural concealment (open plains, open water, roads).
    pub exposed: bool,
    /// The area contains CF no-sneak interior floors (guild/temple/dungeon rooms
    /// where sneak is disallowed).
    pub no_sneak_interior: bool,
    pub derivation: TerrainDerivation,
    pub note: String,
}

/// The whole authored terrain+acoustics table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaTerrainAcousticsTable {
    pub schema: String,
    pub table_id: String,
    pub entries: Vec<AreaTerrainAcoustics>,
}

/// Deterministic readback proving what the table contains, with a by-terrain
/// histogram for blind-spot detection (#1667 deliverable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaTerrainAcousticsReadback {
    pub schema: String,
    pub table_id: String,
    pub area_count: usize,
    /// Count per terrain family; every family appears (0 for an unrepresented one).
    pub terrain_histogram: BTreeMap<String, usize>,
    pub light_histogram: BTreeMap<String, usize>,
    pub surface_histogram: BTreeMap<String, usize>,
    pub reverb_histogram: BTreeMap<String, usize>,
    pub hide_legal_count: usize,
    pub exposed_count: usize,
    pub no_sneak_interior_count: usize,
    pub cf_documented_count: usize,
    pub derived_from_kind_count: usize,
    pub table_sha256: String,
}

/// Coverage readback proving every authored area in the corpus registry carries a
/// terrain+acoustics entry (#1667 corpus lint).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaTerrainAcousticsCoverageReadback {
    pub registry_area_count: usize,
    pub tagged_area_count: usize,
    pub terrain_histogram: BTreeMap<String, usize>,
    pub ranger_homeland_terrain_count: usize,
    pub table_sha256: String,
}
