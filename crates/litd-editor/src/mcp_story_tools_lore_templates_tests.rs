use std::{fs, path::Path};

use serde_json::{Value, json};

use crate::mcp_story_tools::{
    GENERATE_LORE_TEMPLATES_TOOL, UPSERT_LORE_ENTRY_TOOL, call_mcp_story_tool,
    read_lore_template_ledger, read_mcp_story_state,
};

#[test]
fn issue1054_lore_templates_generate_original_starting_points() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1054_lore_templates", "unit").unwrap();
    reset_root(&root);
    let state_path = root.join("story-state-aster");
    let template_ledger = root.join("template-ledger-aster");

    let output = call_mcp_story_tool(
        GENERATE_LORE_TEMPLATES_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": root.join("story-ledger-aster"),
            "template_ledger_path": template_ledger,
            "state_id": "issue1054_lore_template_state",
            "ledger_id": "issue1054_story_ledger",
            "template_ledger_id": "issue1054_template_ledger",
            "request_id": "issue1054_generate_templates",
            "requested_by": "litd_editor_issue1054_test",
            "ward_policy": ward(),
            "templates": happy_templates()
        }),
    )
    .unwrap();
    assert_eq!(output.tool_name, GENERATE_LORE_TEMPLATES_TOOL);

    let state = read_mcp_story_state(&state_path).unwrap();
    let ledger = read_lore_template_ledger(&template_ledger).unwrap();
    assert_eq!(state.lore_entry_count, 9);
    assert_eq!(state.lore_timeline_marker_count, 2);
    assert_eq!(
        state.lore_entry_link_count,
        ledger.ledger.generated_link_count
    );
    assert!(state.lore_entry_link_count >= 18);
    assert_eq!(ledger.ledger.template_count, 4);
    assert_eq!(ledger.ledger.generated_entry_count, 9);
    assert_eq!(ledger.ledger.lowered_fingerprint_count, 9);
    assert_eq!(
        ledger.ledger.template_kinds,
        vec![
            "creation_myth",
            "fallen_empire",
            "pantheon",
            "rival_cabal_set"
        ]
    );
    assert_eq!(ledger.ledger.report_fingerprint.len(), 64);
    assert!(
        state
            .state
            .lore_entries
            .iter()
            .any(|entry| entry.entry_id == "tpl_ashen_deity_0_bellara")
            && state
                .state
                .lore_entries
                .iter()
                .any(|entry| entry.entry_id == "tpl_ashen_creation_myth")
            && ledger
                .ledger
                .results
                .iter()
                .all(|result| result.ward_decision.code == "LITD-WARD-PASS")
    );
}

#[test]
fn issue1054_lore_templates_edges_fail_closed_without_mutation() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1054_lore_templates", "edges").unwrap();
    reset_root(&root);
    let state_path = root.join("story-state-aster");
    seed_state(&state_path, &root.join("seed-ledger-aster"));

    assert_no_write(
        &state_path,
        &root.join("edge-empty-template-ledger-aster"),
        json!([]),
        "template count",
    );
    assert_no_write(
        &state_path,
        &root.join("edge-zero-count-template-ledger-aster"),
        json!([template(
            "zero_count",
            "pantheon",
            "Ashen saints",
            0,
            "zero",
            2
        )]),
        "pantheon count",
    );
    assert_no_write(
        &state_path,
        &root.join("edge-blocked-template-ledger-aster"),
        json!([template(
            "blocked_copy",
            "creation_myth",
            "Copy Carrion Fields mythology into the Ashen Reach",
            1,
            "blocked",
            3
        )]),
        "LITD-LORE-TEMPLATE-BLOCKED",
    );
    assert_no_write(
        &state_path,
        &root.join("edge-duplicate-template-ledger-aster"),
        json!([
            template(
                "first_creation",
                "creation_myth",
                "Ashen origin",
                1,
                "same",
                4
            ),
            template(
                "second_creation",
                "creation_myth",
                "Ashen origin",
                1,
                "same",
                5
            )
        ]),
        "already exists",
    );
}

fn seed_state(state_path: &Path, ledger_path: &Path) {
    call_mcp_story_tool(
        UPSERT_LORE_ENTRY_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger_path,
            "state_id": "issue1054_lore_template_state",
            "ledger_id": "issue1054_seed_ledger",
            "request_id": "issue1054_seed",
            "requested_by": "litd_editor_issue1054_test",
            "entries": [seed_entry()]
        }),
    )
    .unwrap();
}

fn assert_no_write(state_path: &Path, template_ledger: &Path, templates: Value, expected: &str) {
    let before = read_mcp_story_state(state_path).unwrap().sha256;
    let error = call_mcp_story_tool(
        GENERATE_LORE_TEMPLATES_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": template_ledger.with_file_name("edge-story-ledger-aster"),
            "template_ledger_path": template_ledger,
            "state_id": "issue1054_lore_template_state",
            "ledger_id": "issue1054_story_edge_ledger",
            "template_ledger_id": "issue1054_template_edge_ledger",
            "request_id": "issue1054_edge",
            "requested_by": "litd_editor_issue1054_test",
            "ward_policy": ward(),
            "templates": templates
        }),
    )
    .unwrap_err();
    let after = read_mcp_story_state(state_path).unwrap().sha256;
    assert_eq!(before, after);
    assert!(!template_ledger.exists());
    assert!(error.to_string().contains(expected), "{error}");
}

fn happy_templates() -> Value {
    json!([
        template(
            "ashen_pantheon",
            "pantheon",
            "Ashen rites and river oaths",
            4,
            "ashen",
            1
        ),
        template(
            "ashen_cabals",
            "rival_cabal_set",
            "Rival orders contesting ash relics",
            3,
            "ashen",
            2
        ),
        template(
            "ashen_creation",
            "creation_myth",
            "The first dawn after the Sundering",
            1,
            "ashen",
            3
        ),
        template(
            "ashen_empire",
            "fallen_empire",
            "A lost empire under glass bells",
            1,
            "ashen",
            4
        )
    ])
}

fn ward() -> Value {
    json!({
        "policy_id": "ward_issue1054",
        "max_generated_entries": 12,
        "blocked_terms": ["Carrion Fields", "copywritten", "verbatim"]
    })
}

fn template(id: &str, kind: &str, theme: &str, count: usize, prefix: &str, seed: u64) -> Value {
    json!({
        "template_id": id,
        "kind": kind,
        "theme": theme,
        "count": count,
        "prefix": prefix,
        "seed": seed
    })
}

fn seed_entry() -> Value {
    json!({
        "entry_id": "seed_anchor",
        "kind": "place",
        "title": "Seed Anchor",
        "summary": "Existing source-of-truth lore that must remain unchanged on edge errors.",
        "body_rich_text": "## Seed Anchor\nExisting source-of-truth lore.",
        "metadata": { "source": "issue1054_edge_seed" },
        "source_refs": ["issue_1054"],
        "tags": ["edge_seed"],
        "links": [],
        "timeline_markers": []
    })
}

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
