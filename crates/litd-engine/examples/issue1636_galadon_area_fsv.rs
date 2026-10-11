use std::collections::BTreeSet;
use std::fs;

use litd_engine::{
    EngineError,
    calyx_content::{EngineCalyxContentStore, EngineContentConstellationReadback},
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    rpg::{
        AreaAccessRule, AreaConnection, AreaKind, AreaLevelBand, AreaRecallProfile,
        AreaRepopProfile, AreaResetProfile, AreaRoom, AreaSpec, AreaVisualContract,
        ConnectionDirection, ConnectionDiscovery, GALADON_AREA_ID, RPG_AREA_SPEC_SCHEMA,
        RoomConcealment, RoomKind, TravelKind, area_spec_content_records, area_spec_readback,
        persist_and_lower_rpg_authored_content, validate_area_connections_against_registry,
        validate_area_spec,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const AREA_TYPES_PATH: &str = "crates/litd-engine/src/rpg/area_specs_types.rs";
const AREA_IMPL_PATH: &str = "crates/litd-engine/src/rpg/area_specs.rs";
const AREA_CONTENT_PATH: &str = "crates/litd-engine/src/rpg/area_specs_content.rs";
const AREA_FIXTURE_PATH: &str = "crates/litd-engine/src/rpg/area_specs_test_fixture.rs";
const AREA_TEST_PATH: &str = "crates/litd-engine/src/rpg/area_specs_tests.rs";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1636_galadon_area_fsv.rs";
const SOURCE_PATHS: [&str; 6] = [
    AREA_TYPES_PATH,
    AREA_IMPL_PATH,
    AREA_CONTENT_PATH,
    AREA_FIXTURE_PATH,
    AREA_TEST_PATH,
    EXAMPLE_PATH,
];

const LUMBER_YARD: &str = "area.thera.galadon.lumber_yard";
const KOBOLD_WARRENS: &str = "area.thera.galadon.kobold_warrens";
const GRAVEYARD: &str = "area.thera.galadon.graveyard";
const COPEHAM_INN: &str = "area.thera.galadon.copeham_inn";
const GOBLIN_VILLAGE: &str = "area.thera.galadon.goblin_village";
const VALE_OF_ARENDYL: &str = "area.thera.vale_of_arendyl";

fn known_area_ids() -> [&'static str; 7] {
    [
        GALADON_AREA_ID,
        LUMBER_YARD,
        KOBOLD_WARRENS,
        GRAVEYARD,
        COPEHAM_INN,
        GOBLIN_VILLAGE,
        VALE_OF_ARENDYL,
    ]
}

fn galadon_area_spec() -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: GALADON_AREA_ID.to_string(),
        display_name: "Galadon".to_string(),
        player_facing_canon_name: "Galadon".to_string(),
        area_kind: AreaKind::City,
        source_issue: 1636,
        canon_issues: vec![987, 1598, 1632, 1636],
        mechanics_source_url: "https://wiki.carrionfields.net/area/galadon".to_string(),
        source_history_label: "CF Galadon hub-city factual topology and mechanics only".to_string(),
        summary: "The hub city at the center of Thera: four gates, a Large Intersection, guildhalls, \
            a Tribunal, cartographer, inn, a temple recall point, the Academy, and the sewers beneath."
            .to_string(),
        level_band: AreaLevelBand { min_level: 1, max_level: 30 },
        rooms: vec![
            room("room.galadon.north_gate", "North Gate", RoomKind::Gate,
                "Northern chokepoint onto the Crossroads road.",
                RoomConcealment::GateChokepoint, &["npc.galadon.gate_guard"]),
            room("room.galadon.east_gate", "East Gate", RoomKind::Gate,
                "Eastern chokepoint onto the Eastern Road.",
                RoomConcealment::GateChokepoint, &["npc.galadon.gate_guard"]),
            room("room.galadon.south_gate", "South Gate", RoomKind::Gate,
                "Southern chokepoint toward the Rocky Cliffs.",
                RoomConcealment::GateChokepoint, &["npc.galadon.gate_guard"]),
            room("room.galadon.northwest_gate", "Northwest Gate", RoomKind::Gate,
                "Northwest chokepoint onto the Holy Road.",
                RoomConcealment::GateChokepoint, &["npc.galadon.gate_guard"]),
            room("room.galadon.large_intersection", "The Large Intersection", RoomKind::Intersection,
                "The crowded meeting-heart of the city.",
                RoomConcealment::CrowdCover, &["npc.galadon.pickpocket"]),
            room("room.galadon.holy_road", "Holy Road", RoomKind::Street,
                "The processional road out the northwest gate.",
                RoomConcealment::None, &[]),
            room("room.galadon.thieves_guild", "Thieves' Guildhall", RoomKind::Guildhall,
                "A shadowed backroom guild down a narrow alley.",
                RoomConcealment::AlleyShadow, &["npc.galadon.thief_guildmaster"]),
            room("room.galadon.grand_temple", "Grand Temple Altar", RoomKind::Temple,
                "The altar that serves as the city recall point.",
                RoomConcealment::None, &["npc.galadon.high_priest"]),
            room("room.galadon.sewer_underway", "Sewer Underway", RoomKind::Sewer,
                "The hidden layer beneath the streets; thief country, levels 5-30.",
                RoomConcealment::HiddenLayer, &["npc.galadon.sewer_thug"]),
        ],
        connections: vec![
            internal("conn.galadon.north_gate_in", "room.galadon.north_gate",
                "room.galadon.large_intersection", ConnectionDirection::South, TravelKind::Gate),
            internal("conn.galadon.east_gate_in", "room.galadon.east_gate",
                "room.galadon.large_intersection", ConnectionDirection::West, TravelKind::Gate),
            internal("conn.galadon.holy_road_from_nw_gate", "room.galadon.northwest_gate",
                "room.galadon.holy_road", ConnectionDirection::Northwest, TravelKind::Road),
            hidden("conn.galadon.sewer_grate", "room.galadon.large_intersection",
                Some("room.galadon.sewer_underway"), GALADON_AREA_ID, ConnectionDirection::Down,
                TravelKind::Descend, ConnectionDiscovery::Hidden),
            external("conn.galadon.to_graveyard", "room.galadon.holy_road", GRAVEYARD,
                ConnectionDirection::West, TravelKind::Path, ConnectionDiscovery::Visible),
            external("conn.galadon.to_goblin_village", "room.galadon.south_gate", GOBLIN_VILLAGE,
                ConnectionDirection::Southeast, TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail),
            external("conn.galadon.to_vale_of_arendyl", "room.galadon.holy_road", VALE_OF_ARENDYL,
                ConnectionDirection::Northwest, TravelKind::Road, ConnectionDiscovery::Visible),
        ],
        resets: AreaResetProfile {
            reset_id: "reset.galadon.city_cycle".to_string(),
            interval_minutes: 30,
            clears_unlooted_corpses: true,
            resets_doors_and_locks: true,
        },
        repop: AreaRepopProfile {
            repop_id: "repop.galadon.city_population".to_string(),
            baseline_population_budget: 40,
            repop_interval_minutes: 15,
            max_pressure_tier: 3,
        },
        recall: AreaRecallProfile {
            is_hometown_option: true,
            recall_room_id: Some("room.galadon.grand_temple".to_string()),
            has_academy: true,
            hometown_access: AreaAccessRule::Restricted {
                values: vec!["alignment.good".to_string(), "alignment.neutral".to_string()],
            },
        },
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.city.galadon.stone_streets".to_string(),
            model_id: "model.city.galadon.hub".to_string(),
            required_props: vec![
                "prop.city.gate_portcullis".to_string(),
                "prop.city.market_stall".to_string(),
                "prop.city.temple_altar".to_string(),
                "prop.city.sewer_grate".to_string(),
            ],
            silhouette: "walled stone hub city radiating four gate-roads from a broad intersection"
                .to_string(),
            material_notes: "Original Thera-Galadon stonework; no copied iconography.".to_string(),
        },
        variant_of_area_id: None, // #1725: Galadon is a standalone base area, not a variant.
        variant_label: None,
        implementation_notes: vec!["Topology is discoverable (#944); recall at temple (#1634).".to_string()],
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1636_galadon_area_spec")?;
    let spec = galadon_area_spec();
    let readback = area_spec_readback(&spec)?;
    let records = area_spec_content_records(&spec)?;

    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);

    // Source of truth BEFORE persistence: the store has no Galadon rows yet.
    let sot_before = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .is_ok();

    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;

    // Read back SEPARATELY through get_content_for_editor and assert stored==authored.
    let persisted_rows = records
        .iter()
        .map(|record| store.get_content_for_editor(record.kind, &record.id))
        .collect::<Result<Vec<_>, _>>()?;
    let stored_equals_authored = records.iter().zip(&persisted_rows).all(|(record, row)| {
        row.id == record.id && row.payload["area_spec_sha256"] == record.payload["area_spec_sha256"]
    });
    let persisted_json = serde_json::to_string(&persisted_rows)?;
    let content_has_map_or_gps_true = persisted_json.contains("\"built_in_map_required\":true")
        || persisted_json.contains("\"grants_map_or_minimap\":true")
        || persisted_json.contains("\"grants_gps_markers\":true");
    let source_paths_are_editor_authored = persisted_rows.iter().all(|row| {
        row.source_path
            .starts_with("editor://rpg/thera/area_specs/")
    });

    let edges = vec![
        edge_registry_dangling()?,
        edge_empty_rooms()?,
        edge_happy_path(&readback, &persisted_rows)?,
    ];

    let source_readbacks = SOURCE_PATHS
        .into_iter()
        .map(source_readback)
        .collect::<Result<Vec<_>, _>>()?;

    let report = json!({
        "schema": "litd.issue1636.galadon_area_fsv.v1",
        "issue": 1636,
        "parent_issue": 987,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "mechanics_source_url": "https://wiki.carrionfields.net/area/galadon",
            "sources": source_readbacks,
            "sot_before_persist_has_galadon": sot_before,
            "sot_after_persist_row_count": persisted_rows.len(),
        },
        "sufficiency_argument": "The #987 slice proves the generic AreaSpec framework end-to-end via \
            one real city pilot (#1636 Galadon): author->persist->separate readback with \
            stored==authored, plus edges for dangling refs, empty rooms, and the map/GPS doctrine. \
            One exact city is sufficient because the framework, not the full 25-area set, is #987.",
        "expected_actuals": {
            "area_id": GALADON_AREA_ID,
            "source_issue": 1636,
            "area_kind": "city",
            "room_count": readback.room_count,
            "connection_count": readback.connection_count,
            "gate_count": readback.gate_count,
            "external_connection_count": readback.external_connection_count,
            "hidden_connection_count": readback.hidden_connection_count,
            "has_sewers": readback.has_sewers,
            "is_hometown_option": readback.is_hometown_option,
            "has_academy": readback.has_academy,
            "has_recall_point": readback.has_recall_point,
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": persisted_rows.len(),
            "stored_equals_authored": stored_equals_authored,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_map_or_gps_true": content_has_map_or_gps_true,
            "sot_before_persist_has_galadon": sot_before,
            "edge_count": 3,
            "error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "happy_path": readback,
        "calyx": {
            "content_rows": content_rows(&persisted_rows),
            "lowered_entities": &lowered.entities,
        },
        "edges": edges,
    });

    require_i64(&report["expected_actuals"]["gate_count"], 4)?;
    require_i64(
        &report["expected_actuals"]["content_rows_read_back"],
        (1 + spec.rooms.len() + spec.connections.len()) as i64,
    )?;
    require_bool(&report["expected_actuals"]["stored_equals_authored"], true)?;
    require_bool(
        &report["expected_actuals"]["source_paths_are_editor_authored"],
        true,
    )?;
    require_bool(
        &report["expected_actuals"]["content_has_map_or_gps_true"],
        false,
    )?;
    require_bool(
        &report["expected_actuals"]["sot_before_persist_has_galadon"],
        false,
    )?;

    let readback_report = run.write_report(&report)?;
    let actuals = &readback_report.json["expected_actuals"];
    println!(
        "ISSUE1636_GALADON_AREA_FSV report={} sha256={} area={} rows={} stored_eq_authored={} map_true={}",
        readback_report.path.display(),
        readback_report.sha256,
        readback_report.json["happy_path"]["area_id"],
        actuals["content_rows_read_back"],
        actuals["stored_equals_authored"],
        actuals["content_has_map_or_gps_true"],
    );
    Ok(())
}

fn edge_registry_dangling() -> Result<Value, EngineError> {
    let mut spec = galadon_area_spec();
    let before = area_spec_hash(&spec)?;
    spec.connections[0] = external(
        "conn.galadon.to_void",
        "room.galadon.north_gate",
        "area.thera.nonexistent_void",
        ConnectionDirection::North,
        TravelKind::Road,
        ConnectionDiscovery::Visible,
    );
    let known: BTreeSet<&str> = known_area_ids().into_iter().collect();
    let error = match validate_area_connections_against_registry(&spec, &known) {
        Ok(()) => {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: "edge.registry_dangling".to_string(),
                reason: "expected dangling to_area_id to fail".to_string(),
            });
        }
        Err(error) => error,
    };
    let after = area_spec_hash(&spec)?;
    Ok(json!({
        "case": "cross_area_dangling_connection",
        "before_state": "connection[0] targets area.thera.nonexistent_void (not in registry)",
        "after_state": "validate_area_connections_against_registry rejected it; spec unchanged",
        "before_spec_sha256": before,
        "after_spec_sha256": after,
        "state_unchanged": before == after,
        "error_code": error.code(),
        "error": error.to_string(),
    }))
}

fn edge_empty_rooms() -> Result<Value, EngineError> {
    let mut spec = galadon_area_spec();
    let before = area_spec_hash(&spec)?;
    spec.rooms.clear();
    let error = match validate_area_spec(&spec) {
        Ok(()) => {
            return Err(EngineError::RpgStatsInvalidConfig {
                field: "edge.empty_rooms".to_string(),
                reason: "expected empty room set to fail".to_string(),
            });
        }
        Err(error) => error,
    };
    let after = area_spec_hash(&spec)?;
    Ok(json!({
        "case": "empty_room_set",
        "before_state": "spec with all rooms removed",
        "after_state": "validate_area_spec rejected the malformed area loud",
        "before_spec_sha256": before,
        "after_spec_sha256": after,
        "state_unchanged": before == after,
        "error_code": error.code(),
        "error": error.to_string(),
    }))
}

fn edge_happy_path(
    readback: &litd_engine::rpg::AreaSpecReadback,
    persisted_rows: &[EngineContentConstellationReadback],
) -> Result<Value, EngineError> {
    let summary = persisted_rows
        .first()
        .ok_or_else(|| EngineError::RpgStatsInvalidConfig {
            field: "edge.happy_path".to_string(),
            reason: "no persisted rows".to_string(),
        })?;
    Ok(json!({
        "case": "happy_path_galadon_stored_equals_authored",
        "before_state": "authored AreaSpec in memory (sha256 below)",
        "after_state": "read back via get_content_for_editor; sha256 matches authored",
        "authored_sha256": readback.area_spec_sha256,
        "readback_sha256": summary.payload["area_spec_sha256"],
        "state_unchanged": Value::String(readback.area_spec_sha256.clone())
            == summary.payload["area_spec_sha256"],
        "readback_surface_kind": summary.payload["surface_kind"],
    }))
}

fn room(
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

fn internal(
    connection_id: &str,
    from_room_id: &str,
    to_room_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
) -> AreaConnection {
    hidden(
        connection_id,
        from_room_id,
        Some(to_room_id),
        GALADON_AREA_ID,
        direction,
        travel_kind,
        ConnectionDiscovery::Visible,
    )
}

fn external(
    connection_id: &str,
    from_room_id: &str,
    to_area_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    hidden(
        connection_id,
        from_room_id,
        None,
        to_area_id,
        direction,
        travel_kind,
        discovery,
    )
}

#[allow(clippy::too_many_arguments)]
fn hidden(
    connection_id: &str,
    from_room_id: &str,
    to_room_id: Option<&str>,
    to_area_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    AreaConnection {
        connection_id: connection_id.to_string(),
        from_room_id: from_room_id.to_string(),
        to_area_id: to_area_id.to_string(),
        to_room_id: to_room_id.map(str::to_string),
        direction,
        travel_kind,
        discovery,
        locked_by_ref: None,
    }
}

fn area_spec_hash(spec: &AreaSpec) -> Result<String, EngineError> {
    serde_json::to_vec(spec)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize area spec edge: {error}"),
        })
}

fn source_readback(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(json!({
        "path": path,
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
    }))
}

fn content_rows(rows: &[EngineContentConstellationReadback]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            json!({
                "id": row.id,
                "kind": row.kind,
                "database_id": row.database.id,
                "source_path": row.source_path,
                "surface_kind": row.payload["surface_kind"],
                "area_id": row.payload["area_id"],
                "area_spec_sha256": row.payload["area_spec_sha256"],
                "lowering_fingerprint": row.lowering.fingerprint,
                "lowering_hot_path_policy": row.lowering.hot_path_policy,
                "built_in_map_required": row.payload["built_in_map_required"],
                "grants_map_or_minimap": row.payload["grants_map_or_minimap"],
                "grants_gps_markers": row.payload["grants_gps_markers"],
            })
        })
        .collect()
}

fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "issue1636.fsv.require_i64".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "issue1636.fsv.require_bool".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}
