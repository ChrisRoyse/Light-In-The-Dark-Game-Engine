//! The authored CF-factual terrain tag for every open natural area (#1667).
//!
//! Structural areas (City/Town/Road/Dungeon/CabalHall) derive their terrain from
//! `AreaKind` in `area_terrain_acoustics.rs`. Open natural areas -- `Wilderness`,
//! `Region`, and `Lair` kinds -- do NOT encode their surface terrain in the kind
//! (a `Wilderness` may be a forest, a sea, a swamp, or a frozen peak; a `Lair` may
//! be an underground den or an aquatic grotto). This table transcribes the terrain
//! from each area's CF page / authored fixture sector (the western-forest cluster,
//! the sea cluster, the underdark cluster, ...), so the derivation never has to
//! guess. A natural area missing from this table fails the coverage lint loud,
//! by name.

use super::area_terrain_acoustics_types::TerrainTag;

/// The authored terrain for one open natural area.
pub(crate) struct AuthoredTerrain {
    pub terrain: TerrainTag,
    pub secondary: &'static [TerrainTag],
    pub note: &'static str,
}

/// CF-factual terrain for every authored open-natural (`Wilderness`/`Region`/
/// `Lair`) area. Keyed by `area_id`. Grouped by CF sector cluster.
const NATURAL_TERRAIN: &[(&str, TerrainTag, &[TerrainTag], &str)] = &[
    // --- Western forests (#1650): ranger/druid country, camouflage-legal. ------
    (
        "area.thera.feanwyyn_weald",
        TerrainTag::Forest,
        &[TerrainTag::Road],
        "CF Feanwyyn Weald: practice woods with waylay road-edge corridors (#1629).",
    ),
    (
        "area.thera.azreth_forest",
        TerrainTag::Forest,
        &[],
        "CF Azreth Forest.",
    ),
    (
        "area.thera.forest_of_prosimy",
        TerrainTag::Forest,
        &[],
        "CF Forest of Prosimy.",
    ),
    (
        "area.thera.forest_of_nowhere",
        TerrainTag::Forest,
        &[],
        "CF Forest of Nowhere.",
    ),
    (
        "area.thera.whistlewood",
        TerrainTag::Forest,
        &[],
        "CF Whistlewood.",
    ),
    (
        "area.thera.dranettie_wood",
        TerrainTag::Forest,
        &[],
        "CF Dranettie Wood.",
    ),
    (
        "area.thera.emerald_forest",
        TerrainTag::Forest,
        &[],
        "CF Emerald Forest.",
    ),
    (
        "area.thera.spiderhaunt_woods",
        TerrainTag::Forest,
        &[],
        "CF Spiderhaunt Woods.",
    ),
    (
        "area.thera.violet_woodland",
        TerrainTag::Forest,
        &[],
        "CF Violet Woodland.",
    ),
    (
        "area.thera.mal_trakis",
        TerrainTag::Forest,
        &[],
        "CF Mal-trakis: far-south jungle headhunter village (savage-ranger homeland).",
    ),
    (
        "area.thera.vale_of_arendyl",
        TerrainTag::Forest,
        &[TerrainTag::Road],
        "CF Vale of Arendyl: wood-elf glade/grove vale off the Holy Road.",
    ),
    (
        "area.thera.galadon.lumber_yard",
        TerrainTag::Forest,
        &[TerrainTag::Road],
        "CF Lumber Yard (help id 2523): logging camp on Galadon's forested south skirts.",
    ),
    (
        "area.thera.galadon.kobold_warrens",
        TerrainTag::Forest,
        &[TerrainTag::Underground],
        "CF Kobold Warrens (help id 2523): forest-treeline warren with earthen burrows.",
    ),
    // --- Plains / hills / steppes (#1651). -------------------------------------
    (
        "area.thera.open_plains",
        TerrainTag::Plains,
        &[],
        "CF Open Plains: exposed.",
    ),
    (
        "area.thera.udgaardian_plains",
        TerrainTag::Plains,
        &[],
        "CF Udgaardian Plains.",
    ),
    (
        "area.thera.fields_of_balatorn",
        TerrainTag::Plains,
        &[],
        "CF Fields of Balatorn.",
    ),
    (
        "area.thera.drogran_hills",
        TerrainTag::Plains,
        &[],
        "CF Drogran Hills: rolling grassland.",
    ),
    (
        "area.thera.northern_foothills",
        TerrainTag::Plains,
        &[TerrainTag::Mountain],
        "CF Northern Foothills.",
    ),
    (
        "area.thera.nanthor",
        TerrainTag::Plains,
        &[],
        "CF Nanthor plains region.",
    ),
    (
        "area.thera.halfling_lands",
        TerrainTag::Plains,
        &[TerrainTag::Road],
        "CF Halfling Lands: rolling shire pocket of burrow-homes and hedged lanes (grassland).",
    ),
    (
        "area.thera.arial_sky_sanctum",
        TerrainTag::Plains,
        &[],
        "CF Arial Sky Sanctum: open cloud-terraces high above the world -- mapped to the \
         open/exposed acoustic family (no natural cover, sound carries).",
    ),
    // --- Mountains (#1652). ----------------------------------------------------
    (
        "area.thera.northern_mountains",
        TerrainTag::Mountain,
        &[],
        "CF Northern Mountains.",
    ),
    (
        "area.thera.jade_mountains",
        TerrainTag::Mountain,
        &[],
        "CF Jade Mountains.",
    ),
    (
        "area.thera.mount_calandaryl",
        TerrainTag::Mountain,
        &[],
        "CF Mount Calandaryl.",
    ),
    (
        "area.thera.mount_kiadana_rah",
        TerrainTag::Mountain,
        &[],
        "CF Mount Kiadana-Rah.",
    ),
    (
        "area.thera.frigid_wasteland",
        TerrainTag::Mountain,
        &[],
        "CF Frigid Wasteland: frozen high peaks.",
    ),
    (
        "area.thera.tahril_mountains",
        TerrainTag::Mountain,
        &[TerrainTag::Desert],
        "CF Tahril Mountains: far-south rocky mountain spine rising from a dry riverbed.",
    ),
    (
        "area.thera.loch_terradian",
        TerrainTag::Water,
        &[TerrainTag::Mountain],
        "CF Loch Terradian: a mountain loch (water body).",
    ),
    // --- Desert (#1653). -------------------------------------------------------
    (
        "area.thera.desert_of_araile",
        TerrainTag::Desert,
        &[],
        "CF Desert of Araile.",
    ),
    (
        "area.thera.sands_of_sorrow",
        TerrainTag::Desert,
        &[],
        "CF Sands of Sorrow.",
    ),
    (
        "area.thera.wastes_of_nonviel",
        TerrainTag::Desert,
        &[],
        "CF Wastes of Nonviel.",
    ),
    (
        "area.thera.oryx_steppes",
        TerrainTag::Plains,
        &[TerrainTag::Desert],
        "CF Oryx/Western Steppes: dry-grass leveling country (arid grassland), not sand desert.",
    ),
    // --- Wetlands / swamps (#1654). --------------------------------------------
    (
        "area.thera.ysigrath_swamp",
        TerrainTag::Swamp,
        &[],
        "CF Ysigrath Swamp: night-hide flips.",
    ),
    (
        "area.thera.marsh_belt",
        TerrainTag::Swamp,
        &[],
        "CF Marsh Belt.",
    ),
    (
        "area.thera.blackwater",
        TerrainTag::Swamp,
        &[TerrainTag::Water],
        "CF Blackwater: fetid mire.",
    ),
    (
        "area.thera.seantryn_wetlands",
        TerrainTag::Swamp,
        &[],
        "CF Seantryn Wetlands.",
    ),
    (
        "area.thera.ysigrath",
        TerrainTag::Swamp,
        &[],
        "CF Ysigrath: two-tribe saurian mire homeland of reed-mounds (night-hide flips).",
    ),
    (
        "area.thera.loch_grynmear",
        TerrainTag::Water,
        &[TerrainTag::Swamp],
        "CF Loch Grynmear: a loch amid the wetlands.",
    ),
    (
        "area.thera.river_fords_of_the_spine",
        TerrainTag::Water,
        &[TerrainTag::Plains],
        "CF River Fords of the Spine: river crossings.",
    ),
    // --- Seas / oceans / coasts (#1655). ---------------------------------------
    (
        "area.thera.aryth_ocean",
        TerrainTag::Water,
        &[],
        "CF Aryth Ocean.",
    ),
    (
        "area.thera.dragon_sea",
        TerrainTag::Water,
        &[],
        "CF Dragon Sea.",
    ),
    (
        "area.thera.sea_of_despair",
        TerrainTag::Water,
        &[],
        "CF Sea of Despair.",
    ),
    (
        "area.thera.waters_of_the_deep",
        TerrainTag::Water,
        &[],
        "CF Waters of the Deep.",
    ),
    (
        "area.thera.coral_head",
        TerrainTag::Water,
        &[],
        "CF Coral Head: reef waters.",
    ),
    (
        "area.thera.crystal_island",
        TerrainTag::Water,
        &[TerrainTag::Plains],
        "CF Crystal Island: sea isle.",
    ),
    (
        "area.thera.kuo_toa_lair",
        TerrainTag::Water,
        &[],
        "CF Kuo-Toa Lair: an aquatic lair.",
    ),
    // --- Underdark (#1649). ----------------------------------------------------
    (
        "area.thera.cragstone",
        TerrainTag::Underground,
        &[],
        "CF Cragstone: dwarven under-hold gateway.",
    ),
    (
        "area.thera.underdark.gol_galath",
        TerrainTag::Underground,
        &[],
        "CF Gol-Galath underdark region.",
    ),
    (
        "area.thera.underdark.tunnels",
        TerrainTag::Underground,
        &[],
        "CF Underdark tunnels.",
    ),
    (
        "area.thera.underdark.arin_gird",
        TerrainTag::Underground,
        &[],
        "CF Arin-Gird underdark lair.",
    ),
    (
        "area.thera.underdark.svirfneblin_enclave",
        TerrainTag::Underground,
        &[],
        "CF Svirfneblin Enclave.",
    ),
    (
        "area.thera.underdark.troglodyte_caverns",
        TerrainTag::Underground,
        &[],
        "CF Troglodyte Caverns.",
    ),
    (
        "area.thera.underdark.velkyn_oloth",
        TerrainTag::Underground,
        &[],
        "CF Velkyn Oloth drow lair.",
    ),
    (
        "area.thera.xvart_lair",
        TerrainTag::Underground,
        &[TerrainTag::Plains],
        "CF Xvart Lair: a burrow/cave warren set in the low southwestern hills.",
    ),
    // --- Built manor/estate lairs (natural-kind but enclosed interiors). --------
    (
        "area.thera.mansion_of_vdramir",
        TerrainTag::Urban,
        &[TerrainTag::Underground],
        "CF Mansion of V-dramir: a built vampire-manor interior (enclosed foyer/ballroom) \
         over a crypt undercroft -- mapped to the enclosed built acoustic family.",
    ),
    // --- The Past (#1660): open naturals only; built pasts derive from kind. ----
    (
        "area.thera.past.past_grove",
        TerrainTag::Forest,
        &[],
        "CF Past Grove: era-earlier grove.",
    ),
    (
        "area.thera.past.plains_of_arendyl",
        TerrainTag::Plains,
        &[],
        "CF Plains of Arendyl (Past).",
    ),
    (
        "area.thera.past.the_battlefield",
        TerrainTag::Plains,
        &[],
        "CF The Battlefield (Past): open war ground.",
    ),
];

/// Look up the authored terrain for an open natural area, or `None` if it is not
/// yet transcribed (the coverage lint then names it as untagged).
pub(crate) fn authored_natural_terrain(area_id: &str) -> Option<AuthoredTerrain> {
    NATURAL_TERRAIN
        .iter()
        .find(|(id, _, _, _)| *id == area_id)
        .map(|(_, terrain, secondary, note)| AuthoredTerrain {
            terrain: *terrain,
            secondary,
            note,
        })
}
