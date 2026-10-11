use super::*;

/// Castle of Akan: the fortress above the town, staffed by evil servants (ranking 25-30).
pub fn castle_of_akan_area_spec() -> AreaSpec {
    let aid = CASTLE_OF_AKAN_AREA_ID;
    area_spec(
        aid,
        "Castle of Akan",
        AreaKind::Town,
        1644,
        &[],
        "castle-of-akan",
        (25, 35),
        "The dark fortress crowning the town of Akan, its evil servants and workers \
         (ranking 25-30) keeping the lord's cold halls.",
        vec![
            room(
                "room.castle_of_akan.barbican",
                "Barbican",
                RoomKind::Gate,
                "The gatehouse and drawbridge of the castle.",
                RoomConcealment::GateChokepoint,
                &["npc.castle_of_akan.gate_soldier"],
            ),
            room(
                "room.castle_of_akan.courtyard",
                "Inner Courtyard",
                RoomKind::Intersection,
                "A cold courtyard ringed by the keep's halls.",
                RoomConcealment::None,
                &["npc.castle_of_akan.worker"],
            ),
            room(
                "room.castle_of_akan.great_hall",
                "Great Hall",
                RoomKind::Landmark,
                "The lord's banqueting hall of black stone.",
                RoomConcealment::None,
                &["npc.castle_of_akan.servant"],
            ),
            room(
                "room.castle_of_akan.armory",
                "Castle Armory",
                RoomKind::Shop,
                "Racked arms and mail for the garrison.",
                RoomConcealment::None,
                &["npc.castle_of_akan.armorer"],
            ),
            room(
                "room.castle_of_akan.throne",
                "Throne Room",
                RoomKind::Tribunal,
                "The seat from which the lord of Akan rules.",
                RoomConcealment::None,
                &["npc.castle_of_akan.lord"],
            ),
        ],
        vec![
            internal(
                "conn.castle_of_akan.barbican_to_court",
                "room.castle_of_akan.barbican",
                aid,
                "room.castle_of_akan.courtyard",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.castle_of_akan.court_to_hall",
                "room.castle_of_akan.courtyard",
                aid,
                "room.castle_of_akan.great_hall",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.castle_of_akan.court_to_armory",
                "room.castle_of_akan.courtyard",
                aid,
                "room.castle_of_akan.armory",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.castle_of_akan.hall_to_throne",
                "room.castle_of_akan.great_hall",
                aid,
                "room.castle_of_akan.throne",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            external(
                "conn.castle_of_akan.to_akan",
                "room.castle_of_akan.barbican",
                AKAN_AREA_ID,
                ConnectionDirection::South,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.castle_of_akan.cycle", 30),
        repop("repop.castle_of_akan.pop", 22, 15, 3),
        recall(false, None, false, access_any()),
        visual(
            "tileset.town.castle_of_akan.black_stone",
            "model.town.castle_of_akan.keep",
            &[
                "prop.town.castle_barbican",
                "prop.town.great_hall_table",
                "prop.town.throne_dais",
            ],
            "black-stone keep of hall, armory, and throne above a walled courtyard",
            "Original black-keep castle props and heraldry; no copied iconography.",
        ),
        &[
            "Evil servants/workers ranking 25-30 (#1644).",
            "Castle edge descends to the town of Akan (#1632).",
        ],
    )
}
