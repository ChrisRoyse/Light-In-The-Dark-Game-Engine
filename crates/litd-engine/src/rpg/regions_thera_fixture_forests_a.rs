// #1650 world/region: western forests, part A (first 5 of 9 named woods).
// Ranger/druid/wood-elf country. Design constants cited: camouflage-legal
// throughout, waylay corridors along road edges (#1629), treeform biome
// variety (#1614), herb/forage tables (#1613).

use crate::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RoomConcealment,
    RoomKind, TravelKind,
};

use super::regions_thera_support::{AreaBlueprint, build_wild_area, conn, room};

pub(crate) const FEANWYYN_WEALD_AREA_ID: &str = "area.thera.feanwyyn_weald";
pub(crate) const AZRETH_FOREST_AREA_ID: &str = "area.thera.azreth_forest";
pub(crate) const FOREST_OF_PROSIMY_AREA_ID: &str = "area.thera.forest_of_prosimy";
pub(crate) const FOREST_OF_NOWHERE_AREA_ID: &str = "area.thera.forest_of_nowhere";
pub(crate) const WHISTLEWOOD_AREA_ID: &str = "area.thera.whistlewood";

pub(crate) fn forests_a_area_ids() -> [&'static str; 5] {
    [
        FEANWYYN_WEALD_AREA_ID,
        AZRETH_FOREST_AREA_ID,
        FOREST_OF_PROSIMY_AREA_ID,
        FOREST_OF_NOWHERE_AREA_ID,
        WHISTLEWOOD_AREA_ID,
    ]
}

pub(crate) fn feanwyyn_weald_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: FEANWYYN_WEALD_AREA_ID,
        display_name: "Feanwyyn Weald",
        area_kind: AreaKind::Wilderness,
        source_issue: 1650,
        canon_issues: vec![987, 1598, 1632, 1650, 1629],
        wiki_slug: "feanwyyn-weald",
        source_history_label: "CF Feanwyyn Weald factual topology and mechanics only",
        summary: "The forest west of Galadon where rangers practice their craft, flanking the coast.",
        level_band: (1, 20),
        rooms: vec![
            room(
                "room.feanwyyn_weald.west_treeline",
                "West Treeline",
                RoomKind::Wilderness,
                "The forest edge just outside Galadon's northwest gate road.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.feanwyyn_weald.ranger_clearing",
                "Ranger Practice Clearing",
                RoomKind::Landmark,
                "A cleared training ground rangers use to hone woodcraft.",
                RoomConcealment::None,
                &["npc.feanwyyn_weald.ranger_instructor"],
            ),
        ],
        connections: vec![
            conn(
                "conn.feanwyyn_weald.treeline_to_clearing",
                "room.feanwyyn_weald.west_treeline",
                FEANWYYN_WEALD_AREA_ID,
                Some("room.feanwyyn_weald.ranger_clearing"),
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.feanwyyn_weald.to_galadon",
                "room.feanwyyn_weald.west_treeline",
                GALADON_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.feanwyyn_weald.to_azreth_forest",
                "room.feanwyyn_weald.ranger_clearing",
                AZRETH_FOREST_AREA_ID,
                None,
                ConnectionDirection::Southwest,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.feanwyyn_weald.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.feanwyyn_weald.population",
        repop_budget: 20,
        repop_minutes: 18,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.feanwyyn_weald",
        model_id: "model.wilderness.feanwyyn_weald",
        props: vec![
            "prop.forest.pine_stand",
            "prop.forest.training_post",
            "prop.forest.game_trail",
        ],
        silhouette: "open pine-and-oak practice woods just west of the city walls",
        material_notes: "Original Thera-Feanwyyn woodcraft dressing; no copied iconography.",
        notes: vec!["Camouflage-legal throughout; waylay corridors along the road edges (#1629)."],
    })
}

pub(crate) fn azreth_forest_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: AZRETH_FOREST_AREA_ID,
        display_name: "Azreth Forest",
        area_kind: AreaKind::Wilderness,
        source_issue: 1650,
        canon_issues: vec![987, 1598, 1632, 1650],
        wiki_slug: "azreth-forest",
        source_history_label: "CF Azreth Forest factual topology and mechanics only",
        summary: "A forest flanking the southwest, home to taglos and tresnores.",
        level_band: (15, 20),
        rooms: vec![
            room(
                "room.azreth_forest.forest_floor",
                "Azreth Forest Floor",
                RoomKind::Wilderness,
                "Thick undergrowth beneath a taglos-haunted canopy.",
                RoomConcealment::None,
                &["npc.azreth_forest.taglo"],
            ),
            room(
                "room.azreth_forest.taglos_hollow",
                "Taglos Hollow",
                RoomKind::Chamber,
                "A hollow where tresnores and taglos are known to nest.",
                RoomConcealment::None,
                &["npc.azreth_forest.tresnore"],
            ),
        ],
        connections: vec![
            conn(
                "conn.azreth_forest.floor_to_hollow",
                "room.azreth_forest.forest_floor",
                AZRETH_FOREST_AREA_ID,
                Some("room.azreth_forest.taglos_hollow"),
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.azreth_forest.to_feanwyyn_weald",
                "room.azreth_forest.forest_floor",
                FEANWYYN_WEALD_AREA_ID,
                None,
                ConnectionDirection::Northeast,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.azreth_forest.to_forest_of_prosimy",
                "room.azreth_forest.taglos_hollow",
                FOREST_OF_PROSIMY_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.azreth_forest.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.azreth_forest.population",
        repop_budget: 20,
        repop_minutes: 18,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.azreth_forest",
        model_id: "model.wilderness.azreth_forest",
        props: vec![
            "prop.forest.dense_undergrowth",
            "prop.forest.hollow_stump",
            "prop.forest.game_trail",
        ],
        silhouette: "dense southwestern forest with taglos-carved hollows",
        material_notes: "Original Thera-Azreth woodland dressing; no copied iconography.",
        notes: vec!["Camouflage-legal throughout (#1629)."],
    })
}

pub(crate) fn forest_of_prosimy_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: FOREST_OF_PROSIMY_AREA_ID,
        display_name: "Forest of Prosimy",
        area_kind: AreaKind::Wilderness,
        source_issue: 1650,
        canon_issues: vec![987, 1598, 1632, 1650],
        wiki_slug: "forest-of-prosimy",
        source_history_label: "CF Forest of Prosimy factual topology and mechanics only",
        summary: "A low-level forest known for its wolves.",
        level_band: (5, 10),
        rooms: vec![
            room(
                "room.forest_of_prosimy.thicket",
                "Prosimy Thicket",
                RoomKind::Wilderness,
                "A dense low thicket favored by wolf packs.",
                RoomConcealment::None,
                &["npc.forest_of_prosimy.wolf"],
            ),
            room(
                "room.forest_of_prosimy.den_approach",
                "Wolf Den Approach",
                RoomKind::Landmark,
                "A worn trail leading toward a wolf den.",
                RoomConcealment::None,
                &[],
            ),
        ],
        connections: vec![
            conn(
                "conn.forest_of_prosimy.thicket_to_den",
                "room.forest_of_prosimy.thicket",
                FOREST_OF_PROSIMY_AREA_ID,
                Some("room.forest_of_prosimy.den_approach"),
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.forest_of_prosimy.to_azreth_forest",
                "room.forest_of_prosimy.thicket",
                AZRETH_FOREST_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.forest_of_prosimy.to_forest_of_nowhere",
                "room.forest_of_prosimy.den_approach",
                FOREST_OF_NOWHERE_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.forest_of_prosimy.cycle",
        reset_minutes: 30,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.forest_of_prosimy.population",
        repop_budget: 15,
        repop_minutes: 15,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.forest_of_prosimy",
        model_id: "model.wilderness.forest_of_prosimy",
        props: vec![
            "prop.forest.wolf_den",
            "prop.forest.low_thicket",
            "prop.forest.game_trail",
        ],
        silhouette: "low wolf-haunted thicket forest",
        material_notes: "Original Thera-Prosimy woodland dressing; no copied iconography.",
        notes: vec!["Camouflage-legal throughout (#1629)."],
    })
}

pub(crate) fn forest_of_nowhere_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: FOREST_OF_NOWHERE_AREA_ID,
        display_name: "Forest of NoWhere",
        area_kind: AreaKind::Wilderness,
        source_issue: 1650,
        canon_issues: vec![987, 1598, 1632, 1650, 893],
        wiki_slug: "forest-of-nowhere",
        source_history_label: "CF Forest of NoWhere factual topology and mechanics only",
        summary: "Beasts, slugs, angnos, laxians, stalkers, and taglos haunt this forest; its logging- \
            scarred half is the Ashes of NoWhere, environmental storytelling per #893.",
        level_band: (5, 20),
        rooms: vec![
            room(
                "room.forest_of_nowhere.thicket",
                "NoWhere Thicket",
                RoomKind::Wilderness,
                "Wild, tangled forest thick with stalkers and laxians.",
                RoomConcealment::None,
                &["npc.forest_of_nowhere.stalker"],
            ),
            room(
                "room.forest_of_nowhere.ashes",
                "Ashes of NoWhere",
                RoomKind::Landmark,
                "A logging-scarred clearing worked by lumberjacks and burners (#893).",
                RoomConcealment::None,
                &["npc.forest_of_nowhere.lumberjack"],
            ),
        ],
        connections: vec![
            conn(
                "conn.forest_of_nowhere.thicket_to_ashes",
                "room.forest_of_nowhere.thicket",
                FOREST_OF_NOWHERE_AREA_ID,
                Some("room.forest_of_nowhere.ashes"),
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.forest_of_nowhere.to_forest_of_prosimy",
                "room.forest_of_nowhere.thicket",
                FOREST_OF_PROSIMY_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.forest_of_nowhere.to_whistlewood",
                "room.forest_of_nowhere.ashes",
                WHISTLEWOOD_AREA_ID,
                None,
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.forest_of_nowhere.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.forest_of_nowhere.population",
        repop_budget: 22,
        repop_minutes: 18,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.forest_of_nowhere",
        model_id: "model.wilderness.forest_of_nowhere",
        props: vec![
            "prop.forest.burned_stump_field",
            "prop.forest.lumber_pile",
            "prop.forest.tangled_thicket",
        ],
        silhouette: "wild tangled forest giving way to a logging-scarred ash clearing",
        material_notes: "Original Thera-NoWhere woodland and logging-camp dressing; no copied \
            iconography.",
        notes: vec![
            "Ashes of NoWhere is environmental storytelling per #893: a logging-scarred half \
            distinct from the wild thicket.",
        ],
    })
}

pub(crate) fn whistlewood_area_spec() -> AreaSpec {
    build_wild_area(AreaBlueprint {
        area_id: WHISTLEWOOD_AREA_ID,
        display_name: "Whistlewood",
        area_kind: AreaKind::Wilderness,
        source_issue: 1650,
        canon_issues: vec![987, 1598, 1632, 1650],
        wiki_slug: "whistlewood",
        source_history_label: "CF Whistlewood factual topology and mechanics only",
        summary: "The far-western wood, opening into the Shadow Grove / Evergrove corridor; source \
            gives no explicit CF level range for this node, so the band below is estimated from \
            regional context, not a cited number.",
        level_band: (10, 25),
        rooms: vec![
            room(
                "room.whistlewood.canopy",
                "Whistling Canopy",
                RoomKind::Wilderness,
                "Wind through the high canopy makes a constant low whistle.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.whistlewood.shadow_grove_corridor",
                "Shadow Grove Corridor",
                RoomKind::Landmark,
                "The dim corridor linking toward the Shadow Grove / Evergrove.",
                RoomConcealment::None,
                &["npc.whistlewood.grove_warden"],
            ),
        ],
        connections: vec![
            conn(
                "conn.whistlewood.canopy_to_corridor",
                "room.whistlewood.canopy",
                WHISTLEWOOD_AREA_ID,
                Some("room.whistlewood.shadow_grove_corridor"),
                ConnectionDirection::West,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            conn(
                "conn.whistlewood.to_forest_of_nowhere",
                "room.whistlewood.canopy",
                FOREST_OF_NOWHERE_AREA_ID,
                None,
                ConnectionDirection::East,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        reset_id: "reset.whistlewood.cycle",
        reset_minutes: 35,
        clears_corpses: true,
        resets_doors: false,
        repop_id: "repop.whistlewood.population",
        repop_budget: 20,
        repop_minutes: 18,
        pressure_tier: 1,
        tileset_id: "tileset.wilderness.whistlewood",
        model_id: "model.wilderness.whistlewood",
        props: vec![
            "prop.forest.wind_worn_bough",
            "prop.forest.grove_shrine",
            "prop.forest.game_trail",
        ],
        silhouette: "far-western high-canopy wood opening onto a shadowed grove corridor",
        material_notes: "Original Thera-Whistlewood dressing; no copied iconography.",
        notes: vec!["Camouflage-legal throughout (#1629)."],
    })
}

pub(crate) fn all_forests_a_specs() -> Vec<AreaSpec> {
    vec![
        feanwyyn_weald_area_spec(),
        azreth_forest_area_spec(),
        forest_of_prosimy_area_spec(),
        forest_of_nowhere_area_spec(),
        whistlewood_area_spec(),
    ]
}
