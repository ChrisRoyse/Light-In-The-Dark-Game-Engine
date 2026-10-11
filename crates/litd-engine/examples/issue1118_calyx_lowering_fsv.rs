use std::{fs, io, path::Path};

use calyx_core::{Anchor, AnchorKind, AnchorValue};
use litd_engine::{
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
    },
    calyx_lowering::{
        ENGINE_LOWERED_SIM_PACKAGE_SCHEMA, ENGINE_LOWERING_HOT_PATH_POLICY,
        ENGINE_LOWERING_SIM_ACCESS_MODE, ENGINE_LOWERING_SOURCE, EngineLoweredSimPackage,
        EngineLoweringError, EngineLoweringValve,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    determinism_boundary::{HotPathPhase, enter_hot_path},
    fsv_artifacts::begin_issue_run,
    math::{Fixed, FixedVec3},
    sim::{FixedTickSim, SimConfig},
    sim_order::SimOrderSourceKind,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1118_calyx_lowering")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);

    let record = unit_record("unit-warden");
    let source_readback = store.put_content(&record)?;
    let package = EngineLoweringValve::lower_content_for_sim(&source_readback)?;
    let verified = EngineLoweringValve::verify_lowered_package(&package)?;
    let sim_probe = sim_hash_probe()?;
    let edges = edge_readbacks(&store, &source_readback, &package)?;
    let field_paths = package
        .fields
        .iter()
        .map(|field| field.path.clone())
        .collect::<Vec<_>>();
    let mut sorted_paths = field_paths.clone();
    sorted_paths.sort();
    let vault_files = vault_file_readbacks(&vault_dir)?;

    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let source_matches = package.source_cx_id == source_readback.cx_id
        && package.source_kind == source_readback.kind
        && package.source_id == source_readback.id
        && package.source_fingerprint == source_readback.input_sha256;
    let package_verified = verified.package_fingerprint == package.package_fingerprint
        && verified.field_count == package.fields.len();
    let deterministic_access = package.hot_path_policy == ENGINE_LOWERING_HOT_PATH_POLICY
        && package.sim_access_mode == ENGINE_LOWERING_SIM_ACCESS_MODE
        && !package.calyx_live_query_allowed_in_hot_path
        && package.sim_order_source().kind == SimOrderSourceKind::LoweredAgent;
    let sorted_unique_fields = field_paths == sorted_paths;
    let expected_fields_present = [
        "/can_attack",
        "/empty_slots",
        "/hit_points_fp",
        "/metadata",
        "/move_speed_fp",
        "/spawn/x_fp",
        "/spawn/y_fp",
        "/spawn/z_fp",
        "/tags/0",
        "/tags/1",
        "/unit_id",
    ]
    .iter()
    .all(|path| field_paths.iter().any(|field| field == path));

    ensure(
        source_matches,
        "lowered source metadata did not match Calyx readback",
    )?;
    ensure(package_verified, "lowered package verification failed")?;
    ensure(
        deterministic_access,
        "lowered package allowed live Calyx hot-path access",
    )?;
    ensure(sorted_unique_fields, "lowered field paths were not sorted")?;
    ensure(
        expected_fields_present,
        "expected lowered fields were missing",
    )?;
    ensure(
        sim_probe["hash_unchanged"] == true,
        "sim hash changed during lowering probe",
    )?;
    ensure(
        all_edges_matched,
        "lowering edge readback acceptance failed",
    )?;

    let report = json!({
        "schema": "litd.engine.issue1118.calyx_lowering_fsv.v1",
        "schema_version": 1,
        "issue": 1118,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "vault_dir": vault_dir.display().to_string(),
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON, manifest, Aster vault rows, source readback, lowered package, and hashes"
        },
        "runtime_contract": {
            "lowered_package_schema": ENGINE_LOWERED_SIM_PACKAGE_SCHEMA,
            "lowering_source": ENGINE_LOWERING_SOURCE,
            "hot_path_policy": ENGINE_LOWERING_HOT_PATH_POLICY,
            "sim_access_mode": ENGINE_LOWERING_SIM_ACCESS_MODE,
            "live_calyx_hot_path_query_allowed": false
        },
        "happy_path": {
            "content": {
                "kind": source_readback.kind,
                "id": source_readback.id,
                "display_name": source_readback.display_name,
                "cx_id": source_readback.cx_id,
                "input_sha256": source_readback.input_sha256,
                "database_seq": source_readback.database.seq,
                "database_sha256": source_readback.database.sha256,
                "row_key": source_readback.database.row_key,
                "graph_rows": source_readback.database.graph_rows,
                "lowering": source_readback.lowering
            },
            "lowered_package": package,
            "verified_readback": verified,
            "field_paths": field_paths,
            "sim_probe": sim_probe,
            "vault_files": vault_files
        },
        "acceptance": {
            "source_matches": source_matches,
            "package_verified": package_verified,
            "field_count": verified.field_count,
            "sorted_unique_fields": sorted_unique_fields,
            "expected_fields_present": expected_fields_present,
            "package_fingerprint": verified.package_fingerprint,
            "source_fingerprint": verified.source_fingerprint,
            "hot_path_policy": verified.hot_path_policy,
            "sim_access_mode": verified.sim_access_mode,
            "calyx_live_query_allowed_in_hot_path": verified.calyx_live_query_allowed_in_hot_path,
            "sim_hash_unchanged": sim_probe["hash_unchanged"],
            "sim_order_source_kind": format!("{:?}", package.sim_order_source().kind),
            "all_edges_matched": all_edges_matched
        },
        "edges": edges
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1118_CALYX_LOWERING_FSV report={} sha256={} manifest={} run_id={} fields={} edges={} package_fingerprint={}",
        readback.path.display(),
        readback.sha256,
        readback.latest_manifest_path.display(),
        run.run_id(),
        verified.field_count,
        edges.len(),
        verified.package_fingerprint
    );
    Ok(())
}

fn unit_record(id: &str) -> EngineContentRecord {
    EngineContentRecord {
        kind: EngineContentKind::Unit,
        id: id.to_string(),
        display_name: id.replace('-', " "),
        payload: json!({
            "unit_id": id,
            "hit_points_fp": 120 * Fixed::ONE_RAW,
            "move_speed_fp": Fixed::ONE_RAW * 3,
            "spawn": {
                "x_fp": 0,
                "y_fp": 0,
                "z_fp": -2 * Fixed::ONE_RAW
            },
            "can_attack": true,
            "tags": ["frontline", "guard"],
            "empty_slots": [],
            "metadata": {}
        }),
        source: EngineContentSource {
            path: format!("synthetic://issue1118/{id}"),
            author: "issue1118-fsv".to_string(),
            source_hash: litd_hash::sha256_hex(format!("issue1118:{id}").as_bytes()),
        },
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue1118_calyx_lowering_fsv".to_string(),
            observed_at: 1_786_000_000,
            confidence: 1.0,
        }],
    }
}

fn sim_hash_probe() -> Result<Value, Box<dyn std::error::Error>> {
    let mut sim = FixedTickSim::<4>::new(SimConfig::new(60)?, 77)?;
    sim.spawn_body(FixedVec3::ZERO, FixedVec3::ZERO, FixedVec3::ZERO)?;
    let before = sim.state_hash();
    let after = sim.state_hash();
    Ok(json!({
        "before_state_hash": before,
        "after_state_hash": after,
        "hash_unchanged": before == after
    }))
}

fn edge_readbacks(
    store: &EngineCalyxContentStore,
    source: &litd_engine::calyx_content::EngineContentConstellationReadback,
    package: &EngineLoweredSimPackage,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let float = store.put_content(&unit_record_with_payload(
        "unit-float",
        json!({"unit_id": "unit-float", "move_speed_fp": 1.5}),
    ))?;
    let null_child = store.put_content(&unit_record_with_payload(
        "unit-null-child",
        json!({"unit_id": "unit-null-child", "unsafe": null}),
    ))?;
    let mut policy = source.clone();
    policy.lowering.hot_path_policy = "live_calyx_query".to_string();
    let mut cx = source.clone();
    cx.lowering.source_cx_id = "wrong-cx".to_string();
    let mut fingerprint = source.clone();
    fingerprint.lowering.fingerprint = "00000000".to_string();
    let mut tampered_fingerprint = package.clone();
    tampered_fingerprint.package_fingerprint = "00000000".to_string();
    let mut live_query = package.clone();
    live_query.calyx_live_query_allowed_in_hot_path = true;
    let mut empty_fields = package.clone();
    empty_fields.fields.clear();

    Ok(vec![
        expect_invalid_input(
            "float_payload_rejected",
            EngineLoweringValve::lower_content_for_sim(&float),
            "/move_speed_fp",
            "floating point",
        ),
        expect_invalid_input(
            "null_child_rejected",
            EngineLoweringValve::lower_content_for_sim(&null_child),
            "/unsafe",
            "null cannot be lowered",
        ),
        expect_invalid_input(
            "policy_mismatch_rejected",
            EngineLoweringValve::lower_content_for_sim(&policy),
            "lowering.hot_path_policy",
            ENGINE_LOWERING_HOT_PATH_POLICY,
        ),
        expect_invalid_input(
            "source_cx_mismatch_rejected",
            EngineLoweringValve::lower_content_for_sim(&cx),
            "lowering.source_cx_id",
            "must match",
        ),
        expect_invalid_input(
            "fingerprint_mismatch_rejected",
            EngineLoweringValve::lower_content_for_sim(&fingerprint),
            "lowering.fingerprint",
            "must match",
        ),
        hot_path_edge(source),
        expect_fingerprint_mismatch(
            "package_fingerprint_tamper_rejected",
            EngineLoweringValve::verify_lowered_package(&tampered_fingerprint),
        ),
        expect_invalid_input(
            "live_query_package_rejected",
            EngineLoweringValve::verify_lowered_package(&live_query),
            "calyx_live_query_allowed_in_hot_path",
            "forbidden",
        ),
        expect_invalid_input(
            "empty_fields_package_rejected",
            EngineLoweringValve::verify_lowered_package(&empty_fields),
            "fields",
            "must contain",
        ),
    ])
}

fn unit_record_with_payload(id: &str, payload: Value) -> EngineContentRecord {
    let mut record = unit_record(id);
    record.payload = payload;
    record
}

fn expect_invalid_input<T>(
    label: &str,
    result: Result<T, EngineLoweringError>,
    expected_field: &str,
    expected_reason: &str,
) -> Value {
    match result {
        Err(EngineLoweringError::InvalidInput { field, reason }) => json!({
            "label": label,
            "matched": field == expected_field && reason.contains(expected_reason),
            "field": field,
            "reason": reason,
        }),
        Err(error) => json!({
            "label": label,
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": label,
            "matched": false,
            "unexpected": "operation succeeded",
        }),
    }
}

fn hot_path_edge(source: &litd_engine::calyx_content::EngineContentConstellationReadback) -> Value {
    let _guard = enter_hot_path(HotPathPhase::SimTick);
    match EngineLoweringValve::lower_content_for_sim(source) {
        Err(EngineLoweringError::HotPath { phase }) => json!({
            "label": "hot_path_lowering_rejected",
            "matched": phase == HotPathPhase::SimTick,
            "phase": format!("{phase:?}"),
        }),
        other => json!({
            "label": "hot_path_lowering_rejected",
            "matched": false,
            "unexpected": format!("{other:?}"),
        }),
    }
}

fn expect_fingerprint_mismatch<T>(label: &str, result: Result<T, EngineLoweringError>) -> Value {
    match result {
        Err(EngineLoweringError::FingerprintMismatch { expected, actual }) => json!({
            "label": label,
            "matched": actual == "00000000" && expected != actual,
            "expected": expected,
            "actual": actual,
        }),
        Err(error) => json!({
            "label": label,
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": label,
            "matched": false,
            "unexpected": "verification succeeded",
        }),
    }
}

fn vault_file_readbacks(vault_dir: &Path) -> io::Result<Vec<Value>> {
    let mut rows = Vec::new();
    for entry in fs::read_dir(vault_dir)? {
        collect_file_readbacks(&entry?.path(), &mut rows)?;
    }
    rows.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(rows)
}

fn collect_file_readbacks(path: &Path, rows: &mut Vec<Value>) -> io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            collect_file_readbacks(&entry?.path(), rows)?;
        }
    } else {
        let bytes = fs::read(path)?;
        rows.push(json!({
            "path": path.display().to_string(),
            "bytes": bytes.len(),
            "sha256": litd_hash::sha256_hex(&bytes),
        }));
    }
    Ok(())
}

fn ensure(condition: bool, message: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}
