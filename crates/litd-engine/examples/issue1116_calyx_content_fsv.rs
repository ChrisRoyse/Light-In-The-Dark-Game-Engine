use std::fs;

use calyx_core::{Anchor, AnchorKind, AnchorValue};
use litd_engine::{
    calyx_content::{
        EngineCalyxContentError, EngineCalyxContentStore, EngineContentKind, EngineContentRecord,
        EngineContentSource,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1116_calyx_content")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database);
    let records = records();
    let mut readbacks = Vec::new();
    for record in &records {
        let write = store.put_content(record)?;
        let editor = store.get_content_for_editor(record.kind, &record.id)?;
        let engine = store.lowered_content_for_engine(record.kind, &record.id)?;
        readbacks.push(json!({
            "kind": record.kind,
            "id": record.id,
            "cx_id": editor.cx_id,
            "database_seq": editor.database.seq,
            "database_sha256": editor.database.sha256,
            "write_database_sha256": write.database.sha256,
            "row_key": editor.database.row_key,
            "graph_rows": editor.database.graph_rows,
            "payload_match": editor.payload == record.payload,
            "modality": editor.constellation.modality,
            "anchor_count": editor.constellation.anchors.len(),
            "lowering_source_cx_matches": engine.source_cx_id == editor.cx_id,
            "lowering_fingerprint_matches": engine.fingerprint == editor.input_sha256,
            "lowering_source_kind": engine.source_kind,
            "lowering_source_id": engine.source_id,
            "hot_path_policy": engine.hot_path_policy,
            "write_read_sha_match": editor.database.sha256 == write.database.sha256,
        }));
    }
    let seqs = readbacks
        .iter()
        .map(|readback| readback["database_seq"].as_u64().unwrap_or_default())
        .collect::<Vec<_>>();
    let vault_files = vault_file_readbacks(&vault_dir)?;
    let report = json!({
        "schema": "litd.engine.issue1116.calyx_content.v1",
        "schema_version": 1,
        "issue": 1116,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "vault_dir": vault_dir.display().to_string(),
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON, manifest, vault rows, and hashes"
        },
        "happy_path": {
            "record_count": records.len(),
            "readbacks": readbacks,
            "seqs": seqs,
            "first_cx_id": readbacks.first().map(|row| row["cx_id"].clone()).unwrap_or(Value::Null),
            "vault_files": vault_files,
        },
        "acceptance": {
            "record_count": records.len(),
            "all_payloads_match": readbacks.iter().all(|row| row["payload_match"] == true),
            "all_structured_modality": readbacks.iter().all(|row| row["modality"] == "structured"),
            "all_have_anchor": readbacks.iter().all(|row| row["anchor_count"].as_u64().unwrap_or_default() == 1),
            "all_lowering_source_cx_match": readbacks.iter().all(|row| row["lowering_source_cx_matches"] == true),
            "all_lowering_fingerprint_match": readbacks.iter().all(|row| row["lowering_fingerprint_matches"] == true),
            "all_hot_path_policy_frozen": readbacks.iter().all(|row| row["hot_path_policy"] == "frozen_fingerprinted_data_only"),
            "all_write_read_sha_match": readbacks.iter().all(|row| row["write_read_sha_match"] == true),
            "all_graph_rows_present": readbacks.iter().all(|row| row["graph_rows"].as_u64().unwrap_or_default() >= 2),
            "seqs": seqs,
        },
        "edges": {
            "empty_id": empty_id_edge(&store)?,
            "empty_display_name": empty_display_name_edge(&store)?,
            "null_payload": null_payload_edge(&store)?,
            "empty_source_path": empty_source_path_edge(&store)?,
            "missing_content": missing_content_edge(&store)?,
        }
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1116_CALYX_CONTENT_FSV report={} bytes={} sha256={} records={} first_cx={} vault={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["acceptance"]["record_count"],
        readback.json["happy_path"]["first_cx_id"],
        readback.json["source_of_truth"]["vault_dir"],
    );
    Ok(())
}

fn records() -> Vec<EngineContentRecord> {
    [
        (EngineContentKind::Unit, "unit-footman"),
        (EngineContentKind::Ability, "ability-firebolt"),
        (EngineContentKind::Item, "item-iron-sword"),
        (EngineContentKind::Trigger, "trigger-enter-town"),
        (EngineContentKind::Terrain, "terrain-ashen-road"),
        (EngineContentKind::Story, "story-opening"),
        (EngineContentKind::Lore, "lore-dawnward"),
        (EngineContentKind::Asset, "asset-knight-mesh"),
        (EngineContentKind::Agent, "agent-town-warden"),
    ]
    .into_iter()
    .map(|(kind, id)| content(kind, id))
    .collect()
}

fn content(kind: EngineContentKind, id: &str) -> EngineContentRecord {
    EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: id.replace('-', " "),
        payload: json!({
            "id": id,
            "kind": kind.as_str(),
            "known_value": 4,
            "expected_value": 4
        }),
        source: EngineContentSource {
            path: format!("synthetic://issue1116/{id}"),
            author: "issue1116-fsv".to_string(),
            source_hash: litd_hash::sha256_hex(format!("issue1116:{id}").as_bytes()),
        },
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue1116_known_io".to_string(),
            observed_at: 1_786_000_000,
            confidence: 1.0,
        }],
    }
}

fn empty_id_edge(store: &EngineCalyxContentStore) -> Result<Value, EngineCalyxContentError> {
    let mut record = content(EngineContentKind::Unit, "unit-valid");
    record.id.clear();
    expect_invalid_content(store.put_content(&record), "id")
}

fn empty_display_name_edge(
    store: &EngineCalyxContentStore,
) -> Result<Value, EngineCalyxContentError> {
    let mut record = content(EngineContentKind::Story, "story-valid");
    record.display_name.clear();
    expect_invalid_content(store.put_content(&record), "display_name")
}

fn null_payload_edge(store: &EngineCalyxContentStore) -> Result<Value, EngineCalyxContentError> {
    let mut record = content(EngineContentKind::Item, "item-valid");
    record.payload = Value::Null;
    expect_invalid_content(store.put_content(&record), "payload")
}

fn empty_source_path_edge(
    store: &EngineCalyxContentStore,
) -> Result<Value, EngineCalyxContentError> {
    let mut record = content(EngineContentKind::Story, "story-valid");
    record.source.path.clear();
    expect_invalid_content(store.put_content(&record), "source.path")
}

fn missing_content_edge(store: &EngineCalyxContentStore) -> Result<Value, EngineCalyxContentError> {
    match store.get_content_for_editor(EngineContentKind::Asset, "missing-asset") {
        Err(EngineCalyxContentError::Persistence(error)) => Ok(json!({
            "after": {
                "error_kind": "persistence",
                "message": error.to_string(),
            }
        })),
        Err(error) => Err(error),
        Ok(_) => Err(EngineCalyxContentError::InvalidContent {
            field: "fsv.edge.missing_content".to_string(),
            reason: "expected missing content persistence error".to_string(),
        }),
    }
}

fn expect_invalid_content(
    result: Result<impl Sized, EngineCalyxContentError>,
    expected_field: &str,
) -> Result<Value, EngineCalyxContentError> {
    match result {
        Err(EngineCalyxContentError::InvalidContent { field, reason }) => {
            if field != expected_field {
                return Err(EngineCalyxContentError::InvalidContent {
                    field: "fsv.edge".to_string(),
                    reason: format!("expected invalid {expected_field}, got {field}: {reason}"),
                });
            }
            Ok(json!({
                "after": {
                    "error_kind": "invalid_content",
                    "field": field,
                    "reason": reason,
                }
            }))
        }
        Err(error) => Err(error),
        Ok(_) => Err(EngineCalyxContentError::InvalidContent {
            field: "fsv.edge".to_string(),
            reason: format!("expected invalid {expected_field}"),
        }),
    }
}

fn vault_file_readbacks(
    vault_dir: &std::path::Path,
) -> Result<Vec<Value>, EngineCalyxContentError> {
    let mut rows = Vec::new();
    for entry in fs::read_dir(vault_dir).map_err(content_store_failed)? {
        collect_file_readbacks(&entry.map_err(content_store_failed)?.path(), &mut rows)?;
    }
    rows.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(rows)
}

fn collect_file_readbacks(
    path: &std::path::Path,
    rows: &mut Vec<Value>,
) -> Result<(), EngineCalyxContentError> {
    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(content_store_failed)? {
            collect_file_readbacks(&entry.map_err(content_store_failed)?.path(), rows)?;
        }
    } else {
        let bytes = fs::read(path).map_err(content_store_failed)?;
        rows.push(json!({
            "path": path.display().to_string(),
            "bytes": bytes.len(),
            "sha256": litd_hash::sha256_hex(&bytes),
        }));
    }
    Ok(())
}

fn content_store_failed(error: impl std::fmt::Display) -> EngineCalyxContentError {
    EngineCalyxContentError::Codec(error.to_string())
}
