use std::fs;

use litd_engine::{
    calyx_json_store::read_calyx_json_wal_write,
    calyx_persistence::{
        EngineCalyxDatabase, EngineCalyxDatabaseConfig, EngineCalyxPersistenceError,
        EngineCalyxRecord, EngineCalyxRecordKind,
    },
    fsv_artifacts::begin_issue_run,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1115_calyx_persistence")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let db = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let records = records();
    let mut readbacks = Vec::new();
    for record in &records {
        let write = db.put_record(record)?;
        let read = db.get_record(record.kind, &record.id)?;
        let wal = read_calyx_json_wal_write(&vault_dir, read.seq).map_err(|reason| {
            EngineCalyxPersistenceError::Store {
                op: "physical_wal_readback".to_string(),
                reason,
            }
        })?;
        readbacks.push(json!({
            "kind": record.kind,
            "id": record.id,
            "row_key": read.row_key,
            "column": read.column,
            "bytes": read.bytes,
            "sha256": read.sha256,
            "write_sha256": write.sha256,
            "seq": read.seq,
            "graph_rows": read.graph_rows,
            "payload_match": read.payload == record.payload,
            "provenance_match": read.provenance == record.provenance,
            "write_read_sha_match": read.sha256 == write.sha256,
            "read_wal_sha_match": read.sha256 == wal.sha256,
            "wal": {
                "sha256": wal.sha256,
                "bytes": wal.byte_len,
                "seq": wal.seq,
                "wal_row_count": wal.wal_row_count,
                "graph_row_count": wal.graph_row_count,
                "time_index_row_count": wal.time_index_row_count,
                "graph_keys": wal.graph_keys,
                "graph_key_sha256": wal.graph_key_sha256,
                "segment_path": wal.segment_path,
                "segment_sha256": wal.segment_sha256,
                "frame_sha256": wal.frame_sha256,
                "start_offset": wal.start_offset,
                "end_offset": wal.end_offset,
            },
        }));
    }
    let seqs = readbacks
        .iter()
        .map(|readback| readback["seq"].as_u64().unwrap_or_default())
        .collect::<Vec<_>>();
    let seqs_strictly_increasing = seqs.windows(2).all(|pair| pair[1] > pair[0]);
    let all_payloads_match = readbacks.iter().all(|row| row["payload_match"] == true);
    let all_provenance_matches = readbacks.iter().all(|row| row["provenance_match"] == true);
    let all_write_read_sha_match = readbacks
        .iter()
        .all(|row| row["write_read_sha_match"] == true);
    let all_read_wal_sha_match = readbacks
        .iter()
        .all(|row| row["read_wal_sha_match"] == true);
    let all_graph_rows_present = readbacks
        .iter()
        .all(|row| row["graph_rows"].as_u64().unwrap_or_default() >= 2);
    let report = json!({
        "schema": "litd.engine.issue1115.calyx_persistence.v1",
        "schema_version": 1,
        "issue": 1115,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "vault_dir": vault_dir.display().to_string(),
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON, manifest, decoded physical WAL frames, vault rows, and hashes"
        },
        "happy_path": {
            "record_count": records.len(),
            "vault_dir": vault_dir,
            "readbacks": readbacks,
            "seqs": seqs,
            "first_sha256": readbacks.first().map(|row| row["sha256"].clone()).unwrap_or(Value::Null),
        },
        "acceptance": {
            "record_count": records.len(),
            "all_payloads_match": all_payloads_match,
            "all_provenance_matches": all_provenance_matches,
            "all_write_read_sha_match": all_write_read_sha_match,
            "all_read_wal_sha_match": all_read_wal_sha_match,
            "all_graph_rows_present": all_graph_rows_present,
            "seqs_strictly_increasing": seqs_strictly_increasing,
            "seqs": seqs,
        },
        "edges": {
            "empty_id": empty_id_edge(&db)?,
            "null_payload": null_payload_edge(&db)?,
            "empty_provenance": empty_provenance_edge(&db)?,
            "missing_record": missing_record_edge(&db)?,
            "file_path_as_vault": file_path_as_vault_edge(run.run_dir())?,
        }
    });
    let readback = run.write_report(&report)?;
    assert_acceptance(&readback.json)?;
    println!(
        "ISSUE1115_CALYX_PERSISTENCE_FSV report={} bytes={} sha256={} records={} first_sha={} vault={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        readback.json["acceptance"]["record_count"],
        readback.json["happy_path"]["first_sha256"],
        readback.json["source_of_truth"]["vault_dir"],
    );
    Ok(())
}

fn assert_acceptance(report: &Value) -> Result<(), EngineCalyxPersistenceError> {
    if report["acceptance"]["record_count"] != 6 {
        return Err(EngineCalyxPersistenceError::InvalidRecord {
            field: "fsv.acceptance.record_count".to_string(),
            reason: format!("expected 6, got {}", report["acceptance"]["record_count"]),
        });
    }
    for field in [
        "all_payloads_match",
        "all_provenance_matches",
        "all_write_read_sha_match",
        "all_read_wal_sha_match",
        "all_graph_rows_present",
        "seqs_strictly_increasing",
    ] {
        if report["acceptance"][field] != true {
            return Err(EngineCalyxPersistenceError::InvalidRecord {
                field: format!("fsv.acceptance.{field}"),
                reason: "persisted acceptance verdict was false".to_string(),
            });
        }
    }
    Ok(())
}

fn records() -> Vec<EngineCalyxRecord> {
    [
        (EngineCalyxRecordKind::Project, "issue1115-project"),
        (EngineCalyxRecordKind::World, "issue1115-world"),
        (EngineCalyxRecordKind::Object, "issue1115-object"),
        (EngineCalyxRecordKind::AssetMetadata, "issue1115-asset"),
        (EngineCalyxRecordKind::Save, "issue1115-save"),
        (EngineCalyxRecordKind::Provenance, "issue1115-provenance"),
    ]
    .into_iter()
    .map(|(kind, id)| {
        EngineCalyxRecord::new(
            kind,
            id,
            json!({
                "name": id,
                "kind": kind.as_str(),
                "source": "issue1115_fsv"
            }),
            provenance(),
        )
    })
    .collect()
}

fn empty_id_edge(db: &EngineCalyxDatabase) -> Result<Value, EngineCalyxPersistenceError> {
    expect_invalid_record(
        db.put_record(&EngineCalyxRecord::new(
            EngineCalyxRecordKind::Project,
            "",
            json!({"name": "missing id"}),
            provenance(),
        )),
        "id",
    )
}

fn null_payload_edge(db: &EngineCalyxDatabase) -> Result<Value, EngineCalyxPersistenceError> {
    expect_invalid_record(
        db.put_record(&EngineCalyxRecord::new(
            EngineCalyxRecordKind::World,
            "null-payload",
            Value::Null,
            provenance(),
        )),
        "payload",
    )
}

fn empty_provenance_edge(db: &EngineCalyxDatabase) -> Result<Value, EngineCalyxPersistenceError> {
    expect_invalid_record(
        db.put_record(&EngineCalyxRecord::new(
            EngineCalyxRecordKind::World,
            "missing-provenance",
            json!({"name": "missing provenance"}),
            json!({}),
        )),
        "provenance",
    )
}

fn missing_record_edge(db: &EngineCalyxDatabase) -> Result<Value, EngineCalyxPersistenceError> {
    match db.get_record(EngineCalyxRecordKind::Save, "missing-save") {
        Err(EngineCalyxPersistenceError::MissingRecord { kind, id }) => Ok(json!({
            "after": {
                "error_kind": "missing_record",
                "kind": kind,
                "id": id,
            }
        })),
        Err(error) => Err(error),
        Ok(_) => Err(EngineCalyxPersistenceError::InvalidRecord {
            field: "fsv.edge.missing_record".to_string(),
            reason: "expected missing record error".to_string(),
        }),
    }
}

fn file_path_as_vault_edge(
    run_dir: &std::path::Path,
) -> Result<Value, EngineCalyxPersistenceError> {
    let file_path = run_dir.join("not-a-vault.txt");
    fs::write(&file_path, b"not a vault").map_err(|error| EngineCalyxPersistenceError::Store {
        op: "write_invalid_vault_fixture".to_string(),
        reason: error.to_string(),
    })?;
    match EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&file_path)) {
        Err(EngineCalyxPersistenceError::InvalidConfig { field, reason }) => Ok(json!({
            "after": {
                "error_kind": "invalid_config",
                "field": field,
                "reason": reason,
            }
        })),
        Err(error) => Err(error),
        Ok(_) => Err(EngineCalyxPersistenceError::InvalidConfig {
            field: "fsv.edge.file_path_as_vault".to_string(),
            reason: "expected file path vault rejection".to_string(),
        }),
    }
}

fn expect_invalid_record(
    result: Result<impl Sized, EngineCalyxPersistenceError>,
    expected_field: &str,
) -> Result<Value, EngineCalyxPersistenceError> {
    match result {
        Err(EngineCalyxPersistenceError::InvalidRecord { field, reason }) => {
            if field != expected_field {
                return Err(EngineCalyxPersistenceError::InvalidRecord {
                    field: "fsv.edge".to_string(),
                    reason: format!("expected invalid {expected_field}, got {field}: {reason}"),
                });
            }
            Ok(json!({
                "after": {
                    "error_kind": "invalid_record",
                    "field": field,
                    "reason": reason,
                }
            }))
        }
        Err(error) => Err(error),
        Ok(_) => Err(EngineCalyxPersistenceError::InvalidRecord {
            field: "fsv.edge".to_string(),
            reason: format!("expected invalid {expected_field}"),
        }),
    }
}

fn provenance() -> Value {
    json!({
        "source_issue": 1115,
        "source_path": "crates/litd-engine/examples/issue1115_calyx_persistence_fsv.rs",
        "grounding": "synthetic known input/output"
    })
}
