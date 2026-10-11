// #1644 northwest guild towns, group B: Ayr-Trinil, Cragstone, Akan, Castle of Akan.
// CF-factual topology/ranking rosters/level-bands; name and lore originalized (#984).
use litd_engine::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RoomConcealment,
    RoomKind, TravelKind,
};

use super::towns_thera_support::{
    AKAN_AREA_ID, AYR_TRINIL_AREA_ID, BLACKCLAW_AREA_ID, CASTLE_OF_AKAN_AREA_ID, CRAGSTONE_AREA_ID,
    RUINS_OF_MAETHIEN_AREA_ID, TownExpectation, UNDERDARK_AREA_ID, access_any, area_spec, expect,
    external, hidden_internal, internal, recall, repop, resets, room, visual,
};

pub fn push_specs(specs: &mut Vec<(AreaSpec, TownExpectation)>) {
    specs.push((
        ayr_trinil_area_spec(),
        expect(AYR_TRINIL_AREA_ID, 1644, AreaKind::Town, false, true, 5),
    ));
    specs.push((
        cragstone_area_spec(),
        expect(CRAGSTONE_AREA_ID, 1644, AreaKind::Town, false, false, 6),
    ));
    specs.push((
        akan_area_spec(),
        expect(AKAN_AREA_ID, 1644, AreaKind::Town, false, true, 5),
    ));
    specs.push((
        castle_of_akan_area_spec(),
        expect(
            CASTLE_OF_AKAN_AREA_ID,
            1644,
            AreaKind::Town,
            false,
            false,
            5,
        ),
    ));
}

/// Ayr-Trinil: NW prison city near the Ruins of Maethien; a trapper branch and two
/// warring ranking rosters (prisoners/dark guards good 25-45; guards/soldiers evil 30-35).
pub fn ayr_trinil_area_spec() -> AreaSpec {
    let aid = AYR_TRINIL_AREA_ID;
    area_spec(
        aid,
        "Ayr-Trinil",
        AreaKind::Town,
        1644,
        &[1601],
        "ayr-trinil",
        (25, 45),
        "A grim prison-city of high walls in the northwest interior, where the trapper \
         branch trains and jailers and captives make their own bloody ranking war.",
        vec![
            room(
                "room.ayr_trinil.gate",
                "Iron Gate",
                RoomKind::Gate,
                "The barred outer gate of the prison-city.",
                RoomConcealment::GateChokepoint,
                &["npc.ayr_trinil.gate_soldier"],
            ),
            room(
                "room.ayr_trinil.prison_yard",
                "Prison Yard",
                RoomKind::Landmark,
                "A walled yard of prisoners and their dark guards (good ranking 25-45).",
                RoomConcealment::CrowdCover,
                &["npc.ayr_trinil.prisoner", "npc.ayr_trinil.dark_guard"],
            ),
            room(
                "room.ayr_trinil.barracks",
                "Soldier Barracks",
                RoomKind::Landmark,
                "The garrison of guards and soldiers (evil ranking 30-35).",
                RoomConcealment::None,
                &["npc.ayr_trinil.soldier"],
            ),
            room(
                "room.ayr_trinil.trapper_post",
                "Trapper Post",
                RoomKind::Guildhall,
                "The trapper guild's northern branch office.",
                RoomConcealment::None,
                &["npc.ayr_trinil.trapper_master"],
            ),
            room(
                "room.ayr_trinil.warden_shrine",
                "Warden's Shrine",
                RoomKind::Temple,
                "A stern shrine in the keep that serves as the recall point.",
                RoomConcealment::None,
                &["npc.ayr_trinil.warden"],
            ),
        ],
        vec![
            internal(
                "conn.ayr_trinil.gate_to_yard",
                "room.ayr_trinil.gate",
                aid,
                "room.ayr_trinil.prison_yard",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.ayr_trinil.yard_to_barracks",
                "room.ayr_trinil.prison_yard",
                aid,
                "room.ayr_trinil.barracks",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.ayr_trinil.yard_to_trapper",
                "room.ayr_trinil.prison_yard",
                aid,
                "room.ayr_trinil.trapper_post",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            internal(
                "conn.ayr_trinil.yard_to_shrine",
                "room.ayr_trinil.prison_yard",
                aid,
                "room.ayr_trinil.warden_shrine",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            external(
                "conn.ayr_trinil.to_maethien",
                "room.ayr_trinil.gate",
                RUINS_OF_MAETHIEN_AREA_ID,
                ConnectionDirection::Northwest,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.ayr_trinil.cycle", 25),
        repop("repop.ayr_trinil.pop", 30, 12, 3),
        recall(
            false,
            Some("room.ayr_trinil.warden_shrine"),
            false,
            access_any(),
        ),
        visual(
            "tileset.town.ayr_trinil.prison_stone",
            "model.town.ayr_trinil.prison_city",
            &[
                "prop.town.iron_portcullis",
                "prop.town.cell_bars",
                "prop.town.watch_tower",
                "prop.town.warden_shrine",
            ],
            "high-walled prison city of grey stone with watchtowers over a barred yard",
            "Original prison-city stonework and iron props; no copied iconography.",
        ),
        &[
            "Two ranking rosters coexist: good prisoners/dark guards 25-45, evil garrison 30-35 (#1644).",
            "NW-interior edge links toward the Ruins of Maethien (#1632).",
        ],
    )
}

/// Cragstone (Upper + Lower): interior stone town whose Lower district hides the
/// surface door down into the Underdark; Upper hosts a trapper branch.
pub fn cragstone_area_spec() -> AreaSpec {
    let aid = CRAGSTONE_AREA_ID;
    area_spec(
        aid,
        "Cragstone",
        AreaKind::Town,
        1644,
        &[1601, 1632],
        "cragstone",
        (30, 51),
        "A two-tier interior town of grey crag stone; Upper Cragstone keeps a trapper \
         branch, and Lower Cragstone's evil citizens guard the surface door to the Underdark.",
        vec![
            room(
                "room.cragstone.upper_gate",
                "Upper Gate",
                RoomKind::Gate,
                "The high gate onto the interior road.",
                RoomConcealment::GateChokepoint,
                &["npc.cragstone.gate_ward"],
            ),
            room(
                "room.cragstone.upper_market",
                "Upper Market",
                RoomKind::Market,
                "The tiered market of Upper Cragstone.",
                RoomConcealment::CrowdCover,
                &["npc.cragstone.merchant"],
            ),
            room(
                "room.cragstone.trapper_branch",
                "Crag Trapper Branch",
                RoomKind::Guildhall,
                "The trapper guild's mountain branch.",
                RoomConcealment::None,
                &["npc.cragstone.trapper_master"],
            ),
            room(
                "room.cragstone.descent_stair",
                "Great Descent",
                RoomKind::Intersection,
                "A long carved stair joining the upper and lower districts.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.cragstone.lower_commons",
                "Lower Commons",
                RoomKind::Intersection,
                "The sunless lower district; its citizens are evil (ranking 40-45).",
                RoomConcealment::None,
                &["npc.cragstone.lower_citizen"],
            ),
            room(
                "room.cragstone.underdark_door",
                "The Deep Door",
                RoomKind::Gate,
                "A concealed surface door sinking into the Underdark.",
                RoomConcealment::HiddenLayer,
                &["npc.cragstone.door_warden"],
            ),
        ],
        vec![
            internal(
                "conn.cragstone.gate_to_market",
                "room.cragstone.upper_gate",
                aid,
                "room.cragstone.upper_market",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.cragstone.market_to_trapper",
                "room.cragstone.upper_market",
                aid,
                "room.cragstone.trapper_branch",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.cragstone.market_to_stair",
                "room.cragstone.upper_market",
                aid,
                "room.cragstone.descent_stair",
                ConnectionDirection::Down,
                TravelKind::Stair,
            ),
            internal(
                "conn.cragstone.stair_to_lower",
                "room.cragstone.descent_stair",
                aid,
                "room.cragstone.lower_commons",
                ConnectionDirection::Down,
                TravelKind::Stair,
            ),
            internal(
                "conn.cragstone.lower_to_door",
                "room.cragstone.lower_commons",
                aid,
                "room.cragstone.underdark_door",
                ConnectionDirection::West,
                TravelKind::Path,
            ),
            hidden_internal(
                "conn.cragstone.door_seam",
                "room.cragstone.lower_commons",
                aid,
                "room.cragstone.underdark_door",
                ConnectionDirection::Down,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::Hidden,
            ),
            external(
                "conn.cragstone.to_underdark",
                "room.cragstone.underdark_door",
                UNDERDARK_AREA_ID,
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            external(
                "conn.cragstone.to_galadon",
                "room.cragstone.upper_gate",
                GALADON_AREA_ID,
                ConnectionDirection::Southeast,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.cragstone.cycle", 30),
        repop("repop.cragstone.pop", 28, 15, 3),
        recall(false, None, false, access_any()),
        visual(
            "tileset.town.cragstone.crag_stone",
            "model.town.cragstone.two_tier",
            &[
                "prop.town.crag_gate",
                "prop.town.great_stair",
                "prop.town.deep_door",
                "prop.town.market_awning",
            ],
            "two-tier grey crag town with a great carved stair descending to a sealed deep door",
            "Original crag-stone terracing and deep-door props; no copied iconography.",
        ),
        &[
            "The surface door to the Underdark is a Hidden descent (#944/#1632).",
            "Lower Cragstone citizens are evil ranking 40-45 (#1644).",
        ],
    )
}

/// Akan: interior road-web town of evil shopkeepers (ranking 20-25), beside its castle.
pub fn akan_area_spec() -> AreaSpec {
    let aid = AKAN_AREA_ID;
    area_spec(
        aid,
        "Akan",
        AreaKind::Town,
        1644,
        &[],
        "akan",
        (20, 30),
        "A prosperous interior town at a knot of roads, its evil shopkeepers thriving \
         in the shadow of the Castle of Akan.",
        vec![
            room(
                "room.akan.crossroad_gate",
                "Crossroad Gate",
                RoomKind::Gate,
                "The gate where the interior roads knot together.",
                RoomConcealment::GateChokepoint,
                &["npc.akan.gate_guard"],
            ),
            room(
                "room.akan.merchant_row",
                "Merchant Row",
                RoomKind::Market,
                "A prosperous row of evil shopkeepers (ranking 20-25).",
                RoomConcealment::CrowdCover,
                &["npc.akan.shopkeeper"],
            ),
            room(
                "room.akan.town_square",
                "Town Square",
                RoomKind::Intersection,
                "The central square beneath the castle walls.",
                RoomConcealment::None,
                &["npc.akan.townsfolk"],
            ),
            room(
                "room.akan.inn",
                "Roadknot Inn",
                RoomKind::Inn,
                "A busy inn at the meeting of the roads.",
                RoomConcealment::None,
                &["npc.akan.innkeeper"],
            ),
            room(
                "room.akan.chapel",
                "Town Chapel",
                RoomKind::Temple,
                "A modest chapel serving as the town recall point.",
                RoomConcealment::None,
                &["npc.akan.chaplain"],
            ),
        ],
        vec![
            internal(
                "conn.akan.gate_to_row",
                "room.akan.crossroad_gate",
                aid,
                "room.akan.merchant_row",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.akan.row_to_square",
                "room.akan.merchant_row",
                aid,
                "room.akan.town_square",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.akan.square_to_inn",
                "room.akan.town_square",
                aid,
                "room.akan.inn",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.akan.square_to_chapel",
                "room.akan.town_square",
                aid,
                "room.akan.chapel",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            external(
                "conn.akan.to_castle",
                "room.akan.town_square",
                CASTLE_OF_AKAN_AREA_ID,
                ConnectionDirection::North,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.akan.to_blackclaw",
                "room.akan.crossroad_gate",
                BLACKCLAW_AREA_ID,
                ConnectionDirection::Southwest,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.akan.cycle", 30),
        repop("repop.akan.pop", 26, 15, 2),
        recall(false, Some("room.akan.chapel"), false, access_any()),
        visual(
            "tileset.town.akan.road_town",
            "model.town.akan.market_town",
            &[
                "prop.town.crossroad_gate",
                "prop.town.market_stall",
                "prop.town.town_chapel",
            ],
            "prosperous road-knot town of shopfronts under a castle's shadow",
            "Original road-town shopfront and chapel props; no copied iconography.",
        ),
        &[
            "Evil shopkeepers ranking 20-25 (#1644).",
            "Road web links to the Castle of Akan and back toward Blackclaw (#1632).",
        ],
    )
}

#[path = "towns_thera_fixture_nw_b_castle.rs"]
mod split;
pub use split::castle_of_akan_area_spec;
