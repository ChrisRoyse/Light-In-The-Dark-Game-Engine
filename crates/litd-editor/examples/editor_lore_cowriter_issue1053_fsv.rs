use std::path::Path;

use litd_editor::{
    mcp_fsv_tools::{READ_STORY_RENDER_STATE_TOOL, call_mcp_fsv_tool},
    mcp_story_tools::{
        CO_WRITE_LORE_TOOL, UPSERT_LORE_ENTRY_TOOL, call_mcp_story_tool, read_lore_cowriter_ledger,
        read_mcp_story_state,
    },
};
use litd_engine::fsv_artifacts::begin_issue_run;
use litd_hash::sha256_hex;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let run = begin_issue_run("issue1053_lore_cowriter")?;
    let state_path = run.run_dir().join("story-state-aster");
    let seed_ledger = run.run_dir().join("seed-ledger-aster");
    let story_ledger = run.run_dir().join("story-ledger-aster");
    let cowriter_ledger_path = run.run_dir().join("cowriter-ledger-aster");

    seed_lore(&state_path, &seed_ledger)?;
    let cowriter_output = call_mcp_story_tool(
        CO_WRITE_LORE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": story_ledger,
            "cowriter_ledger_path": cowriter_ledger_path,
            "state_id": "issue1053_lore_cowriter_state",
            "ledger_id": "issue1053_story_ledger",
            "cowriter_ledger_id": "issue1053_cowriter_ledger",
            "request_id": "issue1053_cowrite",
            "requested_by": "litd_editor_issue1053_fsv",
            "ward_policy": ward(),
            "drafts": happy_drafts()
        }),
    )?;
    let state = read_mcp_story_state(&state_path)?;
    let cowriter_ledger = read_lore_cowriter_ledger(&cowriter_ledger_path)?;
    let fsv_readback = call_mcp_fsv_tool(
        READ_STORY_RENDER_STATE_TOOL,
        json!({
            "artifacts": [
                { "label": "issue1053_story_state", "kind": "story_state", "path": state_path },
                { "label": "issue1053_story_ledger", "kind": "story_ledger", "path": story_ledger },
                { "label": "issue1053_cowriter_ledger", "kind": "lore_cowriter_ledger", "path": cowriter_ledger_path }
            ]
        }),
    )?;

    let empty_drafts = edge_no_write(
        &state_path,
        &run.run_dir().join("edge-empty-cowriter-ledger-aster"),
        json!([]),
        "draft count",
    )?;
    let blocked = edge_no_write(
        &state_path,
        &run.run_dir().join("edge-blocked-cowriter-ledger-aster"),
        json!([draft(
            "blocked_copy",
            "legend",
            "Copy Carrion Fields lore into Vhael and the Dawnward",
            "cowrite_blocked_copy",
            11
        )]),
        "LITD-LORE-COWRITER-BLOCKED",
    )?;
    let ungrounded = edge_no_write(
        &state_path,
        &run.run_dir().join("edge-ungrounded-cowriter-ledger-aster"),
        json!([draft(
            "ungrounded",
            "deity",
            "zzznomatch qqqtoken nowhere",
            "cowrite_ungrounded",
            12
        )]),
        "LITD-WARD-INSUFFICIENT-GROUNDING",
    )?;
    let duplicate_entry = edge_no_write(
        &state_path,
        &run.run_dir().join("edge-duplicate-cowriter-ledger-aster"),
        json!([draft(
            "duplicate",
            "deity",
            "Draft a Vhael ember oath deity for the Ashen Reach",
            "deity_vhael",
            13
        )]),
        "already exists",
    )?;

    let report = json!({
        "schema_version": 1,
        "issue": 1053,
        "source_of_truth": {
            "state_path": state_path,
            "story_ledger_path": story_ledger,
            "cowriter_ledger_path": cowriter_ledger_path,
            "state_sha256": state.sha256,
            "cowriter_ledger_sha256": cowriter_ledger.sha256,
            "fsv_aggregate_sha256": fsv_readback.aggregate_sha256,
            "research_refs": [
                "https://learn.microsoft.com/azure/search/retrieval-augmented-generation-overview",
                "https://www.nist.gov/itl/ai-risk-management-framework"
            ],
            "readme_refs": ["docs/readme/S18-lore-authoring.png"]
        },
        "sufficiency_argument": "Four synthetic prompts cover the #1053 output classes named in the issue: deity, cabal, history, and legend. The seeded canon covers deity/faction/cabal/place anchors, enough to prove grounding, Ward gating, validated lore-entry lowering, source links, history timeline markers, and persisted provenance. Edges cover empty batch, blocked/copyright-copy request, ungrounded request, and duplicate entry without state mutation.",
        "happy_path": {
            "tool_name": cowriter_output.tool_name,
            "operation_id": cowriter_output.operation_id,
            "cowriter_ledger": cowriter_ledger,
            "fsv_readback": fsv_readback
        },
        "edges": {
            "empty_drafts": empty_drafts,
            "blocked": blocked,
            "ungrounded": ungrounded,
            "duplicate_entry": duplicate_entry
        },
        "assertions": {
            "lore_entry_count": state.lore_entry_count,
            "generated_entry_count": cowriter_ledger.ledger.generated_entry_count,
            "admitted_count": cowriter_ledger.ledger.admitted_count,
            "lowered_fingerprint_count": cowriter_ledger.ledger.lowered_fingerprint_count,
            "source_anchor_count": cowriter_ledger.ledger.source_anchor_count,
            "matched_term_count": cowriter_ledger.ledger.matched_term_count,
            "fsv_artifact_count": fsv_readback.artifact_count
        }
    });
    assert_issue1053(
        state.lore_entry_count,
        &cowriter_ledger,
        fsv_readback.artifact_count,
    )?;
    let report_readback = run.write_report(&report)?;
    if sha256_hex(&report_readback.bytes) != report_readback.sha256 {
        return Err("report sha256 readback mismatch".into());
    }
    println!(
        "ISSUE1053_LORE_COWRITER_FSV report={} bytes={} sha256={} state={} state_sha256={} cowriter_ledger={} cowriter_ledger_sha256={}",
        report_readback.path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        state_path.display(),
        state.sha256,
        cowriter_ledger_path.display(),
        cowriter_ledger.sha256
    );
    Ok(())
}

fn assert_issue1053(
    lore_entry_count: usize,
    ledger: &litd_editor::mcp_story_tools::LoreCowriterLedgerReadback,
    artifacts: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if lore_entry_count != 8
        || ledger.ledger.generated_entry_count != 4
        || ledger.ledger.admitted_count != 4
        || ledger.ledger.lowered_fingerprint_count != 4
        || ledger.ledger.source_anchor_count < 4
        || ledger.ledger.matched_term_count < 6
        || artifacts != 3
    {
        return Err("issue1053 lore co-writer FSV assertions failed".into());
    }
    Ok(())
}

fn seed_lore(state_path: &Path, seed_ledger: &Path) -> Result<(), Box<dyn std::error::Error>> {
    call_mcp_story_tool(
        UPSERT_LORE_ENTRY_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": seed_ledger,
            "state_id": "issue1053_lore_cowriter_state",
            "ledger_id": "issue1053_seed_ledger",
            "request_id": "issue1053_seed",
            "requested_by": "litd_editor_issue1053_fsv",
            "entries": lore_entries()
        }),
    )?;
    Ok(())
}

fn edge_no_write(
    state_path: &Path,
    cowriter_ledger_path: &Path,
    drafts: Value,
    expected: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = read_mcp_story_state(state_path)?;
    let error = call_mcp_story_tool(
        CO_WRITE_LORE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": cowriter_ledger_path.with_file_name("edge-story-ledger-aster"),
            "cowriter_ledger_path": cowriter_ledger_path,
            "state_id": "issue1053_lore_cowriter_state",
            "ledger_id": "issue1053_edge_story_ledger",
            "cowriter_ledger_id": "issue1053_edge_cowriter_ledger",
            "request_id": "issue1053_edge",
            "requested_by": "litd_editor_issue1053_fsv",
            "ward_policy": ward(),
            "drafts": drafts
        }),
    )
    .unwrap_err();
    let after = read_mcp_story_state(state_path)?;
    let actual_error = error.to_string();
    Ok(json!({
        "expected_error": expected,
        "actual_error": actual_error,
        "failed_closed": actual_error.contains(expected) && before.sha256 == after.sha256 && !cowriter_ledger_path.exists(),
        "state_sha256_before": before.sha256,
        "state_sha256_after": after.sha256,
        "cowriter_ledger_exists_after": cowriter_ledger_path.exists()
    }))
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
            json!({"domain":"ember_oaths"}),
            [link("place_ashen_reach", "watches")]
        ),
        entry(
            "faction_dawnward",
            "faction",
            "Dawnward",
            "Holy order carrying lamps against the Hollow.",
            json!({"ethos":"good"}),
            [link("deity_vhael", "serves")]
        ),
        entry(
            "cabal_hollow_pact",
            "cabal",
            "Hollow Pact",
            "Cultists bargaining with the Outer Dark for forbidden power.",
            json!({"ideology":"forbidden_pacts"}),
            [link("place_ashen_reach", "haunts")]
        ),
        entry(
            "place_ashen_reach",
            "place",
            "Ashen Reach",
            "The grey Sundering scar where the Weave thins.",
            json!({"region":"frontier"}),
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
