use std::{collections::BTreeMap, fs, path::Path};

use serde_json::json;

use super::*;

#[path = "mcp_object_tools_tests_part2.rs"]
mod part2;

#[test]
fn issue919_object_mcp_tools_build_edit_tables_and_read_real_document() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue919_mcp_object_tools", "unit").unwrap();
    reset_root(&root);
    let document_path = root.join("object-document-aster");
    let copy_ledger = root.join("copy-ledger-aster");
    let delta_ledger = root.join("delta-ledger-aster");
    let gameplay_ledger = root.join("gameplay-ledger-aster");
    let before = state(&document_path, &copy_ledger);
    println!("issue919 before {before}");

    let copy = call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": copy_ledger,
            "ledger_id": "issue919-copy-ledger",
            "request_id": "issue919-copy",
            "requested_by": "litd-editor-issue919-test",
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
    )
    .unwrap();
    assert_eq!(copy.tool_name, COPY_BASE_OBJECT_TOOL);
    let copied = read_mcp_object_document(&document_path).unwrap();
    let hero = &copied.document.objects["hero.issue919.mountain_king"];
    assert_eq!(hero.kind, "Hero");
    assert_eq!(hero.properties["object.rawcode"], "H019");
    assert_eq!(hero.properties["stats.hit_points"], "1450");

    let delta = call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": delta_ledger,
            "ledger_id": "issue919-delta-ledger",
            "request_id": "issue919-delta",
            "requested_by": "litd-editor-issue919-test",
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
    )
    .unwrap();
    assert_eq!(delta.tool_name, APPLY_OBJECT_DELTA_TOOL);
    let edited = read_mcp_object_document(&document_path).unwrap();
    let hero = &edited.document.objects["hero.issue919.mountain_king"];
    assert_eq!(hero.properties["stats.hit_points"], "1600");
    assert_eq!(hero.properties["stats.move_speed"], "280");
    assert!(!hero.properties.contains_key("balance.role"));

    let gameplay = call_mcp_object_tool(
        UPSERT_GAMEPLAY_TABLES_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": gameplay_ledger,
            "ledger_id": "issue919-gameplay-ledger",
            "request_id": "issue919-gameplay",
            "requested_by": "litd-editor-issue919-test",
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
    )
    .unwrap();
    assert_eq!(gameplay.tool_name, UPSERT_GAMEPLAY_TABLES_TOOL);
    let read = call_mcp_object_tool(
        READ_OBJECT_DATA_TOOL,
        json!({ "document_path": document_path }),
    )
    .unwrap();
    assert_eq!(read.result["document_readback"]["hero_count"], 1);
    assert_eq!(read.result["document_readback"]["gameplay_table_count"], 1);
    let after = state(&document_path, &copy_ledger);
    println!("issue919 after {after}");
    assert_eq!(after["document_exists"], true);
    assert_eq!(after["object_count"], 3);
    assert_eq!(after["hero_count"], 1);
    assert_eq!(after["gameplay_table_count"], 1);
    assert_eq!(after["ledger_exists"], true);
    assert_eq!(after["typed_object_rows"], 3);
    assert_eq!(after["constellation_rows"], 4);
}

#[test]
fn issue919_object_mcp_tools_edges_fail_closed_without_writing() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue919_mcp_object_tools", "edges").unwrap();
    reset_root(&root);
    let document_path = root.join("object-document-aster");
    seed_document(&document_path, &root);
    let edge_ledger = root.join("edge-ledger-aster");

    let before_duplicate = state(&document_path, &edge_ledger);
    let duplicate = call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": edge_ledger,
            "ledger_id": "issue919-duplicate",
            "request_id": "issue919-duplicate",
            "requested_by": "litd-editor-issue919-test",
            "initial_document": null,
            "new_object_id": "ability.issue919.storm_bolt",
            "rawcode": "A019",
            "base": sample_ability_base_json(),
            "overrides": {}
        }),
    )
    .unwrap_err();
    let after_duplicate = state(&document_path, &edge_ledger);
    println!(
        "issue919 duplicate before={before_duplicate} after={after_duplicate} error={duplicate}"
    );
    assert_eq!(before_duplicate, after_duplicate);
    assert!(duplicate.to_string().contains("already exists"));

    let before_missing = state(&document_path, &edge_ledger);
    let missing = call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": edge_ledger,
            "ledger_id": "issue919-missing-object",
            "request_id": "issue919-missing-object",
            "requested_by": "litd-editor-issue919-test",
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
    let after_missing = state(&document_path, &edge_ledger);
    println!("issue919 missing before={before_missing} after={after_missing} error={missing}");
    assert_eq!(before_missing, after_missing);
    assert!(missing.to_string().contains("does not exist"));

    let before_bad_table = state(&document_path, &edge_ledger);
    let bad_table = call_mcp_object_tool(
        UPSERT_GAMEPLAY_TABLES_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": edge_ledger,
            "ledger_id": "issue919-bad-table",
            "request_id": "issue919-bad-table",
            "requested_by": "litd-editor-issue919-test",
            "initial_document": null,
            "tables": [{
                "table_id": "bad.table",
                "category": "progression",
                "rows": []
            }]
        }),
    )
    .unwrap_err();
    let after_bad_table = state(&document_path, &edge_ledger);
    println!(
        "issue919 bad_table before={before_bad_table} after={after_bad_table} error={bad_table}"
    );
    assert_eq!(before_bad_table, after_bad_table);
    assert!(bad_table.to_string().contains("key components"));
}

#[test]
fn issue1518_object_tool_rejects_out_of_root_document_path_without_writing() {
    // #1518 confinement close gate: a poisoned document_path outside the configured
    // project root (both absolute-escape and ..-traversal) is rejected at
    // validate_mutation_paths BEFORE any filesystem effect, and the fs is left untouched.
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1518_mcp_object_tools", "confine").unwrap();
    reset_root(&root);
    let ledger_path = root.join("issue1518-ledger-aster");

    // --- EDGE 1: absolute document_path OUTSIDE the workspace root ---
    let _temp_root = litd_engine::test_temp::TestTempDir::new("issue1518-escape");
    let escape_dir = _temp_root.path().to_path_buf();
    if escape_dir.exists() {
        fs::remove_dir_all(&escape_dir).unwrap();
    }
    let escape_doc = escape_dir.join("object-document-aster");
    let escape_error = call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": escape_doc,
            "ledger_path": ledger_path,
            "ledger_id": "issue1518-escape",
            "request_id": "issue1518-escape",
            "requested_by": "litd-editor-issue1518-test",
            "initial_document": null,
            "new_object_id": "ability.issue1518.escape",
            "rawcode": "A518",
            "base": sample_ability_base_json(),
            "overrides": {}
        }),
    )
    .unwrap_err()
    .to_string();
    println!("issue1518 absolute-escape rejected: {escape_error}");
    assert!(
        escape_error.contains("outside the project root"),
        "expected outside-root rejection, got: {escape_error}"
    );
    // Source of truth: NOTHING was written outside the root.
    assert!(
        !escape_dir.exists(),
        "confinement breach: object tool created {} outside the project root",
        escape_dir.display()
    );

    // --- EDGE 2: ..-traversal document_path (literal relative string so the `..`
    // components actually reach confine_path). ---
    let traversal_error = call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": "issue1518-doc/../../litd-issue1518-object-traversal",
            "ledger_path": ledger_path,
            "ledger_id": "issue1518-traversal",
            "request_id": "issue1518-traversal",
            "requested_by": "litd-editor-issue1518-test",
            "initial_document": null,
            "new_object_id": "ability.issue1518.traversal",
            "rawcode": "A519",
            "base": sample_ability_base_json(),
            "overrides": {}
        }),
    )
    .unwrap_err()
    .to_string();
    println!("issue1518 traversal rejected: {traversal_error}");
    assert!(
        traversal_error.contains("parent-directory"),
        "expected traversal rejection, got: {traversal_error}"
    );
    assert!(
        !std::path::Path::new("../litd-issue1518-object-traversal").exists(),
        "confinement breach: traversal created a directory outside the workspace"
    );
}

fn seed_document(document_path: &Path, root: &Path) {
    call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": root.join("seed-ledger-aster"),
            "ledger_id": "issue919-seed",
            "request_id": "issue919-seed",
            "requested_by": "litd-editor-issue919-test",
            "initial_document": sample_issue919_document(),
            "new_object_id": "ability.issue919.storm_bolt",
            "rawcode": "A019",
            "base": sample_ability_base_json(),
            "overrides": { "balance.cooldown_seconds": "9" }
        }),
    )
    .unwrap();
}

fn sample_ability_base_json() -> serde_json::Value {
    json!({
        "base_object_id": "base.ability.storm_bolt",
        "kind": "Ability",
        "display_name": "Storm Bolt",
        "properties": sample_ability_properties()
    })
}

fn sample_ability_properties() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("ability.mana_cost".to_string(), "75".to_string()),
        // #819 constrains ability.targeting to the canonical set (self/unit/point/none); use a
        // valid member so persist-time validation accepts this fixture's storm-bolt base.
        ("ability.targeting".to_string(), "unit".to_string()),
        ("effect.stun_seconds".to_string(), "2.0".to_string()),
    ])
}

fn state(document_path: &Path, ledger_path: &Path) -> serde_json::Value {
    let readback = if document_path.exists() {
        read_mcp_object_document(document_path).ok()
    } else {
        None
    };
    let ledger_readback = if ledger_path.exists() {
        read_mcp_object_tool_ledger(ledger_path).ok()
    } else {
        None
    };
    json!({
        "document_exists": readback.is_some(),
        "document_path_is_dir": document_path.is_dir(),
        "document_bytes": readback.as_ref().map(|readback| readback.bytes).unwrap_or(0),
        "graph_rows": readback.as_ref().map(|readback| readback.graph_rows).unwrap_or(0),
        "typed_object_rows": readback.as_ref().map(|readback| readback.typed_object_rows).unwrap_or(0),
        "constellation_rows": readback.as_ref().map(|readback| readback.constellation_rows).unwrap_or(0),
        "object_count": readback.as_ref().map(|readback| readback.object_count).unwrap_or(0),
        "unit_count": readback.as_ref().map(|readback| readback.unit_count).unwrap_or(0),
        "hero_count": readback.as_ref().map(|readback| readback.hero_count).unwrap_or(0),
        "ability_count": readback.as_ref().map(|readback| readback.ability_count).unwrap_or(0),
        "gameplay_table_count": readback.as_ref().map(|readback| readback.gameplay_table_count).unwrap_or(0),
        "structured_ingest_ledger_seq": readback.as_ref().and_then(|readback| readback.structured_ingest_ledger_seq),
        "reactive_audit_rows": readback.as_ref().map(|readback| readback.reactive_audit_rows).unwrap_or(0),
        "reactive_fired_rows": readback.as_ref().map(|readback| readback.reactive_fired_rows).unwrap_or(0),
        "ledger_exists": ledger_readback.is_some(),
        "ledger_path_is_dir": ledger_path.is_dir(),
        "ledger_bytes": ledger_readback.as_ref().map(|readback| readback.bytes).unwrap_or(0),
        "ledger_graph_rows": ledger_readback.as_ref().map(|readback| readback.graph_rows).unwrap_or(0)
    })
}

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}

fn physical_files(root: &Path) -> Vec<serde_json::Value> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files);
    files
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<serde_json::Value>) {
    let mut entries = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_files(root, &path, files);
        } else {
            files.push(json!({
                "path": path.strip_prefix(root).unwrap().display().to_string(),
                "bytes": path.metadata().map(|metadata| metadata.len()).unwrap_or(0)
            }));
        }
    }
}

fn legacy_json_side_store_paths(root: &Path) -> Vec<String> {
    physical_files(root)
        .into_iter()
        .filter_map(|value| {
            value
                .get("path")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .filter(|path| path.ends_with("object-document.json") || path.ends_with("ledger.json"))
        .collect()
}
