// #1645 central & eastern villages, group B: Inn of the Eternal Star, Balator,
// Vale of Arendyl. CF-factual topology/rosters/level-bands; lore originalized (#984).
use litd_engine::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RoomConcealment,
    RoomKind, TravelKind,
};

use super::towns_thera_support::{
    BALATOR_AREA_ID, FIELDS_OF_BALATORN_AREA_ID, INN_OF_THE_ETERNAL_STAR_AREA_ID, TownExpectation,
    VALE_OF_ARENDYL_AREA_ID, access_any, area_spec, expect, external, internal, recall, repop,
    resets, room, visual,
};

pub fn push_specs(specs: &mut Vec<(AreaSpec, TownExpectation)>) {
    specs.push((
        inn_of_the_eternal_star_area_spec(),
        expect(
            INN_OF_THE_ETERNAL_STAR_AREA_ID,
            1645,
            AreaKind::Town,
            false,
            false,
            5,
        ),
    ));
    specs.push((
        balator_area_spec(),
        expect(BALATOR_AREA_ID, 1645, AreaKind::Town, false, true, 5),
    ));
    specs.push((
        vale_of_arendyl_area_spec(),
        expect(
            VALE_OF_ARENDYL_AREA_ID,
            1645,
            AreaKind::Wilderness,
            false,
            true,
            5,
        ),
    ));
}

/// Inn of the Eternal Star: the herald cabal's all-ranks social hub off the northern
/// Crossroads; a no-PK place to relax and hear a story (ties #990 cabal HQs).
pub fn inn_of_the_eternal_star_area_spec() -> AreaSpec {
    let aid = INN_OF_THE_ETERNAL_STAR_AREA_ID;
    area_spec(
        aid,
        "Inn of the Eternal Star",
        AreaKind::Town,
        1645,
        &[990],
        "inn-of-the-eternal-star",
        (1, 51),
        "A lantern-crowned inn off the northern Crossroads where the herald cabal keeps \
         an all-ranks social hearth: a place to set down the blade and hear a story.",
        vec![
            room(
                "room.inn_eternal_star.threshold",
                "Star Threshold",
                RoomKind::Gate,
                "The star-carved doorway welcoming travellers of every rank.",
                RoomConcealment::GateChokepoint,
                &["npc.inn_eternal_star.doorkeeper"],
            ),
            room(
                "room.inn_eternal_star.great_common",
                "Great Common",
                RoomKind::Inn,
                "A vast hearth-common where old foes drink under truce.",
                RoomConcealment::CrowdCover,
                &["npc.inn_eternal_star.innkeeper"],
            ),
            room(
                "room.inn_eternal_star.story_circle",
                "Story Circle",
                RoomKind::Landmark,
                "A ring of settles around a teller's stool.",
                RoomConcealment::None,
                &["npc.inn_eternal_star.storyteller"],
            ),
            room(
                "room.inn_eternal_star.herald_alcove",
                "Herald Alcove",
                RoomKind::Guildhall,
                "The herald cabal's alcove and its keeper of tales.",
                RoomConcealment::None,
                &["npc.inn_eternal_star.herald_keeper"],
            ),
            room(
                "room.inn_eternal_star.quiet_rooms",
                "Quiet Rooms",
                RoomKind::Inn,
                "Still upstairs rooms for rest and reflection.",
                RoomConcealment::None,
                &["npc.inn_eternal_star.steward"],
            ),
        ],
        vec![
            internal(
                "conn.inn_eternal_star.threshold_to_common",
                "room.inn_eternal_star.threshold",
                aid,
                "room.inn_eternal_star.great_common",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.inn_eternal_star.common_to_circle",
                "room.inn_eternal_star.great_common",
                aid,
                "room.inn_eternal_star.story_circle",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.inn_eternal_star.common_to_alcove",
                "room.inn_eternal_star.great_common",
                aid,
                "room.inn_eternal_star.herald_alcove",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.inn_eternal_star.common_to_quiet",
                "room.inn_eternal_star.great_common",
                aid,
                "room.inn_eternal_star.quiet_rooms",
                ConnectionDirection::Up,
                TravelKind::Stair,
            ),
            external(
                "conn.inn_eternal_star.to_galadon",
                "room.inn_eternal_star.threshold",
                GALADON_AREA_ID,
                ConnectionDirection::Southwest,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.inn_eternal_star.cycle", 30),
        repop("repop.inn_eternal_star.pop", 16, 20, 1),
        recall(false, None, false, access_any()),
        visual(
            "tileset.town.inn_eternal_star.hearthglow",
            "model.town.inn_eternal_star.great_inn",
            &[
                "prop.town.star_lantern",
                "prop.town.common_hearth",
                "prop.town.teller_stool",
            ],
            "great galleried inn under a crown of star-lanterns around a central hearth",
            "Original herald-inn props and star-lantern dressing; no copied iconography.",
        ),
        &[
            "All-ranks no-PK social space; herald cabal hub ties #990.",
            "Reached off the northern Crossroads; edge runs back toward Galadon (#1632).",
        ],
    )
}

/// Balator: a grim 10-40 town whose orphans (evil ranking 10-15) preserve CF's dark
/// humour; the Fields of Balatorn lie beyond.
pub fn balator_area_spec() -> AreaSpec {
    let aid = BALATOR_AREA_ID;
    area_spec(
        aid,
        "Balator",
        AreaKind::Town,
        1645,
        &[],
        "balator",
        (10, 40),
        "A close-walled town of grey humour where even the orphans (evil ranking 10-15) \
         are not to be turned your back on; the Fields of Balatorn stretch beyond the wall.",
        vec![
            room(
                "room.balator.gate",
                "Town Gate",
                RoomKind::Gate,
                "A soot-stained gate onto the field road.",
                RoomConcealment::GateChokepoint,
                &["npc.balator.gate_guard"],
            ),
            room(
                "room.balator.square",
                "Grim Square",
                RoomKind::Intersection,
                "A cramped square under overhanging eaves.",
                RoomConcealment::CrowdCover,
                &["npc.balator.townsfolk"],
            ),
            room(
                "room.balator.orphanage",
                "The Orphanage",
                RoomKind::Landmark,
                "A crooked house of orphans best given a wide berth (evil ranking 10-15).",
                RoomConcealment::None,
                &["npc.balator.orphan"],
            ),
            room(
                "room.balator.market",
                "Balator Market",
                RoomKind::Market,
                "Thin stalls of hard bread and harder looks.",
                RoomConcealment::CrowdCover,
                &["npc.balator.vendor"],
            ),
            room(
                "room.balator.temple",
                "Grey Temple",
                RoomKind::Temple,
                "A soot-grey temple serving as the town recall point.",
                RoomConcealment::None,
                &["npc.balator.priest"],
            ),
        ],
        vec![
            internal(
                "conn.balator.gate_to_square",
                "room.balator.gate",
                aid,
                "room.balator.square",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.balator.square_to_orphanage",
                "room.balator.square",
                aid,
                "room.balator.orphanage",
                ConnectionDirection::North,
                TravelKind::Path,
            ),
            internal(
                "conn.balator.square_to_market",
                "room.balator.square",
                aid,
                "room.balator.market",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.balator.square_to_temple",
                "room.balator.square",
                aid,
                "room.balator.temple",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            external(
                "conn.balator.to_fields",
                "room.balator.gate",
                FIELDS_OF_BALATORN_AREA_ID,
                ConnectionDirection::South,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.balator.cycle", 30),
        repop("repop.balator.pop", 26, 15, 2),
        recall(false, Some("room.balator.temple"), false, access_any()),
        visual(
            "tileset.town.balator.grey_eaves",
            "model.town.balator.walled_town",
            &[
                "prop.town.soot_gate",
                "prop.town.crooked_house",
                "prop.town.grey_temple",
            ],
            "close-walled grey town of overhanging eaves beside open fields",
            "Original grey-town props and orphanage dressing; no copied iconography.",
        ),
        &[
            "Orphans are evil ranking 10-15 (grim CF humour preserved as content, #1645).",
            "Gate edge runs out to the Fields of Balatorn (#1632).",
        ],
    )
}

/// Vale of Arendyl: a wood-elf-flavoured vale through the Holy Road gate NW of
/// Galadon; resolves Galadon's forward-stub edge. Wilders/scouts/elders evil 5-10.
pub fn vale_of_arendyl_area_spec() -> AreaSpec {
    let aid = VALE_OF_ARENDYL_AREA_ID;
    area_spec(
        aid,
        "Vale of Arendyl",
        AreaKind::Wilderness,
        1645,
        &[1601],
        "vale-of-arendyl",
        (1, 30),
        "A green wood-elf vale reached through the gate on the Holy Road; its cooshees, \
         wilders, scouts, sentries, and elders (evil ranking 5-10) guard the deep glades.",
        vec![
            room(
                "room.vale_of_arendyl.holy_gate",
                "Holy Road Gate",
                RoomKind::Gate,
                "The vale's gate where the Holy Road ends.",
                RoomConcealment::GateChokepoint,
                &["npc.vale_of_arendyl.sentry"],
            ),
            room(
                "room.vale_of_arendyl.glade",
                "Sunlit Glade",
                RoomKind::Wilderness,
                "An open glade of wilders and their cooshee hounds (evil ranking 5-10).",
                RoomConcealment::CrowdCover,
                &["npc.vale_of_arendyl.wilder", "npc.vale_of_arendyl.cooshee"],
            ),
            room(
                "room.vale_of_arendyl.scout_trail",
                "Scout Trail",
                RoomKind::Wilderness,
                "A winding trail walked by scouts and sentries.",
                RoomConcealment::AlleyShadow,
                &["npc.vale_of_arendyl.scout"],
            ),
            room(
                "room.vale_of_arendyl.elder_grove",
                "Elder Grove",
                RoomKind::Landmark,
                "A ring of ancient trees where the elders council.",
                RoomConcealment::None,
                &["npc.vale_of_arendyl.elder"],
            ),
            room(
                "room.vale_of_arendyl.wildshrine",
                "Wild Shrine",
                RoomKind::Temple,
                "A mossy shrine that serves as the vale recall point.",
                RoomConcealment::None,
                &["npc.vale_of_arendyl.shrine_warden"],
            ),
        ],
        vec![
            internal(
                "conn.vale_of_arendyl.gate_to_glade",
                "room.vale_of_arendyl.holy_gate",
                aid,
                "room.vale_of_arendyl.glade",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.vale_of_arendyl.glade_to_trail",
                "room.vale_of_arendyl.glade",
                aid,
                "room.vale_of_arendyl.scout_trail",
                ConnectionDirection::North,
                TravelKind::Path,
            ),
            internal(
                "conn.vale_of_arendyl.trail_to_grove",
                "room.vale_of_arendyl.scout_trail",
                aid,
                "room.vale_of_arendyl.elder_grove",
                ConnectionDirection::North,
                TravelKind::Path,
            ),
            internal(
                "conn.vale_of_arendyl.glade_to_shrine",
                "room.vale_of_arendyl.glade",
                aid,
                "room.vale_of_arendyl.wildshrine",
                ConnectionDirection::West,
                TravelKind::Path,
            ),
            external(
                "conn.vale_of_arendyl.to_galadon",
                "room.vale_of_arendyl.holy_gate",
                GALADON_AREA_ID,
                ConnectionDirection::Southeast,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.vale_of_arendyl.cycle", 30),
        repop("repop.vale_of_arendyl.pop", 24, 15, 2),
        recall(
            false,
            Some("room.vale_of_arendyl.wildshrine"),
            false,
            access_any(),
        ),
        visual(
            "tileset.wild.vale_of_arendyl.woodvale",
            "model.wild.vale_of_arendyl.glades",
            &[
                "prop.wild.vale_gate_arch",
                "prop.wild.elder_tree",
                "prop.wild.moss_shrine",
            ],
            "green wood-elf vale of sunlit glades and ancient elder trees",
            "Original wood-vale props and elder-grove dressing; no copied iconography.",
        ),
        &[
            "Reuses area.thera.vale_of_arendyl to resolve Galadon's forward-stub edge (#1636).",
            "Wilders/scouts/elders evil ranking 5-10; reached via the Holy Road gate (#1632).",
        ],
    )
}
