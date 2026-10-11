// Test-side harness for the #1657-1659 dungeon batch. Kept separate from the
// fixture-building support so each file stays under the <500-line ratchet (#1466).
// The authoritative in-suite FSV lives here: author -> persist_and_lower -> SEPARATE
// readback via `get_content_for_editor`, asserting stored == authored (sha256).

use std::collections::BTreeSet;

use crate::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentKind},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    rpg::{
        AreaKind, AreaSpec, ConnectionDirection, ConnectionDiscovery, TravelKind,
        area_spec_content_records, area_spec_readback, persist_and_lower_rpg_authored_content,
        validate_area_connections_against_registry, validate_area_spec,
        validate_rpg_authored_content_records,
    },
};

use super::rpg_dungeons_thera_support::{INVALID_CONFIG_CODE, dungeon_known_area_ids, external};

pub(crate) fn content_store(
    label: &str,
) -> (EngineCalyxContentStore, crate::test_temp::TestTempDir) {
    let vault_dir = crate::test_temp::TestTempDir::new(&format!("issue1657-1659-{label}"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database, label),
        vault_dir,
    )
}

pub(crate) fn assert_code(result: Result<(), EngineError>, code: &str) {
    let error = result.unwrap_err();
    assert_eq!(error.code(), code);
}

/// The authoritative in-suite FSV: author -> persist_and_lower -> SEPARATE readback
/// via `get_content_for_editor`, asserting stored == authored (sha256 identical) and
/// the acquired-map doctrine flags (#944) are never true. Also proves Dungeon kind,
/// no-hometown/no-recall recall profile, and cross-area registry integrity (#1632).
pub(crate) fn assert_dungeon_roundtrip(spec: &AreaSpec, label: &str) {
    let known: BTreeSet<&str> = dungeon_known_area_ids().into_iter().collect();
    let readback = area_spec_readback(spec).unwrap();
    assert_eq!(readback.area_kind, AreaKind::Dungeon);
    assert!(!readback.is_hometown_option);
    assert!(!readback.has_academy);
    assert!(!readback.has_recall_point);
    assert!(readback.external_connection_count >= 1);
    assert_eq!(readback.area_spec_sha256.len(), 64);
    validate_area_connections_against_registry(spec, &known).unwrap();

    let records = area_spec_content_records(spec).unwrap();
    let validation = validate_rpg_authored_content_records(&records).unwrap();
    let expected_rows = 1 + spec.rooms.len() + spec.connections.len();
    assert_eq!(records.len(), expected_rows);
    assert_eq!(validation.record_count, expected_rows);
    assert!(validation.all_editor_authored_sources);
    assert!(validation.no_builtin_map_runtime);

    let (store, _vault) = content_store(label);
    let lowered = persist_and_lower_rpg_authored_content(&store, &records).unwrap();
    assert_eq!(lowered.row_count, expected_rows);
    assert!(lowered.entities[0].lowered_field_count > 0);

    // SEPARATE readback path: prove the persisted summary row equals what was authored.
    let summary = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .unwrap();
    assert_eq!(summary.kind, EngineContentKind::Terrain);
    assert_eq!(summary.id, spec.area_id);
    assert_eq!(summary.payload["surface_kind"], "area_spec");
    assert_eq!(
        summary.payload["area_id"].as_str(),
        Some(spec.area_id.as_str())
    );
    assert_eq!(summary.payload["area_kind"], "dungeon");
    assert_eq!(
        summary.payload["area_spec_sha256"].as_str(),
        Some(readback.area_spec_sha256.as_str())
    );
    assert_ne!(summary.payload["built_in_map_required"], true);
    assert_ne!(summary.payload["grants_map_or_minimap"], true);
    assert_ne!(summary.payload["grants_gps_markers"], true);
    assert!(
        summary
            .source_path
            .starts_with("editor://rpg/thera/area_specs/")
    );

    // A room row also reads back identically (same whole-spec sha256 stamp).
    let first_room = &spec.rooms[0];
    let room_row = store
        .get_content_for_editor(EngineContentKind::Terrain, &first_room.room_id)
        .unwrap();
    assert_eq!(room_row.payload["surface_kind"], "area_room");
    assert_eq!(
        room_row.payload["room_id"].as_str(),
        Some(first_room.room_id.as_str())
    );
    assert_eq!(
        room_row.payload["area_spec_sha256"].as_str(),
        Some(readback.area_spec_sha256.as_str())
    );
}

/// Consolidated fail-closed edges, parameterized over any dungeon spec: wrong schema,
/// empty rooms, dangling intra-area connection, dangling cross-area edge (registry),
/// and a recall point that is not a real room.
pub(crate) fn assert_fail_closed_edges(base: &AreaSpec) {
    let known: BTreeSet<&str> = dungeon_known_area_ids().into_iter().collect();

    let mut wrong_schema = base.clone();
    wrong_schema.schema = "litd.rpg.area_spec.bad".to_string();
    assert_code(validate_area_spec(&wrong_schema), INVALID_CONFIG_CODE);

    let mut empty_rooms = base.clone();
    empty_rooms.rooms.clear();
    assert_code(validate_area_spec(&empty_rooms), INVALID_CONFIG_CODE);

    let mut dangling_from = base.clone();
    dangling_from.connections[0].from_room_id = "room.void.nowhere".to_string();
    assert_code(validate_area_spec(&dangling_from), INVALID_CONFIG_CODE);

    let mut dangling_area = base.clone();
    dangling_area.connections[0] = external(
        "conn.void.edge",
        &base.rooms[0].room_id,
        "area.thera.nonexistent_void",
        ConnectionDirection::North,
        TravelKind::Road,
        ConnectionDiscovery::Visible,
    );
    assert_code(
        validate_area_connections_against_registry(&dangling_area, &known),
        INVALID_CONFIG_CODE,
    );

    let mut bad_recall = base.clone();
    bad_recall.recall.recall_room_id = Some("room.void.phantom".to_string());
    assert_code(validate_area_spec(&bad_recall), INVALID_CONFIG_CODE);
}
