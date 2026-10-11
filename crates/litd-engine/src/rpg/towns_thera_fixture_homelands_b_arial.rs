use super::*;

/// Arial sky sanctum ("In the Air"): the cloud realm of the arial (sylphkin, #1016)
/// above the map; the Mines of Zakiim lie below. Arial hometown.
pub fn arial_sky_sanctum_area_spec() -> AreaSpec {
    let aid = ARIAL_SKY_SANCTUM_AREA_ID;
    area_spec(
        aid,
        "Arial Sky Sanctum",
        AreaKind::Wilderness,
        1647,
        &[985, 1016, 1601],
        "arial-sky-sanctum",
        (5, 40),
        "A realm of cloud-terraces and wind-halls high above the world, homeland of the \
         arial sylphkin, with the Mines of Zakiim hanging in the gulf below.",
        vec![
            room(
                "room.arial_sky_sanctum.updraft_gate",
                "Updraft Gate",
                RoomKind::Gate,
                "The rushing updraft by which the wingless are borne aloft.",
                RoomConcealment::GateChokepoint,
                &["npc.arial_sky_sanctum.wind_sentry"],
            ),
            room(
                "room.arial_sky_sanctum.cloud_terrace",
                "Cloud Terrace",
                RoomKind::Wilderness,
                "Broad terraces of packed cloud where the arial dwell.",
                RoomConcealment::CrowdCover,
                &["npc.arial_sky_sanctum.arial"],
            ),
            room(
                "room.arial_sky_sanctum.wind_hall",
                "Wind Hall",
                RoomKind::Landmark,
                "An open colonnade of the high sanctum.",
                RoomConcealment::None,
                &["npc.arial_sky_sanctum.sky_warden"],
            ),
            room(
                "room.arial_sky_sanctum.sky_shrine",
                "Sky Shrine",
                RoomKind::Temple,
                "A wind-chimed shrine that serves as the recall point.",
                RoomConcealment::None,
                &["npc.arial_sky_sanctum.sky_priest"],
            ),
            room(
                "room.arial_sky_sanctum.mine_descent",
                "Zakiim Descent",
                RoomKind::Wilderness,
                "A perilous concealed descent toward the hanging Mines of Zakiim.",
                RoomConcealment::HiddenLayer,
                &["npc.arial_sky_sanctum.mine_scout"],
            ),
        ],
        vec![
            internal(
                "conn.arial_sky_sanctum.gate_to_terrace",
                "room.arial_sky_sanctum.updraft_gate",
                aid,
                "room.arial_sky_sanctum.cloud_terrace",
                ConnectionDirection::Up,
                TravelKind::Path,
            ),
            internal(
                "conn.arial_sky_sanctum.terrace_to_hall",
                "room.arial_sky_sanctum.cloud_terrace",
                aid,
                "room.arial_sky_sanctum.wind_hall",
                ConnectionDirection::North,
                TravelKind::Path,
            ),
            internal(
                "conn.arial_sky_sanctum.terrace_to_shrine",
                "room.arial_sky_sanctum.cloud_terrace",
                aid,
                "room.arial_sky_sanctum.sky_shrine",
                ConnectionDirection::East,
                TravelKind::Path,
            ),
            hidden_internal(
                "conn.arial_sky_sanctum.descent_seam",
                "room.arial_sky_sanctum.cloud_terrace",
                aid,
                "room.arial_sky_sanctum.mine_descent",
                ConnectionDirection::Down,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::Hidden,
            ),
            external(
                "conn.arial_sky_sanctum.to_zakiim",
                "room.arial_sky_sanctum.mine_descent",
                MINES_OF_ZAKIIM_AREA_ID,
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            external(
                "conn.arial_sky_sanctum.to_galadon",
                "room.arial_sky_sanctum.updraft_gate",
                GALADON_AREA_ID,
                ConnectionDirection::Down,
                TravelKind::Portal,
                ConnectionDiscovery::Hidden,
            ),
        ],
        resets("reset.arial_sky_sanctum.cycle", 30),
        repop("repop.arial_sky_sanctum.pop", 20, 15, 2),
        recall(
            true,
            Some("room.arial_sky_sanctum.sky_shrine"),
            false,
            access_restricted(&["race.arial"]),
        ),
        visual(
            "tileset.wild.arial_sky_sanctum.cloud_realm",
            "model.wild.arial_sky_sanctum.sky_sanctum",
            &[
                "prop.wild.updraft_column",
                "prop.wild.cloud_terrace",
                "prop.wild.wind_shrine",
            ],
            "cloud-terraced sky sanctum of open wind-halls above the world, a mine hanging below",
            "Original sky-realm cloud props and wind-shrine dressing; no copied iconography.",
        ),
        &[
            "Arial/sylphkin hometown (access race.arial, #1016); ascent and mine descent are Hidden (#944).",
            "Sky realm links down to the Mines of Zakiim and the ground map at Galadon (#1632).",
        ],
    )
}
