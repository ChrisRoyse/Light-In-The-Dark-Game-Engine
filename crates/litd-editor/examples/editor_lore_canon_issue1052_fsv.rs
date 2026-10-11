use std::path::Path;

use litd_editor::{
    mcp_fsv_tools::{READ_STORY_RENDER_STATE_TOOL, call_mcp_fsv_tool},
    mcp_story_tools::{
        QUERY_LORE_CANON_TOOL, UPSERT_LORE_ENTRY_TOOL, UPSERT_LORE_SURFACE_TOOL,
        call_mcp_story_tool, read_lore_canon_query_ledger, read_mcp_story_state,
    },
};
use litd_engine::fsv_artifacts::begin_issue_run;
use litd_hash::sha256_hex;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let run = begin_issue_run("issue1052_lore_canon")?;
    let state_path = run.run_dir().join("story-state-aster");
    let seed_ledger = run.run_dir().join("seed-ledger-aster");
    let surface_ledger = run.run_dir().join("surface-ledger-aster");
    let semantic_vault = run.run_dir().join("semantic-vault-aster");
    let query_ledger_path = run.run_dir().join("query-ledger-aster");

    seed_lore(&state_path, &seed_ledger, &surface_ledger)?;
    let query_output = call_mcp_story_tool(
        QUERY_LORE_CANON_TOOL,
        json!({
            "state_path": state_path,
            "semantic_vault_path": semantic_vault,
            "query_ledger_path": query_ledger_path,
            "state_id": "issue1052_lore_canon_state",
            "semantic_index_id": "issue1052_lore_semantic_index",
            "query_ledger_id": "issue1052_lore_query_ledger",
            "request_id": "issue1052_lore_query",
            "requested_by": "litd_editor_issue1052_fsv",
            "queries": happy_queries()
        }),
    )?;
    let state = read_mcp_story_state(&state_path)?;
    let query_ledger = read_lore_canon_query_ledger(&query_ledger_path)?;
    let fsv_readback = call_mcp_fsv_tool(
        READ_STORY_RENDER_STATE_TOOL,
        json!({
            "artifacts": [
                { "label": "issue1052_story_state", "kind": "story_state", "path": state_path },
                { "label": "issue1052_lore_canon_query", "kind": "lore_canon_query_ledger", "path": query_ledger_path }
            ]
        }),
    )?;

    let empty_queries = edge_no_write(
        &state_path,
        &run.run_dir().join("edge-semantic-empty-aster"),
        &run.run_dir().join("edge-empty-query-ledger-aster"),
        json!([]),
        "query count",
        "issue1052_lore_canon_state",
    )?;
    let zero_limit = {
        let mut queries = happy_queries();
        queries[0]["limit"] = json!(0);
        edge_no_write(
            &state_path,
            &run.run_dir().join("edge-semantic-limit-aster"),
            &run.run_dir().join("edge-limit-query-ledger-aster"),
            queries,
            "query limit",
            "issue1052_lore_canon_state",
        )?
    };
    let blank_text = {
        let mut queries = happy_queries();
        queries[0]["text"] = json!(" ");
        edge_no_write(
            &state_path,
            &run.run_dir().join("edge-semantic-blank-aster"),
            &run.run_dir().join("edge-blank-query-ledger-aster"),
            queries,
            "single-line text",
            "issue1052_lore_canon_state",
        )?
    };
    let state_mismatch = edge_no_write(
        &state_path,
        &run.run_dir().join("edge-semantic-state-aster"),
        &run.run_dir().join("edge-state-query-ledger-aster"),
        happy_queries(),
        "state_id mismatch",
        "wrong_state",
    )?;

    let grounded_answer_present = query_ledger
        .ledger
        .results
        .iter()
        .any(|result| result.grounded_answer.is_some());
    let contradiction_present = query_ledger
        .ledger
        .results
        .iter()
        .flat_map(|result| result.claim_findings.iter())
        .any(|finding| finding.status == "contradiction");
    let ood_present = query_ledger
        .ledger
        .results
        .iter()
        .any(|result| result.status == "out_of_distribution");
    let report = json!({
        "schema_version": 1,
        "issue": 1052,
        "source_of_truth": {
            "state_path": state_path,
            "query_ledger_path": query_ledger_path,
            "semantic_vault_path": semantic_vault,
            "state_sha256": state.sha256,
            "query_ledger_sha256": query_ledger.sha256,
            "fsv_aggregate_sha256": fsv_readback.aggregate_sha256,
            "research_refs": [
                "https://learn.microsoft.com/azure/search/retrieval-augmented-generation-overview",
                "https://neo4j.com/developer-blog/knowledge-graph-llm-multi-hop-reasoning/"
            ],
            "readme_refs": [
                "docs/readme/02-storytelling.png",
                "docs/readme/GP10-dialogue.png",
                "docs/readme/GP11-quest.png",
                "docs/readme/S16-quest-designer.png"
            ]
        },
        "happy_path": {
            "tool_name": query_output.tool_name,
            "operation_id": query_output.operation_id,
            "query_ledger": query_ledger,
            "fsv_readback": fsv_readback
        },
        "edges": {
            "empty_queries": empty_queries,
            "zero_limit": zero_limit,
            "blank_text": blank_text,
            "state_mismatch": state_mismatch
        },
        "assertions": {
            "lore_entry_count": state.lore_entry_count,
            "lore_surface_count": state.lore_surface_count,
            "semantic_perspective_count": query_ledger.ledger.semantic_perspective_count,
            "content_record_count": query_ledger.ledger.content_record_count,
            "query_count": query_ledger.ledger.query_count,
            "contradiction_count": query_ledger.ledger.contradiction_count,
            "out_of_distribution_count": query_ledger.ledger.out_of_distribution_count,
            "grounded_answer_count": query_ledger.ledger.grounded_answer_count,
            "grounded_answer_present": grounded_answer_present,
            "contradiction_present": contradiction_present,
            "ood_present": ood_present,
            "fsv_artifact_count": fsv_readback.artifact_count
        }
    });
    assert_issue1052(
        state.lore_entry_count,
        state.lore_surface_count,
        &query_ledger,
        grounded_answer_present,
        contradiction_present,
        ood_present,
        fsv_readback.artifact_count,
    )?;
    let report_readback = run.write_report(&report)?;
    if sha256_hex(&report_readback.bytes) != report_readback.sha256 {
        return Err("report sha256 readback mismatch".into());
    }
    println!(
        "ISSUE1052_LORE_CANON_FSV report={} bytes={} sha256={} state={} state_sha256={} query_ledger={} query_ledger_sha256={}",
        report_readback.path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        state_path.display(),
        state.sha256,
        query_ledger_path.display(),
        query_ledger.sha256
    );
    Ok(())
}

fn assert_issue1052(
    lore_entry_count: usize,
    lore_surface_count: usize,
    ledger: &litd_editor::mcp_story_tools::LoreCanonQueryLedgerReadback,
    answer: bool,
    contradiction: bool,
    ood: bool,
    artifacts: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if lore_entry_count != 4
        || lore_surface_count != 2
        || ledger.ledger.semantic_perspective_count != 50
        || ledger.ledger.content_record_count != 6
        || ledger.ledger.query_count != 4
        || ledger.ledger.contradiction_count != 1
        || ledger.ledger.out_of_distribution_count != 1
        || ledger.ledger.grounded_answer_count != 1
        || !answer
        || !contradiction
        || !ood
        || artifacts != 2
    {
        return Err("issue1052 lore canon FSV assertions failed".into());
    }
    Ok(())
}

fn seed_lore(
    state_path: &Path,
    seed_ledger: &Path,
    surface_ledger: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    call_mcp_story_tool(
        UPSERT_LORE_ENTRY_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": seed_ledger,
            "state_id": "issue1052_lore_canon_state",
            "ledger_id": "issue1052_seed_ledger",
            "request_id": "issue1052_seed",
            "requested_by": "litd_editor_issue1052_fsv",
            "entries": lore_entries()
        }),
    )?;
    call_mcp_story_tool(
        UPSERT_LORE_SURFACE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": surface_ledger,
            "state_id": "issue1052_lore_canon_state",
            "ledger_id": "issue1052_surface_ledger",
            "request_id": "issue1052_surface_seed",
            "requested_by": "litd_editor_issue1052_fsv",
            "surfaces": lore_surfaces()
        }),
    )?;
    Ok(())
}

fn edge_no_write(
    state_path: &Path,
    semantic_vault: &Path,
    query_ledger: &Path,
    queries: Value,
    expected_error: &str,
    state_id: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = read_mcp_story_state(state_path)?;
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
            "requested_by": "litd_editor_issue1052_fsv",
            "queries": queries
        }),
    )
    .unwrap_err();
    let after = read_mcp_story_state(state_path)?;
    let message = error.to_string();
    if before.sha256 != after.sha256 || query_ledger.exists() || !message.contains(expected_error) {
        return Err(format!("edge did not fail closed: {message}").into());
    }
    Ok(json!({
        "failed_closed": true,
        "expected_error": expected_error,
        "actual_error": message,
        "state_sha256_before": before.sha256,
        "state_sha256_after": after.sha256,
        "query_ledger_exists_after": query_ledger.exists()
    }))
}

fn happy_queries() -> Value {
    json!([
        query(
            "search_vhael",
            "semantic_search",
            "ember oath Vhael cinder astrolabe",
            4,
            json!([])
        ),
        query(
            "check_domain",
            "consistency_check",
            "Vhael domain and Sable Choir service",
            5,
            json!([
                metadata_claim(
                    "claim_domain_supported",
                    "deity_vhael",
                    "domain",
                    "ember_oaths"
                ),
                metadata_claim(
                    "claim_domain_contradiction",
                    "deity_vhael",
                    "domain",
                    "frost_silence"
                ),
                link_claim(
                    "claim_service_supported",
                    "cabal_sable_choir",
                    "serves",
                    "deity_vhael"
                )
            ])
        ),
        query(
            "answer_choir",
            "grounded_answer",
            "Who does the Sable Choir serve in the Ashen Reach?",
            5,
            json!([link_claim(
                "claim_answer_ground",
                "cabal_sable_choir",
                "serves",
                "deity_vhael"
            )])
        ),
        query(
            "ood_question",
            "semantic_search",
            "zzznomatch qqqtoken nowhere",
            3,
            json!([])
        )
    ])
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
    json!({ "query_id": id, "kind": kind, "text": text, "limit": limit, "proposed_claims": claims })
}

fn metadata_claim(id: &str, subject: &str, key: &str, value: &str) -> Value {
    json!({
        "claim_id": id,
        "subject_entry_id": subject,
        "relation": null,
        "target_entry_id": null,
        "metadata_key": key,
        "metadata_value": value,
        "source_ref": "issue_1052_fsv_claim"
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
        "source_ref": "issue_1052_fsv_claim"
    })
}
