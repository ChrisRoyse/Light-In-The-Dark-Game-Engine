// #1659 towers & arcane sites, group A: the climbing towers (parent #1598, world-graph
// #1632, framework #987). Level bands are factual (CF area registry
// https://wiki.carrionfields.net/game/areas + #1659 body); name/lore originalized (#984).
// Towers are the arcane gauntlet: scry/teleport-warded floors (anti-magic zones where
// FLAT and blink fail), puzzle locks, and glyph traps (trapper parity #1644). Shared
// climb: base -> antechamber -> warded floor -> upper hall -> summit sanctum, with a
// puzzle-locked reliquary and a hidden glyph passage (#944).

use crate::rpg::{
    AreaConnection, AreaRoom, AreaSpec, ConnectionDirection, ConnectionDiscovery, RoomConcealment,
    RoomKind, TravelKind,
};

use super::rpg_dungeons_thera_support::{
    DRAGON_SEA_AREA_ID, DungeonSpecInput, EASTERN_ROAD_AREA_ID, NORTHWEST_REACHES_AREA_ID,
    OCTAGONAL_TOWER_AREA_ID, TOWER_OF_DRAGONS_AREA_ID, TOWER_OF_HIGH_SORCERY_AREA_ID,
    TOWER_OF_TROTHON_AREA_ID, WESTERN_REACHES_AREA_ID, conn_id, dungeon_spec, external,
    hidden_internal, internal, locked_internal, room, room_id,
};

// #1659 Tower of High Sorcery (10-30; the mage trial tower, far west, #1632).
pub(crate) fn tower_of_high_sorcery_area_spec() -> AreaSpec {
    let a = TOWER_OF_HIGH_SORCERY_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "The Tower of High Sorcery",
        source_issue: 1659,
        extra_canon: &[1644],
        level: (10, 30),
        summary: "The mage trial tower of the far west, its floors scry- and teleport-warded so \
            that blink and dimension-walk fail within. Apprentices, wards, and a magister guard \
            the climb to the summit sanctum.",
        slug: "tower_of_high_sorcery",
        silhouette: "slender warded spire of stacked arcane floors above the western reaches",
        extra_props: &["prop.tower.arcane_ward", "prop.tower.glyph_trap"],
        notes: &[
            "Level band 10-30 is factual (CF registry + #1659); the far-west mage tower (#1632); \
                originalized name/lore only (#984).",
            "Warded floor is an anti-magic zone (FLAT/blink fail per CF certain-exceptions); the \
                reliquary door is a puzzle lock; glyph passage is a trap-warded hidden way (#1644/#944).",
        ],
        rooms: tower_rooms(
            "tower_of_high_sorcery",
            "npc.tower_of_high_sorcery.apprentice",
            "npc.tower_of_high_sorcery.ward_keeper",
            "npc.tower_of_high_sorcery.magister",
        ),
        connections: tower_connections(
            "tower_of_high_sorcery",
            a,
            WESTERN_REACHES_AREA_ID,
            ConnectionDirection::West,
        ),
    })
}

// #1659 Tower of Dragons / Dragon Tower Ruins (10-51; east, by the Dragon Sea lane, #1632).
pub(crate) fn tower_of_dragons_area_spec() -> AreaSpec {
    let a = TOWER_OF_DRAGONS_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "The Tower of Dragons",
        source_issue: 1659,
        extra_canon: &[1644],
        level: (10, 51),
        summary: "The toppled Dragon Towers by the eastern Dragon Sea, once a gathering place of \
            dragonkind. Draconians and ghosts haunt the warded ruin; the boldest climb to the \
            ancient roost above.",
        slug: "tower_of_dragons",
        silhouette: "shattered draconic tower of broken tiers over the eastern Dragon Sea",
        extra_props: &["prop.tower.draconic_ward", "prop.tower.glyph_trap"],
        notes: &[
            "Level band 10-51 is factual (CF registry + #1659); the eastern Dragon Sea node (#1632); \
                originalized name/lore only (#984).",
            "Warded tier is an anti-magic zone (FLAT/blink fail); puzzle-locked roost; hidden \
                glyph passage (#1644/#944).",
        ],
        rooms: tower_rooms(
            "tower_of_dragons",
            "npc.tower_of_dragons.draconian",
            "npc.tower_of_dragons.dragon_ghost",
            "npc.tower_of_dragons.ancient_wyrm",
        ),
        connections: tower_connections(
            "tower_of_dragons",
            a,
            DRAGON_SEA_AREA_ID,
            ConnectionDirection::East,
        ),
    })
}

// #1659 Octagonal Tower (40-51 CF; explore-class arcane tower, #1632).
pub(crate) fn octagonal_tower_area_spec() -> AreaSpec {
    let a = OCTAGONAL_TOWER_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "The Octagonal Tower",
        source_issue: 1659,
        extra_canon: &[1644],
        level: (40, 51),
        summary: "An explore-class arcane tower of eight-sided floors, each a scry-warded puzzle. \
            Its wardens and constructs guard a climb that punishes teleport and misstep alike.",
        slug: "octagonal_tower",
        silhouette: "eight-sided warded tower of nested octagonal floors",
        extra_props: &["prop.tower.arcane_ward", "prop.tower.puzzle_lock"],
        notes: &[
            "Level band 40-51 is factual (CF registry + #1659); explore-class arcane tower (#1632); \
                originalized name/lore only (#984).",
            "Warded floor is an anti-magic zone (FLAT/blink fail); reliquary is a puzzle lock; \
                hidden glyph passage (#1644/#944).",
        ],
        rooms: tower_rooms(
            "octagonal_tower",
            "npc.octagonal_tower.warden",
            "npc.octagonal_tower.construct",
            "npc.octagonal_tower.tower_master",
        ),
        connections: tower_connections(
            "octagonal_tower",
            a,
            NORTHWEST_REACHES_AREA_ID,
            ConnectionDirection::North,
        ),
    })
}

// #1659 Tower of Trothon (40-51 CF; explore-class arcane tower; glyph traps #1644).
pub(crate) fn tower_of_trothon_area_spec() -> AreaSpec {
    let a = TOWER_OF_TROTHON_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "The Tower of Trothon",
        source_issue: 1659,
        extra_canon: &[1644],
        level: (40, 51),
        summary: "An explore-class tower of the arcane Trothon, its floors thick with glyph traps \
            and scry-wards. Only a careful climb past its guardians reaches the summit sanctum.",
        slug: "tower_of_trothon",
        silhouette: "trap-warded arcane tower of glyph-etched floors",
        extra_props: &["prop.tower.glyph_trap", "prop.tower.arcane_ward"],
        notes: &[
            "Level band 40-51 is factual (CF registry + #1659); explore-class tower (#1632); \
                originalized name/lore only (#984).",
            "Glyph-trapped warded floor is an anti-magic zone (FLAT/blink fail); puzzle-locked \
                reliquary; hidden glyph passage (trapper parity #1644/#944).",
        ],
        rooms: tower_rooms(
            "tower_of_trothon",
            "npc.tower_of_trothon.acolyte",
            "npc.tower_of_trothon.glyph_warden",
            "npc.tower_of_trothon.trothon",
        ),
        connections: tower_connections(
            "tower_of_trothon",
            a,
            EASTERN_ROAD_AREA_ID,
            ConnectionDirection::West,
        ),
    })
}

// Shared climbing-tower interior: base -> antechamber -> warded floor -> upper hall ->
// summit sanctum, plus a puzzle-locked reliquary and a hidden glyph passage (#944).
fn tower_rooms(slug: &str, mook: &str, elite: &str, boss: &str) -> Vec<AreaRoom> {
    vec![
        room(
            &room_id(slug, "base"),
            "Tower Base",
            RoomKind::Gate,
            "The warded threshold where the climb begins under a watchful guardian.",
            RoomConcealment::GateChokepoint,
            &[mook],
        ),
        room(
            &room_id(slug, "antechamber"),
            "Antechamber",
            RoomKind::Chamber,
            "A pillared antechamber of dormant wards at the tower's foot.",
            RoomConcealment::None,
            &[mook],
        ),
        room(
            &room_id(slug, "warded_floor"),
            "Warded Floor",
            RoomKind::Chamber,
            "A scry- and teleport-warded floor where blink and dimension-walk simply fail.",
            RoomConcealment::HiddenLayer,
            &[elite],
        ),
        room(
            &room_id(slug, "upper_hall"),
            "Upper Hall",
            RoomKind::Chamber,
            "The upper hall, its glyph-etched floor climbing toward the sanctum.",
            RoomConcealment::None,
            &[elite],
        ),
        room(
            &room_id(slug, "summit"),
            "Summit Sanctum",
            RoomKind::Chamber,
            "The sanctum at the tower's crown, where its master keeps the arcane heart.",
            RoomConcealment::None,
            &[boss],
        ),
        room(
            &room_id(slug, "reliquary"),
            "Sealed Reliquary",
            RoomKind::Landmark,
            "A puzzle-locked reliquary of the tower's oldest workings.",
            RoomConcealment::HiddenLayer,
            &[],
        ),
    ]
}

fn tower_connections(
    slug: &str,
    area_id: &str,
    approach: &str,
    approach_dir: ConnectionDirection,
) -> Vec<AreaConnection> {
    vec![
        external(
            &conn_id(slug, "approach"),
            &room_id(slug, "base"),
            approach,
            approach_dir,
            TravelKind::Road,
            ConnectionDiscovery::Visible,
        ),
        internal(
            &conn_id(slug, "climb_1"),
            area_id,
            &room_id(slug, "base"),
            &room_id(slug, "antechamber"),
            ConnectionDirection::Up,
            TravelKind::Stair,
        ),
        internal(
            &conn_id(slug, "climb_2"),
            area_id,
            &room_id(slug, "antechamber"),
            &room_id(slug, "warded_floor"),
            ConnectionDirection::Up,
            TravelKind::Stair,
        ),
        internal(
            &conn_id(slug, "climb_3"),
            area_id,
            &room_id(slug, "warded_floor"),
            &room_id(slug, "upper_hall"),
            ConnectionDirection::Up,
            TravelKind::Stair,
        ),
        internal(
            &conn_id(slug, "climb_4"),
            area_id,
            &room_id(slug, "upper_hall"),
            &room_id(slug, "summit"),
            ConnectionDirection::Up,
            TravelKind::Stair,
        ),
        locked_internal(
            &conn_id(slug, "reliquary_door"),
            area_id,
            &room_id(slug, "summit"),
            &room_id(slug, "reliquary"),
            ConnectionDirection::In,
            TravelKind::Stair,
            &format!("lock.{slug}.reliquary"),
        ),
        hidden_internal(
            &conn_id(slug, "glyph_passage"),
            area_id,
            &room_id(slug, "warded_floor"),
            &room_id(slug, "reliquary"),
            ConnectionDirection::Up,
            TravelKind::Portal,
            ConnectionDiscovery::Hidden,
        ),
    ]
}
