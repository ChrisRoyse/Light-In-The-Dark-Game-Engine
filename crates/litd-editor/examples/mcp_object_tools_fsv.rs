use std::{collections::BTreeMap, fs};

use litd_editor::mcp_object_tools::{
    APPLY_OBJECT_DELTA_TOOL, COPY_BASE_OBJECT_TOOL, READ_OBJECT_DATA_TOOL,
    UPSERT_GAMEPLAY_TABLES_TOOL, call_mcp_object_tool, read_mcp_object_document,
    sample_issue919_document,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue919Fsv {
    issue: u32,
    source_of_truth: Value,
    states: Value,
    happy_path: Value,
    edges: Vec<Value>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_MCP_OBJECT_TOOLS_FSV_ROOT",
        "issue919_mcp_object_tools",
        "manual",
    )?;

    let document_path = root.join("object-document.json");
    let copy_ledger = root.join("copy-ledger.json");
    let delta_ledger = root.join("delta-ledger.json");
    let gameplay_ledger = root.join("gameplay-ledger.json");
    let report_path = root.join("mcp-object-tools-fsv-readback.json");
    let before = state(&document_path, &copy_ledger);

    let copy = call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": copy_ledger,
            "ledger_id": "issue919-copy-ledger",
            "request_id": "issue919-copy",
            "requested_by": "litd-editor-issue919-fsv",
            "initial_document": sample_issue919_document(),
            "new_object_id": "hero.issue919.mountain_king",
            "rawcode": "H019",
            "base": {
                "base_object_id": "base.hero.mountain_king",
                "kind": "Hero",
                "display_name": "Mountain King",
                "properties": {
                    "stats.hit_points": "1450",
                    "stats.mana": "300",
                    "combat.damage": "45-55"
                }
            },
            "overrides": {
                "art.model": "units/human/MountainKing/MountainKing.mdl",
                "balance.role": "frontline_control"
            }
        }),
    )?;
    let delta = call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": delta_ledger,
            "ledger_id": "issue919-delta-ledger",
            "request_id": "issue919-delta",
            "requested_by": "litd-editor-issue919-fsv",
            "initial_document": null,
            "object_id": "hero.issue919.mountain_king",
            "delta": {
                "set_kind": null,
                "upsert_properties": {
                    "stats.hit_points": "1600",
                    "stats.move_speed": "280"
                },
                "remove_properties": ["balance.role"]
            }
        }),
    )?;
    let gameplay = call_mcp_object_tool(
        UPSERT_GAMEPLAY_TABLES_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": gameplay_ledger,
            "ledger_id": "issue919-gameplay-ledger",
            "request_id": "issue919-gameplay",
            "requested_by": "litd-editor-issue919-fsv",
            "initial_document": null,
            "tables": [{
                "table_id": "hero_xp_curve",
                "category": "progression",
                "rows": [
                    { "row_id": "level_1", "fields": { "xp_required": "0", "ability_points": "1" } },
                    { "row_id": "level_2", "fields": { "xp_required": "220", "ability_points": "1" } }
                ]
            }]
        }),
    )?;
    let read = call_mcp_object_tool(
        READ_OBJECT_DATA_TOOL,
        json!({ "document_path": document_path }),
    )?;
    let after = state(&document_path, &copy_ledger);
    let edges = vec![
        edge_duplicate_copy(&root, &document_path)?,
        edge_missing_object(&root, &document_path)?,
        edge_invalid_gameplay_table(&root, &document_path)?,
    ];

    let fsv = Issue919Fsv {
        issue: 919,
        source_of_truth: json!({
            "document": document_path,
            "copy_ledger": copy_ledger,
            "delta_ledger": delta_ledger,
            "gameplay_ledger": gameplay_ledger
        }),
        states: json!({ "before": before, "after": after }),
        happy_path: json!({
            "copy_base_object": copy,
            "apply_object_delta": delta,
            "upsert_gameplay_tables": gameplay,
            "read_object_data": read
        }),
        edges,
    };
    let fsv = canonical_json_roundtrip(&fsv)?;
    write_json(&report_path, &fsv)?;
    let readback = read_json::<Issue919Fsv>(&report_path)?;
    if readback != fsv {
        return Err(format!("report readback mismatch at {}", report_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&report_path)?;
    // document_path and every *_ledger path are Calyx Aster VAULT DIRECTORIES, not files.
    // Hashing them with fs::read would open a directory, which on Windows fails with
    // `Os error 5, PermissionDenied` (#1529). Use the SHA-256 the store computed over the
    // canonical stored bytes — read the document back through the store, and take each
    // ledger hash from its tool call readback.
    let document_sha256 = read_mcp_object_document(&document_path)?.sha256;
    println!(
        "issue919 mcp_object_tools_fsv report={} report_sha256={} document_sha256={} copy_ledger_sha256={} delta_ledger_sha256={} gameplay_ledger_sha256={}",
        report_path.display(),
        report_readback.sha256,
        document_sha256,
        ledger_sha256(&copy, "copy")?,
        ledger_sha256(&delta, "delta")?,
        ledger_sha256(&gameplay, "gameplay")?,
    );
    Ok(())
}

fn edge_duplicate_copy(
    root: &std::path::Path,
    document_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-duplicate-ledger.json");
    let before = state(document_path, &ledger);
    let error = call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": ledger,
            "ledger_id": "issue919-duplicate",
            "request_id": "issue919-duplicate",
            "requested_by": "litd-editor-issue919-fsv",
            "initial_document": null,
            "new_object_id": "hero.issue919.mountain_king",
            "rawcode": "H019",
            "base": {
                "base_object_id": "base.hero.mountain_king",
                "kind": "Hero",
                "display_name": "Mountain King",
                "properties": sample_base_properties()
            },
            "overrides": {}
        }),
    )
    .unwrap_err();
    let after = state(document_path, &ledger);
    Ok(json!({
        "name": "duplicate_copy_target",
        "before": before,
        "after": after,
        "error": error.to_string()
    }))
}

fn edge_missing_object(
    root: &std::path::Path,
    document_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-missing-object-ledger.json");
    let before = state(document_path, &ledger);
    let error = call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": ledger,
            "ledger_id": "issue919-missing-object",
            "request_id": "issue919-missing-object",
            "requested_by": "litd-editor-issue919-fsv",
            "initial_document": null,
            "object_id": "unit.issue919.missing",
            "delta": {
                "set_kind": null,
                "upsert_properties": { "stats.hit_points": "10" },
                "remove_properties": []
            }
        }),
    )
    .unwrap_err();
    let after = state(document_path, &ledger);
    Ok(json!({
        "name": "missing_delta_target",
        "before": before,
        "after": after,
        "error": error.to_string()
    }))
}

fn edge_invalid_gameplay_table(
    root: &std::path::Path,
    document_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-invalid-gameplay-ledger.json");
    let before = state(document_path, &ledger);
    let error = call_mcp_object_tool(
        UPSERT_GAMEPLAY_TABLES_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": ledger,
            "ledger_id": "issue919-invalid-gameplay",
            "request_id": "issue919-invalid-gameplay",
            "requested_by": "litd-editor-issue919-fsv",
            "initial_document": null,
            "tables": [{
                "table_id": "bad.table",
                "category": "progression",
                "rows": []
            }]
        }),
    )
    .unwrap_err();
    let after = state(document_path, &ledger);
    Ok(json!({
        "name": "invalid_gameplay_table",
        "before": before,
        "after": after,
        "error": error.to_string()
    }))
}

fn sample_base_properties() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("stats.hit_points".to_string(), "1450".to_string()),
        ("stats.mana".to_string(), "300".to_string()),
        ("combat.damage".to_string(), "45-55".to_string()),
    ])
}

fn state(document_path: &std::path::Path, ledger_path: &std::path::Path) -> Value {
    let readback = if document_path.exists() {
        Some(read_mcp_object_document(document_path).unwrap())
    } else {
        None
    };
    json!({
        "document_exists": document_path.exists(),
        "document_bytes": fs::metadata(document_path).map(|metadata| metadata.len()).unwrap_or(0),
        "object_count": readback.as_ref().map(|readback| readback.object_count).unwrap_or(0),
        "unit_count": readback.as_ref().map(|readback| readback.unit_count).unwrap_or(0),
        "hero_count": readback.as_ref().map(|readback| readback.hero_count).unwrap_or(0),
        "ability_count": readback.as_ref().map(|readback| readback.ability_count).unwrap_or(0),
        "gameplay_table_count": readback.as_ref().map(|readback| readback.gameplay_table_count).unwrap_or(0),
        "ledger_exists": ledger_path.exists(),
        "ledger_bytes": fs::metadata(ledger_path).map(|metadata| metadata.len()).unwrap_or(0)
    })
}

fn write_json<T: Serialize>(
    path: &std::path::Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(
    path: &std::path::Path,
) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn canonical_json_roundtrip<T>(value: &T) -> Result<T, Box<dyn std::error::Error>>
where
    T: Serialize + DeserializeOwned,
{
    Ok(serde_json::from_slice(&serde_json::to_vec_pretty(value)?)?)
}

/// Pull the store-computed ledger SHA-256 out of a tool call output. The ledger, like the
/// object document, lives in a Calyx Aster vault directory, so the canonical hash is the
/// one the store returned over the stored bytes — never `fs::read` of the vault path.
/// Fails loud (naming which tool) if a mutating call returned no ledger readback.
fn ledger_sha256(
    output: &litd_editor::mcp_object_tools::McpObjectToolCallOutput,
    label: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(output
        .ledger_readback
        .as_ref()
        .ok_or_else(|| format!("{label} tool call returned no ledger readback"))?
        .sha256
        .clone())
}
