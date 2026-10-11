// #1657 Graveyard dungeon FSV (parent #987, world-graph #1632). Self-contained:
// authors the Graveyard AreaSpec, persists via persist_and_lower, reads it back
// SEPARATELY through get_content_for_editor, and asserts stored == authored (sha256)
// with the acquired-map doctrine flags (#944) never true, plus fail-closed edges.
//
// Runtime is deferred: standalone this fails with CALYX_LENS_UNREACHABLE / sm_120 ORT
// (the known #1493 env limitation, not a bug). The authoritative FSV is the in-suite
// store round-trip (assert_dungeon_roundtrip), which runs headless via the fixture store.

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

const GRAVEYARD: &str = "area.thera.galadon.graveyard";

fn known_area_ids() -> [&'static str; 2] {
    [GALADON_AREA_ID, GRAVEYARD]
}

fn graveyard_area_spec() -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: GRAVEYARD.to_string(),
        display_name: "Graveyard".to_string(),
        player_facing_canon_name: "Graveyard".to_string(),
        area_kind: AreaKind::Dungeon,
        source_issue: 1657,
        canon_issues: vec![987, 1598, 1632, 1636, 1657],
        mechanics_source_url: "https://wiki.carrionfields.net/game/areas".to_string(),
        source_history_label: "CF Graveyard factual level band and denizen roster only".to_string(),
        summary: "The consecrated ground off Galadon's Holy Road: a walled graveyard whose sunken \
            tombs descend into crypts full of skeletons, zombies, ghouls, and a banshee."
            .to_string(),
        level_band: AreaLevelBand { min_level: 1, max_level: 10 },
        rooms: vec![
            room("room.graveyard.gate", "Iron Grave Gate", RoomKind::Gate,
                "The rusted gate off the Holy Road where gravediggers keep their vigil.",
                RoomConcealment::GateChokepoint, &["npc.graveyard.gravedigger"]),
            room("room.graveyard.grounds", "Graveyard Grounds", RoomKind::Chamber,
                "Leaning headstones and disturbed earth; skeletons and graverobbers wander.",
                RoomConcealment::None, &["npc.graveyard.skeleton"]),
            room("room.graveyard.crypt_stair", "Crypt Stair", RoomKind::Chamber,
                "A stair spiralling down into the sunken tombs; echoes carry every footfall.",
                RoomConcealment::None, &[]),
            room("room.graveyard.tombs", "Sunken Tombs", RoomKind::Chamber,
                "Flooded burial vaults where zombies and ghouls shamble in the dark.",
                RoomConcealment::HiddenLayer, &["npc.graveyard.ghoul"]),
            room("room.graveyard.banshee_vault", "Banshee's Vault", RoomKind::Chamber,
                "The deepest tomb, where a wailing banshee guards the oldest dead.",
                RoomConcealment::None, &["npc.graveyard.banshee"]),
            room("room.graveyard.reliquary", "Hidden Reliquary", RoomKind::Landmark,
                "A sealed reliquary of grave-goods behind a mourner-warded door.",
                RoomConcealment::HiddenLayer, &[]),
        ],
        connections: vec![
            external("conn.graveyard.to_galadon", "room.graveyard.gate", GALADON_AREA_ID,
                ConnectionDirection::East, TravelKind::Path, ConnectionDiscovery::Visible),
            internal("conn.graveyard.enter", "room.graveyard.gate", "room.graveyard.grounds",
                ConnectionDirection::In, TravelKind::Path),
            internal("conn.graveyard.descend", "room.graveyard.grounds", "room.graveyard.crypt_stair",
                ConnectionDirection::Down, TravelKind::Descend),
            internal("conn.graveyard.stair", "room.graveyard.crypt_stair", "room.graveyard.tombs",
                ConnectionDirection::Down, TravelKind::Stair),
            internal("conn.graveyard.to_boss", "room.graveyard.tombs", "room.graveyard.banshee_vault",
                ConnectionDirection::In, TravelKind::Path),
            locked("conn.graveyard.reliquary_door", "room.graveyard.banshee_vault",
                "room.graveyard.reliquary", ConnectionDirection::In, TravelKind::Stair,
                "lock.graveyard.reliquary"),
            hidden("conn.graveyard.secret", "room.graveyard.crypt_stair", "room.graveyard.reliquary",
                ConnectionDirection::Down, TravelKind::Descend, ConnectionDiscovery::Hidden),
        ],
        resets: AreaResetProfile {
            reset_id: "reset.graveyard.cycle".to_string(),
            interval_minutes: 45,
            clears_unlooted_corpses: true,
            resets_doors_and_locks: true,
        },
        repop: AreaRepopProfile {
            repop_id: "repop.graveyard.denizens".to_string(),
            baseline_population_budget: 24,
            repop_interval_minutes: 30,
            max_pressure_tier: 2,
        },
        recall: AreaRecallProfile {
            is_hometown_option: false,
            recall_room_id: None,
            has_academy: false,
            hometown_access: AreaAccessRule::Any,
        },
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.dungeon.graveyard.stone".to_string(),
            model_id: "model.dungeon.graveyard".to_string(),
            required_props: vec![
                "prop.dungeon.graveyard.entry_arch".to_string(),
                "prop.dungeon.torch_sconce".to_string(),
                "prop.dungeon.rubble_pile".to_string(),
                "prop.crypt.headstone".to_string(),
                "prop.crypt.iron_grave_gate".to_string(),
            ],
            silhouette: "walled hillside graveyard of leaning headstones over a descending crypt warren"
                .to_string(),
            material_notes: "Original Thera Graveyard stonework, warding glyphs, and biers; no copied \
                iconography from the source MUD."
                .to_string(),
        },
        // #1725 era/variant link: the Graveyard is a standalone base area, not a variant.
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: vec![
            "Level band 1-10 and denizen roster are factual (CF area registry + #1657); \
                originalized name/lore only (#984)."
                .to_string(),
            "area_id reuses Galadon's `conn.galadon.to_graveyard` target so the world-graph edge \
                resolves (#1632); undead use sound-only perception (#1600), crypt-echo acoustics (#1628)."
                .to_string(),
        ],
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    // Wall-clock fail-fast: self-abort loud if any blocking op wedges (#1712).
    let _deadline =
        litd_engine::fsv_deadline::FsvDeadlineGuard::install("issue1657_graveyard_area_fsv");
    let run = begin_issue_run("issue1657_graveyard_area_spec")?;
    let spec = graveyard_area_spec();
    let readback = area_spec_readback(&spec)?;
    let records = area_spec_content_records(&spec)?;

    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);

    // BEFORE: the store has no Graveyard rows yet.
    let sot_before = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .is_ok();

    let lowered = persist_and_lower_rpg_authored_content(&store, &records)?;

    // AFTER: read back SEPARATELY through get_content_for_editor; assert stored == authored.
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
        "schema": "litd.issue1657.graveyard_area_fsv.v1",
        "issue": 1657,
        "parent_issue": 987,
        "world_graph_issue": 1632,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "calyx_vault_dir": vault_dir.display().to_string(),
            "mechanics_source_url": "https://wiki.carrionfields.net/game/areas",
            "sot_before_persist_has_graveyard": sot_before,
            "sot_after_persist_row_count": persisted_rows.len(),
        },
        "sufficiency_argument": "One exact dungeon (Graveyard, #1657) proves the AreaSpec dungeon \
            slice end-to-end: author -> persist -> separate readback with stored == authored, the \
            world-graph edge back to Galadon, and edges for dangling refs, empty rooms, and the \
            map/GPS doctrine. The remaining 19 dungeons are proven headless by the in-suite \
            round-trip tests.",
        "expected_actuals": {
            "area_id": GRAVEYARD,
            "source_issue": 1657,
            "area_kind": "dungeon",
            "room_count": readback.room_count,
            "connection_count": readback.connection_count,
            "external_connection_count": readback.external_connection_count,
            "hidden_connection_count": readback.hidden_connection_count,
            "is_hometown_option": readback.is_hometown_option,
            "has_recall_point": readback.has_recall_point,
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": persisted_rows.len(),
            "stored_equals_authored": stored_equals_authored,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_map_or_gps_true": content_has_map_or_gps_true,
            "sot_before_persist_has_graveyard": sot_before,
            "edge_count": 3,
            "error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "happy_path": readback,
        "sources": SOURCE_PATHS
            .into_iter()
            .map(source_readback)
            .collect::<Result<Vec<_>, _>>()?,
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
        &report["expected_actuals"]["sot_before_persist_has_graveyard"],
        false,
    )?;

    let readback_report = run.write_report(&report)?;
    let actuals = &readback_report.json["expected_actuals"];
    println!(
        "ISSUE1657_GRAVEYARD_AREA_FSV report={} sha256={} area={} rows={} stored_eq_authored={} map_true={}",
        readback_report.path.display(),
        readback_report.sha256,
        readback_report.json["happy_path"]["area_id"],
        actuals["content_rows_read_back"],
        actuals["stored_equals_authored"],
        actuals["content_has_map_or_gps_true"],
    );
    Ok(())
}

const EXAMPLE_PATH: &str = "crates/litd-engine/examples/issue1657_graveyard_area_fsv.rs";
const SOURCE_PATHS: [&str; 2] = [
    "crates/litd-engine/src/rpg/dungeons_thera_support.rs",
    EXAMPLE_PATH,
];

fn edge_registry_dangling() -> Result<Value, EngineError> {
    let mut spec = graveyard_area_spec();
    let before = area_spec_hash(&spec)?;
    spec.connections[0] = external(
        "conn.graveyard.to_void",
        "room.graveyard.gate",
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
        "before_spec_sha256": before,
        "after_spec_sha256": after,
        "state_unchanged": before == after,
        "error_code": error.code(),
    }))
}

fn edge_empty_rooms() -> Result<Value, EngineError> {
    let mut spec = graveyard_area_spec();
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
        "before_spec_sha256": before,
        "after_spec_sha256": after,
        "state_unchanged": before == after,
        "error_code": error.code(),
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
        "case": "happy_path_graveyard_stored_equals_authored",
        "authored_sha256": readback.area_spec_sha256,
        "readback_sha256": summary.payload["area_spec_sha256"],
        "state_unchanged": Value::String(readback.area_spec_sha256.clone())
            == summary.payload["area_spec_sha256"],
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
    conn(
        connection_id,
        from_room_id,
        GRAVEYARD,
        Some(to_room_id),
        direction,
        travel_kind,
        ConnectionDiscovery::Visible,
        None,
    )
}

fn hidden(
    connection_id: &str,
    from_room_id: &str,
    to_room_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
) -> AreaConnection {
    conn(
        connection_id,
        from_room_id,
        GRAVEYARD,
        Some(to_room_id),
        direction,
        travel_kind,
        discovery,
        None,
    )
}

fn locked(
    connection_id: &str,
    from_room_id: &str,
    to_room_id: &str,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    lock_ref: &str,
) -> AreaConnection {
    conn(
        connection_id,
        from_room_id,
        GRAVEYARD,
        Some(to_room_id),
        direction,
        travel_kind,
        ConnectionDiscovery::Visible,
        Some(lock_ref),
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
    conn(
        connection_id,
        from_room_id,
        to_area_id,
        None,
        direction,
        travel_kind,
        discovery,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn conn(
    connection_id: &str,
    from_room_id: &str,
    to_area_id: &str,
    to_room_id: Option<&str>,
    direction: ConnectionDirection,
    travel_kind: TravelKind,
    discovery: ConnectionDiscovery,
    locked_by_ref: Option<&str>,
) -> AreaConnection {
    AreaConnection {
        connection_id: connection_id.to_string(),
        from_room_id: from_room_id.to_string(),
        to_area_id: to_area_id.to_string(),
        to_room_id: to_room_id.map(str::to_string),
        direction,
        travel_kind,
        discovery,
        locked_by_ref: locked_by_ref.map(str::to_string),
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
    Ok(json!({ "path": path, "bytes": bytes.len(), "sha256": sha256_hex(&bytes) }))
}

fn require_i64(value: &Value, expected: i64) -> Result<(), EngineError> {
    if value.as_i64() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "issue1657.fsv.require_i64".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}

fn require_bool(value: &Value, expected: bool) -> Result<(), EngineError> {
    if value.as_bool() == Some(expected) {
        return Ok(());
    }
    Err(EngineError::RpgStatsInvalidConfig {
        field: "issue1657.fsv.require_bool".to_string(),
        reason: format!("expected {expected}, got {value}"),
    })
}
