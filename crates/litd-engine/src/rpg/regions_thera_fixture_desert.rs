// #1653 world/region: desert & far south, part A (Sands of Sorrow, Desert of
// Araile, Pyramid of Azhan, Oryx Steppes). Split from a single file to respect
// the <500-line ratchet (#1466/#800 sec.6) after rustfmt expansion; part B
// (Tahril Mountains, Wastes of Nonviel, Xvart Lair) lives in
// regions_thera_fixture_desert_b.rs. Bedouin country (#1612) south of Hamsah
// Mu'tazz. Design constants cited: heat survival (endure heat), sandstorms as
// vision/audio maskers (#848 weather), mirage shimmer at range, no water =
// find-water/stash gameplay (#953), open dunes exposed by day / cold clear
// nights.

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, RoomConcealment, RoomKind,
    TravelKind,
};

use super::regions_thera_fixture_desert_b::{TAHRIL_MOUNTAINS_AREA_ID, XVART_LAIR_AREA_ID};
use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

/// Seaport city named on the #1632 map ("HAMSAH MU-TAZZ (Seaport east of
/// Southern Market)"). Authored by the #1637-1643 city batch as
/// `area.thera.hamsah_mu_tazz` (the mu'tazz apostrophe rendered as an
/// underscore, matching that batch's id). The southern desert road exits north
/// into this city, so the id MUST match the authored spec verbatim -- an earlier
/// `hamsah_mutazz` spelling was a real dangling cross-area edge caught by the
/// #1704 registry.
pub(crate) const HAMSAH_MUTAZZ_AREA_ID: &str = "area.thera.hamsah_mu_tazz";
pub(crate) const SANDS_OF_SORROW_AREA_ID: &str = "area.thera.sands_of_sorrow";
pub(crate) const DESERT_OF_ARAILE_AREA_ID: &str = "area.thera.desert_of_araile";
pub(crate) const PYRAMID_OF_AZHAN_AREA_ID: &str = "area.thera.pyramid_of_azhan";
pub(crate) const ORYX_STEPPES_AREA_ID: &str = "area.thera.oryx_steppes";

pub(crate) fn desert_a_area_ids() -> [&'static str; 5] {
    [
        HAMSAH_MUTAZZ_AREA_ID,
        SANDS_OF_SORROW_AREA_ID,
        DESERT_OF_ARAILE_AREA_ID,
        PYRAMID_OF_AZHAN_AREA_ID,
        ORYX_STEPPES_AREA_ID,
    ]
}

pub(crate) fn sands_of_sorrow_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: SANDS_OF_SORROW_AREA_ID,
        display_name: "Sands of Sorrow",
        area_kind: AreaKind::Wilderness,
        source_issue: 1653,
        canon_issues: vec![987, 1598, 1632, 1653, 1612],
        wiki_slug: "sands-of-sorrow",
        source_history_label: "CF Sands of Sorrow factual topology and mechanics only",
        summary: "One of the twin deserts on the southern road out of Hamsah Mu'tazz.",
        level_band: (10, 20),
        rooms: vec![
            room(
                "room.sands_of_sorrow.dune_field",
                "Dune Field",
                RoomKind::Wilderness,
                "Rolling dunes shimmering with mirage at range.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.sands_of_sorrow.oasis_mirage",
                "Oasis Mirage",
                RoomKind::Landmark,
                "A false-water shimmer that vanishes on approach.",
                RoomConcealment::None,
                &["npc.sands_of_sorrow.sand_raider"],
            ),
        ],
        connections: vec![
            conn(
                "conn.sands_of_sorrow.dune_to_mirage",
                "room.sands_of_sorrow.dune_field",
                SANDS_OF_SORROW_AREA_ID,
                Some("room.sands_of_sorrow.oasis_mirage"),
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.sands_of_sorrow.to_hamsah_mutazz",
                "room.sands_of_sorrow.dune_field",
                HAMSAH_MUTAZZ_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.sands_of_sorrow.to_desert_of_araile",
                "room.sands_of_sorrow.oasis_mirage",
                DESERT_OF_ARAILE_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.sands_of_sorrow.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.sands_of_sorrow.population",
        repop_budget: 18,
        repop_minutes: 18,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.sands_of_sorrow",
        model_id: "model.wilderness.sands_of_sorrow",
        props: vec![
            "prop.desert.dune_ridge",
            "prop.desert.mirage_shimmer",
            "prop.desert.sun_bleached_bone",
        ],
        silhouette: "rolling sun-bleached dunes shimmering with a false-water mirage",
        material_notes: "Original Thera-Sands of Sorrow dressing; no copied iconography.",
        notes: vec![
            "Heat survival (endure heat) and mirage shimmer at range apply; no water = \
            find-water/stash gameplay (#953).",
        ],
    })
}

pub(crate) fn desert_of_araile_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: DESERT_OF_ARAILE_AREA_ID,
        display_name: "Desert of Araile",
        area_kind: AreaKind::Wilderness,
        source_issue: 1653,
        canon_issues: vec![987, 1598, 1632, 1653],
        wiki_slug: "desert-of-araile",
        source_history_label: "CF Desert of Araile factual topology and mechanics only",
        summary: "The second of the twin deserts on the southern road.",
        level_band: (5, 25),
        rooms: vec![
            room(
                "room.desert_of_araile.dune_crossing",
                "Araile Dune Crossing",
                RoomKind::Wilderness,
                "A wide sandy crossing, exposed by day, cold and clear by night.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.desert_of_araile.buried_stones",
                "Buried Stones",
                RoomKind::Landmark,
                "Half-buried stonework hinting at older ruins beneath the sand.",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.desert_of_araile.crossing_to_stones",
                "room.desert_of_araile.dune_crossing",
                DESERT_OF_ARAILE_AREA_ID,
                Some("room.desert_of_araile.buried_stones"),
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.desert_of_araile.to_sands_of_sorrow",
                "room.desert_of_araile.dune_crossing",
                SANDS_OF_SORROW_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.desert_of_araile.to_pyramid_of_azhan",
                "room.desert_of_araile.buried_stones",
                PYRAMID_OF_AZHAN_AREA_ID,
                None,
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.desert_of_araile.to_oryx_steppes",
                "room.desert_of_araile.dune_crossing",
                ORYX_STEPPES_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.desert_of_araile.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.desert_of_araile.population",
        repop_budget: 18,
        repop_minutes: 18,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.desert_of_araile",
        model_id: "model.wilderness.desert_of_araile",
        props: vec![
            "prop.desert.dune_ridge",
            "prop.desert.buried_stonework",
            "prop.desert.sun_bleached_bone",
        ],
        silhouette: "wide dune crossing over half-buried older stonework",
        material_notes: "Original Thera-Araile dressing; no copied iconography.",
        notes: vec![
            "Sandstorms mask vision/audio (#848); the pyramid entrance is Hidden under the \
            sand (#944).",
        ],
    })
}

pub(crate) fn pyramid_of_azhan_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: PYRAMID_OF_AZHAN_AREA_ID,
        display_name: "Pyramid of Azhan",
        area_kind: AreaKind::Dungeon,
        source_issue: 1653,
        canon_issues: vec![987, 1598, 1632, 1653],
        wiki_slug: "pyramid-of-azhan",
        source_history_label: "CF Pyramid of Azhan factual topology and mechanics only",
        summary: "A tomb dungeon buried under the dunes.",
        level_band: (25, 45),
        rooms: vec![
            room(
                "room.pyramid_of_azhan.sand_choked_entry",
                "Sand-Choked Entry",
                RoomKind::Gate,
                "A stairway down into buried tomb passages.",
                RoomConcealment::GateChokepoint,
                &["npc.pyramid_of_azhan.tomb_guardian"],
            ),
            room(
                "room.pyramid_of_azhan.burial_hall",
                "Burial Hall",
                RoomKind::Chamber,
                "A long hall of sealed sarcophagi.",
                RoomConcealment::None,
                &["npc.pyramid_of_azhan.mummy"],
            ),
            room(
                "room.pyramid_of_azhan.inner_sanctum",
                "Inner Sanctum",
                RoomKind::Chamber,
                "The pyramid's sealed inner sanctum.",
                RoomConcealment::HiddenLayer,
                &["npc.pyramid_of_azhan.azhan_guardian"],
            ),
        ],
        connections: vec![
            conn(
                "conn.pyramid_of_azhan.entry_to_hall",
                "room.pyramid_of_azhan.sand_choked_entry",
                PYRAMID_OF_AZHAN_AREA_ID,
                Some("room.pyramid_of_azhan.burial_hall"),
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.pyramid_of_azhan.hall_to_sanctum",
                "room.pyramid_of_azhan.burial_hall",
                PYRAMID_OF_AZHAN_AREA_ID,
                Some("room.pyramid_of_azhan.inner_sanctum"),
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.pyramid_of_azhan.to_desert_of_araile",
                "room.pyramid_of_azhan.sand_choked_entry",
                DESERT_OF_ARAILE_AREA_ID,
                None,
                ConnectionDirection::Up,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
        ],
        reset_id: "reset.pyramid_of_azhan.cycle",
        reset_minutes: 45,
        clears_corpses: true,
        resets_doors: true,
        repop_id: "repop.pyramid_of_azhan.population",
        repop_budget: 16,
        repop_minutes: 20,
        pressure_tier: 3,
        tileset_id: "tileset.dungeon.pyramid_of_azhan",
        model_id: "model.dungeon.pyramid_of_azhan",
        props: vec![
            "prop.desert.sarcophagus_row",
            "prop.desert.buried_obelisk",
            "prop.desert.sand_choked_stair",
        ],
        silhouette: "sand-buried stepped pyramid tomb sealed beneath the dunes",
        material_notes: "Original Thera-Azhan tomb dressing; no copied iconography.",
        notes: vec!["Doors and locks reset here (tomb seals); no built-in map (#944)."],
    })
}

pub(crate) fn oryx_steppes_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: ORYX_STEPPES_AREA_ID,
        display_name: "Oryx Steppes",
        area_kind: AreaKind::Region,
        source_issue: 1653,
        canon_issues: vec![987, 1598, 1632, 1653],
        wiki_slug: "oryx-steppes",
        source_history_label: "CF Oryx Steppes (Western Steppes) factual topology and mechanics only",
        summary: "Dry-grass leveling country, also mapped as the Western Steppes band.",
        level_band: (20, 51),
        rooms: vec![
            room(
                "room.oryx_steppes.dry_grass_flat",
                "Dry Grass Flat",
                RoomKind::Wilderness,
                "A wide dry-grass leveling ground.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.oryx_steppes.steppe_camp",
                "Steppe Camp",
                RoomKind::Landmark,
                "A nomad waycamp on the steppes.",
                RoomConcealment::None,
                &["npc.oryx_steppes.steppe_rider"],
            ),
        ],
        connections: vec![
            conn(
                "conn.oryx_steppes.flat_to_camp",
                "room.oryx_steppes.dry_grass_flat",
                ORYX_STEPPES_AREA_ID,
                Some("room.oryx_steppes.steppe_camp"),
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.oryx_steppes.to_desert_of_araile",
                "room.oryx_steppes.dry_grass_flat",
                DESERT_OF_ARAILE_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.oryx_steppes.to_tahril_mountains",
                "room.oryx_steppes.steppe_camp",
                TAHRIL_MOUNTAINS_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.oryx_steppes.to_xvart_lair",
                "room.oryx_steppes.dry_grass_flat",
                XVART_LAIR_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
        ],
        reset_id: "reset.oryx_steppes.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.oryx_steppes.population",
        repop_budget: 24,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.oryx_steppes",
        model_id: "model.wilderness.oryx_steppes",
        props: vec![
            "prop.desert.dry_grass_tuft",
            "prop.desert.nomad_waycamp",
            "prop.desert.wind_worn_marker",
        ],
        silhouette: "dry grass steppe leveling country dotted with nomad waycamps",
        material_notes: "Original Thera-Oryx Steppes dressing; no copied iconography.",
        notes: vec![
            "The Xvart Lair approach is a ConcealedTrail (#944), a low-hills warren off the \
            steppe.",
        ],
    })
}

pub(crate) fn all_desert_a_specs() -> Vec<AreaSpec> {
    vec![
        sands_of_sorrow_area_spec(),
        desert_of_araile_area_spec(),
        pyramid_of_azhan_area_spec(),
        oryx_steppes_area_spec(),
    ]
}
