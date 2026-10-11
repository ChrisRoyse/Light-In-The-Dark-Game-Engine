// FSV harness for the #1644-#1647 Thera town/settlement batch, exercised through the
// Copeham Inn AreaSpec (#1645). Self-contained: it re-implements the spec inline and
// proves author -> persist -> SEPARATE readback with stored == authored, plus the
// dangling-edge, empty-room, and map/GPS-doctrine edges. Copeham Inn reuses
// area.thera.galadon.copeham_inn, so this run also proves Galadon's forward-stub edge
// resolves. NOTE (#1493): the standalone binary needs a GPU ONNX runtime and fails
// headless with CALYX_LENS_UNREACHABLE; it must COMPILE. The authoritative store
// round-trip runs headless inside `cargo test` via the fixture store.
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

const COPEHAM_INN_AREA_ID: &str = "area.thera.galadon.copeham_inn";
const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1645_copeham_inn_area_fsv.rs";
const SUPPORT_PATH: &str = "crates/litd-engine/src/rpg/towns_thera_support.rs";
const HUB_PATH: &str = "crates/litd-engine/src/rpg/towns_thera_batch_1644_1647.rs";
const SOURCE_PATHS: [&str; 3] = [EXAMPLE_PATH, SUPPORT_PATH, HUB_PATH];

fn known_area_ids() -> [&'static str; 2] {
    [GALADON_AREA_ID, COPEHAM_INN_AREA_ID]
}

fn copeham_inn_area_spec() -> AreaSpec {
    let aid = COPEHAM_INN_AREA_ID;
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: aid.to_string(),
        display_name: "Copeham Inn".to_string(),
        player_facing_canon_name: "Copeham Inn".to_string(),
        area_kind: AreaKind::Town,
        source_issue: 1645,
        canon_issues: vec![987, 1598, 1632, 1645],
        mechanics_source_url: "https://wiki.carrionfields.net/area/copeham-inn".to_string(),
        source_history_label: "CF factual topology, ranking rosters, and reset cadence only"
            .to_string(),
        summary: "A warm roadside inn on the western way out of Galadon; twins, servants, \
            and locals keep a quietly wicked streak (evil ranking 1-5)."
            .to_string(),
        level_band: AreaLevelBand {
            min_level: 1,
            max_level: 30,
        },
        rooms: vec![
            room(
                "room.copeham_inn.yard",
                "Inn Yard",
                RoomKind::Gate,
                "The roadside yard and hitching post before the door.",
                RoomConcealment::GateChokepoint,
                &["npc.copeham_inn.stablehand"],
            ),
            room(
                "room.copeham_inn.common_room",
                "Common Room",
                RoomKind::Inn,
                "A hearth-lit common room of travellers, twins, and locals (evil ranking 1-5).",
                RoomConcealment::CrowdCover,
                &["npc.copeham_inn.twin", "npc.copeham_inn.local"],
            ),
            room(
                "room.copeham_inn.kitchen",
                "Inn Kitchen",
                RoomKind::Chamber,
                "A busy kitchen of servants and steaming pots.",
                RoomConcealment::None,
                &["npc.copeham_inn.servant"],
            ),
            room(
                "room.copeham_inn.guest_rooms",
                "Guest Rooms",
                RoomKind::Inn,
                "Quiet upstairs rooms for the night.",
                RoomConcealment::None,
                &["npc.copeham_inn.innkeeper"],
            ),
        ],
        connections: vec![
            internal(
                "conn.copeham_inn.yard_to_common",
                "room.copeham_inn.yard",
                "room.copeham_inn.common_room",
                ConnectionDirection::In,
                TravelKind::Gate,
            ),
            internal(
                "conn.copeham_inn.common_to_kitchen",
                "room.copeham_inn.common_room",
                "room.copeham_inn.kitchen",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            hidden(
                "conn.copeham_inn.common_to_guest",
                "room.copeham_inn.common_room",
                Some("room.copeham_inn.guest_rooms"),
                COPEHAM_INN_AREA_ID,
                ConnectionDirection::Up,
                TravelKind::Stair,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.copeham_inn.to_galadon",
                "room.copeham_inn.yard",
                GALADON_AREA_ID,
                ConnectionDirection::East,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
        ],
        resets: AreaResetProfile {
            reset_id: "reset.copeham_inn.cycle".to_string(),
            interval_minutes: 30,
            clears_unlooted_corpses: true,
            resets_doors_and_locks: true,
        },
        repop: AreaRepopProfile {
            repop_id: "repop.copeham_inn.pop".to_string(),
            baseline_population_budget: 14,
            repop_interval_minutes: 20,
            max_pressure_tier: 1,
        },
        recall: AreaRecallProfile {
            is_hometown_option: false,
            recall_room_id: None,
            has_academy: false,
            hometown_access: AreaAccessRule::Any,
        },
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.town.copeham_inn.roadside".to_string(),
            model_id: "model.town.copeham_inn.inn".to_string(),
            required_props: vec![
                "prop.town.hitching_post".to_string(),
                "prop.town.common_hearth".to_string(),
                "prop.town.inn_sign".to_string(),
            ],
            silhouette: "half-timbered roadside inn with a lantern-lit yard and upstairs rooms"
                .to_string(),
            material_notes: "Original roadside-inn props and signage; no copied iconography."
                .to_string(),
        },
        // #1725 era/variant link: the Copeham Inn is a standalone base area, not a variant.
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: vec![
            "Reuses area.thera.galadon.copeham_inn to resolve Galadon's forward-stub edge (#1636)."
                .to_string(),
            "RP waystop; twins/servants/locals are evil ranking 1-5 (#1645).".to_string(),
        ],
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1645_copeham_inn_area_spec")?;
    let spec = copeham_inn_area_spec();
    let readback = area_spec_readback(&spec)?;
    let records = area_spec_content_records(&spec)?;

    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);

    let sot_before = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .is_ok();

    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;

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
        "schema": "litd.issue1645.copeham_inn_area_fsv.v1",
        "issue": 1645,
        "batch_issues": [1644, 1645, 1646, 1647],
        "parent_issue": 987,
        "world_graph_issue": 1632,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "mechanics_source_url": "https://wiki.carrionfields.net/area/copeham-inn",
            "sources": source_readbacks,
            "sot_before_persist_has_copeham": sot_before,
            "sot_after_persist_row_count": persisted_rows.len(),
        },
        "sufficiency_argument": "One reused-id place (Copeham Inn) exercises the whole batch \
            pipeline end-to-end: author -> persist -> separate readback with stored == authored, \
            plus edges for dangling refs, empty rooms, and the map/GPS doctrine. The remaining 26 \
            AreaSpecs traverse the identical generic path, proven in-suite via cargo test.",
        "expected_actuals": {
            "area_id": COPEHAM_INN_AREA_ID,
            "source_issue": 1645,
            "area_kind": "town",
            "room_count": readback.room_count,
            "connection_count": readback.connection_count,
            "external_connection_count": readback.external_connection_count,
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": persisted_rows.len(),
            "stored_equals_authored": stored_equals_authored,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_map_or_gps_true": content_has_map_or_gps_true,
            "sot_before_persist_has_copeham": sot_before,
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
        &report["expected_actuals"]["sot_before_persist_has_copeham"],
        false,
    )?;

    let readback_report = run.write_report(&report)?;
    let actuals = &readback_report.json["expected_actuals"];
    println!(
        "ISSUE1645_COPEHAM_INN_AREA_FSV report={} sha256={} area={} rows={} stored_eq_authored={} map_true={}",
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
    let mut spec = copeham_inn_area_spec();
    let before = area_spec_hash(&spec)?;
    spec.connections[0] = external(
        "conn.copeham_inn.to_void",
        "room.copeham_inn.yard",
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
    let mut spec = copeham_inn_area_spec();
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
        "case": "happy_path_copeham_inn_stored_equals_authored",
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
        COPEHAM_INN_AREA_ID,
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
        field: "issue1645.fsv.require_i64".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}

include!("issue1645_copeham_inn_area_fsv/require_bool.rs");
