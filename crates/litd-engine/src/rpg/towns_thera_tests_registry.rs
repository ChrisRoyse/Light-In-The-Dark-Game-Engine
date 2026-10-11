// #1644-#1647 world-graph & fail-closed tests: every AreaSpec's connections pass
// registry referential integrity (#1632), and every spec fails closed loud on
// malformed input (parameterized over the whole batch).
use super::towns_thera_support::{all_town_specs, assert_code, known_area_ids};
use crate::rpg::{
    AreaConnection, ConnectionDirection, ConnectionDiscovery, TravelKind,
    validate_area_connections_against_registry, validate_area_spec,
};

const CONFIG_ERR: &str = "LITD-RPG-STATS-INVALID-CONFIG";

#[test]
fn towns_thera_connections_pass_registry_referential_integrity() {
    let known = known_area_ids();
    for (spec, exp) in all_town_specs() {
        validate_area_connections_against_registry(&spec, &known).unwrap_or_else(|error| {
            panic!("registry integrity failed for {}: {error}", exp.area_id)
        });
    }
}

#[test]
fn towns_thera_specs_fail_closed_on_malformed_input() {
    let known = known_area_ids();
    for (spec, _exp) in all_town_specs() {
        // Edge: wrong schema.
        let mut wrong_schema = spec.clone();
        wrong_schema.schema = "litd.rpg.area_spec.bad".to_string();
        assert_code(validate_area_spec(&wrong_schema), CONFIG_ERR);

        // Edge: empty room set.
        let mut empty_rooms = spec.clone();
        empty_rooms.rooms.clear();
        assert_code(validate_area_spec(&empty_rooms), CONFIG_ERR);

        // Edge: intra-area connection leaving from a room that does not exist.
        let mut dangling_from = spec.clone();
        dangling_from.connections[0].from_room_id = "room.thera.does_not_exist".to_string();
        assert_code(validate_area_spec(&dangling_from), CONFIG_ERR);

        // Edge: cross-area connection to an unknown area, flagged by the registry pass.
        let mut dangling_area = spec.clone();
        dangling_area.connections[0] = AreaConnection {
            connection_id: "conn.thera.to_void".to_string(),
            from_room_id: spec.rooms[0].room_id.clone(),
            to_area_id: "area.thera.nonexistent_void".to_string(),
            to_room_id: None,
            direction: ConnectionDirection::North,
            travel_kind: TravelKind::Road,
            discovery: ConnectionDiscovery::Visible,
            locked_by_ref: None,
        };
        assert_code(
            validate_area_connections_against_registry(&dangling_area, &known),
            CONFIG_ERR,
        );

        // Edge: recall point that is not a room in the area.
        let mut bad_recall = spec.clone();
        bad_recall.recall.recall_room_id = Some("room.thera.phantom_altar".to_string());
        assert_code(validate_area_spec(&bad_recall), CONFIG_ERR);
    }
}
