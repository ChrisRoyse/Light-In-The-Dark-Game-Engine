// #1658 keeps & citadels, group A (parent #1598, world-graph #1632, framework #987).
// Level bands and rosters are factual (CF area registry
// https://wiki.carrionfields.net/game/areas + #1658 body); name/lore originalized (#984).
// Keeps are the counter-stealth gauntlet: patrolled interiors, guards that detect
// hidden, locked doors for locksmith/pass-door parity (#1617), and prisoner cells
// for the binder/rescue chain (#1622). Shared interior shape lives in support:
// gate -> courtyard -> guard hall -> inner ward -> lord's hall, with a locked cell
// block and a hidden sally port (#944).

use crate::rpg::{AreaSpec, ConnectionDirection};

use super::rpg_dungeons_thera_support::{
    BASILICA_AREA_ID, DungeonSpecInput, KEEP_OF_FHALAUGASH_AREA_ID, KEEP_OF_THE_RIGHTEOUS_AREA_ID,
    LORD_TABERSHAW_KEEP_AREA_ID, NORTHWEST_REACHES_AREA_ID, RUINED_KEEP_AREA_ID, dungeon_spec,
    keep_connections, keep_rooms,
};

// #1658 Keep of the Fhalaugash (11-25 CF; orcs, good ranking).
pub(crate) fn keep_of_fhalaugash_area_spec() -> AreaSpec {
    let a = KEEP_OF_FHALAUGASH_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "The Keep of Fhalaugash",
        source_issue: 1658,
        extra_canon: &[1617, 1622],
        level: (11, 25),
        summary: "An orc-held keep of the northwest marches. Warbands patrol its wards; the righteous \
            rank on its orcs. Locked cell-blocks hold captives, and a sally port hides in the wall.",
        slug: "keep_of_fhalaugash",
        silhouette: "squat orc-held stone keep of palisaded wards and watch-towers",
        extra_props: &["prop.keep.portcullis", "prop.keep.orc_banner"],
        notes: &[
            "Level band 11-25 and orc roster are factual (CF registry + #1658); originalized \
                name/lore only (#984).",
            "Patrolling orcs detect hidden (counter-stealth gauntlet); cell door is locked \
                (locksmith/pass-door parity #1617); captives feed the rescue chain (#1622).",
        ],
        rooms: keep_rooms(
            "keep_of_fhalaugash",
            "npc.keep_of_fhalaugash.orc",
            "npc.keep_of_fhalaugash.orc_captain",
            "npc.keep_of_fhalaugash.warlord",
        ),
        connections: keep_connections(
            "keep_of_fhalaugash",
            a,
            NORTHWEST_REACHES_AREA_ID,
            ConnectionDirection::North,
        ),
    })
}

// #1658 Keep of the Righteous (10-25 CF; guards/students, evil ranking; by the Basilica, #1632).
pub(crate) fn keep_of_the_righteous_area_spec() -> AreaSpec {
    let a = KEEP_OF_THE_RIGHTEOUS_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "The Keep of the Righteous",
        source_issue: 1658,
        extra_canon: &[1617, 1622],
        level: (10, 25),
        summary: "A militant order's keep beside the Basilica, where guards drill students of the \
            faith. The wicked rank on its defenders. Cells and a hidden postern lie within.",
        slug: "keep_of_the_righteous",
        silhouette: "white-walled order keep with drill-yard and chapel spire by the Basilica",
        extra_props: &["prop.keep.portcullis", "prop.keep.order_banner"],
        notes: &[
            "Level band 10-25, guard/student roster, and Basilica adjacency are factual \
                (CF registry + #1658/#1632); originalized name/lore only (#984).",
            "Guards detect hidden; locked cell door (#1617); captive students feed the rescue \
                chain (#1622).",
        ],
        rooms: keep_rooms(
            "keep_of_the_righteous",
            "npc.keep_of_the_righteous.guard",
            "npc.keep_of_the_righteous.knight",
            "npc.keep_of_the_righteous.lord_commander",
        ),
        connections: keep_connections(
            "keep_of_the_righteous",
            a,
            BASILICA_AREA_ID,
            ConnectionDirection::East,
        ),
    })
}

// #1658 Ruined Keep (10-25; a fallen keep of the northwest interior web, #1632).
pub(crate) fn ruined_keep_area_spec() -> AreaSpec {
    let a = RUINED_KEEP_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "The Ruined Keep",
        source_issue: 1658,
        extra_canon: &[1617, 1622],
        level: (10, 25),
        summary: "A broken keep of the northwest interior, its wards half-collapsed and held by \
            squatting brigands. Locked strongrooms and a hidden bolt-hole remain among the rubble.",
        slug: "ruined_keep",
        silhouette: "half-collapsed keep of tumbled walls and a leaning gate-tower",
        extra_props: &["prop.keep.collapsed_wall", "prop.keep.brigand_barricade"],
        notes: &[
            "Level band 10-25 (CF #1658); a node of the northwest interior web (#1632); \
                originalized name/lore only (#984).",
            "Brigand pickets detect hidden; locked strongroom (#1617); prisoners feed the rescue \
                chain (#1622).",
        ],
        rooms: keep_rooms(
            "ruined_keep",
            "npc.ruined_keep.brigand",
            "npc.ruined_keep.brigand_sergeant",
            "npc.ruined_keep.brigand_chief",
        ),
        connections: keep_connections(
            "ruined_keep",
            a,
            NORTHWEST_REACHES_AREA_ID,
            ConnectionDirection::West,
        ),
    })
}

// #1658 Lord Tabershaw's Keep (5-25 CF; the LT Keep of the northwest interior web, #1632).
pub(crate) fn lord_tabershaw_keep_area_spec() -> AreaSpec {
    let a = LORD_TABERSHAW_KEEP_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "Lord Tabershaw's Keep",
        source_issue: 1658,
        extra_canon: &[1617, 1622],
        level: (5, 25),
        summary: "The seat of a minor northwest lord, garrisoned and patrolled from gate to donjon. \
            A low-level infiltration keep with a locked treasury and a hidden servants' passage.",
        slug: "lord_tabershaw_keep",
        silhouette: "tidy fortified manor-keep with curtain wall and square donjon",
        extra_props: &["prop.keep.portcullis", "prop.keep.house_banner"],
        notes: &[
            "Level band 5-25 is factual (CF registry + #1658); a node of the northwest interior \
                web (#1632); originalized name/lore only (#984).",
            "House guards detect hidden; locked treasury (#1617); held servants feed the rescue \
                chain (#1622).",
        ],
        rooms: keep_rooms(
            "lord_tabershaw_keep",
            "npc.lord_tabershaw_keep.guard",
            "npc.lord_tabershaw_keep.sergeant",
            "npc.lord_tabershaw_keep.lord_tabershaw",
        ),
        connections: keep_connections(
            "lord_tabershaw_keep",
            a,
            NORTHWEST_REACHES_AREA_ID,
            ConnectionDirection::West,
        ),
    })
}
