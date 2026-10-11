// #1650 world/region: western forests, part B (last 4 of 9 named woods).
// Design constants cited: camouflage-legal throughout, waylay corridors along
// road edges (#1629), treeform biome variety (#1614), herb/forage tables (#1613).

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RoomConcealment,
    RoomKind, TravelKind,
};

use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const DRANETTIE_WOOD_AREA_ID: &str = "area.thera.dranettie_wood";
pub(crate) const SPIDERHAUNT_WOODS_AREA_ID: &str = "area.thera.spiderhaunt_woods";
pub(crate) const VIOLET_WOODLAND_AREA_ID: &str = "area.thera.violet_woodland";
pub(crate) const EMERALD_FOREST_AREA_ID: &str = "area.thera.emerald_forest";

pub(crate) fn forests_b_area_ids() -> [&'static str; 4] {
    [
        DRANETTIE_WOOD_AREA_ID,
        SPIDERHAUNT_WOODS_AREA_ID,
        VIOLET_WOODLAND_AREA_ID,
        EMERALD_FOREST_AREA_ID,
    ]
}

pub(crate) fn dranettie_wood_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: DRANETTIE_WOOD_AREA_ID,
        display_name: "Dranettie Wood",
        area_kind: AreaKind::Wilderness,
        source_issue: 1650,
        canon_issues: vec![987, 1598, 1632, 1650],
        wiki_slug: "dranettie-wood",
        source_history_label: "CF Dranettie Wood factual topology and mechanics only",
        summary: "A wood of owlbears (good ranking) and dranetties (evil ranking).",
        level_band: (30, 35),
        rooms: vec![
            room(
                "room.dranettie_wood.owlbear_glen",
                "Owlbear Glen",
                RoomKind::Wilderness,
                "A glen favored by owlbears.",
                RoomConcealment::None,
                &["npc.dranettie_wood.owlbear"],
            ),
            room(
                "room.dranettie_wood.dranettie_thicket",
                "Dranettie Thicket",
                RoomKind::Chamber,
                "A dense thicket where dranetties lurk.",
                RoomConcealment::None,
                &["npc.dranettie_wood.dranettie"],
            ),
        ],
        connections: vec![
            conn(
                "conn.dranettie_wood.glen_to_thicket",
                "room.dranettie_wood.owlbear_glen",
                DRANETTIE_WOOD_AREA_ID,
                Some("room.dranettie_wood.dranettie_thicket"),
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.dranettie_wood.to_galadon",
                "room.dranettie_wood.owlbear_glen",
                GALADON_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.dranettie_wood.to_spiderhaunt_woods",
                "room.dranettie_wood.dranettie_thicket",
                SPIDERHAUNT_WOODS_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.dranettie_wood.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.dranettie_wood.population",
        repop_budget: 22,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.dranettie_wood",
        model_id: "model.wilderness.dranettie_wood",
        props: vec![
            "prop.forest.owlbear_nest",
            "prop.forest.dense_thicket",
            "prop.forest.game_trail",
        ],
        silhouette: "mid-level mixed wood with owlbear nesting glens and dranettie thickets",
        material_notes: "Original Thera-Dranettie woodland dressing; no copied iconography.",
        notes: vec!["Camouflage-legal throughout (#1629)."],
    })
}

pub(crate) fn spiderhaunt_woods_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: SPIDERHAUNT_WOODS_AREA_ID,
        display_name: "Spiderhaunt Woods",
        area_kind: AreaKind::Wilderness,
        source_issue: 1650,
        canon_issues: vec![987, 1598, 1632, 1650],
        wiki_slug: "spiderhaunt-woods",
        source_history_label: "CF Spiderhaunt Woods factual topology and mechanics only",
        summary: "CF ranking guide places spiders at 30-35 for good-aligned characters.",
        level_band: (20, 40),
        rooms: vec![
            room(
                "room.spiderhaunt_woods.web_canopy",
                "Web-Draped Canopy",
                RoomKind::Wilderness,
                "Silk strands stretch between every bough.",
                RoomConcealment::None,
                &["npc.spiderhaunt_woods.sword_spider"],
            ),
            room(
                "room.spiderhaunt_woods.brood_hollow",
                "Brood Hollow",
                RoomKind::Chamber,
                "A hollow thick with egg sacs and skittering broods.",
                RoomConcealment::None,
                &["npc.spiderhaunt_woods.spider_broodmother"],
            ),
        ],
        connections: vec![
            conn(
                "conn.spiderhaunt_woods.canopy_to_hollow",
                "room.spiderhaunt_woods.web_canopy",
                SPIDERHAUNT_WOODS_AREA_ID,
                Some("room.spiderhaunt_woods.brood_hollow"),
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            conn(
                "conn.spiderhaunt_woods.to_dranettie_wood",
                "room.spiderhaunt_woods.web_canopy",
                DRANETTIE_WOOD_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.spiderhaunt_woods.to_violet_woodland",
                "room.spiderhaunt_woods.brood_hollow",
                VIOLET_WOODLAND_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.spiderhaunt_woods.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.spiderhaunt_woods.population",
        repop_budget: 25,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.spiderhaunt_woods",
        model_id: "model.wilderness.spiderhaunt_woods",
        props: vec![
            "prop.forest.spider_web_strand",
            "prop.forest.egg_sac_cluster",
            "prop.forest.game_trail",
        ],
        silhouette: "web-draped woods with a hidden brood hollow at its heart",
        material_notes: "Original Thera-Spiderhaunt dressing; no copied iconography.",
        notes: vec![
            "Brood Hollow is a Hidden connection (#944), not plainly visible from the canopy.",
        ],
    })
}

pub(crate) fn violet_woodland_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: VIOLET_WOODLAND_AREA_ID,
        display_name: "Violet Woodland",
        area_kind: AreaKind::Wilderness,
        source_issue: 1650,
        canon_issues: vec![987, 1598, 1632, 1650],
        wiki_slug: "violet-woodland",
        source_history_label: "CF Violet Woodland factual topology and mechanics only",
        summary: "A low-level woodland known for gibberlings.",
        level_band: (5, 10),
        rooms: vec![
            room(
                "room.violet_woodland.violet_glade",
                "Violet Glade",
                RoomKind::Wilderness,
                "A glade of violet-hued undergrowth.",
                RoomConcealment::None,
                &["npc.violet_woodland.gibberling"],
            ),
            room(
                "room.violet_woodland.gibberling_warren",
                "Gibberling Warren",
                RoomKind::Chamber,
                "A shallow warren where gibberlings swarm.",
                RoomConcealment::None,
                &["npc.violet_woodland.gibberling_pack"],
            ),
        ],
        connections: vec![
            conn(
                "conn.violet_woodland.glade_to_warren",
                "room.violet_woodland.violet_glade",
                VIOLET_WOODLAND_AREA_ID,
                Some("room.violet_woodland.gibberling_warren"),
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.violet_woodland.to_spiderhaunt_woods",
                "room.violet_woodland.violet_glade",
                SPIDERHAUNT_WOODS_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.violet_woodland.to_emerald_forest",
                "room.violet_woodland.gibberling_warren",
                EMERALD_FOREST_AREA_ID,
                None,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.violet_woodland.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.violet_woodland.population",
        repop_budget: 16,
        repop_minutes: 15,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.violet_woodland",
        model_id: "model.wilderness.violet_woodland",
        props: vec![
            "prop.forest.violet_underbrush",
            "prop.forest.warren_mound",
            "prop.forest.game_trail",
        ],
        silhouette: "low woodland with distinctive violet-hued undergrowth",
        material_notes: "Original Thera-Violet Woodland dressing; no copied iconography.",
        notes: vec!["Camouflage-legal throughout (#1629)."],
    })
}

pub(crate) fn emerald_forest_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: EMERALD_FOREST_AREA_ID,
        display_name: "Emerald Forest",
        area_kind: AreaKind::Wilderness,
        source_issue: 1650,
        canon_issues: vec![987, 1598, 1632, 1650, 990],
        wiki_slug: "emerald-forest",
        source_history_label: "CF Emerald Forest factual topology and mechanics only",
        summary: "Pixies, sprites, brownies, dryads, nymphs, and rangers; OUTLANDER cabal heartland \
            flavor (#990). CF ranking spans 15-30 on the evil side.",
        level_band: (15, 30),
        rooms: vec![
            room(
                "room.emerald_forest.fae_canopy",
                "Fae Canopy",
                RoomKind::Wilderness,
                "A vivid green canopy alive with pixies and sprites.",
                RoomConcealment::None,
                &["npc.emerald_forest.pixie"],
            ),
            room(
                "room.emerald_forest.dryad_glade",
                "Dryad Glade",
                RoomKind::Landmark,
                "A sacred glade where dryads and nymphs are said to dwell.",
                RoomConcealment::None,
                &["npc.emerald_forest.dryad"],
            ),
            room(
                "room.emerald_forest.outlander_camp",
                "Outlander Waycamp",
                RoomKind::Landmark,
                "A ranger waycamp tied to the OUTLANDER cabal (#990).",
                RoomConcealment::CrowdCover,
                &["npc.emerald_forest.outlander_ranger"],
            ),
        ],
        connections: vec![
            conn(
                "conn.emerald_forest.canopy_to_glade",
                "room.emerald_forest.fae_canopy",
                EMERALD_FOREST_AREA_ID,
                Some("room.emerald_forest.dryad_glade"),
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.emerald_forest.glade_to_camp",
                "room.emerald_forest.dryad_glade",
                EMERALD_FOREST_AREA_ID,
                Some("room.emerald_forest.outlander_camp"),
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::ConcealedTrail,
            ),
            conn(
                "conn.emerald_forest.to_violet_woodland",
                "room.emerald_forest.fae_canopy",
                VIOLET_WOODLAND_AREA_ID,
                None,
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.emerald_forest.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.emerald_forest.population",
        repop_budget: 24,
        repop_minutes: 18,
        pressure_tier: 2,
        tileset_id: "tileset.wilderness.emerald_forest",
        model_id: "model.wilderness.emerald_forest",
        props: vec![
            "prop.forest.fae_ring",
            "prop.forest.dryad_tree",
            "prop.forest.ranger_waycamp",
        ],
        silhouette: "vivid emerald-green fae-touched forest with a hidden ranger waycamp",
        material_notes: "Original Thera-Emerald Forest dressing; no copied iconography.",
        notes: vec![
            "Outlander Waycamp is a ConcealedTrail per #944; OUTLANDER cabal heartland flavor \
            ties to #990.",
        ],
    })
}

pub(crate) fn all_forests_b_specs() -> Vec<AreaSpec> {
    vec![
        dranettie_wood_area_spec(),
        spiderhaunt_woods_area_spec(),
        violet_woodland_area_spec(),
        emerald_forest_area_spec(),
    ]
}
