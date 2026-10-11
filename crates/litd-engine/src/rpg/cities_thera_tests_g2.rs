// Tests for #1641 Arkham, #1642 Tir-Talath, and #1643 Darsylon, plus one
// shared fail-closed-edges test parameterized over all seven cities in the
// #1637-1643 batch (mirrors `issue1636_area_spec_fail_closed_edges`'s five
// edge cases: wrong schema, empty rooms, intra-area dangling connection,
// cross-area dangling connection via the registry pass, and a bad recall
// room -- consolidated here per the task's "your call" note on edge coverage).
use std::collections::BTreeSet;

use crate::{
    EngineError,
    rpg::{
        AreaConnection, AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, TravelKind,
        area_spec_readback, validate_area_connections_against_registry, validate_area_spec,
    },
};

use super::rpg_cities_thera_fixture_arkham::{
    ARKHAM_AREA_ID, arkham_area_spec, arkham_known_area_ids,
};
use super::rpg_cities_thera_fixture_darsylon::{
    DARSYLON_AREA_ID, darsylon_area_spec, darsylon_known_area_ids,
};
use super::rpg_cities_thera_fixture_hamsah::{hamsah_area_spec, hamsah_known_area_ids};
use super::rpg_cities_thera_fixture_seantryn::{seantryn_area_spec, seantryn_known_area_ids};
use super::rpg_cities_thera_fixture_support::assert_code;
use super::rpg_cities_thera_fixture_tirtalath::{
    TIR_TALATH_AREA_ID, tir_talath_area_spec, tir_talath_known_area_ids,
};
use super::rpg_cities_thera_fixture_udgaard::{udgaard_area_spec, udgaard_known_area_ids};
use super::rpg_cities_thera_fixture_voralian::{voralian_area_spec, voralian_known_area_ids};
use super::rpg_cities_thera_tests_g1::persist_and_readback;

#[test]
fn issue1641_arkham_area_spec_matches_facts_persists_and_is_registry_sound() {
    let spec = arkham_area_spec();
    let readback = area_spec_readback(&spec).unwrap();
    assert_eq!(readback.area_id, ARKHAM_AREA_ID);
    assert_eq!(readback.display_name, "Arkham");
    assert_eq!(readback.area_kind, AreaKind::City);
    assert_eq!(readback.source_issue, 1641);
    assert!(readback.guildhall_count >= 2);
    assert!(readback.is_hometown_option);
    assert!(readback.has_recall_point);
    assert!(readback.external_connection_count >= 1);
    assert!(readback.hidden_connection_count >= 1);

    let (records, lowered, summary) = persist_and_readback(&spec, "arkham-area-spec");
    let expected_rows = 1 + spec.rooms.len() + spec.connections.len();
    assert_eq!(records.len(), expected_rows);
    assert_eq!(summary.id, ARKHAM_AREA_ID);
    assert_eq!(lowered.row_count, expected_rows);

    let known: BTreeSet<&str> = arkham_known_area_ids().into_iter().collect();
    validate_area_connections_against_registry(&spec, &known).unwrap();
}

#[test]
fn issue1642_tir_talath_area_spec_matches_facts_persists_and_is_registry_sound() {
    let spec = tir_talath_area_spec();
    let readback = area_spec_readback(&spec).unwrap();
    assert_eq!(readback.area_id, TIR_TALATH_AREA_ID);
    assert_eq!(readback.display_name, "Tir-Talath");
    assert_eq!(readback.area_kind, AreaKind::City);
    assert_eq!(readback.source_issue, 1642);
    assert!(readback.guildhall_count >= 2);
    assert!(readback.is_hometown_option);
    assert!(readback.has_academy);
    assert!(readback.has_recall_point);
    assert!(readback.external_connection_count >= 1);
    assert!(readback.hidden_connection_count >= 1);

    let (records, lowered, summary) = persist_and_readback(&spec, "tirtalath-area-spec");
    let expected_rows = 1 + spec.rooms.len() + spec.connections.len();
    assert_eq!(records.len(), expected_rows);
    assert_eq!(summary.id, TIR_TALATH_AREA_ID);
    assert_eq!(lowered.row_count, expected_rows);

    let known: BTreeSet<&str> = tir_talath_known_area_ids().into_iter().collect();
    validate_area_connections_against_registry(&spec, &known).unwrap();
}

#[test]
fn issue1643_darsylon_area_spec_matches_facts_persists_and_is_registry_sound() {
    let spec = darsylon_area_spec();
    let readback = area_spec_readback(&spec).unwrap();
    assert_eq!(readback.area_id, DARSYLON_AREA_ID);
    assert_eq!(readback.display_name, "Darsylon");
    assert_eq!(readback.area_kind, AreaKind::City);
    assert_eq!(readback.source_issue, 1643);
    assert!(readback.guildhall_count >= 2);
    assert!(readback.is_hometown_option);
    assert!(readback.has_academy);
    assert!(readback.has_recall_point);
    assert!(readback.external_connection_count >= 1);
    assert!(readback.hidden_connection_count >= 1);

    let (records, lowered, summary) = persist_and_readback(&spec, "darsylon-area-spec");
    let expected_rows = 1 + spec.rooms.len() + spec.connections.len();
    assert_eq!(records.len(), expected_rows);
    assert_eq!(summary.id, DARSYLON_AREA_ID);
    assert_eq!(lowered.row_count, expected_rows);

    let known: BTreeSet<&str> = darsylon_known_area_ids().into_iter().collect();
    validate_area_connections_against_registry(&spec, &known).unwrap();
}

#[test]
fn issue1637_1643_area_specs_fail_closed_edges() {
    check_fail_closed_edges(voralian_area_spec, voralian_known_area_ids().to_vec());
    check_fail_closed_edges(hamsah_area_spec, hamsah_known_area_ids().to_vec());
    check_fail_closed_edges(udgaard_area_spec, udgaard_known_area_ids().to_vec());
    check_fail_closed_edges(seantryn_area_spec, seantryn_known_area_ids().to_vec());
    check_fail_closed_edges(arkham_area_spec, arkham_known_area_ids().to_vec());
    check_fail_closed_edges(tir_talath_area_spec, tir_talath_known_area_ids().to_vec());
    check_fail_closed_edges(darsylon_area_spec, darsylon_known_area_ids().to_vec());
}

fn check_fail_closed_edges(spec_fn: fn() -> AreaSpec, known_ids: Vec<&'static str>) {
    let known: BTreeSet<&str> = known_ids.into_iter().collect();

    // Edge: wrong schema.
    let mut wrong_schema = spec_fn();
    wrong_schema.schema = "litd.rpg.area_spec.bad".to_string();
    assert_code(
        validate_area_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Edge: empty/malformed room set rejected loud.
    let mut empty_rooms = spec_fn();
    empty_rooms.rooms.clear();
    assert_code(
        validate_area_spec(&empty_rooms),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Edge: intra-area dangling connection (from a room that does not exist).
    let mut dangling_from = spec_fn();
    dangling_from.connections[0].from_room_id = "room.does_not_exist".to_string();
    assert_code(
        validate_area_spec(&dangling_from),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Edge: cross-area dangling connection flagged by the registry pass.
    let mut dangling_area = spec_fn();
    let first_room = dangling_area.rooms[0].room_id.clone();
    dangling_area.connections[0] = AreaConnection {
        connection_id: "conn.to_void".to_string(),
        from_room_id: first_room,
        to_area_id: "area.thera.nonexistent_void".to_string(),
        to_room_id: None,
        direction: ConnectionDirection::North,
        travel_kind: TravelKind::Road,
        discovery: ConnectionDiscovery::Visible,
        locked_by_ref: None,
    };
    let error: Result<(), EngineError> =
        validate_area_connections_against_registry(&dangling_area, &known);
    assert_code(error, "LITD-RPG-STATS-INVALID-CONFIG");

    // Edge: recall point that is not a room in the area.
    let mut bad_recall = spec_fn();
    bad_recall.recall.recall_room_id = Some("room.phantom_altar".to_string());
    assert_code(
        validate_area_spec(&bad_recall),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}
