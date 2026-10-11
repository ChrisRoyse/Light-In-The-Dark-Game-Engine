// #1646 far-east & south settlements, group B: Dra-Melkhur, Isle of Corte,
// Mansion of V-dramir. CF-factual topology/rosters/level-bands; lore originalized (#984).
use litd_engine::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, RoomConcealment, RoomKind,
    TravelKind,
};

use super::towns_thera_support::{
    ARKHAM_AREA_ID, BAROVIA_AREA_ID, BASILICA_AREA_ID, DRA_MELKHUR_AREA_ID, ISLE_OF_CORTE_AREA_ID,
    MANSION_OF_VDRAMIR_AREA_ID, TownExpectation, access_any, area_spec, expect, external,
    hidden_internal, internal, recall, repop, resets, room, visual,
};

pub fn push_specs(specs: &mut Vec<(AreaSpec, TownExpectation)>) {
    specs.push((
        dra_melkhur_area_spec(),
        expect(DRA_MELKHUR_AREA_ID, 1646, AreaKind::Town, false, true, 5),
    ));
    specs.push((
        isle_of_corte_area_spec(),
        expect(ISLE_OF_CORTE_AREA_ID, 1646, AreaKind::Town, false, false, 5),
    ));
    specs.push((
        mansion_of_vdramir_area_spec(),
        expect(
            MANSION_OF_VDRAMIR_AREA_ID,
            1646,
            AreaKind::Lair,
            false,
            false,
            5,
        ),
    ));
}

/// Dra-Melkhur: an eastern temple-city on the road branch; its devouts and protectors
/// rank good (30-40).
pub fn dra_melkhur_area_spec() -> AreaSpec {
    let aid = DRA_MELKHUR_AREA_ID;
    area_spec(
        aid,
        "Dra-Melkhur",
        AreaKind::Town,
        1646,
        &[954],
        "dra-melkhur",
        (30, 45),
        "A white temple-city on the eastern road branch, kept by devouts and protectors \
         (good ranking 30-40) along its long processional way.",
        vec![
            room(
                "room.dra_melkhur.gate",
                "Processional Gate",
                RoomKind::Gate,
                "The pillared gate onto the temple-city's way.",
                RoomConcealment::GateChokepoint,
                &["npc.dra_melkhur.gate_protector"],
            ),
            room(
                "room.dra_melkhur.processional",
                "Processional Way",
                RoomKind::Street,
                "A broad colonnaded avenue toward the high shrine.",
                RoomConcealment::CrowdCover,
                &["npc.dra_melkhur.pilgrim"],
            ),
            room(
                "room.dra_melkhur.devout_temple",
                "Temple of the Devout",
                RoomKind::Guildhall,
                "The order-hall of the devouts (good ranking 30-40).",
                RoomConcealment::None,
                &["npc.dra_melkhur.devout"],
            ),
            room(
                "room.dra_melkhur.protector_hall",
                "Protectors' Hall",
                RoomKind::Landmark,
                "The muster hall of the city's protectors.",
                RoomConcealment::None,
                &["npc.dra_melkhur.protector"],
            ),
            room(
                "room.dra_melkhur.high_shrine",
                "High Shrine",
                RoomKind::Temple,
                "The high shrine that serves as the recall point.",
                RoomConcealment::None,
                &["npc.dra_melkhur.high_devout"],
            ),
        ],
        vec![
            internal(
                "conn.dra_melkhur.gate_to_way",
                "room.dra_melkhur.gate",
                aid,
                "room.dra_melkhur.processional",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.dra_melkhur.way_to_temple",
                "room.dra_melkhur.processional",
                aid,
                "room.dra_melkhur.devout_temple",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.dra_melkhur.way_to_protectors",
                "room.dra_melkhur.processional",
                aid,
                "room.dra_melkhur.protector_hall",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            internal(
                "conn.dra_melkhur.way_to_shrine",
                "room.dra_melkhur.processional",
                aid,
                "room.dra_melkhur.high_shrine",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            external(
                "conn.dra_melkhur.to_arkham",
                "room.dra_melkhur.gate",
                ARKHAM_AREA_ID,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.dra_melkhur.to_basilica",
                "room.dra_melkhur.gate",
                BASILICA_AREA_ID,
                ConnectionDirection::Northeast,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.dra_melkhur.cycle", 30),
        repop("repop.dra_melkhur.pop", 26, 15, 2),
        recall(
            false,
            Some("room.dra_melkhur.high_shrine"),
            false,
            access_any(),
        ),
        visual(
            "tileset.town.dra_melkhur.white_temple",
            "model.town.dra_melkhur.temple_city",
            &[
                "prop.town.pillared_gate",
                "prop.town.colonnade",
                "prop.town.high_shrine",
            ],
            "white colonnaded temple-city rising along a processional way to a high shrine",
            "Original temple-city colonnade and shrine props; no copied iconography.",
        ),
        &[
            "Devouts/protectors rank good 30-40 (#1646).",
            "Road branch links west to Arkham and northeast to the Basilica (#1632).",
        ],
    )
}

/// Isle of Corte: an island estate off the east coast, reached by ferry from the Cove.
pub fn isle_of_corte_area_spec() -> AreaSpec {
    let aid = ISLE_OF_CORTE_AREA_ID;
    area_spec(
        aid,
        "Isle of Corte",
        AreaKind::Town,
        1646,
        &[954],
        "isle-of-corte",
        (30, 50),
        "A windswept island estate off the eastern coast, approached by ferry across the \
         Cove to its walled hall, tiered gardens, and cliff watchtower.",
        vec![
            room(
                "room.isle_of_corte.cove_landing",
                "Cove Landing",
                RoomKind::Gate,
                "The stone jetty where the ferry from the mainland makes fast.",
                RoomConcealment::GateChokepoint,
                &["npc.isle_of_corte.ferryman"],
            ),
            room(
                "room.isle_of_corte.shore_path",
                "Shore Path",
                RoomKind::Street,
                "A salt-worn path climbing from the landing to the estate.",
                RoomConcealment::None,
                &["npc.isle_of_corte.warden"],
            ),
            room(
                "room.isle_of_corte.estate_hall",
                "Estate Hall",
                RoomKind::Landmark,
                "The walled hall at the island's crown.",
                RoomConcealment::None,
                &["npc.isle_of_corte.steward"],
            ),
            room(
                "room.isle_of_corte.gardens",
                "Tiered Gardens",
                RoomKind::Wilderness,
                "Terraced sea-gardens above the surf.",
                RoomConcealment::AlleyShadow,
                &["npc.isle_of_corte.gardener"],
            ),
            room(
                "room.isle_of_corte.watchtower",
                "Cliff Watchtower",
                RoomKind::Landmark,
                "A lone tower watching the eastern sea-lanes.",
                RoomConcealment::None,
                &["npc.isle_of_corte.watcher"],
            ),
        ],
        vec![
            internal(
                "conn.isle_of_corte.landing_to_path",
                "room.isle_of_corte.cove_landing",
                aid,
                "room.isle_of_corte.shore_path",
                ConnectionDirection::Up,
                TravelKind::Path,
            ),
            internal(
                "conn.isle_of_corte.path_to_hall",
                "room.isle_of_corte.shore_path",
                aid,
                "room.isle_of_corte.estate_hall",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.isle_of_corte.hall_to_gardens",
                "room.isle_of_corte.estate_hall",
                aid,
                "room.isle_of_corte.gardens",
                ConnectionDirection::East,
                TravelKind::Path,
            ),
            internal(
                "conn.isle_of_corte.hall_to_tower",
                "room.isle_of_corte.estate_hall",
                aid,
                "room.isle_of_corte.watchtower",
                ConnectionDirection::West,
                TravelKind::Path,
            ),
            external(
                "conn.isle_of_corte.to_arkham",
                "room.isle_of_corte.cove_landing",
                ARKHAM_AREA_ID,
                ConnectionDirection::West,
                TravelKind::Portal,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.isle_of_corte.cycle", 35),
        repop("repop.isle_of_corte.pop", 18, 18, 2),
        recall(false, None, false, access_any()),
        visual(
            "tileset.town.isle_of_corte.sea_estate",
            "model.town.isle_of_corte.island",
            &[
                "prop.town.stone_jetty",
                "prop.town.terraced_garden",
                "prop.town.cliff_tower",
            ],
            "walled island estate of tiered sea-gardens and a lone cliff watchtower",
            "Original island-estate props and sea-garden dressing; no copied iconography.",
        ),
        &[
            "Ferry crossing modelled as a Portal-kind sea edge to the mainland (#1632).",
            "Island estate off the east coast beyond the Dragon Sea (#1646).",
        ],
    )
}

/// Mansion of V-dramir: a vampire-lord manor lair on the Barovia corridor.
pub fn mansion_of_vdramir_area_spec() -> AreaSpec {
    let aid = MANSION_OF_VDRAMIR_AREA_ID;
    area_spec(
        aid,
        "Mansion of V-dramir",
        AreaKind::Lair,
        1646,
        &[954],
        "mansion-of-vdramir",
        (35, 51),
        "The candle-black manor of the vampire lord V-dramir, crouched on the Barovia \
         corridor: a foyer of dust, a silent ballroom, and a crypt where the lord sleeps.",
        vec![
            room(
                "room.mansion_of_vdramir.iron_gate",
                "Iron Gate",
                RoomKind::Gate,
                "A rusted iron gate hung with dead ivy.",
                RoomConcealment::GateChokepoint,
                &["npc.mansion_of_vdramir.gate_ghoul"],
            ),
            room(
                "room.mansion_of_vdramir.foyer",
                "Dust Foyer",
                RoomKind::Chamber,
                "A grand foyer under sheeted chandeliers.",
                RoomConcealment::None,
                &["npc.mansion_of_vdramir.thrall"],
            ),
            room(
                "room.mansion_of_vdramir.ballroom",
                "Silent Ballroom",
                RoomKind::Landmark,
                "A mirrored ballroom where nothing has danced in an age.",
                RoomConcealment::CrowdCover,
                &["npc.mansion_of_vdramir.spectre"],
            ),
            room(
                "room.mansion_of_vdramir.undercroft",
                "Undercroft",
                RoomKind::Sewer,
                "Damp cellars twisting beneath the manor.",
                RoomConcealment::HiddenLayer,
                &["npc.mansion_of_vdramir.grave_rat"],
            ),
            room(
                "room.mansion_of_vdramir.crypt",
                "Lord's Crypt",
                RoomKind::Chamber,
                "The sealed crypt where the vampire lord rests.",
                RoomConcealment::HiddenLayer,
                &["npc.mansion_of_vdramir.vampire_lord"],
            ),
        ],
        vec![
            internal(
                "conn.mansion_of_vdramir.gate_to_foyer",
                "room.mansion_of_vdramir.iron_gate",
                aid,
                "room.mansion_of_vdramir.foyer",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.mansion_of_vdramir.foyer_to_ballroom",
                "room.mansion_of_vdramir.foyer",
                aid,
                "room.mansion_of_vdramir.ballroom",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            hidden_internal(
                "conn.mansion_of_vdramir.cellar_stair",
                "room.mansion_of_vdramir.foyer",
                aid,
                "room.mansion_of_vdramir.undercroft",
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            hidden_internal(
                "conn.mansion_of_vdramir.crypt_seam",
                "room.mansion_of_vdramir.undercroft",
                aid,
                "room.mansion_of_vdramir.crypt",
                ConnectionDirection::Down,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::Hidden,
            ),
            external(
                "conn.mansion_of_vdramir.to_barovia",
                "room.mansion_of_vdramir.iron_gate",
                BAROVIA_AREA_ID,
                ConnectionDirection::North,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
        ],
        resets("reset.mansion_of_vdramir.cycle", 20),
        repop("repop.mansion_of_vdramir.pop", 16, 12, 3),
        recall(false, None, false, access_any()),
        visual(
            "tileset.lair.mansion_of_vdramir.black_manor",
            "model.lair.mansion_of_vdramir.mansion",
            &[
                "prop.lair.rusted_gate",
                "prop.lair.sheeted_chandelier",
                "prop.lair.sealed_crypt",
                "prop.lair.dead_ivy",
            ],
            "candle-black vampire manor of dust-foyer, mirrored ballroom, and sealed crypt",
            "Original gothic-manor lair props and crypt dressing; no copied iconography.",
        ),
        &[
            "Crypt access is a Hidden descent through the undercroft (#944).",
            "Concealed trail links the manor to the Barovia corridor (#1632/#1646).",
        ],
    )
}
