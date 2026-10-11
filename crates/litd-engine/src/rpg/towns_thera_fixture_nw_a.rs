// #1644 northwest guild towns, group A: Hillcrest, Dagdan, Blackclaw.
// CF-factual topology/guild-presence/level-bands; name and lore originalized (#984).
use litd_engine::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RoomConcealment,
    RoomKind, TravelKind,
};

use super::towns_thera_support::{
    BLACKCLAW_AREA_ID, DAGDAN_AREA_ID, HILLCREST_AREA_ID, THE_WASTES_AREA_ID, TownExpectation,
    access_any, area_spec, expect, external, hidden_internal, internal, recall, repop, resets,
    room, visual,
};

pub fn push_specs(specs: &mut Vec<(AreaSpec, TownExpectation)>) {
    specs.push((
        hillcrest_area_spec(),
        expect(HILLCREST_AREA_ID, 1644, AreaKind::Town, false, true, 5),
    ));
    specs.push((
        dagdan_area_spec(),
        expect(DAGDAN_AREA_ID, 1644, AreaKind::Town, false, false, 4),
    ));
    specs.push((
        blackclaw_area_spec(),
        expect(BLACKCLAW_AREA_ID, 1644, AreaKind::Town, false, false, 4),
    ));
}

/// Hillcrest: frontier mining town north past the Wastes on the Northern Trail;
/// hosts the poisoner path branch.
pub fn hillcrest_area_spec() -> AreaSpec {
    let aid = HILLCREST_AREA_ID;
    area_spec(
        aid,
        "Hillcrest",
        AreaKind::Town,
        1644,
        &[1601, 1629],
        "hillcrest",
        (15, 40),
        "A hard frontier mining town at the cold end of the Northern Trail, a quiet \
         waypoint that hides the poisoner path's back rooms.",
        vec![
            room(
                "room.hillcrest.trail_gate",
                "Trail Gate",
                RoomKind::Gate,
                "The palisade gate where the Northern Trail enters town.",
                RoomConcealment::GateChokepoint,
                &["npc.hillcrest.gate_watch"],
            ),
            room(
                "room.hillcrest.market_square",
                "Market Square",
                RoomKind::Market,
                "A muddy square of ore-buyers and provisioners.",
                RoomConcealment::CrowdCover,
                &["npc.hillcrest.ore_buyer"],
            ),
            room(
                "room.hillcrest.mine_head",
                "Mine Head",
                RoomKind::Landmark,
                "The gaping pit-head and windlass of the town's silver mine.",
                RoomConcealment::None,
                &["npc.hillcrest.miner"],
            ),
            room(
                "room.hillcrest.poisoner_den",
                "Alchemist's Back Room",
                RoomKind::Guildhall,
                "A concealed still-room where the poisoner branch trains.",
                RoomConcealment::AlleyShadow,
                &["npc.hillcrest.poison_master"],
            ),
            room(
                "room.hillcrest.roadside_inn",
                "Roadside Inn",
                RoomKind::Inn,
                "Bunks and stew for trail-weary prospectors.",
                RoomConcealment::None,
                &["npc.hillcrest.innkeeper"],
            ),
            room(
                "room.hillcrest.shrine",
                "Wayside Shrine",
                RoomKind::Temple,
                "A small shrine that serves as the town recall point.",
                RoomConcealment::None,
                &["npc.hillcrest.shrine_keeper"],
            ),
        ],
        vec![
            internal(
                "conn.hillcrest.gate_to_market",
                "room.hillcrest.trail_gate",
                aid,
                "room.hillcrest.market_square",
                ConnectionDirection::North,
                TravelKind::Gate,
            ),
            internal(
                "conn.hillcrest.market_to_mine",
                "room.hillcrest.market_square",
                aid,
                "room.hillcrest.mine_head",
                ConnectionDirection::East,
                TravelKind::Path,
            ),
            internal(
                "conn.hillcrest.market_to_inn",
                "room.hillcrest.market_square",
                aid,
                "room.hillcrest.roadside_inn",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            internal(
                "conn.hillcrest.market_to_shrine",
                "room.hillcrest.market_square",
                aid,
                "room.hillcrest.shrine",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            hidden_internal(
                "conn.hillcrest.den_alley",
                "room.hillcrest.market_square",
                aid,
                "room.hillcrest.poisoner_den",
                ConnectionDirection::Down,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
            external(
                "conn.hillcrest.to_wastes",
                "room.hillcrest.trail_gate",
                THE_WASTES_AREA_ID,
                ConnectionDirection::South,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.hillcrest.cycle", 30),
        repop("repop.hillcrest.pop", 22, 15, 2),
        recall(false, Some("room.hillcrest.shrine"), false, access_any()),
        visual(
            "tileset.town.hillcrest.frontier_timber",
            "model.town.hillcrest.mining_camp",
            &[
                "prop.town.palisade_gate",
                "prop.town.mine_windlass",
                "prop.town.market_awning",
                "prop.town.wayside_shrine",
            ],
            "log-palisade mining town clinging to a cold hillside above a pit-head",
            "Original frontier timber and ore-cart props; no copied iconography.",
        ),
        &[
            "Poisoner branch is a ConcealedTrail off the market (#944).",
            "Northern Trail edge runs south toward the Wastes and the Crossroads (#1632).",
        ],
    )
}

/// Dagdan: rough west-road town, headquarters of the thug guild (membership).
pub fn dagdan_area_spec() -> AreaSpec {
    let aid = DAGDAN_AREA_ID;
    area_spec(
        aid,
        "Dagdan",
        AreaKind::Town,
        1644,
        &[1601],
        "dagdan",
        (10, 35),
        "A rough road town of mud and muscle on the western way, where the thug guild \
         keeps its unruly hall.",
        vec![
            room(
                "room.dagdan.west_gate",
                "West Gate",
                RoomKind::Gate,
                "A leaning gate onto the rutted western road.",
                RoomConcealment::GateChokepoint,
                &["npc.dagdan.gate_tough"],
            ),
            room(
                "room.dagdan.mud_market",
                "Mud Market",
                RoomKind::Market,
                "A crowded, thievish market of cheap goods.",
                RoomConcealment::CrowdCover,
                &["npc.dagdan.fence"],
            ),
            room(
                "room.dagdan.thug_hall",
                "Thug Hall",
                RoomKind::Guildhall,
                "The membership hall and guildmaster of the thug guild.",
                RoomConcealment::None,
                &["npc.dagdan.thug_guildmaster"],
            ),
            room(
                "room.dagdan.tavern",
                "Broken Tooth Tavern",
                RoomKind::Inn,
                "A brawling tavern with rooms above.",
                RoomConcealment::None,
                &["npc.dagdan.barkeep"],
            ),
            room(
                "room.dagdan.smithy",
                "Back-Alley Smithy",
                RoomKind::Shop,
                "A soot-black smithy dealing in blunt iron.",
                RoomConcealment::AlleyShadow,
                &["npc.dagdan.smith"],
            ),
        ],
        vec![
            internal(
                "conn.dagdan.gate_to_market",
                "room.dagdan.west_gate",
                aid,
                "room.dagdan.mud_market",
                ConnectionDirection::East,
                TravelKind::Gate,
            ),
            internal(
                "conn.dagdan.market_to_hall",
                "room.dagdan.mud_market",
                aid,
                "room.dagdan.thug_hall",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.dagdan.market_to_tavern",
                "room.dagdan.mud_market",
                aid,
                "room.dagdan.tavern",
                ConnectionDirection::South,
                TravelKind::Road,
            ),
            internal(
                "conn.dagdan.market_to_smithy",
                "room.dagdan.mud_market",
                aid,
                "room.dagdan.smithy",
                ConnectionDirection::West,
                TravelKind::Path,
            ),
            external(
                "conn.dagdan.to_galadon",
                "room.dagdan.west_gate",
                GALADON_AREA_ID,
                ConnectionDirection::East,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.dagdan.cycle", 30),
        repop("repop.dagdan.pop", 26, 15, 2),
        recall(false, None, false, access_any()),
        visual(
            "tileset.town.dagdan.rough_road",
            "model.town.dagdan.mud_town",
            &[
                "prop.town.leaning_gate",
                "prop.town.market_stall",
                "prop.town.guild_banner",
            ],
            "rutted mud-street town with a fortified guild hall and brawl-worn tavern",
            "Original grimy road-town props and thug-guild heraldry; no copied iconography.",
        ),
        &[
            "Thug guild HQ is a visible membership hall (#1644).",
            "Western road edge links back to the Galadon hub (#1632).",
        ],
    )
}

/// Blackclaw Village: trapper guild headquarters on the Osta / Loch corridor.
pub fn blackclaw_area_spec() -> AreaSpec {
    let aid = BLACKCLAW_AREA_ID;
    area_spec(
        aid,
        "Blackclaw Village",
        AreaKind::Town,
        1644,
        &[1601],
        "blackclaw",
        (10, 30),
        "A backwoods trapping village on the western corridor, home to the trapper \
         guild's lodge and its snare-craft.",
        vec![
            room(
                "room.blackclaw.village_gate",
                "Village Gate",
                RoomKind::Gate,
                "A trophy-hung gate of antler and hide.",
                RoomConcealment::GateChokepoint,
                &["npc.blackclaw.gate_hunter"],
            ),
            room(
                "room.blackclaw.commons",
                "Village Commons",
                RoomKind::Intersection,
                "The central green where trails from the woods meet.",
                RoomConcealment::None,
                &["npc.blackclaw.villager"],
            ),
            room(
                "room.blackclaw.trapper_lodge",
                "Trapper Lodge",
                RoomKind::Guildhall,
                "The trapper guild's lodge and membership master.",
                RoomConcealment::None,
                &["npc.blackclaw.trapper_guildmaster"],
            ),
            room(
                "room.blackclaw.trading_post",
                "Fur Trading Post",
                RoomKind::Shop,
                "Racks of pelts, snares, and trail supplies.",
                RoomConcealment::None,
                &["npc.blackclaw.trader"],
            ),
            room(
                "room.blackclaw.hunters_rest",
                "Hunter's Rest",
                RoomKind::Inn,
                "A hearth-warmed lodge-inn for the trap lines.",
                RoomConcealment::None,
                &["npc.blackclaw.innkeeper"],
            ),
        ],
        vec![
            internal(
                "conn.blackclaw.gate_to_commons",
                "room.blackclaw.village_gate",
                aid,
                "room.blackclaw.commons",
                ConnectionDirection::North,
                TravelKind::Gate,
            ),
            internal(
                "conn.blackclaw.commons_to_lodge",
                "room.blackclaw.commons",
                aid,
                "room.blackclaw.trapper_lodge",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            internal(
                "conn.blackclaw.commons_to_post",
                "room.blackclaw.commons",
                aid,
                "room.blackclaw.trading_post",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.blackclaw.commons_to_inn",
                "room.blackclaw.commons",
                aid,
                "room.blackclaw.hunters_rest",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            external(
                "conn.blackclaw.to_galadon",
                "room.blackclaw.village_gate",
                GALADON_AREA_ID,
                ConnectionDirection::South,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.blackclaw.cycle", 30),
        repop("repop.blackclaw.pop", 20, 15, 2),
        recall(false, None, false, access_any()),
        visual(
            "tileset.town.blackclaw.backwoods",
            "model.town.blackclaw.trap_village",
            &[
                "prop.town.antler_gate",
                "prop.town.pelt_rack",
                "prop.town.lodge_hearth",
            ],
            "antler-gated woodland village of hide lodges around a trail commons",
            "Original backwoods trapping props and antler heraldry; no copied iconography.",
        ),
        &[
            "Trapper guild HQ is the visible lodge (#1644).",
            "Corridor edge links back toward the Galadon hub (#1632).",
        ],
    )
}
