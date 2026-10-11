// #1645 central & eastern villages, group A: Evermoon Hollow, Aldevari, Copeham Inn.
// CF-factual topology/ranking rosters/level-bands; name and lore originalized (#984).
use litd_engine::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RoomConcealment,
    RoomKind, TravelKind,
};

use super::towns_thera_support::{
    ALDEVARI_AREA_ID, COPEHAM_INN_AREA_ID, EVERMOON_HOLLOW_AREA_ID, TownExpectation,
    VORALIAN_AREA_ID, access_any, area_spec, expect, external, hidden_internal, internal, recall,
    repop, resets, room, visual,
};

pub fn push_specs(specs: &mut Vec<(AreaSpec, TownExpectation)>) {
    specs.push((
        evermoon_hollow_area_spec(),
        expect(
            EVERMOON_HOLLOW_AREA_ID,
            1645,
            AreaKind::Town,
            false,
            true,
            5,
        ),
    ));
    specs.push((
        aldevari_area_spec(),
        expect(ALDEVARI_AREA_ID, 1645, AreaKind::Town, false, true, 5),
    ));
    specs.push((
        copeham_inn_area_spec(),
        expect(COPEHAM_INN_AREA_ID, 1645, AreaKind::Town, false, false, 4),
    ));
}

/// Evermoon Hollow: a hollow northwest of Voralian; a poisoner branch and evil
/// citizenry (ranking 20-25).
pub fn evermoon_hollow_area_spec() -> AreaSpec {
    let aid = EVERMOON_HOLLOW_AREA_ID;
    area_spec(
        aid,
        "Evermoon Hollow",
        AreaKind::Town,
        1645,
        &[1601],
        "evermoon-hollow",
        (20, 30),
        "A moonlit hollow of the northwest woods below Voralian, its evil folk quietly \
         sheltering a poisoner branch among the herb-groves.",
        vec![
            room(
                "room.evermoon_hollow.gate",
                "Hollow Gate",
                RoomKind::Gate,
                "A vine-wound gate into the shadowed hollow.",
                RoomConcealment::GateChokepoint,
                &["npc.evermoon_hollow.gate_ward"],
            ),
            room(
                "room.evermoon_hollow.green",
                "Moonlit Green",
                RoomKind::Intersection,
                "The central green where the evil citizens gather (ranking 20-25).",
                RoomConcealment::CrowdCover,
                &["npc.evermoon_hollow.citizen", "npc.evermoon_hollow.guard"],
            ),
            room(
                "room.evermoon_hollow.poisoner_grove",
                "Nightshade Grove",
                RoomKind::Guildhall,
                "A screened herb-grove where the poisoner branch works.",
                RoomConcealment::AlleyShadow,
                &["npc.evermoon_hollow.poison_master"],
            ),
            room(
                "room.evermoon_hollow.apothecary",
                "Hollow Apothecary",
                RoomKind::Shop,
                "A shop of tinctures, roots, and vials.",
                RoomConcealment::None,
                &["npc.evermoon_hollow.apothecary"],
            ),
            room(
                "room.evermoon_hollow.moonwell",
                "Moonwell Shrine",
                RoomKind::Temple,
                "A silver-watered well that serves as the recall point.",
                RoomConcealment::None,
                &["npc.evermoon_hollow.well_keeper"],
            ),
        ],
        vec![
            internal(
                "conn.evermoon_hollow.gate_to_green",
                "room.evermoon_hollow.gate",
                aid,
                "room.evermoon_hollow.green",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.evermoon_hollow.green_to_apothecary",
                "room.evermoon_hollow.green",
                aid,
                "room.evermoon_hollow.apothecary",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.evermoon_hollow.green_to_moonwell",
                "room.evermoon_hollow.green",
                aid,
                "room.evermoon_hollow.moonwell",
                ConnectionDirection::North,
                TravelKind::Path,
            ),
            hidden_internal(
                "conn.evermoon_hollow.grove_screen",
                "room.evermoon_hollow.green",
                aid,
                "room.evermoon_hollow.poisoner_grove",
                ConnectionDirection::West,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
            external(
                "conn.evermoon_hollow.to_voralian",
                "room.evermoon_hollow.gate",
                VORALIAN_AREA_ID,
                ConnectionDirection::Southeast,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.evermoon_hollow.cycle", 30),
        repop("repop.evermoon_hollow.pop", 22, 15, 2),
        recall(
            false,
            Some("room.evermoon_hollow.moonwell"),
            false,
            access_any(),
        ),
        visual(
            "tileset.town.evermoon_hollow.moonwood",
            "model.town.evermoon_hollow.hollow",
            &[
                "prop.town.vine_gate",
                "prop.town.moonwell",
                "prop.town.herb_grove",
            ],
            "moonlit woodland hollow of thatched cottages around a silver well",
            "Original moonwood hollow props and herb-grove dressing; no copied iconography.",
        ),
        &[
            "Poisoner branch is a ConcealedTrail off the green (#944).",
            "Citizens/guards are evil ranking 20-25 (#1645); road runs toward Voralian (#1632).",
        ],
    )
}

/// Aldevari: pastoral village east of Galadon's east gate; the classic 1-10 evil
/// newbie roster (men, women, guards, hunters, wolves).
pub fn aldevari_area_spec() -> AreaSpec {
    let aid = ALDEVARI_AREA_ID;
    area_spec(
        aid,
        "Aldevari",
        AreaKind::Town,
        1645,
        &[],
        "aldevari",
        (1, 10),
        "A deceptively pastoral village down the Eastern Road, whose men, women, guards, \
         and hunters (with their wolves) form the classic low-rank evil roster.",
        vec![
            room(
                "room.aldevari.east_gate",
                "Village Gate",
                RoomKind::Gate,
                "A rustic gate onto the Eastern Road.",
                RoomConcealment::GateChokepoint,
                &["npc.aldevari.guard"],
            ),
            room(
                "room.aldevari.village_green",
                "Village Green",
                RoomKind::Intersection,
                "A sunny green of villagers who bite (men and women, evil ranking 1-10).",
                RoomConcealment::CrowdCover,
                &["npc.aldevari.man", "npc.aldevari.woman"],
            ),
            room(
                "room.aldevari.hunters_lodge",
                "Hunters' Lodge",
                RoomKind::Landmark,
                "A lodge of hunters and their wolves.",
                RoomConcealment::None,
                &["npc.aldevari.hunter", "npc.aldevari.wolf"],
            ),
            room(
                "room.aldevari.tavern",
                "Green Bough Tavern",
                RoomKind::Inn,
                "A cheery tavern with a darker undertone.",
                RoomConcealment::None,
                &["npc.aldevari.barkeep"],
            ),
            room(
                "room.aldevari.chapel",
                "Village Chapel",
                RoomKind::Temple,
                "A small chapel that serves as the recall point.",
                RoomConcealment::None,
                &["npc.aldevari.priest"],
            ),
        ],
        vec![
            internal(
                "conn.aldevari.gate_to_green",
                "room.aldevari.east_gate",
                aid,
                "room.aldevari.village_green",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.aldevari.green_to_lodge",
                "room.aldevari.village_green",
                aid,
                "room.aldevari.hunters_lodge",
                ConnectionDirection::North,
                TravelKind::Path,
            ),
            internal(
                "conn.aldevari.green_to_tavern",
                "room.aldevari.village_green",
                aid,
                "room.aldevari.tavern",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.aldevari.green_to_chapel",
                "room.aldevari.village_green",
                aid,
                "room.aldevari.chapel",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            external(
                "conn.aldevari.to_galadon",
                "room.aldevari.east_gate",
                GALADON_AREA_ID,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.aldevari.cycle", 30),
        repop("repop.aldevari.pop", 24, 15, 2),
        recall(false, Some("room.aldevari.chapel"), false, access_any()),
        visual(
            "tileset.town.aldevari.pastoral",
            "model.town.aldevari.village",
            &[
                "prop.town.rustic_gate",
                "prop.town.village_well",
                "prop.town.chapel_bell",
            ],
            "sunny pastoral village of cottages and a green, hiding a predatory streak",
            "Original pastoral village props and chapel dressing; no copied iconography.",
        ),
        &[
            "The full 1-10 evil newbie roster: men, women, guards, hunters, wolves (#1645).",
            "Eastern Road edge runs back to the Galadon east gate (#1632).",
        ],
    )
}

/// Copeham Inn: roadside RP inn west of Galadon; resolves Galadon's forward-stub
/// edge. Twins/servants/locals are low-rank evil (1-5).
pub fn copeham_inn_area_spec() -> AreaSpec {
    let aid = COPEHAM_INN_AREA_ID;
    area_spec(
        aid,
        "Copeham Inn",
        AreaKind::Town,
        1645,
        &[],
        "copeham-inn",
        (1, 30),
        "A warm roadside inn on the western way out of Galadon, a roleplay waystop whose \
         twins, servants, and locals keep a quietly wicked streak (ranking 1-5).",
        vec![
            room(
                "room.copeham_inn.yard",
                "Inn Yard",
                RoomKind::Gate,
                "The roadside yard and hitching post before the door.",
                RoomConcealment::GateChokepoint,
                &["npc.copeham_inn.stablehand"],
            ),
            room(
                "room.copeham_inn.common_room",
                "Common Room",
                RoomKind::Inn,
                "A hearth-lit common room of travellers, twins, and locals (evil ranking 1-5).",
                RoomConcealment::CrowdCover,
                &["npc.copeham_inn.twin", "npc.copeham_inn.local"],
            ),
            room(
                "room.copeham_inn.kitchen",
                "Inn Kitchen",
                RoomKind::Chamber,
                "A busy kitchen of servants and steaming pots.",
                RoomConcealment::None,
                &["npc.copeham_inn.servant"],
            ),
            room(
                "room.copeham_inn.guest_rooms",
                "Guest Rooms",
                RoomKind::Inn,
                "Quiet upstairs rooms for the night.",
                RoomConcealment::None,
                &["npc.copeham_inn.innkeeper"],
            ),
        ],
        vec![
            internal(
                "conn.copeham_inn.yard_to_common",
                "room.copeham_inn.yard",
                aid,
                "room.copeham_inn.common_room",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.copeham_inn.common_to_kitchen",
                "room.copeham_inn.common_room",
                aid,
                "room.copeham_inn.kitchen",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.copeham_inn.common_to_guest",
                "room.copeham_inn.common_room",
                aid,
                "room.copeham_inn.guest_rooms",
                ConnectionDirection::Up,
                TravelKind::Stair,
            ),
            external(
                "conn.copeham_inn.to_galadon",
                "room.copeham_inn.yard",
                GALADON_AREA_ID,
                ConnectionDirection::East,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.copeham_inn.cycle", 30),
        repop("repop.copeham_inn.pop", 14, 20, 1),
        recall(false, None, false, access_any()),
        visual(
            "tileset.town.copeham_inn.roadside",
            "model.town.copeham_inn.inn",
            &[
                "prop.town.hitching_post",
                "prop.town.common_hearth",
                "prop.town.inn_sign",
            ],
            "half-timbered roadside inn with a lantern-lit yard and upstairs rooms",
            "Original roadside-inn props and signage; no copied iconography.",
        ),
        &[
            "Reuses area.thera.galadon.copeham_inn to resolve Galadon's forward-stub edge (#1636).",
            "RP waystop; twins/servants/locals are evil ranking 1-5 (#1645).",
        ],
    )
}
