use super::*;

#[test]
fn issue1358_object_gameplay_lives_in_calyx_aster_fsv() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1358_object_gameplay_calyx", "fsv").unwrap();
    reset_root(&root);
    let happy_root = root.join("happy");
    fs::create_dir_all(&happy_root).unwrap();
    let document_path = happy_root.join("object-gameplay-aster");
    let copy_ledger = happy_root.join("copy-ledger-aster");
    let delta_ledger = happy_root.join("delta-ledger-aster");
    let gameplay_ledger = happy_root.join("gameplay-ledger-aster");
    let happy_before = state(&document_path, &copy_ledger);

    call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": copy_ledger,
            "ledger_id": "issue1358-copy-ledger",
            "request_id": "issue1358-copy",
            "requested_by": "litd-editor-issue1358-test",
            "initial_document": sample_issue919_document(),
            "new_object_id": "hero.issue1358.archmage",
            "rawcode": "H135",
            "base": {
                "base_object_id": "base.hero.archmage",
                "kind": "Hero",
                "display_name": "Archmage",
                "properties": {
                    "stats.hit_points": "725",
                    "stats.mana": "600",
                    "combat.damage": "21-31",
                    "cost.gold": "425",
                    "tech.requires": "keep"
                }
            },
            "overrides": {
                "ability.primary": "blizzard",
                "balance.role": "ranged_control"
            }
        }),
    )
    .unwrap();

    call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": delta_ledger,
            "ledger_id": "issue1358-delta-ledger",
            "request_id": "issue1358-delta",
            "requested_by": "litd-editor-issue1358-test",
            "initial_document": null,
            "object_id": "hero.issue1358.archmage",
            "delta": {
                "set_kind": null,
                "upsert_properties": {
                    "stats.mana": "675",
                    "stats.move_speed": "300"
                },
                "remove_properties": ["balance.role"]
            }
        }),
    )
    .unwrap();

    call_mcp_object_tool(
        UPSERT_GAMEPLAY_TABLES_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": gameplay_ledger,
            "ledger_id": "issue1358-gameplay-ledger",
            "request_id": "issue1358-gameplay",
            "requested_by": "litd-editor-issue1358-test",
            "initial_document": null,
            "tables": [{
                "table_id": "hero_mana_curve",
                "category": "progression",
                "rows": [
                    { "row_id": "level_1", "fields": { "mana": "675", "ability_points": "1" } },
                    { "row_id": "level_2", "fields": { "mana": "735", "ability_points": "1" } }
                ]
            }]
        }),
    )
    .unwrap();

    let document_readback = read_mcp_object_document(&document_path).unwrap();
    let ledger_readback = read_mcp_object_tool_ledger(&gameplay_ledger).unwrap();
    let happy_after = state(&document_path, &gameplay_ledger);
    assert_eq!(document_readback.hero_count, 1);
    assert_eq!(document_readback.gameplay_table_count, 1);
    assert_eq!(
        document_readback.typed_object_rows,
        document_readback.object_count
    );
    assert!(document_readback.constellation_rows >= document_readback.object_count);
    assert!(document_path.is_dir());
    assert!(gameplay_ledger.is_dir());

    let edge_ledger = root.join("edge-ledger-aster");
    let before_duplicate = state(&document_path, &edge_ledger);
    let duplicate = call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": edge_ledger,
            "ledger_id": "issue1358-duplicate",
            "request_id": "issue1358-duplicate",
            "requested_by": "litd-editor-issue1358-test",
            "initial_document": null,
            "new_object_id": "hero.issue1358.archmage",
            "rawcode": "H135",
            "base": {
                "base_object_id": "base.hero.archmage",
                "kind": "Hero",
                "display_name": "Archmage",
                "properties": {
                    "stats.hit_points": "725"
                }
            },
            "overrides": {}
        }),
    )
    .unwrap_err()
    .to_string();
    let after_duplicate = state(&document_path, &edge_ledger);
    println!(
        "issue1358 duplicate before={before_duplicate} after={after_duplicate} error={duplicate}"
    );
    assert_eq!(before_duplicate, after_duplicate);

    let before_missing = state(&document_path, &edge_ledger);
    let missing = call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": edge_ledger,
            "ledger_id": "issue1358-missing",
            "request_id": "issue1358-missing",
            "requested_by": "litd-editor-issue1358-test",
            "initial_document": null,
            "object_id": "unit.issue1358.missing",
            "delta": {
                "set_kind": null,
                "upsert_properties": { "stats.hit_points": "1" },
                "remove_properties": []
            }
        }),
    )
    .unwrap_err()
    .to_string();
    let after_missing = state(&document_path, &edge_ledger);
    println!("issue1358 missing before={before_missing} after={after_missing} error={missing}");
    assert_eq!(before_missing, after_missing);

    let legacy_path = root.join("legacy-object-document.json");
    fs::write(&legacy_path, b"{}").unwrap();
    let before_legacy = json!({
        "legacy_path_exists": legacy_path.exists(),
        "legacy_path_is_file": legacy_path.is_file(),
        "legacy_bytes": legacy_path.metadata().map(|metadata| metadata.len()).unwrap_or(0)
    });
    let legacy = read_mcp_object_document(&legacy_path)
        .unwrap_err()
        .to_string();
    let after_legacy = json!({
        "legacy_path_exists": legacy_path.exists(),
        "legacy_path_is_file": legacy_path.is_file(),
        "legacy_bytes": legacy_path.metadata().map(|metadata| metadata.len()).unwrap_or(0)
    });
    println!("issue1358 legacy before={before_legacy} after={after_legacy} error={legacy}");
    assert_eq!(before_legacy, after_legacy);
    assert!(legacy.contains("Aster vault directory"));

    let report = json!({
        "issue": 1358,
        "source_of_truth": "Calyx Aster Graph CF object/gameplay rows, object tool ledger rows, and Aster Base CF structured constellations",
        "happy": {
            "before": happy_before,
            "after": happy_after,
            "document": document_readback,
            "gameplay_ledger": ledger_readback,
            "physical_files": physical_files(&happy_root),
            "legacy_json_side_store_paths": legacy_json_side_store_paths(&happy_root)
        },
        "edges": {
            "duplicate_object": {
                "before": before_duplicate,
                "after": after_duplicate,
                "error": duplicate
            },
            "missing_object_delta": {
                "before": before_missing,
                "after": after_missing,
                "error": missing
            },
            "legacy_json_file": {
                "before": before_legacy,
                "after": after_legacy,
                "error": legacy
            }
        }
    });
    let report_path = root.join("issue1358-object-gameplay-calyx-readback.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    let report_readback: serde_json::Value =
        serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    assert_eq!(report_readback, report);
    println!("ISSUE1358_FSV_ROOT={}", root.display());
    println!("ISSUE1358_READBACK={}", report_path.display());
    println!(
        "{}",
        serde_json::to_string_pretty(&report_readback).unwrap()
    );
}

/// #818/#829 security property: an out-of-band WC3 enum or targeting flag must be rejected by
/// the object MCP tool BEFORE any Aster write. For every invalid class we prove (a) the copy
/// returns a typed error and (b) an INDEPENDENT fresh-handle vault readback shows the object
/// was never persisted and the whole document is byte-identical before and after the attempt.
#[test]
fn issue818_persist_time_enum_flag_rejection_persists_nothing() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue818_persist_time_enum_flag_reject", "unit").unwrap();
    reset_root(&root);
    let document_path = root.join("object-document-aster");
    let seed_ledger = root.join("seed-ledger-aster");
    let bad_ledger = root.join("bad-ledger-aster");

    // Seed a valid vault so we can prove a REJECTED copy leaves an existing document untouched.
    call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": seed_ledger,
            "ledger_id": "issue818-seed-ledger",
            "request_id": "issue818-seed",
            "requested_by": "litd-editor-issue818-test",
            "initial_document": sample_issue919_document(),
            "new_object_id": "unit.issue818.seed_champion",
            "rawcode": "u818",
            "base": {
                "base_object_id": "base.unit.footman",
                "kind": "Unit",
                "display_name": "Footman",
                "properties": {
                    "stats.hit_points": "420",
                    "combat.attack_type": "Normal",
                    "combat.armor_type": "Heavy",
                    "combat.targets_allowed": "ground,structure",
                    "movement.type": "Foot"
                }
            },
            "overrides": {}
        }),
    )
    .unwrap();

    // (property_key, bad_value, error-substring) for each invalid class the valve must reject.
    let cases: &[(&str, &str, &str)] = &[
        ("combat.attack_type", "Bogus", "is not one of"),
        ("combat.armor_type", "Spaceship", "is not one of"),
        ("movement.type", "Teleport", "is not one of"),
        (
            "combat.targets_allowed",
            "ground,spaceship",
            "is not one of",
        ),
        ("combat.targets_allowed", "ground,ground", "duplicate flag"),
    ];

    for (idx, (key, bad_value, needle)) in cases.iter().enumerate() {
        let bad_object_id = format!("unit.issue818.bad_{idx}");
        let before = state(&document_path, &bad_ledger);
        let before_present = read_mcp_object_document(&document_path)
            .unwrap()
            .document
            .objects
            .contains_key(&bad_object_id);

        let mut overrides = serde_json::Map::new();
        overrides.insert((*key).to_string(), json!(*bad_value));
        let error = call_mcp_object_tool(
            COPY_BASE_OBJECT_TOOL,
            json!({
                "document_path": document_path,
                "ledger_path": bad_ledger,
                "ledger_id": "issue818-bad-ledger",
                "request_id": format!("issue818-bad-{idx}"),
                "requested_by": "litd-editor-issue818-test",
                "initial_document": null,
                "new_object_id": bad_object_id,
                "rawcode": "uBd0",
                "base": {
                    "base_object_id": "base.unit.footman",
                    "kind": "Unit",
                    "display_name": "Footman",
                    "properties": { "stats.hit_points": "420" }
                },
                "overrides": overrides
            }),
        )
        .unwrap_err()
        .to_string();

        // Independent fresh-handle readback: the bad object was NEVER persisted.
        let after_present = read_mcp_object_document(&document_path)
            .unwrap()
            .document
            .objects
            .contains_key(&bad_object_id);
        let after = state(&document_path, &bad_ledger);
        println!(
            "issue818 persist-reject key={key} bad={bad_value} before={before} after={after} error={error}"
        );
        assert!(
            error.contains(needle),
            "expected typed enum/flag error for {key}={bad_value}, got: {error}"
        );
        assert!(!before_present, "bad object should not pre-exist for {key}");
        assert!(
            !after_present,
            "rejected {key}={bad_value} must NOT persist into the Aster vault"
        );
        assert_eq!(
            before, after,
            "a rejected {key}={bad_value} copy must leave the vault byte-identical"
        );
    }
}
