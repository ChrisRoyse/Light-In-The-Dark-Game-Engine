// #1656 world/region: rivers & lochs, part B of the swamps+rivers file group
// (Blackwater, Loch Grynmear, River Fords of the Spine). Split from
// regions_thera_fixture_wetlands.rs (part A: Ysigrath Swamp, Marsh Belt,
// Seantryn Wetlands, the #1654 half) to respect the <500-line ratchet
// (#1466/#800 sec.6) after rustfmt expansion. Design constants cited: fords/
// bridges as PK-geography chokepoints (#955), streams break track trails
// (#1620), mud banks for smear mud (#1613), shallow water = no-sneak splash
// terrain (#1629).

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RoomConcealment,
    RoomKind, TravelKind,
};

use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const BLACKWATER_AREA_ID: &str = "area.thera.blackwater";
pub(crate) const LOCH_GRYNMEAR_AREA_ID: &str = "area.thera.loch_grynmear";
pub(crate) const RIVER_FORDS_OF_THE_SPINE_AREA_ID: &str = "area.thera.river_fords_of_the_spine";

pub(crate) fn wetlands_b_area_ids() -> [&'static str; 3] {
    [
        BLACKWATER_AREA_ID,
        LOCH_GRYNMEAR_AREA_ID,
        RIVER_FORDS_OF_THE_SPINE_AREA_ID,
    ]
}

pub(crate) fn blackwater_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: BLACKWATER_AREA_ID,
        display_name: "Blackwater",
        area_kind: AreaKind::Wilderness,
        source_issue: 1654,
        canon_issues: vec![987, 1598, 1632, 1654],
        wiki_slug: "blackwater",
        source_history_label: "CF Blackwater factual topology and mechanics only",
        summary: "The northwest interior bog on the map spine.",
        level_band: (20, 40),
        rooms: vec![
            room(
                "room.blackwater.bog",
                "Blackwater Bog",
                RoomKind::Wilderness,
                "Dark still water hiding a treacherous bog floor.",
                RoomConcealment::None,
                &["npc.blackwater.bog_lurker"],
            ),
            room(
                "room.blackwater.sunken_grove",
                "Sunken Grove",
                RoomKind::Landmark,
                "A half-drowned grove of dead trees.",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.blackwater.bog_to_grove",
                "room.blackwater.bog",
                BLACKWATER_AREA_ID,
                Some("room.blackwater.sunken_grove"),
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.blackwater.to_galadon",
                "room.blackwater.bog",
                GALADON_AREA_ID,
                None,
                ConnectionDirection::Northwest,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.blackwater.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.blackwater.population",
        repop_budget: 20,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.blackwater",
        model_id: "model.wilderness.blackwater",
        props: vec![
            "prop.swamp.black_still_water",
            "prop.swamp.dead_tree_stand",
            "prop.swamp.mud_pool",
        ],
        silhouette: "dark still bog water threading a drowned dead-tree grove",
        material_notes: "Original Thera-Blackwater dressing; no copied iconography.",
        notes: vec![
            "Mud sources for smear mud (#1613); water channels break track trails (#1620).",
        ],
    })
}

pub(crate) fn loch_grynmear_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: LOCH_GRYNMEAR_AREA_ID,
        display_name: "Loch Grynmear",
        area_kind: AreaKind::Region,
        source_issue: 1656,
        canon_issues: vec![987, 1598, 1632, 1656, 1637],
        wiki_slug: "loch-grynmear",
        source_history_label: "CF Loch Grynmear factual topology and mechanics only",
        summary: "The loch with its Temple (#1637) reached via the Silverpine Road east of Voralian; \
            this issue claims its open-water body.",
        level_band: (10, 30),
        rooms: vec![
            room(
                "room.loch_grynmear.shore",
                "Grynmear Shore",
                RoomKind::Wilderness,
                "A grassy shore along the loch.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.loch_grynmear.open_water",
                "Loch Grynmear Open Water",
                RoomKind::Wilderness,
                "The loch's calm open water, temple spire visible on its far shore.",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.loch_grynmear.shore_to_water",
                "room.loch_grynmear.shore",
                LOCH_GRYNMEAR_AREA_ID,
                Some("room.loch_grynmear.open_water"),
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.loch_grynmear.to_galadon",
                "room.loch_grynmear.shore",
                GALADON_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.loch_grynmear.to_river_fords",
                "room.loch_grynmear.open_water",
                RIVER_FORDS_OF_THE_SPINE_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.loch_grynmear.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.loch_grynmear.population",
        repop_budget: 16,
        repop_minutes: 15,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.loch_grynmear",
        model_id: "model.wilderness.loch_grynmear",
        props: vec![
            "prop.water.calm_loch_surface",
            "prop.water.grassy_shoreline",
            "prop.water.temple_spire_view",
        ],
        silhouette: "calm loch with a temple spire visible across the water",
        material_notes: "Original Thera-Loch Grynmear dressing; no copied iconography.",
        notes: vec!["Create spring (druid L11) interactions apply on this shore."],
    })
}

pub(crate) fn river_fords_of_the_spine_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: RIVER_FORDS_OF_THE_SPINE_AREA_ID,
        display_name: "River Fords of the Spine",
        area_kind: AreaKind::Region,
        source_issue: 1656,
        canon_issues: vec![987, 1598, 1632, 1656, 955],
        wiki_slug: "river-fords-of-the-spine",
        source_history_label: "CF river/ford factual topology and mechanics only",
        summary: "Rivers crossing the Galadon-Voralian-Arkham spine, with fords and bridges as \
            chokepoints (#955 PK hotspots).",
        level_band: (1, 51),
        rooms: vec![
            room(
                "room.river_fords_of_the_spine.shallow_ford",
                "Shallow Ford",
                RoomKind::Landmark,
                "A shallow-water ford; splashing gives away a no-sneak crosser (#1629).",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.river_fords_of_the_spine.stone_bridge",
                "Stone Bridge",
                RoomKind::Landmark,
                "A stone bridge chokepoint over the river (#955).",
                RoomConcealment::GateChokepoint,
                &["npc.river_fords_of_the_spine.toll_watcher"],
            ),
        ],
        connections: vec![
            conn(
                "conn.river_fords_of_the_spine.ford_to_bridge",
                "room.river_fords_of_the_spine.shallow_ford",
                RIVER_FORDS_OF_THE_SPINE_AREA_ID,
                Some("room.river_fords_of_the_spine.stone_bridge"),
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.river_fords_of_the_spine.to_loch_grynmear",
                "room.river_fords_of_the_spine.shallow_ford",
                LOCH_GRYNMEAR_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.river_fords_of_the_spine.cycle",
        reset_minutes: 25,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.river_fords_of_the_spine.population",
        repop_budget: 14,
        repop_minutes: 12,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.river_fords_of_the_spine",
        model_id: "model.wilderness.river_fords_of_the_spine",
        props: vec![
            "prop.water.shallow_ford_stones",
            "prop.water.stone_bridge_span",
            "prop.water.mud_bank",
        ],
        silhouette: "shallow river ford beside a stone bridge chokepoint",
        material_notes: "Original Thera-river-ford dressing; no copied iconography.",
        notes: vec![
            "Streams break track trails (#1620); mud banks for smear mud (#1613); shallow \
            water is no-sneak splash terrain (#1629).",
        ],
    })
}

pub(crate) fn all_wetlands_b_specs() -> Vec<AreaSpec> {
    vec![
        blackwater_area_spec(),
        loch_grynmear_area_spec(),
        river_fords_of_the_spine_area_spec(),
    ]
}
