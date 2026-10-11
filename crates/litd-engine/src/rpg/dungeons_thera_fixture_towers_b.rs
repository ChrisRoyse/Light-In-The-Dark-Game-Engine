// #1659 towers & arcane sites, group B: the non-tower arcane sites (parent #1598,
// world-graph #1632, framework #987). Level bands and rosters are factual (CF area
// registry https://wiki.carrionfields.net/game/areas + #1659 body); name/lore
// originalized (#984). Each has a bespoke topology with a scry/teleport-warded
// anti-magic chamber (FLAT/blink fail), a locked vault, and a hidden glyph passage
// (trapper parity #1644 / acquired-map doctrine #944).

use crate::rpg::{
    AreaSpec, ConnectionDirection, ConnectionDiscovery, RoomConcealment, RoomKind, TravelKind,
};

use super::rpg_dungeons_thera_support::{
    AYR_TRINIL_AREA_ID, DungeonSpecInput, ELEMENTAL_TEMPLE_AREA_ID, MINES_OF_ZAKIIM_AREA_ID,
    RUINS_OF_DELAR_TOL_AREA_ID, TIR_TALATH_AREA_ID, WESTERN_REACHES_AREA_ID, dungeon_spec,
    external, hidden_internal, internal, locked_internal, room,
};

// #1659 Elemental Temple (30-50 CF; acolytes, good ranking; far west by Glauruk's ground, #1632).
pub(crate) fn elemental_temple_area_spec() -> AreaSpec {
    let a = ELEMENTAL_TEMPLE_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "The Elemental Temple",
        source_issue: 1659,
        extra_canon: &[1644],
        level: (30, 50),
        summary: "A four-shrine elemental temple of the far west, near Glauruk's spawning ground. \
            Acolytes tend its warded sanctum; the righteous rank on them before the high altar.",
        slug: "elemental_temple",
        silhouette: "four-winged elemental temple of fire, water, earth, and air shrines",
        extra_props: &["prop.temple.elemental_font", "prop.tower.glyph_trap"],
        notes: &[
            "Level band 30-50 and the acolyte (good ranking) roster are factual (CF registry + \
                #1659); far-west node by Glauruk's ground (#1632); originalized name/lore only (#984).",
            "Warded sanctum is an anti-magic zone (FLAT/blink fail); locked reliquary; hidden \
                glyph passage (#1644/#944).",
        ],
        rooms: vec![
            room(
                "room.elemental_temple.gate",
                "Temple Gate",
                RoomKind::Gate,
                "The elemental-carved gate where acolytes challenge pilgrims.",
                RoomConcealment::GateChokepoint,
                &["npc.elemental_temple.acolyte"],
            ),
            room(
                "room.elemental_temple.nave",
                "Elemental Nave",
                RoomKind::Intersection,
                "A central nave ringed by the four elemental shrines.",
                RoomConcealment::None,
                &["npc.elemental_temple.acolyte"],
            ),
            room(
                "room.elemental_temple.warded_sanctum",
                "Warded Sanctum",
                RoomKind::Chamber,
                "A scry-warded sanctum where teleport and blink fail against the elements.",
                RoomConcealment::HiddenLayer,
                &["npc.elemental_temple.elementalist"],
            ),
            room(
                "room.elemental_temple.inner_shrine",
                "Inner Shrine",
                RoomKind::Chamber,
                "The inner shrine descending toward the high altar's fire.",
                RoomConcealment::None,
                &["npc.elemental_temple.elementalist"],
            ),
            room(
                "room.elemental_temple.high_altar",
                "High Altar",
                RoomKind::Chamber,
                "The high altar where the temple's hierophant binds the four elements.",
                RoomConcealment::None,
                &["npc.elemental_temple.hierophant"],
            ),
            room(
                "room.elemental_temple.reliquary",
                "Sealed Reliquary",
                RoomKind::Landmark,
                "A locked reliquary of elemental relics behind a warded door.",
                RoomConcealment::HiddenLayer,
                &[],
            ),
        ],
        connections: vec![
            external(
                "conn.elemental_temple.to_west",
                "room.elemental_temple.gate",
                WESTERN_REACHES_AREA_ID,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            internal(
                "conn.elemental_temple.enter",
                a,
                "room.elemental_temple.gate",
                "room.elemental_temple.nave",
                ConnectionDirection::In,
                TravelKind::Path,
            ),
            internal(
                "conn.elemental_temple.to_sanctum",
                a,
                "room.elemental_temple.nave",
                "room.elemental_temple.warded_sanctum",
                ConnectionDirection::In,
                TravelKind::Portal,
            ),
            internal(
                "conn.elemental_temple.descend",
                a,
                "room.elemental_temple.warded_sanctum",
                "room.elemental_temple.inner_shrine",
                ConnectionDirection::Down,
                TravelKind::Descend,
            ),
            internal(
                "conn.elemental_temple.to_altar",
                a,
                "room.elemental_temple.inner_shrine",
                "room.elemental_temple.high_altar",
                ConnectionDirection::In,
                TravelKind::Path,
            ),
            locked_internal(
                "conn.elemental_temple.reliquary_door",
                a,
                "room.elemental_temple.high_altar",
                "room.elemental_temple.reliquary",
                ConnectionDirection::In,
                TravelKind::Stair,
                "lock.elemental_temple.reliquary",
            ),
            hidden_internal(
                "conn.elemental_temple.glyph_passage",
                a,
                "room.elemental_temple.nave",
                "room.elemental_temple.reliquary",
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
        ],
    })
}

// #1659 Mines of Zakiim (30-51 CF; arial miners, good ranking; below the sky sanctum, #1647).
pub(crate) fn mines_of_zakiim_area_spec() -> AreaSpec {
    let a = MINES_OF_ZAKIIM_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "The Mines of Zakiim",
        source_issue: 1659,
        extra_canon: &[1644, 1647],
        level: (30, 51),
        summary: "Deep mines beneath the sky sanctum, worked by arial miners the righteous rank on. \
            Scry-warded shafts descend past the deep digs to a foreman's deep and a sealed lode.",
        slug: "mines_of_zakiim",
        silhouette: "descending mine of timbered shafts and galleries below the sky sanctum",
        extra_props: &["prop.mine.ore_cart", "prop.tower.arcane_ward"],
        notes: &[
            "Level band 30-51 and the arial-miner (good ranking) roster are factual (CF registry + \
                #1659); below the sky sanctum (#1647); originalized name/lore only (#984).",
            "Warded shaft is an anti-magic zone (FLAT/blink fail); locked lode vault; hidden \
                glyph passage (#1644/#944).",
        ],
        rooms: vec![
            room(
                "room.mines_of_zakiim.adit",
                "Mine Adit",
                RoomKind::Gate,
                "The timbered adit where arial overseers count the miners in and out.",
                RoomConcealment::GateChokepoint,
                &["npc.mines_of_zakiim.miner"],
            ),
            room(
                "room.mines_of_zakiim.galleries",
                "Mining Galleries",
                RoomKind::Intersection,
                "Branching galleries loud with pick and cart; miners work every seam.",
                RoomConcealment::None,
                &["npc.mines_of_zakiim.miner"],
            ),
            room(
                "room.mines_of_zakiim.warded_shaft",
                "Warded Shaft",
                RoomKind::Chamber,
                "A scry-warded shaft where teleport and blink fail; only the stair descends.",
                RoomConcealment::HiddenLayer,
                &["npc.mines_of_zakiim.overseer"],
            ),
            room(
                "room.mines_of_zakiim.deep_dig",
                "Deep Dig",
                RoomKind::Chamber,
                "The deep digs where the richest and most dangerous seams are worked.",
                RoomConcealment::HiddenLayer,
                &["npc.mines_of_zakiim.miner"],
            ),
            room(
                "room.mines_of_zakiim.foreman_deep",
                "Foreman's Deep",
                RoomKind::Chamber,
                "The foreman's deep, where the mine's master keeps order and the lode-key.",
                RoomConcealment::None,
                &["npc.mines_of_zakiim.mine_foreman"],
            ),
            room(
                "room.mines_of_zakiim.lode_vault",
                "Sealed Lode Vault",
                RoomKind::Landmark,
                "A locked vault of the mine's richest lode behind an iron door.",
                RoomConcealment::HiddenLayer,
                &[],
            ),
        ],
        connections: vec![
            external(
                "conn.mines_of_zakiim.to_ayr_trinil",
                "room.mines_of_zakiim.adit",
                AYR_TRINIL_AREA_ID,
                ConnectionDirection::Up,
                TravelKind::Stair,
                ConnectionDiscovery::Visible,
            ),
            internal(
                "conn.mines_of_zakiim.enter",
                a,
                "room.mines_of_zakiim.adit",
                "room.mines_of_zakiim.galleries",
                ConnectionDirection::Down,
                TravelKind::Descend,
            ),
            internal(
                "conn.mines_of_zakiim.to_shaft",
                a,
                "room.mines_of_zakiim.galleries",
                "room.mines_of_zakiim.warded_shaft",
                ConnectionDirection::Down,
                TravelKind::Stair,
            ),
            internal(
                "conn.mines_of_zakiim.deeper",
                a,
                "room.mines_of_zakiim.warded_shaft",
                "room.mines_of_zakiim.deep_dig",
                ConnectionDirection::Down,
                TravelKind::Descend,
            ),
            internal(
                "conn.mines_of_zakiim.to_foreman",
                a,
                "room.mines_of_zakiim.deep_dig",
                "room.mines_of_zakiim.foreman_deep",
                ConnectionDirection::In,
                TravelKind::Path,
            ),
            locked_internal(
                "conn.mines_of_zakiim.lode_door",
                a,
                "room.mines_of_zakiim.foreman_deep",
                "room.mines_of_zakiim.lode_vault",
                ConnectionDirection::In,
                TravelKind::Stair,
                "lock.mines_of_zakiim.lode",
            ),
            hidden_internal(
                "conn.mines_of_zakiim.glyph_passage",
                a,
                "room.mines_of_zakiim.galleries",
                "room.mines_of_zakiim.lode_vault",
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
        ],
    })
}

// #1659 Ruins of Delar-Tol (20-40 CF; miners, evil ranking; dwarven ruin on the Tir-Talath web, #1632).
pub(crate) fn ruins_of_delar_tol_area_spec() -> AreaSpec {
    let a = RUINS_OF_DELAR_TOL_AREA_ID;
    dungeon_spec(DungeonSpecInput {
        area_id: a,
        display_name: "The Ruins of Delar-Tol",
        source_issue: 1659,
        extra_canon: &[1644],
        level: (20, 40),
        summary: "A fallen dwarven hold on the Tir-Talath web, its forges cold and its deeps worked \
            by scavenging miners the wicked rank on. A rune-warded forge guards the ruin-lord's deep.",
        slug: "ruins_of_delar_tol",
        silhouette: "collapsed dwarven hold of cold forges and rubble-choked deeps",
        extra_props: &["prop.mine.rune_anvil", "prop.tower.glyph_trap"],
        notes: &[
            "Level band 20-40 and the miner (evil ranking) roster are factual (CF registry + #1659); \
                a node of the Tir-Talath web (#1632); originalized name/lore only (#984).",
            "Rune-warded forge is an anti-magic zone (FLAT/blink fail); locked dwarf-treasury; \
                hidden glyph passage (#1644/#944).",
        ],
        rooms: vec![
            room(
                "room.ruins_of_delar_tol.gate",
                "Ruined Gate",
                RoomKind::Gate,
                "The broken gate of Delar-Tol, where scavenger-miners keep a wary watch.",
                RoomConcealment::GateChokepoint,
                &["npc.ruins_of_delar_tol.miner"],
            ),
            room(
                "room.ruins_of_delar_tol.hall",
                "Dwarven Hall",
                RoomKind::Intersection,
                "A pillared hall of the fallen hold, half-buried in rubble.",
                RoomConcealment::None,
                &["npc.ruins_of_delar_tol.miner"],
            ),
            room(
                "room.ruins_of_delar_tol.warded_forge",
                "Warded Forge",
                RoomKind::Chamber,
                "A rune-warded forge where teleport and blink fail among the cold anvils.",
                RoomConcealment::HiddenLayer,
                &["npc.ruins_of_delar_tol.runesmith"],
            ),
            room(
                "room.ruins_of_delar_tol.deep_ruin",
                "Deep Ruin",
                RoomKind::Chamber,
                "The deep ruin below the forge, worked for what the dwarves left behind.",
                RoomConcealment::HiddenLayer,
                &["npc.ruins_of_delar_tol.miner"],
            ),
            room(
                "room.ruins_of_delar_tol.ruin_lord",
                "Ruin-Lord's Deep",
                RoomKind::Chamber,
                "The deep where the ruin's self-styled lord hoards the hold's remnants.",
                RoomConcealment::None,
                &["npc.ruins_of_delar_tol.ruin_lord"],
            ),
            room(
                "room.ruins_of_delar_tol.treasury",
                "Sealed Dwarf-Treasury",
                RoomKind::Landmark,
                "A locked dwarf-treasury behind a rune-bound door.",
                RoomConcealment::HiddenLayer,
                &[],
            ),
        ],
        connections: vec![
            external(
                "conn.ruins_of_delar_tol.to_tir_talath",
                "room.ruins_of_delar_tol.gate",
                TIR_TALATH_AREA_ID,
                ConnectionDirection::North,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            internal(
                "conn.ruins_of_delar_tol.enter",
                a,
                "room.ruins_of_delar_tol.gate",
                "room.ruins_of_delar_tol.hall",
                ConnectionDirection::In,
                TravelKind::Path,
            ),
            internal(
                "conn.ruins_of_delar_tol.descend",
                a,
                "room.ruins_of_delar_tol.hall",
                "room.ruins_of_delar_tol.warded_forge",
                ConnectionDirection::Down,
                TravelKind::Descend,
            ),
            internal(
                "conn.ruins_of_delar_tol.deeper",
                a,
                "room.ruins_of_delar_tol.warded_forge",
                "room.ruins_of_delar_tol.deep_ruin",
                ConnectionDirection::Down,
                TravelKind::Stair,
            ),
            internal(
                "conn.ruins_of_delar_tol.to_lord",
                a,
                "room.ruins_of_delar_tol.deep_ruin",
                "room.ruins_of_delar_tol.ruin_lord",
                ConnectionDirection::In,
                TravelKind::Path,
            ),
            locked_internal(
                "conn.ruins_of_delar_tol.treasury_door",
                a,
                "room.ruins_of_delar_tol.ruin_lord",
                "room.ruins_of_delar_tol.treasury",
                ConnectionDirection::In,
                TravelKind::Stair,
                "lock.ruins_of_delar_tol.treasury",
            ),
            hidden_internal(
                "conn.ruins_of_delar_tol.glyph_passage",
                a,
                "room.ruins_of_delar_tol.hall",
                "room.ruins_of_delar_tol.treasury",
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
        ],
    })
}
