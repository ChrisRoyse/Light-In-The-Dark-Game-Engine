// #1658 keeps & citadels, group B (parent #1598, world-graph #1632, framework #987).
// Level bands and rosters are factual (CF area registry
// https://wiki.carrionfields.net/game/areas + #1658 body); name/lore originalized (#984).
// Ceawlin, Moudrilar's Monastery, and High Lord's Keep use the shared keep interior;
// the Imperial Lands + Sewers (EMPIRE cabal seat, #990) get a bespoke topology that
// includes the Imperial Sewers, with locked doors (#1617) and a hidden grate (#944).

use crate::rpg::{
    AreaSpec, ConnectionDirection, ConnectionDiscovery, RoomConcealment, RoomKind, TravelKind,
};

use super::rpg_dungeons_thera_support::{
    ATURI_AREA_ID, DungeonSpecInput, EASTERN_ROAD_AREA_ID, HIGH_LORDS_KEEP_AREA_ID,
    IMPERIAL_LANDS_SEWERS_AREA_ID, KEEP_OF_CEAWLIN_AREA_ID, MOUDRILAR_MONASTERY_AREA_ID,
    SOUTHERN_REACHES_AREA_ID, dungeon_spec, external, hidden_internal, internal, keep_connections,
    keep_rooms, locked_internal, room,
};

// #1658 Keep of Ceawlin (15-30 CF; servants/prisoners 15-20 evil, guards 25-35 good; by Aturi #1640).
pub(crate) fn keep_of_ceawlin_area_spec() -> AreaSpec {
    let a = KEEP_OF_CEAWLIN_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "The Keep of Ceawlin",
        source_issue: 1658,
        extra_canon: &[1617, 1622, 1640],
        level: (15, 30),
        summary: "A working keep in the Aturi corridor: servants and prisoners for the wicked to \
            rank on at 15-20, hardened guards at 25-35 for the righteous. Cells hold captives.",
        slug: "keep_of_ceawlin",
        silhouette: "lived-in stone keep with servant halls, drill-yard, and prison undercroft",
        extra_props: &["prop.keep.portcullis", "prop.keep.ceawlin_banner"],
        notes: &[
            "Level band 15-30, the servant/prisoner (15-20 evil) and guard (25-35 good) rankings, \
                and Aturi adjacency are factual (CF registry + #1658/#1640); name/lore only (#984).",
            "Guards detect hidden; locked cell block (#1617); prisoners feed the rescue chain (#1622).",
        ],
        rooms: keep_rooms(
            "keep_of_ceawlin",
            "npc.keep_of_ceawlin.servant",
            "npc.keep_of_ceawlin.guard",
            "npc.keep_of_ceawlin.castellan",
        ),
        connections: keep_connections(
            "keep_of_ceawlin",
            a,
            ATURI_AREA_ID,
            ConnectionDirection::South,
        ),
    })
}

// #1658 Moudrilar's Monastery (20-35 CF; monks, evil ranking).
pub(crate) fn moudrilar_monastery_area_spec() -> AreaSpec {
    let a = MOUDRILAR_MONASTERY_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "Moudrilar's Monastery",
        source_issue: 1658,
        extra_canon: &[1617, 1622],
        level: (20, 35),
        summary: "A cloistered fighting monastery whose disciplined monks the wicked rank on. Its \
            gates, cloisters, and inner sanctum are patrolled; a locked vault holds its relics.",
        slug: "moudrilar_monastery",
        silhouette: "walled cloistered monastery with bell-tower and terraced training yards",
        extra_props: &["prop.keep.cloister_arch", "prop.keep.prayer_bell"],
        notes: &[
            "Level band 20-35 and the monk (evil ranking) roster are factual (CF registry + #1658); \
                originalized name/lore only (#984).",
            "Monks detect hidden; locked relic vault (#1617); held novices feed the rescue chain (#1622).",
        ],
        rooms: keep_rooms(
            "moudrilar_monastery",
            "npc.moudrilar_monastery.monk",
            "npc.moudrilar_monastery.master_monk",
            "npc.moudrilar_monastery.abbot",
        ),
        connections: keep_connections(
            "moudrilar_monastery",
            a,
            EASTERN_ROAD_AREA_ID,
            ConnectionDirection::West,
        ),
    })
}

// #1658 High Lord's Keep (35-51 CF; the southern fortress, #1632).
pub(crate) fn high_lords_keep_area_spec() -> AreaSpec {
    let a = HIGH_LORDS_KEEP_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "The High Lord's Keep",
        source_issue: 1658,
        extra_canon: &[1617, 1622],
        level: (35, 51),
        summary: "The great southern fortress of a High Lord: an endgame infiltration keep of \
            elite guards and wards, with a locked treasury and a hidden postern in the curtain wall.",
        slug: "high_lords_keep",
        silhouette: "towering southern fortress of concentric curtain walls and a high donjon",
        extra_props: &["prop.keep.portcullis", "prop.keep.high_lord_banner"],
        notes: &[
            "Level band 35-51 is factual (CF registry + #1658); the southern fortress node (#1632); \
                originalized name/lore only (#984).",
            "Elite guards detect hidden; locked treasury (#1617); held hostages feed the rescue \
                chain (#1622).",
        ],
        rooms: keep_rooms(
            "high_lords_keep",
            "npc.high_lords_keep.guard",
            "npc.high_lords_keep.knight_captain",
            "npc.high_lords_keep.high_lord",
        ),
        connections: keep_connections(
            "high_lords_keep",
            a,
            SOUTHERN_REACHES_AREA_ID,
            ConnectionDirection::South,
        ),
    })
}

// #1658 Imperial Lands + Imperial Sewers (20-25; guild guards, good ranking; EMPIRE seat #990).
// Bespoke: the imperial ground above and the sewers beneath, joined by a hidden grate.
pub(crate) fn imperial_lands_sewers_area_spec() -> AreaSpec {
    let a = IMPERIAL_LANDS_SEWERS_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "The Imperial Lands and Sewers",
        source_issue: 1658,
        extra_canon: &[990, 1617, 1622],
        level: (20, 25),
        summary: "The imperial demesne of the EMPIRE and the Imperial Sewers beneath it. Guild \
            guards patrol the ground above; the wicked and the bold descend the sewers to the \
            undervault. Locked gates and a hidden grate join the two.",
        slug: "imperial_lands_sewers",
        silhouette: "ordered imperial estate above a vaulted sewer network beneath",
        extra_props: &["prop.keep.imperial_eagle", "prop.keep.sewer_grate"],
        notes: &[
            "Level band 20-25 and the guild-guard (good ranking) roster are factual (CF registry + \
                #1658); the EMPIRE cabal seat ties #990; originalized name/lore only (#984).",
            "Guild guards detect hidden; the sewer gate is locked (#1617); the grate is a hidden \
                descent (#944); held agents feed the rescue chain (#1622).",
        ],
        rooms: vec![
            room(
                "room.imperial_lands_sewers.gate",
                "Imperial Gate",
                RoomKind::Gate,
                "The eagle-crowned gate of the imperial demesne, watched by guild guards.",
                RoomConcealment::GateChokepoint,
                &["npc.imperial_lands_sewers.guild_guard"],
            ),
            room(
                "room.imperial_lands_sewers.court",
                "Imperial Court",
                RoomKind::Intersection,
                "The ordered court of the EMPIRE, patrolled and overlooked; little cover.",
                RoomConcealment::None,
                &[
                    "npc.imperial_lands_sewers.guild_guard",
                    "npc.imperial_lands_sewers.imperial_officer",
                ],
            ),
            room(
                "room.imperial_lands_sewers.hall",
                "Hall of Governance",
                RoomKind::Chamber,
                "The hall where the Lords of Empire hold their seat under heavy guard.",
                RoomConcealment::None,
                &["npc.imperial_lands_sewers.imperial_officer"],
            ),
            room(
                "room.imperial_lands_sewers.sewer_mouth",
                "Sewer Mouth",
                RoomKind::Sewer,
                "A grated mouth where the Imperial Sewers open beneath the estate.",
                RoomConcealment::HiddenLayer,
                &["npc.imperial_lands_sewers.sewer_rat"],
            ),
            room(
                "room.imperial_lands_sewers.undersewers",
                "The Under-Sewers",
                RoomKind::Sewer,
                "Vaulted sewer tunnels running black beneath the imperial ground.",
                RoomConcealment::HiddenLayer,
                &["npc.imperial_lands_sewers.sewer_lurker"],
            ),
            room(
                "room.imperial_lands_sewers.strongroom",
                "Imperial Strongroom",
                RoomKind::Landmark,
                "A locked strongroom of imperial coin at the sewers' deepest turn.",
                RoomConcealment::HiddenLayer,
                &[],
            ),
        ],
        connections: vec![
            external(
                "conn.imperial_lands_sewers.to_eastern_road",
                "room.imperial_lands_sewers.gate",
                EASTERN_ROAD_AREA_ID,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            internal(
                "conn.imperial_lands_sewers.enter",
                a,
                "room.imperial_lands_sewers.gate",
                "room.imperial_lands_sewers.court",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.imperial_lands_sewers.to_hall",
                a,
                "room.imperial_lands_sewers.court",
                "room.imperial_lands_sewers.hall",
                ConnectionDirection::North,
                TravelKind::Path,
            ),
            locked_internal(
                "conn.imperial_lands_sewers.sewer_gate",
                a,
                "room.imperial_lands_sewers.court",
                "room.imperial_lands_sewers.sewer_mouth",
                ConnectionDirection::Down,
                TravelKind::Descend,
                "lock.imperial_lands_sewers.sewer_gate",
            ),
            internal(
                "conn.imperial_lands_sewers.into_sewers",
                a,
                "room.imperial_lands_sewers.sewer_mouth",
                "room.imperial_lands_sewers.undersewers",
                ConnectionDirection::Down,
                TravelKind::Stair,
            ),
            internal(
                "conn.imperial_lands_sewers.to_strongroom",
                a,
                "room.imperial_lands_sewers.undersewers",
                "room.imperial_lands_sewers.strongroom",
                ConnectionDirection::In,
                TravelKind::Path,
            ),
            hidden_internal(
                "conn.imperial_lands_sewers.hidden_grate",
                a,
                "room.imperial_lands_sewers.hall",
                "room.imperial_lands_sewers.undersewers",
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
        ],
    })
}
