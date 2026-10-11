use std::{fs, path::Path};

use serde_json::{Value, json};

use crate::mcp_story_tools::{
    QUERY_LORE_CANON_TOOL, READ_STORY_TOOL, UPSERT_LORE_ENTRY_TOOL, UPSERT_LORE_SURFACE_TOOL,
    call_mcp_story_tool, read_lore_canon_query_ledger, read_mcp_story_state,
};

#[test]
fn issue1052_lore_canon_search_checks_and_answers_from_real_state() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1052_lore_canon", "unit").unwrap();
    reset_root(&root);
    let state_path = root.join("story-state-aster");
    let seed_ledger = root.join("seed-ledger-aster");
    let surface_ledger = root.join("surface-ledger-aster");
    let semantic_vault = root.join("semantic-vault-aster");
    let query_ledger = root.join("query-ledger-aster");
    seed_lore(&state_path, &seed_ledger, &surface_ledger);

    let output = crate::asset_vault_fixture::expect_calyx_substrate(
        call_mcp_story_tool(
            QUERY_LORE_CANON_TOOL,
            json!({
                "state_path": state_path,
                "semantic_vault_path": semantic_vault,
                "query_ledger_path": query_ledger,
                "state_id": "issue1052_lore_canon_state",
                "semantic_index_id": "issue1052_lore_semantic_index",
                "query_ledger_id": "issue1052_lore_query_ledger",
                "request_id": "issue1052_lore_query",
                "requested_by": "litd_editor_issue1052_test",
                "queries": [
                    query("search_vhael", "semantic_search", "ember oath Vhael cinder astrolabe", 4, json!([])),
                    query("check_domain", "consistency_check", "Vhael domain and Sable Choir service", 5, json!([
                        metadata_claim("claim_domain_supported", "deity_vhael", "domain", "ember_oaths"),
                        metadata_claim("claim_domain_contradiction", "deity_vhael", "domain", "frost_silence"),
                        link_claim("claim_service_supported", "cabal_sable_choir", "serves", "deity_vhael")
                    ])),
                    query("answer_choir", "grounded_answer", "Who does the Sable Choir serve in the Ashen Reach?", 5, json!([
                        link_claim("claim_answer_ground", "cabal_sable_choir", "serves", "deity_vhael")
                    ])),
                    query("ood_question", "semantic_search", "zzznomatch qqqtoken nowhere", 3, json!([]))
                ]
            }),
        ),
        "editor.mcp_story_tools.lore_canon",
    );
    assert_eq!(output.tool_name, QUERY_LORE_CANON_TOOL);

    let ledger = read_lore_canon_query_ledger(&query_ledger).unwrap();
    assert_eq!(ledger.ledger.semantic_perspective_count, 50);
    assert_eq!(ledger.ledger.content_record_count, 6);
    assert_eq!(ledger.ledger.query_count, 4);
    assert_eq!(ledger.ledger.contradiction_count, 1);
    assert_eq!(ledger.ledger.out_of_distribution_count, 1);
    assert_eq!(ledger.ledger.grounded_answer_count, 1);
    assert_eq!(ledger.ledger.results[0].status, "grounded");
    assert!(
        ledger.ledger.results[0]
            .hits
            .iter()
            .any(|hit| hit.source_id == "deity_vhael")
    );
    let findings = &ledger.ledger.results[1].claim_findings;
    assert!(findings.iter().any(|finding| finding.status == "supported"));
    assert!(
        findings
            .iter()
            .any(|finding| finding.status == "contradiction")
    );
    assert!(ledger.ledger.results[2].grounded_answer.is_some());
    assert_eq!(ledger.ledger.results[3].status, "out_of_distribution");
    assert_eq!(ledger.ledger.report_fingerprint.len(), 64);

    let read = call_mcp_story_tool(READ_STORY_TOOL, json!({ "state_path": state_path })).unwrap();
    assert_eq!(read.result["state_readback"]["lore_entry_count"], 4);
    assert_eq!(read.result["state_readback"]["lore_surface_count"], 2);
}

#[test]
fn issue1052_lore_canon_edges_fail_closed_without_query_ledger() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1052_lore_canon", "edges").unwrap();
    reset_root(&root);
    let state_path = root.join("story-state-aster");
    seed_lore(
        &state_path,
        &root.join("seed-ledger-aster"),
        &root.join("surface-ledger-aster"),
    );
    assert_no_query_write(
        &state_path,
        &root.join("semantic-empty-aster"),
        &root.join("empty-query-ledger-aster"),
        json!([]),
        "query count",
        "issue1052_lore_canon_state",
    );
    let mut bad_limit = sample_queries();
    bad_limit[0]["limit"] = json!(0);
    assert_no_query_write(
        &state_path,
        &root.join("semantic-limit-aster"),
        &root.join("limit-query-ledger-aster"),
        bad_limit,
        "query limit",
        "issue1052_lore_canon_state",
    );
    let mut blank_text = sample_queries();
    blank_text[0]["text"] = json!(" ");
    assert_no_query_write(
        &state_path,
        &root.join("semantic-blank-aster"),
        &root.join("blank-query-ledger-aster"),
        blank_text,
        "single-line text",
        "issue1052_lore_canon_state",
    );
    assert_no_query_write(
        &state_path,
        &root.join("semantic-state-mismatch-aster"),
        &root.join("state-mismatch-ledger-aster"),
        sample_queries(),
        "state_id mismatch",
        "wrong_state",
    );
}

fn seed_lore(state_path: &Path, seed_ledger: &Path, surface_ledger: &Path) {
    call_mcp_story_tool(
        UPSERT_LORE_ENTRY_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": seed_ledger,
            "state_id": "issue1052_lore_canon_state",
            "ledger_id": "issue1052_seed_ledger",
            "request_id": "issue1052_seed",
            "requested_by": "litd_editor_issue1052_test",
            "entries": lore_entries()
        }),
    )
    .unwrap();
    call_mcp_story_tool(
        UPSERT_LORE_SURFACE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": surface_ledger,
            "state_id": "issue1052_lore_canon_state",
            "ledger_id": "issue1052_surface_ledger",
            "request_id": "issue1052_surface_seed",
            "requested_by": "litd_editor_issue1052_test",
            "surfaces": lore_surfaces()
        }),
    )
    .unwrap();
}

fn assert_no_query_write(
    state_path: &Path,
    semantic_vault: &Path,
    query_ledger: &Path,
    queries: Value,
    expected: &str,
    state_id: &str,
) {
    let before = read_mcp_story_state(state_path).unwrap().sha256;
    let error = call_mcp_story_tool(
        QUERY_LORE_CANON_TOOL,
        json!({
            "state_path": state_path,
            "semantic_vault_path": semantic_vault,
            "query_ledger_path": query_ledger,
            "state_id": state_id,
            "semantic_index_id": "issue1052_edge_semantic_index",
            "query_ledger_id": "issue1052_edge_query_ledger",
            "request_id": "issue1052_edge",
            "requested_by": "litd_editor_issue1052_test",
            "queries": queries
        }),
    )
    .unwrap_err();
    let after = read_mcp_story_state(state_path).unwrap().sha256;
    assert_eq!(before, after);
    assert!(!query_ledger.exists());
    assert!(error.to_string().contains(expected), "{error}");
}

fn sample_queries() -> Value {
    json!([query(
        "edge_query",
        "semantic_search",
        "Vhael ember oath",
        4,
        json!([])
    )])
}

fn lore_entries() -> Value {
    json!([
        entry(
            "deity_vhael",
            "deity",
            "Vhael",
            "Star-deity of ember oaths.",
            json!({ "domain": "ember_oaths" }),
            [link("place_ashen_reach", "watches")]
        ),
        entry(
            "cabal_sable_choir",
            "cabal",
            "Sable Choir",
            "A hidden cabal of ash-singers that serves Vhael.",
            json!({ "ideology": "memory" }),
            [link("deity_vhael", "serves")]
        ),
        entry(
            "place_ashen_reach",
            "place",
            "Ashen Reach",
            "A frontier of cinder roads and black bells.",
            json!({ "region": "frontier" }),
            json!([])
        ),
        entry(
            "artifact_cinder_astrolabe",
            "artifact",
            "Cinder Astrolabe",
            "A relic that maps ember constellations.",
            json!({ "material": "smoke_bronze" }),
            [link("deity_vhael", "attuned_to")]
        )
    ])
}

fn lore_surfaces() -> Value {
    json!([
        surface(
            "surface_book_cinders",
            "book",
            "Book of Cinders",
            ["deity_vhael", "artifact_cinder_astrolabe"],
            "object://book/book_of_cinders"
        ),
        surface(
            "surface_dialogue_choir",
            "npc_dialogue",
            "Choir Initiate Rumor",
            ["cabal_sable_choir", "deity_vhael"],
            "dialogue://choir_initiate/opening"
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
        "source_refs": ["issue_1048", "issue_1052"],
        "tags": ["ashen_reach", "canon_query"],
        "links": links,
        "timeline_markers": []
    })
}

fn link(target_entry_id: &str, relation: &str) -> Value {
    json!({
        "target_entry_id": target_entry_id,
        "relation": relation,
        "description": format!("{relation} relationship to {target_entry_id}")
    })
}

fn surface(
    surface_id: &str,
    kind: &str,
    title: &str,
    lore_entry_ids: impl serde::Serialize,
    world_ref: &str,
) -> Value {
    json!({
        "surface_id": surface_id,
        "kind": kind,
        "title": title,
        "body_rich_text": format!("## {title}\nA canon surface for {title}."),
        "excerpt": format!("{title} canon excerpt"),
        "lore_entry_ids": lore_entry_ids,
        "world_ref": world_ref,
        "codex_unlock_id": null,
        "discovery_trigger": "inspect_or_talk",
        "source_refs": ["issue_1052"],
        "tags": ["ashen_reach", "canon_surface"]
    })
}

fn query(id: &str, kind: &str, text: &str, limit: usize, claims: Value) -> Value {
    json!({
        "query_id": id,
        "kind": kind,
        "text": text,
        "limit": limit,
        "proposed_claims": claims
    })
}

fn metadata_claim(id: &str, subject: &str, key: &str, value: &str) -> Value {
    json!({
        "claim_id": id,
        "subject_entry_id": subject,
        "relation": null,
        "target_entry_id": null,
        "metadata_key": key,
        "metadata_value": value,
        "source_ref": "issue_1052_test_claim"
    })
}

fn link_claim(id: &str, subject: &str, relation: &str, target: &str) -> Value {
    json!({
        "claim_id": id,
        "subject_entry_id": subject,
        "relation": relation,
        "target_entry_id": target,
        "metadata_key": null,
        "metadata_value": null,
        "source_ref": "issue_1052_test_claim"
    })
}

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
