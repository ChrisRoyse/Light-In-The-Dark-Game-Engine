use std::{fs, path::Path};

use serde_json::{Value, json};

use crate::mcp_story_tools::{
    CO_WRITE_LORE_TOOL, UPSERT_LORE_ENTRY_TOOL, call_mcp_story_tool, read_lore_cowriter_ledger,
    read_mcp_story_state,
};

#[test]
fn issue1053_lore_cowriter_generates_original_grounded_lore() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1053_lore_cowriter", "unit").unwrap();
    reset_root(&root);
    let state_path = root.join("story-state-aster");
    seed_lore(&state_path, &root.join("seed-ledger-aster"));
    let cowriter_ledger = root.join("cowriter-ledger-aster");

    let output = call_mcp_story_tool(
        CO_WRITE_LORE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": root.join("story-ledger-aster"),
            "cowriter_ledger_path": cowriter_ledger,
            "state_id": "issue1053_lore_cowriter_state",
            "ledger_id": "issue1053_story_ledger",
            "cowriter_ledger_id": "issue1053_cowriter_ledger",
            "request_id": "issue1053_cowrite",
            "requested_by": "litd_editor_issue1053_test",
            "ward_policy": ward(),
            "drafts": happy_drafts()
        }),
    )
    .unwrap();
    assert_eq!(output.tool_name, CO_WRITE_LORE_TOOL);

    let state = read_mcp_story_state(&state_path).unwrap();
    let ledger = read_lore_cowriter_ledger(&cowriter_ledger).unwrap();
    assert_eq!(state.lore_entry_count, 8);
    assert_eq!(ledger.ledger.draft_count, 4);
    assert_eq!(ledger.ledger.admitted_count, 4);
    assert_eq!(ledger.ledger.generated_entry_count, 4);
    assert_eq!(ledger.ledger.lowered_fingerprint_count, 4);
    assert!(ledger.ledger.source_anchor_count >= 4);
    assert!(ledger.ledger.matched_term_count >= 6);
    assert_eq!(ledger.ledger.results[0].status, "admitted");
    assert!(
        ledger
            .ledger
            .results
            .iter()
            .all(|result| result.ward_decision.code == "LITD-WARD-PASS")
    );
    assert!(
        state
            .state
            .lore_entries
            .iter()
            .any(|entry| entry.entry_id == "cowrite_velmorn_lantern")
            && state
                .state
                .lore_entries
                .iter()
                .any(|entry| entry.entry_id == "cowrite_cinderfold_covenant")
            && state
                .state
                .lore_entries
                .iter()
                .any(|entry| !entry.timeline_markers.is_empty())
    );
    assert_eq!(ledger.ledger.report_fingerprint.len(), 64);
}

#[test]
fn issue1053_lore_cowriter_edges_fail_closed_without_mutation() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1053_lore_cowriter", "edges").unwrap();
    reset_root(&root);
    let state_path = root.join("story-state-aster");
    seed_lore(&state_path, &root.join("seed-ledger-aster"));

    assert_no_write(
        &state_path,
        &root.join("edge-empty-ledger-aster"),
        json!([]),
        "draft count",
    );
    assert_no_write(
        &state_path,
        &root.join("edge-blocked-ledger-aster"),
        json!([draft(
            "blocked_copy",
            "legend",
            "Copy Carrion Fields lore into Vhael and the Dawnward",
            "cowrite_blocked_copy",
            11
        )]),
        "LITD-LORE-COWRITER-BLOCKED",
    );
    assert_no_write(
        &state_path,
        &root.join("edge-ungrounded-ledger-aster"),
        json!([draft(
            "ungrounded",
            "deity",
            "zzznomatch qqqtoken nowhere",
            "cowrite_ungrounded",
            12
        )]),
        "LITD-WARD-INSUFFICIENT-GROUNDING",
    );
    assert_no_write(
        &state_path,
        &root.join("edge-duplicate-ledger-aster"),
        json!([draft(
            "duplicate",
            "deity",
            "Draft a Vhael ember oath deity for the Ashen Reach",
            "deity_vhael",
            13
        )]),
        "already exists",
    );
}

fn seed_lore(state_path: &Path, seed_ledger: &Path) {
    call_mcp_story_tool(
        UPSERT_LORE_ENTRY_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": seed_ledger,
            "state_id": "issue1053_lore_cowriter_state",
            "ledger_id": "issue1053_seed_ledger",
            "request_id": "issue1053_seed",
            "requested_by": "litd_editor_issue1053_test",
            "entries": lore_entries()
        }),
    )
    .unwrap();
}

fn assert_no_write(state_path: &Path, cowriter_ledger: &Path, drafts: Value, expected: &str) {
    let before = read_mcp_story_state(state_path).unwrap().sha256;
    let error = call_mcp_story_tool(
        CO_WRITE_LORE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": cowriter_ledger.with_file_name("story-edge-ledger-aster"),
            "cowriter_ledger_path": cowriter_ledger,
            "state_id": "issue1053_lore_cowriter_state",
            "ledger_id": "issue1053_story_edge_ledger",
            "cowriter_ledger_id": "issue1053_cowriter_edge_ledger",
            "request_id": "issue1053_edge",
            "requested_by": "litd_editor_issue1053_test",
            "ward_policy": ward(),
            "drafts": drafts
        }),
    )
    .unwrap_err();
    let after = read_mcp_story_state(state_path).unwrap().sha256;
    assert_eq!(before, after);
    assert!(!cowriter_ledger.exists());
    assert!(error.to_string().contains(expected), "{error}");
}

fn happy_drafts() -> Value {
    json!([
        draft(
            "velmorn_lantern",
            "deity",
            "Draft an original ember oath deity tied to Vhael, Dawnward, and the Ashen Reach",
            "cowrite_velmorn_lantern",
            1
        ),
        draft(
            "cinderfold_covenant",
            "cabal",
            "Create a rival cabal around the Hollow Pact, Vesper Court, and memory songs",
            "cowrite_cinderfold_covenant",
            4
        ),
        draft(
            "third_bell_chronicle",
            "history",
            "Write a history of Galdmere bells after the Sundering and Ashen Reach",
            "cowrite_third_bell_chronicle",
            8
        ),
        draft(
            "grey_lantern_legend",
            "legend",
            "Author a legend about the Dawnward lamp, Hollow shadow, and Vhael",
            "cowrite_grey_lantern_legend",
            12
        )
    ])
}

fn ward() -> Value {
    json!({
        "policy_id": "ward_issue1053",
        "min_source_anchors": 2,
        "min_grounded_terms": 2,
        "blocked_terms": ["Carrion Fields", "copywritten", "verbatim"]
    })
}

fn lore_entries() -> Value {
    json!([
        entry(
            "deity_vhael",
            "deity",
            "Vhael",
            "Star-deity of ember oaths and names remembered.",
            json!({ "domain": "ember_oaths" }),
            [link("place_ashen_reach", "watches")]
        ),
        entry(
            "faction_dawnward",
            "faction",
            "Dawnward",
            "Holy order carrying lamps against the Hollow.",
            json!({ "ethos": "good" }),
            [link("deity_vhael", "serves")]
        ),
        entry(
            "cabal_hollow_pact",
            "cabal",
            "Hollow Pact",
            "Cultists bargaining with the Outer Dark for forbidden power.",
            json!({ "ideology": "forbidden_pacts" }),
            [link("place_ashen_reach", "haunts")]
        ),
        entry(
            "place_ashen_reach",
            "place",
            "Ashen Reach",
            "The grey Sundering scar where the Weave thins.",
            json!({ "region": "frontier" }),
            json!([])
        )
    ])
}

fn entry(
    entry_id: &str,
    kind: &str,
    title: &str,
    summary: &str,
    metadata: Value,
    links: impl serde::Serialize,
) -> Value {
    json!({
        "entry_id": entry_id,
        "kind": kind,
        "title": title,
        "summary": summary,
        "body_rich_text": format!("## {title}\n{summary}"),
        "metadata": metadata,
        "source_refs": ["issue_1053"],
        "tags": ["ashen_reach", "canon_seed"],
        "links": links,
        "timeline_markers": []
    })
}

fn draft(id: &str, kind: &str, prompt: &str, entry_id: &str, seed: u64) -> Value {
    json!({
        "draft_id": id,
        "kind": kind,
        "prompt": prompt,
        "requested_entry_id": entry_id,
        "seed": seed
    })
}

fn link(target_entry_id: &str, relation: &str) -> Value {
    json!({
        "target_entry_id": target_entry_id,
        "relation": relation,
        "description": format!("{relation} relationship to {target_entry_id}")
    })
}

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
