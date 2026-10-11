//! Terrain-tag + acoustics derivation, validation, coverage lint, and readback
//! for the authored world (#1667).
//!
//! `derive_area_terrain_acoustics` turns one `AreaSpec` into its terrain +
//! acoustic profile: structural kinds map directly (City/Town/CabalHall -> urban,
//! Road -> road, Dungeon -> underground interior), and open natural kinds
//! (Wilderness/Region/Lair) read their CF-factual terrain from
//! `area_terrain_acoustics_table.rs`, failing loud (by name) on any untagged
//! natural area. Acoustics are a deterministic function of the terrain family.
//!
//! `validate_terrain_acoustics_coverage` is the corpus lint: it proves the tagged
//! set is EXACTLY the registry set, so no authored area is left untagged and no
//! entry references a non-existent area. Every failure names the offending areas.

use std::collections::{BTreeMap, BTreeSet};

use litd_hash::sha256_hex;

use crate::EngineError;

use super::area_specs_types::{AreaKind, AreaSpec};
use super::area_terrain_acoustics_table::authored_natural_terrain;
use super::area_terrain_acoustics_types::*;

/// The acoustic + stealth-legality profile a terrain family confers. This is the
/// CF stealth doctrine (#1629) encoded once: forests camouflage-legal, cities
/// civilized-hide, swamps night-hide, roads exposed, caves dark+echo.
struct TerrainProfile {
    surface: SurfaceMaterialClass,
    reverb: ReverbClass,
    occlusion: OcclusionClass,
    light: LightLevel,
    hide_legal: bool,
    exposed: bool,
}

const fn terrain_profile(terrain: TerrainTag) -> TerrainProfile {
    match terrain {
        // Deep woods: grass underfoot, open-air reverb, partial canopy cover, dim
        // light. Camouflage-legal (ranger/druid home terrain).
        TerrainTag::Forest => TerrainProfile {
            surface: SurfaceMaterialClass::Grass,
            reverb: ReverbClass::Open,
            occlusion: OcclusionClass::Partial,
            light: LightLevel::Dim,
            hide_legal: true,
            exposed: false,
        },
        // Open grassland: nothing to hide behind, bright and exposed.
        TerrainTag::Plains => TerrainProfile {
            surface: SurfaceMaterialClass::Grass,
            reverb: ReverbClass::Open,
            occlusion: OcclusionClass::Open,
            light: LightLevel::Bright,
            hide_legal: false,
            exposed: true,
        },
        // Bare rock: echoing, partial boulder cover, dim. Hide-legal (crags).
        TerrainTag::Mountain => TerrainProfile {
            surface: SurfaceMaterialClass::Stone,
            reverb: ReverbClass::Echoing,
            occlusion: OcclusionClass::Partial,
            light: LightLevel::Dim,
            hide_legal: true,
            exposed: false,
        },
        // Open water: exposed, bright, sound carries.
        TerrainTag::Water => TerrainProfile {
            surface: SurfaceMaterialClass::Water,
            reverb: ReverbClass::Open,
            occlusion: OcclusionClass::Open,
            light: LightLevel::Bright,
            hide_legal: false,
            exposed: true,
        },
        // Sand swallows footfalls (dead reverb) but offers no cover; bright, exposed.
        TerrainTag::Desert => TerrainProfile {
            surface: SurfaceMaterialClass::Sand,
            reverb: ReverbClass::Dead,
            occlusion: OcclusionClass::Open,
            light: LightLevel::Bright,
            hide_legal: false,
            exposed: true,
        },
        // Mire: mud underfoot (leaves tracks), dead reverb, reed cover, dim. Hide-
        // legal, and CF night-hide flips here.
        TerrainTag::Swamp => TerrainProfile {
            surface: SurfaceMaterialClass::Mud,
            reverb: ReverbClass::Dead,
            occlusion: OcclusionClass::Partial,
            light: LightLevel::Dim,
            hide_legal: true,
            exposed: false,
        },
        // Caves: dark and echoing, heavy winding cover, stone floor. Hide-legal.
        TerrainTag::Underground => TerrainProfile {
            surface: SurfaceMaterialClass::Stone,
            reverb: ReverbClass::Echoing,
            occlusion: OcclusionClass::Heavy,
            light: LightLevel::Dark,
            hide_legal: true,
            exposed: false,
        },
        // City streets: cobblestone, enclosed reverb between buildings, partial
        // crowd/alley cover, bright. Civilized-hide legal.
        TerrainTag::Urban => TerrainProfile {
            surface: SurfaceMaterialClass::Cobblestone,
            reverb: ReverbClass::Enclosed,
            occlusion: OcclusionClass::Partial,
            light: LightLevel::Bright,
            hide_legal: true,
            exposed: false,
        },
        // Roads: packed dirt, open reverb, no cover, bright. Exposed by design.
        TerrainTag::Road => TerrainProfile {
            surface: SurfaceMaterialClass::Dirt,
            reverb: ReverbClass::Open,
            occlusion: OcclusionClass::Open,
            light: LightLevel::Bright,
            hide_legal: false,
            exposed: true,
        },
    }
}

/// Legal surface materials for a terrain family. The coverage/entry validator
/// rejects any entry whose surface is not in this set, so a bogus acoustic tag
/// (e.g. a forest paved in sand) fails loud instead of silently mislabelling the
/// footstep timbre.
fn surface_legal_for(terrain: TerrainTag, surface: SurfaceMaterialClass) -> bool {
    use SurfaceMaterialClass::*;
    let allowed: &[SurfaceMaterialClass] = match terrain {
        TerrainTag::Forest => &[Grass, Dirt],
        TerrainTag::Plains => &[Grass, Dirt],
        TerrainTag::Mountain => &[Stone, Dirt],
        TerrainTag::Water => &[Water],
        TerrainTag::Desert => &[Sand, Stone],
        TerrainTag::Swamp => &[Mud, Water],
        TerrainTag::Underground => &[Stone, Dirt],
        TerrainTag::Urban => &[Cobblestone, Stone, Wood],
        TerrainTag::Road => &[Dirt, Cobblestone],
    };
    allowed.contains(&surface)
}

/// Derive the terrain + acoustic profile for one authored area (#1667).
pub fn derive_area_terrain_acoustics(spec: &AreaSpec) -> Result<AreaTerrainAcoustics, EngineError> {
    let (primary, secondary, derivation, note, no_sneak_interior) = match spec.area_kind {
        AreaKind::City | AreaKind::Town | AreaKind::CabalHall => (
            TerrainTag::Urban,
            Vec::new(),
            TerrainDerivation::DerivedFromKind,
            format!("{:?} -> urban terrain (civilized-hide).", spec.area_kind),
            true,
        ),
        AreaKind::Road => (
            TerrainTag::Road,
            Vec::new(),
            TerrainDerivation::DerivedFromKind,
            "Road -> exposed road terrain.".to_string(),
            false,
        ),
        AreaKind::Dungeon => (
            TerrainTag::Underground,
            Vec::new(),
            TerrainDerivation::DerivedFromKind,
            "Dungeon -> underground interior (dark+echo, no-sneak floors).".to_string(),
            true,
        ),
        AreaKind::Wilderness | AreaKind::Region | AreaKind::Lair => {
            let Some(authored) = authored_natural_terrain(&spec.area_id) else {
                return Err(invalid(
                    "area_terrain_acoustics.untagged_natural_area",
                    format!(
                        "open natural area {} ({:?}) has no authored terrain tag -- add a \
                         CF-factual entry to area_terrain_acoustics_table.rs",
                        spec.area_id, spec.area_kind
                    ),
                ));
            };
            (
                authored.terrain,
                authored.secondary.to_vec(),
                TerrainDerivation::CfDocumented,
                authored.note.to_string(),
                false,
            )
        }
    };

    let profile = terrain_profile(primary);
    let entry = AreaTerrainAcoustics {
        area_id: spec.area_id.clone(),
        primary_terrain: primary,
        secondary_terrain: secondary,
        light_level: profile.light,
        surface_material: profile.surface,
        reverb: profile.reverb,
        occlusion: profile.occlusion,
        hide_legal: profile.hide_legal,
        exposed: profile.exposed,
        no_sneak_interior,
        derivation,
        note,
    };
    validate_area_terrain_acoustics(&entry)?;
    Ok(entry)
}

/// Validate one terrain+acoustics entry. Fails loud (named) on an empty id, an
/// empty note, a duplicated/self-referential secondary terrain, or a surface
/// material illegal for the terrain family.
pub fn validate_area_terrain_acoustics(entry: &AreaTerrainAcoustics) -> Result<(), EngineError> {
    if entry.area_id.trim().is_empty() {
        return Err(invalid(
            "area_terrain_acoustics.area_id",
            "must be non-empty",
        ));
    }
    if entry.note.trim().is_empty() {
        return Err(invalid(
            "area_terrain_acoustics.note",
            format!("area {} must carry a derivation note", entry.area_id),
        ));
    }
    let mut seen = BTreeSet::new();
    for secondary in &entry.secondary_terrain {
        if *secondary == entry.primary_terrain {
            return Err(invalid(
                "area_terrain_acoustics.secondary_terrain",
                format!(
                    "area {} lists its primary terrain {} as a secondary",
                    entry.area_id,
                    entry.primary_terrain.label()
                ),
            ));
        }
        if !seen.insert(*secondary) {
            return Err(invalid(
                "area_terrain_acoustics.secondary_terrain",
                format!(
                    "area {} repeats secondary terrain {}",
                    entry.area_id,
                    secondary.label()
                ),
            ));
        }
    }
    if !surface_legal_for(entry.primary_terrain, entry.surface_material) {
        return Err(invalid(
            "area_terrain_acoustics.surface_material",
            format!(
                "area {} tags {} terrain with an illegal {} surface",
                entry.area_id,
                entry.primary_terrain.label(),
                entry.surface_material.label()
            ),
        ));
    }
    Ok(())
}

/// Build the full terrain+acoustics table for a set of authored specs. Every spec
/// is derived and validated; duplicate area ids fail loud.
pub fn build_area_terrain_acoustics_table(
    specs: &[AreaSpec],
) -> Result<AreaTerrainAcousticsTable, EngineError> {
    let mut entries = Vec::with_capacity(specs.len());
    let mut seen = BTreeSet::new();
    for spec in specs {
        if !seen.insert(spec.area_id.clone()) {
            return Err(invalid(
                "area_terrain_acoustics.duplicate_area",
                format!("area {} tagged twice", spec.area_id),
            ));
        }
        entries.push(derive_area_terrain_acoustics(spec)?);
    }
    Ok(AreaTerrainAcousticsTable {
        schema: RPG_AREA_TERRAIN_ACOUSTICS_SCHEMA.to_string(),
        table_id: AREA_TERRAIN_ACOUSTICS_TABLE_ID.to_string(),
        entries,
    })
}

/// Deterministic readback with the by-terrain histogram (#1667 blind-spot report).
pub fn area_terrain_acoustics_readback(
    table: &AreaTerrainAcousticsTable,
) -> Result<AreaTerrainAcousticsReadback, EngineError> {
    let mut terrain_histogram: BTreeMap<String, usize> = TerrainTag::all()
        .iter()
        .map(|t| (t.label().to_string(), 0))
        .collect();
    let mut light_histogram: BTreeMap<String, usize> = BTreeMap::new();
    let mut surface_histogram: BTreeMap<String, usize> = BTreeMap::new();
    let mut reverb_histogram: BTreeMap<String, usize> = BTreeMap::new();
    let mut hide_legal_count = 0;
    let mut exposed_count = 0;
    let mut no_sneak_interior_count = 0;
    let mut cf_documented_count = 0;
    let mut derived_from_kind_count = 0;

    for entry in &table.entries {
        validate_area_terrain_acoustics(entry)?;
        *terrain_histogram
            .entry(entry.primary_terrain.label().to_string())
            .or_insert(0) += 1;
        *light_histogram
            .entry(entry.light_level.label().to_string())
            .or_insert(0) += 1;
        *surface_histogram
            .entry(entry.surface_material.label().to_string())
            .or_insert(0) += 1;
        *reverb_histogram
            .entry(entry.reverb.label().to_string())
            .or_insert(0) += 1;
        hide_legal_count += usize::from(entry.hide_legal);
        exposed_count += usize::from(entry.exposed);
        no_sneak_interior_count += usize::from(entry.no_sneak_interior);
        match entry.derivation {
            TerrainDerivation::CfDocumented => cf_documented_count += 1,
            TerrainDerivation::DerivedFromKind => derived_from_kind_count += 1,
        }
    }

    Ok(AreaTerrainAcousticsReadback {
        schema: table.schema.clone(),
        table_id: table.table_id.clone(),
        area_count: table.entries.len(),
        terrain_histogram,
        light_histogram,
        surface_histogram,
        reverb_histogram,
        hide_legal_count,
        exposed_count,
        no_sneak_interior_count,
        cf_documented_count,
        derived_from_kind_count,
        table_sha256: table_hash(table)?,
    })
}

/// The corpus coverage lint (#1667). Proves the tagged set is EXACTLY the registry
/// set: every authored area carries a terrain tag, and no entry references an area
/// that is not authored. Fails loud listing every untagged and every orphan area.
///
/// The seven ranger-homeland natural terrains (forest, plains, mountain, water,
/// desert, swamp, underground) must each be present in meaningful quantity so
/// every ranger build has a homeland (#1667); the readback reports how many of the
/// seven are represented.
pub fn validate_terrain_acoustics_coverage(
    registry_area_ids: &BTreeSet<String>,
    table: &AreaTerrainAcousticsTable,
) -> Result<AreaTerrainAcousticsCoverageReadback, EngineError> {
    let readback = area_terrain_acoustics_readback(table)?;
    let tagged: BTreeSet<String> = table.entries.iter().map(|e| e.area_id.clone()).collect();
    if tagged.len() != table.entries.len() {
        return Err(invalid(
            "area_terrain_acoustics.duplicate_area",
            "the terrain table contains duplicate area ids",
        ));
    }

    let untagged: Vec<&String> = registry_area_ids.difference(&tagged).collect();
    if !untagged.is_empty() {
        return Err(invalid(
            "area_terrain_acoustics.untagged_areas",
            format!(
                "{} authored area(s) have no terrain+acoustics tag: {}",
                untagged.len(),
                untagged
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        ));
    }

    let orphans: Vec<&String> = tagged.difference(registry_area_ids).collect();
    if !orphans.is_empty() {
        return Err(invalid(
            "area_terrain_acoustics.orphan_tags",
            format!(
                "{} terrain tag(s) reference areas not in the registry: {}",
                orphans.len(),
                orphans
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        ));
    }

    let ranger_homelands = [
        TerrainTag::Forest,
        TerrainTag::Plains,
        TerrainTag::Mountain,
        TerrainTag::Water,
        TerrainTag::Desert,
        TerrainTag::Swamp,
        TerrainTag::Underground,
    ];
    let ranger_homeland_terrain_count = ranger_homelands
        .iter()
        .filter(|t| {
            readback
                .terrain_histogram
                .get(t.label())
                .copied()
                .unwrap_or(0)
                > 0
        })
        .count();

    Ok(AreaTerrainAcousticsCoverageReadback {
        registry_area_count: registry_area_ids.len(),
        tagged_area_count: tagged.len(),
        terrain_histogram: readback.terrain_histogram,
        ranger_homeland_terrain_count,
        table_sha256: readback.table_sha256,
    })
}

fn table_hash(table: &AreaTerrainAcousticsTable) -> Result<String, EngineError> {
    serde_json::to_vec(table)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize area terrain acoustics table: {error}"),
        })
}

fn invalid(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    }
}
