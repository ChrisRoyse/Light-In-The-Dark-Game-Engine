// #1651 world/region: plains, hills & fields, part A (Udgaardian Plains,
// Nanthor, Open Plains). Split from a single file to respect the <500-line
// ratchet (#1466/#800 sec.6) after rustfmt expansion; part B (Drogran Hills,
// Northern Foothills, Fields of Balatorn) lives in
// regions_thera_fixture_plains_b.rs. Plainsrunner country (#1612). Design
// constants cited: long sightlines = exposed travel; tall-grass camouflage
// patches are the ONLY concealment (field terrain is camouflage-legal per
// helpfile 467); plainsrunning/pursuit movement bonuses; bramble pit ability
// terrain (#1612).

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RoomConcealment,
    RoomKind, TravelKind,
};

use super::regions_thera_fixture_plains_b::{DROGRAN_HILLS_AREA_ID, FIELDS_OF_BALATORN_AREA_ID};
use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const UDGAARDIAN_PLAINS_AREA_ID: &str = "area.thera.udgaardian_plains";
pub(crate) const NANTHOR_AREA_ID: &str = "area.thera.nanthor";
pub(crate) const OPEN_PLAINS_AREA_ID: &str = "area.thera.open_plains";

pub(crate) fn plains_a_area_ids() -> [&'static str; 3] {
    [
        UDGAARDIAN_PLAINS_AREA_ID,
        NANTHOR_AREA_ID,
        OPEN_PLAINS_AREA_ID,
    ]
}

pub(crate) fn udgaardian_plains_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: UDGAARDIAN_PLAINS_AREA_ID,
        display_name: "Udgaardian Plains",
        area_kind: AreaKind::Wilderness,
        source_issue: 1651,
        canon_issues: vec![987, 1598, 1632, 1651, 1612],
        wiki_slug: "udgaardian-plains",
        source_history_label: "CF Udgaardian Plains (Plains of Udgaard) factual topology and \
            mechanics only",
        summary: "The newbie plains north of Galadon, beyond the Crossroads and Inn of the Eternal \
            Star; onehearts and seers rank 15-20 evil.",
        level_band: (1, 20),
        rooms: vec![
            room(
                "room.udgaardian_plains.tall_grass",
                "Tall Grass Reach",
                RoomKind::Wilderness,
                "Rolling grassland where tall-grass patches are the only cover.",
                RoomConcealment::None,
                &["npc.udgaardian_plains.oneheart"],
            ),
            room(
                "room.udgaardian_plains.seer_camp",
                "Seer's Camp",
                RoomKind::Landmark,
                "A wandering seer's camp pitched in the open plains.",
                RoomConcealment::None,
                &["npc.udgaardian_plains.seer"],
            ),
        ],
        connections: vec![
            conn(
                "conn.udgaardian_plains.grass_to_camp",
                "room.udgaardian_plains.tall_grass",
                UDGAARDIAN_PLAINS_AREA_ID,
                Some("room.udgaardian_plains.seer_camp"),
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.udgaardian_plains.to_galadon",
                "room.udgaardian_plains.tall_grass",
                GALADON_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.udgaardian_plains.to_nanthor",
                "room.udgaardian_plains.seer_camp",
                NANTHOR_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.udgaardian_plains.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.udgaardian_plains.population",
        repop_budget: 20,
        repop_minutes: 15,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.udgaardian_plains",
        model_id: "model.wilderness.udgaardian_plains",
        props: vec![
            "prop.plains.tall_grass_patch",
            "prop.plains.wandering_camp",
            "prop.plains.worn_path",
        ],
        silhouette: "open rolling newbie plains under a wide sky",
        material_notes: "Original Thera-Udgaard plains dressing; no copied iconography.",
        notes: vec![
            "Long sightlines mean exposed travel; tall-grass patches are the only legal \
            concealment on field terrain (helpfile 467).",
        ],
    })
}

pub(crate) fn nanthor_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: NANTHOR_AREA_ID,
        display_name: "Nanthor",
        area_kind: AreaKind::Region,
        source_issue: 1651,
        canon_issues: vec![987, 1598, 1632, 1651],
        wiki_slug: "nanthor",
        source_history_label: "CF Nanthor factual topology and mechanics only",
        summary: "A map block on the northern plains beyond Udgaard; source gives no explicit CF \
            level range, so the band below is estimated from its neighbors, not a cited number.",
        level_band: (1, 20),
        rooms: vec![
            room(
                "room.nanthor.grass_flat",
                "Nanthor Flat",
                RoomKind::Wilderness,
                "A broad grass flat on the northern plains block.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.nanthor.cairn",
                "Nanthor Cairn",
                RoomKind::Landmark,
                "An old stone cairn marking the block's edge.",
                RoomConcealment::None,
                &["npc.nanthor.plains_wanderer"],
            ),
        ],
        connections: vec![
            conn(
                "conn.nanthor.flat_to_cairn",
                "room.nanthor.grass_flat",
                NANTHOR_AREA_ID,
                Some("room.nanthor.cairn"),
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.nanthor.to_udgaardian_plains",
                "room.nanthor.grass_flat",
                UDGAARDIAN_PLAINS_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.nanthor.to_open_plains",
                "room.nanthor.cairn",
                OPEN_PLAINS_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.nanthor.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.nanthor.population",
        repop_budget: 16,
        repop_minutes: 15,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.nanthor",
        model_id: "model.wilderness.nanthor",
        props: vec![
            "prop.plains.stone_cairn",
            "prop.plains.tall_grass_patch",
            "prop.plains.worn_path",
        ],
        silhouette: "broad grass flat marked by a single weathered cairn",
        material_notes: "Original Thera-Nanthor plains dressing; no copied iconography.",
        notes: vec![
            "Camouflage on field terrain is limited to tall-grass patches per helpfile 467.",
        ],
    })
}

pub(crate) fn open_plains_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: OPEN_PLAINS_AREA_ID,
        display_name: "The Open Plains",
        area_kind: AreaKind::Region,
        source_issue: 1651,
        canon_issues: vec![987, 1598, 1632, 1651, 1612],
        wiki_slug: "open-plains",
        source_history_label: "CF Open Plains factual topology and mechanics only",
        summary: "1-51 traverse country east of the central spine; a wide-open plainsrunner corridor.",
        level_band: (1, 51),
        rooms: vec![
            room(
                "room.open_plains.crossing",
                "Long Crossing",
                RoomKind::Wilderness,
                "A long open crossing with plainsrunning movement bonuses (#1612).",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.open_plains.bramble_pit",
                "Bramble Pit",
                RoomKind::Landmark,
                "A bramble-pit ability terrain feature (#1612).",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.open_plains.crossing_to_pit",
                "room.open_plains.crossing",
                OPEN_PLAINS_AREA_ID,
                Some("room.open_plains.bramble_pit"),
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.open_plains.to_nanthor",
                "room.open_plains.crossing",
                NANTHOR_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.open_plains.to_drogran_hills",
                "room.open_plains.bramble_pit",
                DROGRAN_HILLS_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.open_plains.to_fields_of_balatorn",
                "room.open_plains.crossing",
                FIELDS_OF_BALATORN_AREA_ID,
                None,
                ConnectionDirection::Southeast,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.open_plains.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.open_plains.population",
        repop_budget: 28,
        repop_minutes: 15,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.open_plains",
        model_id: "model.wilderness.open_plains",
        props: vec![
            "prop.plains.bramble_pit",
            "prop.plains.tall_grass_patch",
            "prop.plains.horizon_marker",
        ],
        silhouette: "vast open traverse plains with a single bramble-pit landmark",
        material_notes: "Original Thera-Open Plains dressing; no copied iconography.",
        notes: vec![
            "Plainsrunning/pursuit movement bonuses and bramble pit ability terrain per #1612.",
        ],
    })
}

pub(crate) fn all_plains_a_specs() -> Vec<AreaSpec> {
    vec![
        udgaardian_plains_area_spec(),
        nanthor_area_spec(),
        open_plains_area_spec(),
    ]
}
