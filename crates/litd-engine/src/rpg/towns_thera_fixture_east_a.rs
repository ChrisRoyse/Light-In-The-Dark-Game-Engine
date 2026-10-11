// #1646 far-east & south settlements, group A: Barovia, Teth-Azeleth, Basilica.
// CF-factual topology/rosters/level-bands; name and lore originalized (#984).
use litd_engine::rpg::{
    AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, RoomConcealment, RoomKind,
    TravelKind,
};

use super::towns_thera_support::{
    ARKHAM_AREA_ID, BAROVIA_AREA_ID, BASILICA_AREA_ID, MANSION_OF_VDRAMIR_AREA_ID,
    TETH_AZELETH_AREA_ID, TownExpectation, access_any, area_spec, expect, external,
    hidden_internal, internal, recall, repop, resets, room, visual,
};

pub fn push_specs(specs: &mut Vec<(AreaSpec, TownExpectation)>) {
    specs.push((
        barovia_area_spec(),
        expect(BAROVIA_AREA_ID, 1646, AreaKind::Town, false, true, 6),
    ));
    specs.push((
        teth_azeleth_area_spec(),
        expect(TETH_AZELETH_AREA_ID, 1646, AreaKind::Town, false, true, 5),
    ));
    specs.push((
        basilica_area_spec(),
        expect(BASILICA_AREA_ID, 1646, AreaKind::Town, false, true, 5),
    ));
}

/// Barovia: a gothic pocket of village, keep, and mist-bound forest. The village
/// hides a binder branch; the keep's skeletons/zombies rank good (30-40).
pub fn barovia_area_spec() -> AreaSpec {
    let aid = BAROVIA_AREA_ID;
    area_spec(
        aid,
        "Barovia",
        AreaKind::Town,
        1646,
        &[954],
        "barovia",
        (25, 45),
        "A gothic pocket of the far east: a fog-shrouded village, a haunted keep whose \
         skeletons and zombies rank good (30-40), and a border forest where the mists devour the road.",
        vec![
            room(
                "room.barovia.village_square",
                "Village Square",
                RoomKind::Intersection,
                "A cobbled square under leaning gables and a grey sky.",
                RoomConcealment::CrowdCover,
                &["npc.barovia.villager"],
            ),
            room(
                "room.barovia.binder_house",
                "Binder House",
                RoomKind::Guildhall,
                "A shuttered house where the binder thief-branch trains.",
                RoomConcealment::AlleyShadow,
                &["npc.barovia.binder_master"],
            ),
            room(
                "room.barovia.chapel",
                "Old Chapel",
                RoomKind::Temple,
                "A crumbling chapel that still serves as a recall point.",
                RoomConcealment::None,
                &["npc.barovia.priest"],
            ),
            room(
                "room.barovia.keep_gate",
                "Keep Gate",
                RoomKind::Gate,
                "The broken gate of the keep above the village.",
                RoomConcealment::GateChokepoint,
                &["npc.barovia.keep_watcher"],
            ),
            room(
                "room.barovia.keep_halls",
                "Keep Halls",
                RoomKind::Chamber,
                "Cold halls stalked by skeletons and zombies (good ranking 30-40).",
                RoomConcealment::None,
                &["npc.barovia.skeleton", "npc.barovia.zombie"],
            ),
            room(
                "room.barovia.mistwood",
                "Mistwood Verge",
                RoomKind::Wilderness,
                "The forest verge where borderland mists swallow the trail.",
                RoomConcealment::HiddenLayer,
                &["npc.barovia.mist_wraith"],
            ),
        ],
        vec![
            internal(
                "conn.barovia.square_to_binder",
                "room.barovia.village_square",
                aid,
                "room.barovia.binder_house",
                ConnectionDirection::West,
                TravelKind::Road,
            ),
            internal(
                "conn.barovia.square_to_chapel",
                "room.barovia.village_square",
                aid,
                "room.barovia.chapel",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.barovia.square_to_keep",
                "room.barovia.village_square",
                aid,
                "room.barovia.keep_gate",
                ConnectionDirection::North,
                TravelKind::Path,
            ),
            internal(
                "conn.barovia.keep_gate_to_halls",
                "room.barovia.keep_gate",
                aid,
                "room.barovia.keep_halls",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.barovia.square_to_mistwood",
                "room.barovia.village_square",
                aid,
                "room.barovia.mistwood",
                ConnectionDirection::South,
                TravelKind::Path,
            ),
            hidden_internal(
                "conn.barovia.mist_seam",
                "room.barovia.mistwood",
                aid,
                "room.barovia.keep_halls",
                ConnectionDirection::Up,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::Hidden,
            ),
            external(
                "conn.barovia.to_arkham",
                "room.barovia.village_square",
                ARKHAM_AREA_ID,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.barovia.to_vdramir",
                "room.barovia.mistwood",
                MANSION_OF_VDRAMIR_AREA_ID,
                ConnectionDirection::South,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
        ],
        resets("reset.barovia.cycle", 25),
        repop("repop.barovia.pop", 28, 12, 3),
        recall(false, Some("room.barovia.chapel"), false, access_any()),
        visual(
            "tileset.town.barovia.gothic_fog",
            "model.town.barovia.village_keep",
            &[
                "prop.town.leaning_gable",
                "prop.town.broken_keep_gate",
                "prop.town.mist_veil",
                "prop.town.chapel_cross",
            ],
            "fog-drowned gothic village beneath a haunted keep at a mist-eaten forest verge",
            "Original gothic-village and keep props with volumetric mist; no copied iconography.",
        ),
        &[
            "Border mists are a HiddenLayer/ConcealedTrail into the corridor (#944).",
            "Keep undead rank good 30-40; a concealed trail links to the Mansion of V-dramir (#1632).",
        ],
    )
}

/// Teth-Azeleth: warrior-monk monastery in the eastern hills where warriors learn
/// legacy techniques (ties #951/#1020); students rank good (35-45).
pub fn teth_azeleth_area_spec() -> AreaSpec {
    let aid = TETH_AZELETH_AREA_ID;
    area_spec(
        aid,
        "Teth-Azeleth",
        AreaKind::Town,
        1646,
        &[951, 1020],
        "teth-azeleth",
        (35, 45),
        "A cliff-clinging warrior-monk monastery in the southeastern hills, where \
         disciplined students (good ranking 35-45) learn the legacy techniques of the blade.",
        vec![
            room(
                "room.teth_azeleth.mountain_gate",
                "Mountain Gate",
                RoomKind::Gate,
                "A stair-cut gate high in the eastern hills.",
                RoomConcealment::GateChokepoint,
                &["npc.teth_azeleth.gate_monk"],
            ),
            room(
                "room.teth_azeleth.courtyard",
                "Training Courtyard",
                RoomKind::Intersection,
                "A swept stone courtyard ringing with drill.",
                RoomConcealment::None,
                &["npc.teth_azeleth.student"],
            ),
            room(
                "room.teth_azeleth.training_hall",
                "Hall of Legacies",
                RoomKind::Guildhall,
                "The hall where warrior legacy techniques are taught.",
                RoomConcealment::None,
                &["npc.teth_azeleth.weapon_master"],
            ),
            room(
                "room.teth_azeleth.dormitory",
                "Cell Dormitory",
                RoomKind::Chamber,
                "Spare cells for the monastery's disciples.",
                RoomConcealment::None,
                &["npc.teth_azeleth.disciple"],
            ),
            room(
                "room.teth_azeleth.meditation_shrine",
                "Meditation Shrine",
                RoomKind::Temple,
                "A quiet shrine that serves as the recall point.",
                RoomConcealment::None,
                &["npc.teth_azeleth.abbot"],
            ),
        ],
        vec![
            internal(
                "conn.teth_azeleth.gate_to_court",
                "room.teth_azeleth.mountain_gate",
                aid,
                "room.teth_azeleth.courtyard",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.teth_azeleth.court_to_hall",
                "room.teth_azeleth.courtyard",
                aid,
                "room.teth_azeleth.training_hall",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.teth_azeleth.court_to_dorm",
                "room.teth_azeleth.courtyard",
                aid,
                "room.teth_azeleth.dormitory",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.teth_azeleth.court_to_shrine",
                "room.teth_azeleth.courtyard",
                aid,
                "room.teth_azeleth.meditation_shrine",
                ConnectionDirection::West,
                TravelKind::Path,
            ),
            external(
                "conn.teth_azeleth.to_arkham",
                "room.teth_azeleth.mountain_gate",
                ARKHAM_AREA_ID,
                ConnectionDirection::Northwest,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.teth_azeleth.cycle", 30),
        repop("repop.teth_azeleth.pop", 20, 15, 2),
        recall(
            false,
            Some("room.teth_azeleth.meditation_shrine"),
            false,
            access_any(),
        ),
        visual(
            "tileset.town.teth_azeleth.cliff_monastery",
            "model.town.teth_azeleth.monastery",
            &[
                "prop.town.stair_gate",
                "prop.town.drill_court",
                "prop.town.meditation_shrine",
            ],
            "cliff-cut stone monastery of tiered courtyards above the eastern hills",
            "Original warrior-monk monastery props and shrine dressing; no copied iconography.",
        ),
        &[
            "Legacy-technique training ties #951/#1020; students rank good 35-45 (#1646).",
            "Hill path descends toward Arkham and the eastern road (#1632).",
        ],
    )
}

/// Basilica: a fortified cathedral complex (5-45) whose guards and knights rank evil (30-45).
pub fn basilica_area_spec() -> AreaSpec {
    let aid = BASILICA_AREA_ID;
    area_spec(
        aid,
        "Basilica",
        AreaKind::Town,
        1646,
        &[954],
        "basilica",
        (5, 45),
        "A vast fortified cathedral-city of the east, whose armoured guards and knights \
         (evil ranking 30-45) hold its nave, crypts, and high altar against all comers.",
        vec![
            room(
                "room.basilica.gate",
                "Cathedral Gate",
                RoomKind::Gate,
                "The great bronze gate of the fortified basilica.",
                RoomConcealment::GateChokepoint,
                &["npc.basilica.gate_knight"],
            ),
            room(
                "room.basilica.nave",
                "Grand Nave",
                RoomKind::Landmark,
                "A soaring nave patrolled by armoured guards.",
                RoomConcealment::CrowdCover,
                &["npc.basilica.guard"],
            ),
            room(
                "room.basilica.knight_hall",
                "Knights' Hall",
                RoomKind::Guildhall,
                "The mustering hall of the basilica's knights (evil ranking 30-45).",
                RoomConcealment::None,
                &["npc.basilica.knight"],
            ),
            room(
                "room.basilica.crypt",
                "Under Crypt",
                RoomKind::Chamber,
                "Cold vaulted crypts beneath the nave.",
                RoomConcealment::HiddenLayer,
                &["npc.basilica.crypt_warden"],
            ),
            room(
                "room.basilica.high_altar",
                "High Altar",
                RoomKind::Temple,
                "The high altar that serves as the recall point.",
                RoomConcealment::None,
                &["npc.basilica.high_priest"],
            ),
        ],
        vec![
            internal(
                "conn.basilica.gate_to_nave",
                "room.basilica.gate",
                aid,
                "room.basilica.nave",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.basilica.nave_to_hall",
                "room.basilica.nave",
                aid,
                "room.basilica.knight_hall",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.basilica.nave_to_altar",
                "room.basilica.nave",
                aid,
                "room.basilica.high_altar",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            hidden_internal(
                "conn.basilica.crypt_stair",
                "room.basilica.nave",
                aid,
                "room.basilica.crypt",
                ConnectionDirection::Down,
                TravelKind::Descend,
                ConnectionDiscovery::Hidden,
            ),
            external(
                "conn.basilica.to_arkham",
                "room.basilica.gate",
                ARKHAM_AREA_ID,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets("reset.basilica.cycle", 30),
        repop("repop.basilica.pop", 30, 15, 3),
        recall(false, Some("room.basilica.high_altar"), false, access_any()),
        visual(
            "tileset.town.basilica.cathedral_stone",
            "model.town.basilica.fortified_cathedral",
            &[
                "prop.town.bronze_gate",
                "prop.town.nave_column",
                "prop.town.crypt_slab",
                "prop.town.high_altar",
            ],
            "fortified cathedral of soaring nave and vaulted crypt behind bronze gates",
            "Original cathedral-fortress props and altar dressing; no copied iconography.",
        ),
        &[
            "Crypt descent is Hidden (#944); guards/knights rank evil 30-45 (#1646).",
            "Cathedral road runs west toward Arkham and the eastern chain (#1632).",
        ],
    )
}
