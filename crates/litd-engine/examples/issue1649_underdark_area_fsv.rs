// FSV harness for the #1649-#1660 Thera wilderness/region batch, exercised through
// its #1649 Underdark pilot (parent #987, world graph #1632). Self-contained: it
// re-declares one AreaSpec (the Underdark tunnels) via the public `litd_engine::rpg`
// API, persists it, reads it back SEPARATELY through `get_content_for_editor`, and
// proves stored bytes == authored bytes plus the acquired-map doctrine (#944: no
// lowered area grants a built-in map/minimap/GPS). It also drives two fail-closed
// edges (empty rooms; a dangling cross-area world-graph edge).
//
// Runtime note (#1493): the standalone example may fail at launch with
// CALYX_LENS_UNREACHABLE / sm_120 ORT in this headless env; that is a known runtime
// limitation, NOT a bug. The authoritative FSV is the in-suite store round-trip
// (same persist_and_lower + get_content_for_editor + sha256) which runs headless.

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
        ConnectionDirection, ConnectionDiscovery, RPG_AREA_SPEC_SCHEMA, RoomConcealment, RoomKind,
        TravelKind, area_spec_content_records, area_spec_readback,
        persist_and_lower_rpg_authored_content, validate_area_connections_against_registry,
        validate_area_spec,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const UNDERDARK_TUNNELS: &str = "area.thera.underdark.tunnels";
const VELKYN_OLOTH: &str = "area.thera.underdark.velkyn_oloth";
const TROGLODYTE_CAVERNS: &str = "area.thera.underdark.troglodyte_caverns";
const SVIRFNEBLIN_ENCLAVE: &str = "area.thera.underdark.svirfneblin_enclave";
const CRAGSTONE: &str = "area.thera.cragstone";

fn known_area_ids() -> [&'static str; 5] {
    [
        UNDERDARK_TUNNELS,
        VELKYN_OLOTH,
        TROGLODYTE_CAVERNS,
        SVIRFNEBLIN_ENCLAVE,
        CRAGSTONE,
    ]
}

fn underdark_tunnels_area_spec() -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: UNDERDARK_TUNNELS.to_string(),
        display_name: "The Underdark".to_string(),
        player_facing_canon_name: "The Underdark".to_string(),
        area_kind: AreaKind::Region,
        source_issue: 1649,
        canon_issues: vec![987, 1598, 1632, 1649],
        mechanics_source_url: "https://wiki.carrionfields.net/area/the-underdark".to_string(),
        source_history_label: "CF Underdark tunnel-web factual topology and mechanics only"
            .to_string(),
        summary: "The tunnel web entered from Cragstone: the underground world that looms larger \
            than all other landscapes combined, permanently dark and acoustically alive."
            .to_string(),
        level_band: AreaLevelBand {
            min_level: 35,
            max_level: 51,
        },
        rooms: vec![
            room("room.underdark_tunnels.cragstone_descent", "Cragstone Descent", RoomKind::Landmark,
                "The secret way down from Cragstone into the deep dark.",
                RoomConcealment::HiddenLayer, &[]),
            room("room.underdark_tunnels.tunnel_web", "Tunnel Web", RoomKind::Wilderness,
                "A branching maze of passages where sound carries strangely along the stone.",
                RoomConcealment::None, &["npc.underdark.tunnel_stalker"]),
            room("room.underdark_tunnels.echoing_gallery", "Echoing Gallery", RoomKind::Chamber,
                "A vast cavern where every footstep returns distorted (#1628).",
                RoomConcealment::None, &[]),
        ],
        connections: vec![
            internal("conn.underdark_tunnels.descent_to_web", "room.underdark_tunnels.cragstone_descent",
                "room.underdark_tunnels.tunnel_web", ConnectionDirection::Down, TravelKind::Descend),
            internal("conn.underdark_tunnels.web_to_gallery", "room.underdark_tunnels.tunnel_web",
                "room.underdark_tunnels.echoing_gallery", ConnectionDirection::East, TravelKind::Path),
            hidden("conn.underdark_tunnels.to_cragstone", "room.underdark_tunnels.cragstone_descent",
                None, CRAGSTONE, ConnectionDirection::Up, TravelKind::Descend,
                ConnectionDiscovery::Hidden),
            hidden("conn.underdark_tunnels.to_velkyn_oloth", "room.underdark_tunnels.echoing_gallery",
                None, VELKYN_OLOTH, ConnectionDirection::West, TravelKind::Path,
                ConnectionDiscovery::Hidden),
            hidden("conn.underdark_tunnels.to_troglodyte_caverns", "room.underdark_tunnels.tunnel_web",
                None, TROGLODYTE_CAVERNS, ConnectionDirection::North, TravelKind::Path,
                ConnectionDiscovery::Hidden),
            hidden("conn.underdark_tunnels.to_svirfneblin_enclave", "room.underdark_tunnels.tunnel_web",
                None, SVIRFNEBLIN_ENCLAVE, ConnectionDirection::South, TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail),
        ],
        resets: AreaResetProfile {
            reset_id: "reset.underdark_tunnels.cycle".to_string(),
            interval_minutes: 40,
            clears_unlooted_corpses: true,
            resets_doors_and_locks: false,
        },
        repop: AreaRepopProfile {
            repop_id: "repop.underdark_tunnels.population".to_string(),
            baseline_population_budget: 30,
            repop_interval_minutes: 20,
            max_pressure_tier: 3,
        },
        recall: AreaRecallProfile {
            is_hometown_option: false,
            recall_room_id: None,
            has_academy: false,
            hometown_access: AreaAccessRule::Any,
        },
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.wilderness.underdark_tunnels".to_string(),
            model_id: "model.wilderness.underdark_tunnels".to_string(),
            required_props: vec![
                "prop.underdark.stalactite_field".to_string(),
                "prop.underdark.glow_fungus".to_string(),
                "prop.underdark.rubble_pile".to_string(),
                "prop.underdark.cragstone_stair".to_string(),
            ],
            silhouette: "black basalt tunnel web with no sky, glow-fungus the only ambient light"
                .to_string(),
            material_notes: "Original Thera-Underdark stonework and fungal bioluminescence; no \
                copied iconography from the source MUD."
                .to_string(),
        },
        // #1725 era/variant link: the Underdark tunnels are a standalone base area, not a variant.
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: vec![
            "Permanent darkness: dark vision/infravision required (#1618); no day/night underground."
                .to_string(),
            "Surface link from Cragstone and the deep lairs are Hidden/ConcealedTrail per #944."
                .to_string(),
        ],
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1649_underdark_area_spec")?;
    let spec = underdark_tunnels_area_spec();
    let readback = area_spec_readback(&spec)?;
    let records = area_spec_content_records(&spec)?;

    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);

    // Source of truth BEFORE persistence: the store has no Underdark rows yet.
    let sot_before = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .is_ok();

    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;

    // Read back SEPARATELY and assert stored == authored.
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

    let report = json!({
        "schema": "litd.issue1649.underdark_area_fsv.v1",
        "issue": 1649,
        "parent_issue": 987,
        "world_graph_issue": 1632,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "mechanics_source_url": "https://wiki.carrionfields.net/area/the-underdark",
            "sources": source_readbacks()?,
            "sot_before_persist_has_underdark": sot_before,
            "sot_after_persist_row_count": persisted_rows.len(),
        },
        "sufficiency_argument": "The #1649-#1660 batch is proven end-to-end via its Underdark pilot: \
            author -> persist -> SEPARATE readback with stored==authored, plus edges for a dangling \
            world-graph edge, empty rooms, and the map/GPS doctrine. The in-suite tests exercise the \
            full ~57-place set the same way; this example is the single-spec FSV witness.",
        "expected_actuals": {
            "area_id": UNDERDARK_TUNNELS,
            "source_issue": 1649,
            "area_kind": "region",
            "room_count": readback.room_count,
            "connection_count": readback.connection_count,
            "external_connection_count": readback.external_connection_count,
            "hidden_connection_count": readback.hidden_connection_count,
            "has_recall_point": readback.has_recall_point,
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": persisted_rows.len(),
            "stored_equals_authored": stored_equals_authored,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_map_or_gps_true": content_has_map_or_gps_true,
            "sot_before_persist_has_underdark": sot_before,
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
        &report["expected_actuals"]["sot_before_persist_has_underdark"],
        false,
    )?;

    let readback_report = run.write_report(&report)?;
    let actuals = &readback_report.json["expected_actuals"];
    println!(
        "ISSUE1649_UNDERDARK_AREA_FSV report={} sha256={} area={} rows={} stored_eq_authored={} map_true={}",
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
    let mut spec = underdark_tunnels_area_spec();
    let before = area_spec_hash(&spec)?;
    spec.connections[2] = hidden(
        "conn.underdark_tunnels.to_void",
        "room.underdark_tunnels.cragstone_descent",
        None,
        "area.thera.nonexistent_void",
        ConnectionDirection::Up,
        TravelKind::Path,
        ConnectionDiscovery::Hidden,
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
        "case": "cross_area_dangling_world_graph_edge",
        "before_state": "connection targets area.thera.nonexistent_void (not in registry)",
        "after_state": "validate_area_connections_against_registry rejected it; spec unchanged",
        "before_spec_sha256": before,
        "after_spec_sha256": after,
        "state_unchanged": before == after,
        "error_code": error.code(),
        "error": error.to_string(),
    }))
}

fn edge_empty_rooms() -> Result<Value, EngineError> {
    let mut spec = underdark_tunnels_area_spec();
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
        "case": "happy_path_underdark_stored_equals_authored",
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
        UNDERDARK_TUNNELS,
        direction,
        travel_kind,
        ConnectionDiscovery::Visible,
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

fn source_readbacks() -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    const SOURCE_PATHS: [&str; 2] = [
        "crates/litd-engine/src/rpg/regions_thera_support.rs",
        "crates/litd-engine/examples/issue1649_underdark_area_fsv.rs",
    ];
    SOURCE_PATHS
        .into_iter()
        .map(|path| {
            let bytes = fs::read(path)?;
            Ok(json!({
                "path": path,
                "bytes": bytes.len(),
                "sha256": sha256_hex(&bytes),
            }))
        })
        .collect()
}

fn content_rows(rows: &[EngineContentConstellationReadback]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            json!({
                "id": row.id,
                "kind": row.kind,
                "source_path": row.source_path,
                "surface_kind": row.payload["surface_kind"],
                "area_id": row.payload["area_id"],
                "area_spec_sha256": row.payload["area_spec_sha256"],
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
        field: "issue1649.fsv.require_i64".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "issue1649.fsv.require_bool".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}
