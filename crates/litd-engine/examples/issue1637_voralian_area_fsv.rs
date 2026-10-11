// FSV for #1637 Voralian City. Self-contained: re-implements a representative
// slice of the spec inline (not importing the fixture) and proves author ->
// persist -> SEPARATE readback via the real EngineCalyxContentStore, mirroring
// issue1636_galadon_area_fsv.rs. Topology cited in the sibling fixture (#984).
use std::collections::BTreeSet;

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

const VORALIAN_AREA_ID: &str = "area.thera.voralian";
const LOCH_GRYNMEAR: &str = "area.thera.voralian.loch_grynmear";
const BRAMBLEFIELD: &str = "area.thera.voralian.bramblefield";
const BLACKWATER_APPROACH: &str = "area.thera.voralian.blackwater_approach";

fn voralian_area_spec() -> AreaSpec {
    AreaSpec {
        schema: RPG_AREA_SPEC_SCHEMA.to_string(),
        area_id: VORALIAN_AREA_ID.to_string(),
        display_name: "Voralian City".to_string(),
        player_facing_canon_name: "Voralian City".to_string(),
        area_kind: AreaKind::City,
        source_issue: 1637,
        canon_issues: vec![987, 1598, 1632, 1637],
        mechanics_source_url: "https://wiki.carrionfields.net/guide/isildurs-ranking-guide"
            .to_string(),
        source_history_label: "CF Voralian City factual topology/mechanics only; name/flavor \
            phrasing original."
            .to_string(),
        summary: "The western twin city, reached from Galadon via Shepherds Row: a North-South \
            Road spine, Silverpine Road east toward Loch Grynmear, and the poisoner thieves' \
            guildhall."
            .to_string(),
        level_band: AreaLevelBand {
            min_level: 1,
            max_level: 40,
        },
        rooms: vec![
            room(
                "room.voralian.shepherds_gate",
                "Shepherds Gate",
                RoomKind::Gate,
                "The western gate onto Shepherds Row toward Galadon.",
                RoomConcealment::GateChokepoint,
                &["npc.voralian.gate_guard"],
            ),
            room(
                "room.voralian.shepherds_row",
                "Shepherds Row",
                RoomKind::Street,
                "The approach road outside the gate.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.voralian.city_square",
                "Voralian City Square",
                RoomKind::Intersection,
                "The crowded heart of the city.",
                RoomConcealment::CrowdCover,
                &["npc.voralian.pickpocket"],
            ),
            room(
                "room.voralian.silverpine_road",
                "Silverpine Road",
                RoomKind::Street,
                "The eastern road toward Loch Grynmear.",
                RoomConcealment::None,
                &[],
            ),
            room(
                "room.voralian.poisoner_guildhall",
                "Poisoner Guildhall",
                RoomKind::Guildhall,
                "The venom-guild's backstreet hall; membership switches hometown.",
                RoomConcealment::AlleyShadow,
                &["npc.voralian.poisoner_kingpin"],
            ),
            room(
                "room.voralian.temple",
                "Voralian Temple",
                RoomKind::Temple,
                "The altar recall point.",
                RoomConcealment::None,
                &["npc.voralian.priest"],
            ),
            room(
                "room.voralian.academy",
                "Voralian Academy",
                RoomKind::Academy,
                "The level 1-5 school entrance.",
                RoomConcealment::None,
                &["npc.voralian.academy_guildmaster"],
            ),
        ],
        connections: vec![
            internal(
                "conn.voralian.gate_to_row",
                "room.voralian.shepherds_gate",
                "room.voralian.shepherds_row",
                ConnectionDirection::West,
                TravelKind::Gate,
            ),
            internal(
                "conn.voralian.row_to_square",
                "room.voralian.shepherds_row",
                "room.voralian.city_square",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.voralian.square_to_silverpine",
                "room.voralian.city_square",
                "room.voralian.silverpine_road",
                ConnectionDirection::East,
                TravelKind::Road,
            ),
            internal(
                "conn.voralian.square_to_temple",
                "room.voralian.city_square",
                "room.voralian.temple",
                ConnectionDirection::North,
                TravelKind::Road,
            ),
            internal(
                "conn.voralian.square_to_academy",
                "room.voralian.city_square",
                "room.voralian.academy",
                ConnectionDirection::South,
                TravelKind::Road,
            ),
            hidden(
                "conn.voralian.square_to_poisoners",
                "room.voralian.city_square",
                Some("room.voralian.poisoner_guildhall"),
                VORALIAN_AREA_ID,
                ConnectionDirection::Southeast,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
            external(
                "conn.voralian.to_galadon",
                "room.voralian.shepherds_row",
                GALADON_AREA_ID,
                ConnectionDirection::West,
                TravelKind::Road,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.voralian.to_loch_grynmear",
                "room.voralian.silverpine_road",
                LOCH_GRYNMEAR,
                ConnectionDirection::East,
                TravelKind::ConcealedTrail,
                ConnectionDiscovery::ConcealedTrail,
            ),
            external(
                "conn.voralian.to_bramblefield",
                "room.voralian.city_square",
                BRAMBLEFIELD,
                ConnectionDirection::North,
                TravelKind::Path,
                ConnectionDiscovery::Visible,
            ),
            external(
                "conn.voralian.to_blackwater",
                "room.voralian.city_square",
                BLACKWATER_APPROACH,
                ConnectionDirection::South,
                TravelKind::Path,
                ConnectionDiscovery::Hidden,
            ),
        ],
        resets: AreaResetProfile {
            reset_id: "reset.voralian.city_cycle".to_string(),
            interval_minutes: 30,
            clears_unlooted_corpses: true,
            resets_doors_and_locks: true,
        },
        repop: AreaRepopProfile {
            repop_id: "repop.voralian.city_population".to_string(),
            baseline_population_budget: 35,
            repop_interval_minutes: 15,
            max_pressure_tier: 3,
        },
        recall: AreaRecallProfile {
            is_hometown_option: true,
            recall_room_id: Some("room.voralian.temple".to_string()),
            has_academy: true,
            hometown_access: AreaAccessRule::Restricted {
                values: vec![
                    "alignment.good".to_string(),
                    "alignment.neutral".to_string(),
                ],
            },
        },
        visual: AreaVisualContract {
            renderer: "Rust/wgpu".to_string(),
            tileset_id: "tileset.city.voralian.stone_streets".to_string(),
            model_id: "model.city.voralian.twin_city".to_string(),
            required_props: vec![
                "prop.city.gate_portcullis".to_string(),
                "prop.city.market_stall".to_string(),
                "prop.city.temple_altar".to_string(),
                "prop.city.venom_vial_sign".to_string(),
            ],
            silhouette: "twin-city crossroads split by a North-South Road spine".to_string(),
            material_notes: "Original Voralian stonework; no copied iconography.".to_string(),
        },
        // #1725 era/variant link: Voralian is a standalone base area, not a variant.
        variant_of_area_id: None,
        variant_label: None,
        implementation_notes: vec![
            "Topology is discoverable (#944): poisoner alley and Blackwater approach are \
                Hidden; Loch Grynmear path is a ConcealedTrail."
                .to_string(),
        ],
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    // Wall-clock fail-fast: self-abort loud if any blocking op wedges (#1712).
    let _deadline =
        litd_engine::fsv_deadline::FsvDeadlineGuard::install("issue1637_voralian_area_fsv");
    let run = begin_issue_run("issue1637_voralian_area_spec")?;
    let spec = voralian_area_spec();
    let readback = area_spec_readback(&spec)?;
    let records = area_spec_content_records(&spec)?;

    let vault_dir = run.artifact_dir("calyx-vault")?;
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);

    // Source of truth BEFORE persistence: the store has no Voralian rows yet.
    let sot_before = store
        .get_content_for_editor(records[0].kind, &records[0].id)
        .is_ok();
    println!("BEFORE: store has Voralian rows = {sot_before} (expected false)");

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
    println!(
        "AFTER: rows persisted={} rows read back={} stored_equals_authored={} \
            content_has_map_or_gps_true={} source_paths_editor_authored={}",
        lowered.row_count,
        persisted_rows.len(),
        stored_equals_authored,
        content_has_map_or_gps_true,
        source_paths_are_editor_authored
    );

    let edges = vec![
        edge_happy_path(&readback, &persisted_rows)?,
        edge_registry_dangling()?,
        edge_empty_rooms()?,
    ];

    let report = json!({
        "schema": "litd.issue1637.voralian_area_fsv.v1",
        "issue": 1637,
        "parent_issues": [987, 1598, 1632],
        "mechanics_source_url": "https://wiki.carrionfields.net/guide/isildurs-ranking-guide",
        "calyx_vault_dir": vault_dir.display().to_string(),
        "expected_actuals": {
            "area_id": VORALIAN_AREA_ID,
            "gate_count": readback.gate_count,
            "external_connection_count": readback.external_connection_count,
            "hidden_connection_count": readback.hidden_connection_count,
            "has_recall_point": readback.has_recall_point,
            "content_rows_written": lowered.row_count,
            "content_rows_read_back": persisted_rows.len(),
            "stored_equals_authored": stored_equals_authored,
            "source_paths_are_editor_authored": source_paths_are_editor_authored,
            "content_has_map_or_gps_true": content_has_map_or_gps_true,
            "sot_before_persist_has_voralian": sot_before,
            "error_code": "LITD-RPG-STATS-INVALID-CONFIG"
        },
        "happy_path": readback,
        "edges": edges,
    });

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
        &report["expected_actuals"]["sot_before_persist_has_voralian"],
        false,
    )?;
    require_i64(
        &report["expected_actuals"]["content_rows_read_back"],
        (1 + spec.rooms.len() + spec.connections.len()) as i64,
    )?;

    let readback_report = run.write_report(&report)?;
    let actuals = &readback_report.json["expected_actuals"];
    println!(
        "ISSUE1637_VORALIAN_AREA_FSV report={} sha256={} area={} rows={} stored_eq_authored={} \
            map_true={}",
        readback_report.path.display(),
        readback_report.sha256,
        readback_report.json["happy_path"]["area_id"],
        actuals["content_rows_read_back"],
        actuals["stored_equals_authored"],
        actuals["content_has_map_or_gps_true"],
    );
    Ok(())
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
    let matches =
        Value::String(readback.area_spec_sha256.clone()) == summary.payload["area_spec_sha256"];
    println!(
        "EDGE happy_path: authored_sha256={} readback_sha256={} match={}",
        readback.area_spec_sha256, summary.payload["area_spec_sha256"], matches
    );
    Ok(json!({
        "case": "happy_path_voralian_stored_equals_authored",
        "before_state": "authored AreaSpec in memory (sha256 below)",
        "after_state": "read back via get_content_for_editor; sha256 matches authored",
        "authored_sha256": readback.area_spec_sha256,
        "readback_sha256": summary.payload["area_spec_sha256"],
        "state_unchanged": matches,
        "readback_surface_kind": summary.payload["surface_kind"],
    }))
}

fn edge_registry_dangling() -> Result<Value, EngineError> {
    let mut spec = voralian_area_spec();
    let before = area_spec_hash(&spec)?;
    spec.connections[0] = external(
        "conn.voralian.to_void",
        "room.voralian.shepherds_gate",
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
    println!(
        "EDGE cross_area_dangling_connection: before_sha256={before} after_sha256={after} \
            unchanged={} rejected_with={}",
        before == after,
        error.code()
    );
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
    let mut spec = voralian_area_spec();
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
    println!(
        "EDGE empty_room_set: before_sha256={before} after_sha256={after} unchanged={} \
            rejected_with={}",
        before == after,
        error.code()
    );
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
        VORALIAN_AREA_ID,
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

include!("issue1637_voralian_area_fsv/support.rs");
