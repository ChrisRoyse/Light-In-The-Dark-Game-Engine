// #1647 racial homelands, group B: Mal-trakis, Halfling Lands, Mortorn, arial sky
// sanctum. CF-factual topology/rosters/level-bands; name and lore originalized (#984).
use litd_engine::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RoomConcealment,
    RoomKind, TravelKind,
};

use super::towns_thera_support::{
    ARIAL_SKY_SANCTUM_AREA_ID, HALFLING_LANDS_AREA_ID, MAL_TRAKIS_AREA_ID, MINES_OF_ZAKIIM_AREA_ID,
    MORTORN_AREA_ID, TownExpectation, access_any, access_restricted, area_spec, expect, external,
    hidden_internal, internal, recall, repop, resets, room, visual,
};

pub fn push_specs(specs: &mut Vec<(AreaSpec, TownExpectation)>) {
    specs.push((
        mal_trakis_area_spec(),
        expect(
            MAL_TRAKIS_AREA_ID,
            1647,
            AreaKind::Wilderness,
            false,
            true,
            5,
        ),
    ));
    specs.push((
        halfling_lands_area_spec(),
        expect(
            HALFLING_LANDS_AREA_ID,
            1647,
            AreaKind::Wilderness,
            true,
            true,
            5,
        ),
    ));
    specs.push((
        mortorn_area_spec(),
        expect(MORTORN_AREA_ID, 1647, AreaKind::Town, true, true, 5),
    ));
    specs.push((
        arial_sky_sanctum_area_spec(),
        expect(
            ARIAL_SKY_SANCTUM_AREA_ID,
            1647,
            AreaKind::Wilderness,
            true,
            true,
            5,
        ),
    ));
}

/// Mal-trakis: a far-south jungle village of headhunters (35-51); savage ranger lore homeland.
pub fn mal_trakis_area_spec() -> AreaSpec {
    let aid = MAL_TRAKIS_AREA_ID;
    area_spec(
        aid,
        "Mal-trakis",
        AreaKind::Wilderness,
        1647,
        &[985, 1601],
        "mal-trakis",
        (35, 51),
        "A serpent-carved headhunter village deep in the far-south jungle, the savage \
         homeland of ranger lore where trophies hang from every lintel.",
        vec![
            room(
                "room.mal_trakis.jungle_gate",
                "Jungle Gate",
                RoomKind::Gate,
                "A vine-hung gate of carved serpent-posts.",
                RoomConcealment::GateChokepoint,
                &["npc.mal_trakis.blowgun_sentry"],
            ),
            room(
                "room.mal_trakis.village",
                "Headhunter Village",
                RoomKind::Intersection,
                "A canopy village of headhunters (ranking 35-51).",
                RoomConcealment::CrowdCover,
                &["npc.mal_trakis.headhunter"],
            ),
            room(
                "room.mal_trakis.trophy_lodge",
                "Trophy Lodge",
                RoomKind::Landmark,
                "A lodge strung with grim hunting trophies.",
                RoomConcealment::None,
                &["npc.mal_trakis.hunt_chief"],
            ),
            room(
                "room.mal_trakis.hunting_trail",
                "Hunting Trail",
                RoomKind::Wilderness,
                "A concealed hunting trail into the deep jungle.",
                RoomConcealment::AlleyShadow,
                &["npc.mal_trakis.stalker"],
            ),
            room(
                "room.mal_trakis.serpent_shrine",
                "Serpent Shrine",
                RoomKind::Temple,
                "A coiled-serpent shrine that serves as the recall point.",
                RoomConcealment::None,
                &["npc.mal_trakis.serpent_priest"],
            ),
        ],
        vec![
            internal(
                "conn.mal_trakis.gate_to_village",
                "room.mal_trakis.jungle_gate",
                aid,
                "room.mal_trakis.village",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.mal_trakis.village_to_lodge",
                "room.mal_trakis.village",
                aid,
                "room.mal_trakis.trophy_lodge",
                ConnectionDirection::North,
                TravelKind::Path,
            ),
            internal(
                "conn.mal_trakis.village_to_shrine",
                "room.mal_trakis.village",
                aid,
                "room.mal_trakis.serpent_shrine",
                ConnectionDirection::East,
                TravelKind::Path,
            ),
            hidden_internal(
                "conn.mal_trakis.hunt_trail",
                "room.mal_trakis.village",
                aid,
                "room.mal_trakis.hunting_trail",
                ConnectionDirection::West,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
            external(
                "conn.mal_trakis.to_galadon",
                "room.mal_trakis.jungle_gate",
                GALADON_AREA_ID,
                ConnectionDirection::North,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.mal_trakis.cycle", 30),
        repop("repop.mal_trakis.pop", 22, 15, 3),
        recall(
            false,
            Some("room.mal_trakis.serpent_shrine"),
            false,
            access_any(),
        ),
        visual(
            "tileset.wild.mal_trakis.deep_jungle",
            "model.wild.mal_trakis.canopy_village",
            &[
                "prop.wild.serpent_post_gate",
                "prop.wild.trophy_lodge",
                "prop.wild.serpent_shrine",
            ],
            "serpent-carved canopy village of trophy lodges in deep southern jungle",
            "Original jungle-village props and serpent-shrine dressing; no copied iconography.",
        ),
        &[
            "Savage ranger lore homeland; headhunters rank 35-51 (#1647).",
            "Hunting trail is a ConcealedTrail (#944); long road runs north to Galadon (#1632).",
        ],
    )
}

/// Halfling Lands: a pastoral shire pocket in the NW interior; shopkeepers rank evil
/// (25-30). Halfling hometown.
pub fn halfling_lands_area_spec() -> AreaSpec {
    let aid = HALFLING_LANDS_AREA_ID;
    area_spec(
        aid,
        "Halfling Lands",
        AreaKind::Wilderness,
        1647,
        &[985, 1601],
        "halfling-lands",
        (1, 30),
        "A rolling shire pocket of burrow-homes in the northwest interior, homeland of \
         the halflings, whose plump shopkeepers keep a surprisingly hard bargain (evil ranking 25-30).",
        vec![
            room(
                "room.halfling_lands.shire_road",
                "Shire Road",
                RoomKind::Gate,
                "A hedged lane entering the shire pocket.",
                RoomConcealment::GateChokepoint,
                &["npc.halfling_lands.shirriff"],
            ),
            room(
                "room.halfling_lands.green",
                "Village Green",
                RoomKind::Intersection,
                "A tidy green ringed by round burrow-doors.",
                RoomConcealment::CrowdCover,
                &["npc.halfling_lands.halfling"],
            ),
            room(
                "room.halfling_lands.market",
                "Shire Market",
                RoomKind::Market,
                "A cheerful market of hard-bargaining shopkeepers (evil ranking 25-30).",
                RoomConcealment::CrowdCover,
                &["npc.halfling_lands.shopkeeper"],
            ),
            room(
                "room.halfling_lands.burrow_inn",
                "Burrow Inn",
                RoomKind::Inn,
                "A snug burrow-inn of low beams and good ale.",
                RoomConcealment::None,
                &["npc.halfling_lands.innkeeper"],
            ),
            room(
                "room.halfling_lands.hearth_shrine",
                "Hearth Shrine",
                RoomKind::Temple,
                "A homely hearth-shrine that serves as the recall point.",
                RoomConcealment::None,
                &["npc.halfling_lands.hearth_keeper"],
            ),
        ],
        vec![
            internal(
                "conn.halfling_lands.road_to_green",
                "room.halfling_lands.shire_road",
                aid,
                "room.halfling_lands.green",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.halfling_lands.green_to_market",
                "room.halfling_lands.green",
                aid,
                "room.halfling_lands.market",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.halfling_lands.green_to_inn",
                "room.halfling_lands.green",
                aid,
                "room.halfling_lands.burrow_inn",
                ConnectionDirection::South,
                TravelKind::Road,
            ),
            internal(
                "conn.halfling_lands.green_to_shrine",
                "room.halfling_lands.green",
                aid,
                "room.halfling_lands.hearth_shrine",
                ConnectionDirection::North,
                TravelKind::Path,
            ),
            external(
                "conn.halfling_lands.to_galadon",
                "room.halfling_lands.shire_road",
                GALADON_AREA_ID,
                ConnectionDirection::Southeast,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.halfling_lands.cycle", 30),
        repop("repop.halfling_lands.pop", 22, 15, 2),
        recall(
            true,
            Some("room.halfling_lands.hearth_shrine"),
            false,
            access_restricted(&["race.halfling"]),
        ),
        visual(
            "tileset.wild.halfling_lands.green_shire",
            "model.wild.halfling_lands.shire",
            &[
                "prop.wild.hedge_lane_gate",
                "prop.wild.burrow_door",
                "prop.wild.hearth_shrine",
            ],
            "rolling green shire of round burrow-doors around a tidy village green",
            "Original shire burrow props and hearth-shrine dressing; no copied iconography.",
        ),
        &[
            "Halfling hometown (access race.halfling); shopkeepers rank evil 25-30 (#1647).",
            "Hedged lane runs southeast toward the interior road and Galadon (#1632).",
        ],
    )
}

/// Mortorn: a dwarven mountain hold in the east-interior peaks; various dwarves rank
/// evil (40-51). Dwarf hometown (#1005).
pub fn mortorn_area_spec() -> AreaSpec {
    let aid = MORTORN_AREA_ID;
    area_spec(
        aid,
        "Mortorn",
        AreaKind::Town,
        1647,
        &[985, 1005, 1601],
        "mortorn",
        (25, 51),
        "A deep dwarven hold cut into the east-interior peaks, homeland of the dwarves, \
         its forge-lit halls warded by clan-dwarves of grim temper (evil ranking 40-51).",
        vec![
            room(
                "room.mortorn.hold_gate",
                "Hold Gate",
                RoomKind::Gate,
                "The great iron-banded gate of the mountain hold.",
                RoomConcealment::GateChokepoint,
                &["npc.mortorn.gate_warden"],
            ),
            room(
                "room.mortorn.great_hall",
                "Pillared Hall",
                RoomKind::Intersection,
                "A vast pillared hall echoing with hammer-song.",
                RoomConcealment::None,
                &["npc.mortorn.clan_dwarf"],
            ),
            room(
                "room.mortorn.forge",
                "Deep Forge",
                RoomKind::Shop,
                "A roaring forge worked by master smiths (evil ranking 40-51).",
                RoomConcealment::None,
                &["npc.mortorn.forge_master"],
            ),
            room(
                "room.mortorn.deep_barracks",
                "Deep Barracks",
                RoomKind::Chamber,
                "Barracks hewn into the living rock.",
                RoomConcealment::HiddenLayer,
                &["npc.mortorn.hold_guard"],
            ),
            room(
                "room.mortorn.ancestor_shrine",
                "Ancestor Shrine",
                RoomKind::Temple,
                "A shrine of stone forebears that serves as the recall point.",
                RoomConcealment::None,
                &["npc.mortorn.runepriest"],
            ),
        ],
        vec![
            internal(
                "conn.mortorn.gate_to_hall",
                "room.mortorn.hold_gate",
                aid,
                "room.mortorn.great_hall",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.mortorn.hall_to_forge",
                "room.mortorn.great_hall",
                aid,
                "room.mortorn.forge",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.mortorn.hall_to_shrine",
                "room.mortorn.great_hall",
                aid,
                "room.mortorn.ancestor_shrine",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            hidden_internal(
                "conn.mortorn.barracks_seam",
                "room.mortorn.great_hall",
                aid,
                "room.mortorn.deep_barracks",
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            external(
                "conn.mortorn.to_galadon",
                "room.mortorn.hold_gate",
                GALADON_AREA_ID,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.mortorn.cycle", 30),
        repop("repop.mortorn.pop", 26, 15, 3),
        recall(
            true,
            Some("room.mortorn.ancestor_shrine"),
            false,
            access_restricted(&["race.dwarf"]),
        ),
        visual(
            "tileset.town.mortorn.deep_hold",
            "model.town.mortorn.mountain_hold",
            &[
                "prop.town.iron_hold_gate",
                "prop.town.forge_bellows",
                "prop.town.ancestor_shrine",
            ],
            "forge-lit dwarven hold of pillared halls hewn into east-interior peaks",
            "Original dwarven-hold props and ancestor-shrine dressing; no copied iconography.",
        ),
        &[
            "Dwarf hometown (access race.dwarf, #1005); clan-dwarves rank evil 40-51 (#1647).",
            "Deep barracks is a Hidden descent (#944); mountain road runs west to Galadon (#1632).",
        ],
    )
}

#[path = "towns_thera_fixture_homelands_b_arial.rs"]
mod split;
pub use split::arial_sky_sanctum_area_spec;
