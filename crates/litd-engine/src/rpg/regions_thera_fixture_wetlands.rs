// #1654 world/region: swamps & wetlands, part A (Ysigrath Swamp, Marsh Belt,
// Seantryn Wetlands). Split from a single file to respect the <500-line
// ratchet (#1466/#800 sec.6) after rustfmt expansion; part B (Blackwater, Loch
// Grynmear, River Fords of the Spine -- the #1656 rivers & lochs half) lives
// in regions_thera_fixture_wetlands_b.rs. Marshdweller country (#1612).
// Design constants cited: night-hide terrain for orc berserkers/goblin
// raiders per helpfile 503, mud sources for smear mud (#1613), leech/mosquito
// hazard swarms, water channels that break track trails (#1620), draw breath
// for submerged hiding.

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RoomConcealment,
    RoomKind, TravelKind,
};

use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const YSIGRATH_SWAMP_AREA_ID: &str = "area.thera.ysigrath_swamp";
pub(crate) const MARSH_BELT_AREA_ID: &str = "area.thera.marsh_belt";
pub(crate) const SEANTRYN_WETLANDS_AREA_ID: &str = "area.thera.seantryn_wetlands";

pub(crate) fn wetlands_a_area_ids() -> [&'static str; 3] {
    [
        YSIGRATH_SWAMP_AREA_ID,
        MARSH_BELT_AREA_ID,
        SEANTRYN_WETLANDS_AREA_ID,
    ]
}

pub(crate) fn ysigrath_swamp_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: YSIGRATH_SWAMP_AREA_ID,
        display_name: "Ysigrath Swamp",
        area_kind: AreaKind::Wilderness,
        source_issue: 1654,
        canon_issues: vec![987, 1598, 1632, 1654, 1612, 1647],
        wiki_slug: "ysigrath-swamp",
        source_history_label: "CF Ysigrath swamp complex factual topology and mechanics only",
        summary: "The southwest swamp complex itself; the saurian tribe villages within it are #1647's \
            scope, not this issue's.",
        level_band: (15, 35),
        rooms: vec![
            room(
                "room.ysigrath_swamp.mudflat",
                "Ysigrath Mudflat",
                RoomKind::Wilderness,
                "A sucking mudflat, a source for smear mud (#1613).",
                RoomConcealment::None,
                &["npc.ysigrath_swamp.leech_swarm"],
            ),
            room(
                "room.ysigrath_swamp.mire_channel",
                "Mire Channel",
                RoomKind::Landmark,
                "A slow water channel that breaks track trails (#1620).",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.ysigrath_swamp.mudflat_to_channel",
                "room.ysigrath_swamp.mudflat",
                YSIGRATH_SWAMP_AREA_ID,
                Some("room.ysigrath_swamp.mire_channel"),
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.ysigrath_swamp.to_galadon",
                "room.ysigrath_swamp.mudflat",
                GALADON_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.ysigrath_swamp.to_marsh_belt",
                "room.ysigrath_swamp.mire_channel",
                MARSH_BELT_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.ysigrath_swamp.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.ysigrath_swamp.population",
        repop_budget: 20,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.ysigrath_swamp",
        model_id: "model.wilderness.ysigrath_swamp",
        props: vec![
            "prop.swamp.mud_pool",
            "prop.swamp.reed_stand",
            "prop.swamp.sunken_log",
        ],
        silhouette: "sucking mudflat swamp threaded by slow mire channels",
        material_notes: "Original Thera-Ysigrath swamp dressing; no copied iconography.",
        notes: vec![
            "Night-hide terrain for orc berserkers (L25+) and goblin raiders (L30+) per \
            helpfile 503; leech/mosquito hazard swarms present.",
        ],
    })
}

pub(crate) fn marsh_belt_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: MARSH_BELT_AREA_ID,
        display_name: "Marsh Belt",
        area_kind: AreaKind::Region,
        source_issue: 1654,
        canon_issues: vec![987, 1598, 1632, 1654],
        wiki_slug: "marsh-belt",
        source_history_label: "CF marsh-belt factual topology and mechanics only",
        summary: "The marsh belts along the map's river and loch systems (the ~ bands), connecting the \
            Ysigrath swamp complex toward the Seantryn Wetlands.",
        level_band: (5, 25),
        rooms: vec![
            room(
                "room.marsh_belt.reed_bank",
                "Reed Bank",
                RoomKind::Wilderness,
                "A shallow reed bank along the river fringe.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.marsh_belt.submerged_hide",
                "Submerged Hide",
                RoomKind::Landmark,
                "A spot where marshdwellers and mariners draw breath to hide under the waterline.",
                RoomConcealment::HiddenLayer,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.marsh_belt.bank_to_hide",
                "room.marsh_belt.reed_bank",
                MARSH_BELT_AREA_ID,
                Some("room.marsh_belt.submerged_hide"),
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.marsh_belt.to_ysigrath_swamp",
                "room.marsh_belt.reed_bank",
                YSIGRATH_SWAMP_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.marsh_belt.to_seantryn_wetlands",
                "room.marsh_belt.reed_bank",
                SEANTRYN_WETLANDS_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.marsh_belt.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.marsh_belt.population",
        repop_budget: 16,
        repop_minutes: 15,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.marsh_belt",
        model_id: "model.wilderness.marsh_belt",
        props: vec![
            "prop.swamp.reed_stand",
            "prop.swamp.waterline_hide_spot",
            "prop.swamp.mud_pool",
        ],
        silhouette: "shallow reed-banked marsh belt threading the river and loch systems",
        material_notes: "Original Thera-marsh-belt dressing; no copied iconography.",
        notes: vec![
            "Draw breath for submerged hiding is a marshdweller/mariner verb; the hide spot is \
            Hidden (#944).",
        ],
    })
}

pub(crate) fn seantryn_wetlands_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: SEANTRYN_WETLANDS_AREA_ID,
        display_name: "Seantryn Wetlands",
        area_kind: AreaKind::Wilderness,
        source_issue: 1654,
        canon_issues: vec![987, 1598, 1632, 1654],
        wiki_slug: "seantryn-wetlands",
        source_history_label: "CF Seantryn Wetlands factual topology and mechanics only",
        summary: "The southeast coastal marsh, near Seantryn Modan.",
        level_band: (10, 30),
        rooms: vec![
            room(
                "room.seantryn_wetlands.tidal_flat",
                "Tidal Flat",
                RoomKind::Wilderness,
                "A brackish tidal flat where the marsh meets the coast.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.seantryn_wetlands.reed_maze",
                "Reed Maze",
                RoomKind::Landmark,
                "A maze of tall reeds good for night-hide (helpfile 503).",
                RoomConcealment::None,
                &["npc.seantryn_wetlands.goblin_raider"],
            ),
        ],
        connections: vec![
            conn(
                "conn.seantryn_wetlands.flat_to_maze",
                "room.seantryn_wetlands.tidal_flat",
                SEANTRYN_WETLANDS_AREA_ID,
                Some("room.seantryn_wetlands.reed_maze"),
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.seantryn_wetlands.to_marsh_belt",
                "room.seantryn_wetlands.tidal_flat",
                MARSH_BELT_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.seantryn_wetlands.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.seantryn_wetlands.population",
        repop_budget: 18,
        repop_minutes: 15,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.seantryn_wetlands",
        model_id: "model.wilderness.seantryn_wetlands",
        props: vec![
            "prop.swamp.tidal_flat",
            "prop.swamp.reed_stand",
            "prop.swamp.sunken_log",
        ],
        silhouette: "brackish tidal marsh giving way to a tall reed maze",
        material_notes: "Original Thera-Seantryn dressing; no copied iconography.",
        notes: vec![
            "Night-hide terrain for goblin raiders (L30+) per helpfile 503; movement-cost \
            punishment for non-marshdwellers.",
        ],
    })
}

pub(crate) fn all_wetlands_a_specs() -> Vec<AreaSpec> {
    vec![
        ysigrath_swamp_area_spec(),
        marsh_belt_area_spec(),
        seantryn_wetlands_area_spec(),
    ]
}
