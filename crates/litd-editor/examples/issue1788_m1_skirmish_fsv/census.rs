//! #1788 phase-1 content census + cross-ref proofs + negative controls. Every authored object
//! is lowered through its REAL editor valve (so all cross-refs are validated: hero->ability,
//! tech prerequisites acyclic, art refs resolve to real vault hashes). Exact per-faction counts
//! are asserted. Two typed negative controls: a removed tech prerequisite record and a dangling
//! art ref both fail lowering/validation with a stable diagnostics code.

use litd_editor::history::EditorDocument;
use litd_editor::object_editor::{
    lower_ability_from_object, lower_constants_from_document, lower_hero_from_object,
    lower_item_from_document, lower_tech_from_document, lower_unit_from_object,
    lower_upgrade_from_document_path,
};
use litd_editor::terrain_lowering::{
    HeightFieldLowerParams, lower_height_field, lower_nav_bake_input, lower_tile_splat_map,
};
use litd_engine::EngineError;
use litd_engine::nav::{NavCellCoord, bake_navmesh, find_path};

use crate::ashford::{START_P0, START_P1};
use crate::factions::{
    ABILITIES_PER_FACTION, FactionIds, HEROES_PER_FACTION, ITEMS_PER_FACTION,
    STRUCTURES_PER_FACTION, TECH_PER_FACTION, UNITS_PER_FACTION, UPGRADES_PER_FACTION,
};

const INVALID_CONFIG: &str = "LITD-RENDER-UI-INVALID-CONFIG";

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct FactionCensus {
    pub units: usize,
    pub structures: usize,
    pub heroes: usize,
    pub abilities: usize,
    pub items: usize,
    pub tech: usize,
    pub upgrades: usize,
}

impl FactionCensus {
    pub fn matches_expected(&self) -> bool {
        self.units == UNITS_PER_FACTION
            && self.structures == STRUCTURES_PER_FACTION
            && self.heroes == HEROES_PER_FACTION
            && self.abilities == ABILITIES_PER_FACTION
            && self.items == ITEMS_PER_FACTION
            && self.tech == TECH_PER_FACTION
            && self.upgrades == UPGRADES_PER_FACTION
    }
}

/// Lower every authored record of one faction through its real editor valve. Fails loud on the
/// first bad object or unresolved cross-ref. `aster_path` is the persisted Calyx Aster document
/// the p2 (upgrade) valve re-reads.
pub fn lower_faction(
    document: &EditorDocument,
    aster_path: &std::path::Path,
    ids: &FactionIds,
) -> Result<FactionCensus, EngineError> {
    for id in &ids.units {
        lower_unit_from_object(document, id)?;
    }
    for id in &ids.structures {
        lower_unit_from_object(document, id)?;
    }
    for id in &ids.heroes {
        lower_hero_from_object(document, id)?;
    }
    for id in &ids.abilities {
        lower_ability_from_object(document, id)?;
    }
    for id in &ids.items {
        lower_item_from_document(document, id)?;
    }
    for id in &ids.upgrades {
        lower_upgrade_from_document_path(aster_path, id)?;
    }
    Ok(FactionCensus {
        units: ids.units.len(),
        structures: ids.structures.len(),
        heroes: ids.heroes.len(),
        abilities: ids.abilities.len(),
        items: ids.items.len(),
        tech: ids.tech.len(),
        upgrades: ids.upgrades.len(),
    })
}

/// Lower the whole document's tech tree (both factions) once; the valve validates prerequisite
/// resolution + acyclicity across every `Tech` node.
pub fn lower_tech(document: &EditorDocument) -> Result<(), EngineError> {
    lower_tech_from_document(document)?;
    Ok(())
}

/// Lower the shared gameplay-constants singleton, then a Veilspun-delta variant, proving a
/// faction-distinct constants delta (#827) lowers to a distinct fingerprint.
pub fn lower_constants_with_delta(document: &EditorDocument) -> Result<bool, EngineError> {
    let base = lower_constants_from_document(document)?;
    let mut variant = document.clone();
    if let Some(config) = variant.objects.get_mut("ashford_config") {
        // Veilspun (shadow-weavers) favor longer nights + agility-to-armor.
        config
            .properties
            .insert("cycle.night_length_secs".to_string(), "240".to_string());
        config
            .properties
            .insert("hero.agi_to_armor".to_string(), "0.4".to_string());
    }
    let delta = lower_constants_from_document(&variant)?;
    Ok(base.fingerprint != delta.fingerprint)
}

/// The map lowers to real render/nav artifacts and is pathing-valid: a route exists from slot 0's
/// start through the crossing to slot 1's start. Returns (tile_palette_len, nav_cells, path_len).
pub fn lower_map(document: &EditorDocument) -> Result<(usize, usize, usize), EngineError> {
    let splat = lower_tile_splat_map(document)?;
    let _height = lower_height_field(document, &HeightFieldLowerParams::default())?;
    let nav = lower_nav_bake_input(document, HeightFieldLowerParams::default().cliff_step)?;
    let navmesh = bake_navmesh(&nav.input)?;
    let path = find_path(
        &navmesh,
        NavCellCoord::new(START_P0.0, START_P0.1),
        NavCellCoord::new(START_P1.0, START_P1.1),
    )?;
    Ok((splat.palette.len(), nav.cell_count, path.nodes.len()))
}

/// Negative control 1: remove a tier-1 tech record that a tier-2 node still requires; the tech
/// valve must fail typed (dangling prerequisite). Returns the diagnostics code observed.
pub fn negative_tech_prereq(document: &EditorDocument, removed_tech_id: &str) -> String {
    let mut broken = document.clone();
    if broken.remove_object(removed_tech_id).is_err() {
        return "REMOVE_FAILED".to_string();
    }
    match lower_tech_from_document(&broken) {
        Ok(_) => "ACCEPTED_SILENTLY".to_string(),
        Err(error) => error.code().to_string(),
    }
}

/// Negative control 2: point a unit at a vault product that is not in the document; document
/// validation must fail typed (dangling art ref). Returns the diagnostics code observed.
pub fn negative_dangling_art(document: &EditorDocument, unit_id: &str) -> String {
    let mut broken = document.clone();
    let Some(unit) = broken.objects.get_mut(unit_id) else {
        return "UNIT_MISSING".to_string();
    };
    unit.properties.insert(
        "model_product_id".to_string(),
        "product.does_not_exist".to_string(),
    );
    match broken.validate() {
        Ok(()) => "ACCEPTED_SILENTLY".to_string(),
        Err(error) => error.code().to_string(),
    }
}

pub fn negative_controls_pass(tech_code: &str, art_code: &str) -> bool {
    tech_code == INVALID_CONFIG && art_code == INVALID_CONFIG
}
