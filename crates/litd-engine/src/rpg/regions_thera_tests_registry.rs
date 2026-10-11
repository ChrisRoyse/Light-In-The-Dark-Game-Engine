// World-graph registry referential integrity (#1632) and fail-closed edge
// cases for the #1649-#1660 batch, mirroring
// `issue1636_area_spec_fail_closed_edges` but run once against a real spec
// from this batch rather than duplicating the framework's own edge coverage.

use crate::rpg::{
    AreaConnection, ConnectionDirection, ConnectionDiscovery, TravelKind,
    validate_area_connections_against_registry, validate_area_spec,
};

use super::regions_thera_fixture_underdark::underdark_tunnels_area_spec;
use super::regions_thera_registry::{all_region_specs, known_area_ids};
use super::regions_thera_support::assert_code;

#[test]
fn issue1649_1660_every_spec_passes_registry_referential_integrity() {
    let specs = all_region_specs();
    let known = known_area_ids();
    for spec in &specs {
        validate_area_connections_against_registry(spec, &known).unwrap_or_else(|e| {
            panic!(
                "spec {} failed registry referential integrity: {e}",
                spec.area_id
            );
        });
    }
}

#[test]
fn issue1649_1660_fail_closed_edges() {
    // Edge: wrong schema.
    let mut wrong_schema = underdark_tunnels_area_spec();
    wrong_schema.schema = "litd.rpg.area_spec.bad".to_string();
    assert_code(
        validate_area_spec(&wrong_schema),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Edge: empty room set rejected loud.
    let mut empty_rooms = underdark_tunnels_area_spec();
    empty_rooms.rooms.clear();
    assert_code(
        validate_area_spec(&empty_rooms),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Edge: intra-area dangling connection (from a room that does not exist).
    let mut dangling_from = underdark_tunnels_area_spec();
    dangling_from.connections[0].from_room_id = "room.underdark_tunnels.does_not_exist".to_string();
    assert_code(
        validate_area_spec(&dangling_from),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Edge: cross-area dangling connection flagged by the registry pass.
    let mut dangling_area = underdark_tunnels_area_spec();
    dangling_area.connections[0] = AreaConnection {
        connection_id: "conn.underdark_tunnels.to_void".to_string(),
        from_room_id: "room.underdark_tunnels.cragstone_descent".to_string(),
        to_area_id: "area.thera.nonexistent_void".to_string(),
        to_room_id: None,
        direction: ConnectionDirection::North,
        travel_kind: TravelKind::Road,
        discovery: ConnectionDiscovery::Visible,
        locked_by_ref: None,
    };
    let known = known_area_ids();
    assert_code(
        validate_area_connections_against_registry(&dangling_area, &known),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );

    // Edge: recall point that is not a room in the area.
    let mut bad_recall = underdark_tunnels_area_spec();
    bad_recall.recall.recall_room_id = Some("room.underdark_tunnels.phantom_altar".to_string());
    assert_code(
        validate_area_spec(&bad_recall),
        "LITD-RPG-STATS-INVALID-CONFIG",
    );
}
