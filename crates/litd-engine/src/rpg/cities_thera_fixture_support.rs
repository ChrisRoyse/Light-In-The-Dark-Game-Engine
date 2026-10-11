// Shared room/connection constructors + test helpers for the #1637-1643 Thera
// city batch (7 cities, one AreaSpec each). Mirrors area_specs_test_fixture.rs's
// per-field helpers (room/internal/hidden_internal/external) but parameterized by
// area_id so one copy serves all seven fixtures instead of duplicating the same
// four small functions seven times (#1466 line-count ratchet discipline).
use litd_engine::rpg::{
    AreaConnection, AreaRoom, ConnectionDirection, ConnectionDiscovery, RoomConcealment, RoomKind,
    TravelKind,
};

pub(crate) fn room(
    room_id: &str,
    display_name: &str,
    room_kind: RoomKind,
    summary: &str,
    concealment: RoomConcealment,
    npcs: &[&str],
) -> AreaRoom {
    AreaRoom {
        room_id: room_id.to_string(),
        display_name: display_name.to_string(),
        room_kind,
        summary: summary.to_string(),
        concealment,
        npc_population_ids: npcs.iter().map(|id| id.to_string()).collect(),
    }
}

/// Internal (same-area) visible connection.
pub(crate) fn internal(
    connection_id: &str,
    from_room_id: &str,
    area_id: &str,
    to_room_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
) -> AreaConnection {
    hidden_internal(
        connection_id,
        from_room_id,
        area_id,
        to_room_id,
        direction,
        travel_kind,
        ConnectionDiscovery::Visible,
    )
}

/// Internal (same-area) connection with an explicit discovery mode, for guild
/// backrooms, sewer descents, and other topology that must be found (#944).
pub(crate) fn hidden_internal(
    connection_id: &str,
    from_room_id: &str,
    area_id: &str,
    to_room_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    AreaConnection {
        connection_id: connection_id.to_string(),
        from_room_id: from_room_id.to_string(),
        to_area_id: area_id.to_string(),
        to_room_id: Some(to_room_id.to_string()),
        direction,
        travel_kind,
        discovery,
        locked_by_ref: None,
    }
}

/// World-graph edge (#1632) to another area, satellite, or sibling city.
pub(crate) fn external(
    connection_id: &str,
    from_room_id: &str,
    to_area_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    AreaConnection {
        connection_id: connection_id.to_string(),
        from_room_id: from_room_id.to_string(),
        to_area_id: to_area_id.to_string(),
        to_room_id: None,
        direction,
        travel_kind,
        discovery,
        locked_by_ref: None,
    }
}

pub(crate) fn assert_code(result: Result<(), litd_engine::EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}

pub(crate) fn content_store(
    label: &str,
) -> (
    litd_engine::calyx_content::EngineCalyxContentStore,
    litd_engine::test_temp::TestTempDir,
) {
    let vault_dir = litd_engine::test_temp::TestTempDir::new(&format!("issue1637-1643-{label}"));
    let database = litd_engine::calyx_persistence::EngineCalyxDatabase::open(
        litd_engine::calyx_persistence::EngineCalyxDatabaseConfig::for_vault(&vault_dir),
    )
    .unwrap();
    (
        litd_engine::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}
