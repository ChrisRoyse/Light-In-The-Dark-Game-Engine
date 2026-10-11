use std::{fs, path::Path};

use calyx_aster::{
    cf::ColumnFamily,
    vault::{AsterVault, VaultOptions},
};
use calyx_core::{SystemClock, VaultId};
use calyx_loom::{ReactiveRowKind, decode_audit_entry, decode_trigger_fired, reactive_row_key};
use litd_editor::mcp_object_tools::{
    APPLY_OBJECT_DELTA_TOOL, COPY_BASE_OBJECT_TOOL, UPSERT_GAMEPLAY_TABLES_TOOL,
    call_mcp_object_tool, read_mcp_object_document, sample_issue919_document,
};
use serde_json::{Value, json};

const OBJECT_STORE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAY";
const OBJECT_STORE_VAULT_PURPOSE: &[u8] = b"litd-editor-object-tools-issue1358";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (_fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE1390_REACTIVE_FSV_ROOT",
        "issue1390_editor_reactive_notifications",
        "manual",
    )?;
    reset_root(&root)?;

    let document_path = root.join("object-document-aster");
    let ledger_path = root.join("object-ledger-aster");
    let before = aster_state(&document_path)?;

    let output = call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": ledger_path,
            "ledger_id": "issue1390-copy-ledger",
            "request_id": "issue1390-copy",
            "requested_by": "litd-editor-issue1390-fsv",
            "initial_document": sample_issue919_document(),
            "new_object_id": "hero.issue1390.blood_mage",
            "rawcode": "H390",
            "base": {
                "base_object_id": "base.hero.blood_mage",
                "kind": "Hero",
                "display_name": "Blood Mage",
                "properties": {
                    "stats.hit_points": "575",
                    "stats.mana": "690",
                    "combat.damage": "21-27",
                    "cost.gold": "425",
                    "tech.requires": "keep",
                    "ability.primary": "flame_strike"
                }
            },
            "overrides": {
                "balance.role": "caster_area_denial",
                "stats.move_speed": "290"
            }
        }),
    )?;
    let tool_readback = output.document_readback.clone().expect("document readback");
    let manual_readback = read_mcp_object_document(&document_path)?;
    let after = aster_state(&document_path)?;

    assert_eq!(before["vault_exists"], false);
    assert_eq!(manual_readback.object_count, 2);
    assert_eq!(manual_readback.structured_ingest_cx_ids.len(), 2);
    assert!(manual_readback.structured_ingest_ledger_seq.is_some());
    assert_eq!(manual_readback.reactive_audit_rows, 2);
    assert_eq!(manual_readback.reactive_fired_rows, 1);
    assert_eq!(manual_readback.reactive_notifications.len(), 1);
    assert_eq!(tool_readback.document, manual_readback.document);
    assert_eq!(
        tool_readback.reactive_notifications,
        manual_readback.reactive_notifications
    );
    assert_eq!(after["base_rows"], manual_readback.constellation_rows);
    assert_eq!(
        after["reactive_audit_rows"],
        manual_readback.reactive_audit_rows
    );
    assert_eq!(
        after["reactive_fired_rows"],
        manual_readback.reactive_fired_rows
    );
    assert_eq!(
        after["reactive_notifications"][0]["ledger_seq"].as_u64(),
        manual_readback.structured_ingest_ledger_seq
    );

    let duplicate_edge = edge_duplicate_copy(&document_path, &root)?;
    let missing_edge = edge_missing_delta(&document_path, &root)?;
    let bad_table_edge = edge_bad_table(&document_path, &root)?;
    let file_edge = edge_file_path(&root)?;

    let report = json!({
        "issue": 1390,
        "source_of_truth": "Calyx Aster Base/Graph/Reactive/Ledger CF rows read after editor object-tool structured ingest",
        "happy": {
            "before": before,
            "after": after,
            "tool_readback": tool_readback,
            "manual_readback": manual_readback,
            "physical_files": physical_files(&root)?
        },
        "edges": [
            duplicate_edge,
            missing_edge,
            bad_table_edge,
            file_edge
        ]
    });
    let report_path = root.join("issue1390-editor-reactive-notifications-readback.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    let report_readback: Value = serde_json::from_slice(&fs::read(&report_path)?)?;
    assert_eq!(report_readback, report);

    println!("ISSUE1390_FSV_ROOT={}", root.display());
    println!("ISSUE1390_READBACK={}", report_path.display());
    println!("{}", serde_json::to_string_pretty(&report_readback)?);
    Ok(())
}

fn edge_duplicate_copy(
    document_path: &Path,
    root: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger_path = root.join("edge-duplicate-ledger-aster");
    let before = aster_state(document_path)?;
    let error = call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": ledger_path,
            "ledger_id": "issue1390-duplicate",
            "request_id": "issue1390-duplicate",
            "requested_by": "litd-editor-issue1390-fsv",
            "initial_document": null,
            "new_object_id": "hero.issue1390.blood_mage",
            "rawcode": "H390",
            "base": {
                "base_object_id": "base.hero.blood_mage",
                "kind": "Hero",
                "display_name": "Blood Mage",
                "properties": { "stats.hit_points": "575" }
            },
            "overrides": {}
        }),
    )
    .unwrap_err()
    .to_string();
    let after = aster_state(document_path)?;
    assert_eq!(before, after);
    assert!(error.contains("already exists"));
    Ok(json!({
        "name": "duplicate_object",
        "before": before,
        "after": after,
        "error": error
    }))
}

fn edge_missing_delta(
    document_path: &Path,
    root: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger_path = root.join("edge-missing-ledger-aster");
    let before = aster_state(document_path)?;
    let error = call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": ledger_path,
            "ledger_id": "issue1390-missing",
            "request_id": "issue1390-missing",
            "requested_by": "litd-editor-issue1390-fsv",
            "initial_document": null,
            "object_id": "unit.issue1390.missing",
            "delta": {
                "set_kind": null,
                "upsert_properties": { "stats.hit_points": "1" },
                "remove_properties": []
            }
        }),
    )
    .unwrap_err()
    .to_string();
    let after = aster_state(document_path)?;
    assert_eq!(before, after);
    assert!(error.contains("does not exist"));
    Ok(json!({
        "name": "missing_object_delta",
        "before": before,
        "after": after,
        "error": error
    }))
}

fn edge_bad_table(document_path: &Path, root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger_path = root.join("edge-bad-table-ledger-aster");
    let before = aster_state(document_path)?;
    let error = call_mcp_object_tool(
        UPSERT_GAMEPLAY_TABLES_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": ledger_path,
            "ledger_id": "issue1390-bad-table",
            "request_id": "issue1390-bad-table",
            "requested_by": "litd-editor-issue1390-fsv",
            "initial_document": null,
            "tables": [{
                "table_id": "bad.table",
                "category": "progression",
                "rows": []
            }]
        }),
    )
    .unwrap_err()
    .to_string();
    let after = aster_state(document_path)?;
    assert_eq!(before, after);
    assert!(error.contains("key components"));
    Ok(json!({
        "name": "bad_gameplay_table",
        "before": before,
        "after": after,
        "error": error
    }))
}

fn edge_file_path(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let path = root.join("edge-file-path.json");
    fs::write(&path, b"{\"not\":\"a-vault\"}")?;
    let before = file_state(&path);
    let error = read_mcp_object_document(&path).unwrap_err().to_string();
    let after = file_state(&path);
    assert_eq!(before, after);
    assert!(error.contains("Aster vault directory"));
    Ok(json!({
        "name": "file_path",
        "before": before,
        "after": after,
        "error": error
    }))
}

fn aster_state(document_path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    if !document_path.is_dir() {
        return Ok(json!({
            "vault_exists": false,
            "base_rows": 0,
            "graph_rows": 0,
            "reactive_audit_rows": 0,
            "reactive_fired_rows": 0,
            "ledger_rows": 0,
            "reactive_notifications": []
        }));
    }
    let vault = AsterVault::<SystemClock>::open(
        document_path,
        object_store_vault_id(),
        OBJECT_STORE_VAULT_PURPOSE.to_vec(),
        VaultOptions::default(),
    )?;
    let snapshot = vault.latest_seq();
    let base_rows = vault.scan_cf_at(snapshot, ColumnFamily::Base)?.len();
    let graph_rows = vault.scan_cf_at(snapshot, ColumnFamily::Graph)?.len();
    let ledger_rows = vault.scan_cf_at(snapshot, ColumnFamily::Ledger)?.len();
    let mut reactive_audit_rows = 0usize;
    let mut reactive_notifications = Vec::new();
    for (key, value) in vault.scan_cf_at(snapshot, ColumnFamily::Reactive)? {
        match reactive_row_key(&key)?.kind {
            ReactiveRowKind::Audit => {
                decode_audit_entry(&value)?;
                reactive_audit_rows += 1;
            }
            ReactiveRowKind::Fired => {
                let event = decode_trigger_fired(&value)?;
                reactive_notifications.push(json!({
                    "trigger_id": event.trigger_id.to_string(),
                    "cx_id": event.cx_id.to_string(),
                    "fired_at": event.fired_at,
                    "ledger_seq": event.ledger_ref.seq,
                    "ledger_hash": hex(&event.ledger_ref.hash),
                    "condition": serde_json::to_value(&event.condition_snapshot)?
                }));
            }
        }
    }
    reactive_notifications.sort_by_key(|value| {
        format!(
            "{}:{}:{}",
            value["ledger_seq"], value["trigger_id"], value["cx_id"]
        )
    });
    Ok(json!({
        "vault_exists": true,
        "snapshot": snapshot,
        "base_rows": base_rows,
        "graph_rows": graph_rows,
        "reactive_audit_rows": reactive_audit_rows,
        "reactive_fired_rows": reactive_notifications.len(),
        "ledger_rows": ledger_rows,
        "reactive_notifications": reactive_notifications
    }))
}

fn physical_files(root: &Path) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    Ok(files)
}

fn collect_files(
    root: &Path,
    dir: &Path,
    files: &mut Vec<Value>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut entries = fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else {
            files.push(json!({
                "path": path.strip_prefix(root)?.display().to_string(),
                "bytes": path.metadata()?.len()
            }));
        }
    }
    Ok(())
}

fn file_state(path: &Path) -> Value {
    json!({
        "exists": path.exists(),
        "is_file": path.is_file(),
        "bytes": path.metadata().map(|metadata| metadata.len()).unwrap_or(0)
    })
}

fn reset_root(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    fs::create_dir_all(root)?;
    Ok(())
}

fn object_store_vault_id() -> VaultId {
    OBJECT_STORE_VAULT_ID
        .parse()
        .expect("valid object vault id")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
