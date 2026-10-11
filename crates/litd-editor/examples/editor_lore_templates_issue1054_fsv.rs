use std::path::Path;

use litd_editor::{
    mcp_fsv_tools::{READ_STORY_RENDER_STATE_TOOL, call_mcp_fsv_tool},
    mcp_story_tools::{
        GENERATE_LORE_TEMPLATES_TOOL, call_mcp_story_tool, read_lore_template_ledger,
        read_mcp_story_state,
    },
};
use litd_engine::fsv_artifacts::begin_issue_run;
use litd_hash::sha256_hex;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let run = begin_issue_run("issue1054_lore_templates")?;
    let state_path = run.run_dir().join("story-state-aster");
    let story_ledger = run.run_dir().join("story-ledger-aster");
    let template_ledger_path = run.run_dir().join("template-ledger-aster");

    let template_output = call_mcp_story_tool(
        GENERATE_LORE_TEMPLATES_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": story_ledger,
            "template_ledger_path": template_ledger_path,
            "state_id": "issue1054_lore_template_state",
            "ledger_id": "issue1054_story_ledger",
            "template_ledger_id": "issue1054_template_ledger",
            "request_id": "issue1054_generate_templates",
            "requested_by": "litd_editor_issue1054_fsv",
            "ward_policy": ward(),
            "templates": happy_templates()
        }),
    )?;
    let state = read_mcp_story_state(&state_path)?;
    let template_ledger = read_lore_template_ledger(&template_ledger_path)?;
    let fsv_readback = call_mcp_fsv_tool(
        READ_STORY_RENDER_STATE_TOOL,
        json!({
            "artifacts": [
                { "label": "issue1054_story_state", "kind": "story_state", "path": state_path },
                { "label": "issue1054_story_ledger", "kind": "story_ledger", "path": story_ledger },
                { "label": "issue1054_template_ledger", "kind": "lore_template_ledger", "path": template_ledger_path }
            ]
        }),
    )?;

    let empty_templates = edge_no_write(
        &state_path,
        &run.run_dir().join("edge-empty-template-ledger-aster"),
        json!([]),
        "template count",
    )?;
    let zero_count = edge_no_write(
        &state_path,
        &run.run_dir().join("edge-zero-count-template-ledger-aster"),
        json!([template(
            "zero_count",
            "pantheon",
            "Ashen saints",
            0,
            "zero",
            2
        )]),
        "pantheon count",
    )?;
    let blocked = edge_no_write(
        &state_path,
        &run.run_dir().join("edge-blocked-template-ledger-aster"),
        json!([template(
            "blocked_copy",
            "creation_myth",
            "Copy Carrion Fields mythology into the Ashen Reach",
            1,
            "blocked",
            3
        )]),
        "LITD-LORE-TEMPLATE-BLOCKED",
    )?;
    let duplicate_entry = edge_no_write(
        &state_path,
        &run.run_dir().join("edge-duplicate-template-ledger-aster"),
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
    )?;

    assert_issue1054(
        state.lore_entry_count,
        state.lore_entry_link_count,
        state.lore_timeline_marker_count,
        &template_ledger,
        fsv_readback.artifact_count,
    )?;
    let report = json!({
        "schema_version": 1,
        "issue": 1054,
        "source_of_truth": {
            "state_path": state_path,
            "story_ledger_path": story_ledger,
            "template_ledger_path": template_ledger_path,
            "state_sha256": state.sha256,
            "template_ledger_sha256": template_ledger.sha256,
            "fsv_aggregate_sha256": fsv_readback.aggregate_sha256,
            "research_refs": [
                "https://learn.microsoft.com/en-us/azure/search/retrieval-augmented-generation-overview",
                "https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.600-1.pdf"
            ],
            "readme_refs": [
                "docs/readme/S18-lore-authoring.png",
                "docs/readme/L05-pantheon.png",
                "docs/readme/L17-faction-hollow.png"
            ]
        },
        "sufficiency_argument": "The run covers every #1054 template class: pantheon, rival cabal set, creation myth, and fallen empire. The generated entries are lowered into the real Calyx Aster story state as validated LoreEntry data with same-batch links, history timeline markers, Ward policy metadata, and a separate template provenance ledger. Edge probes cover empty template sets, invalid counts, blocked copy terms, and duplicate generated entry ids without changing the source-of-truth state hash or writing template ledgers.",
        "happy_path": {
            "tool_name": template_output.tool_name,
            "operation_id": template_output.operation_id,
            "template_ledger": template_ledger,
            "fsv_readback": fsv_readback
        },
        "edges": {
            "empty_templates": empty_templates,
            "zero_count": zero_count,
            "blocked": blocked,
            "duplicate_entry": duplicate_entry
        },
        "assertions": {
            "lore_entry_count": state.lore_entry_count,
            "lore_entry_link_count": state.lore_entry_link_count,
            "lore_timeline_marker_count": state.lore_timeline_marker_count,
            "template_count": template_ledger.ledger.template_count,
            "generated_entry_count": template_ledger.ledger.generated_entry_count,
            "lowered_fingerprint_count": template_ledger.ledger.lowered_fingerprint_count,
            "template_kinds": template_ledger.ledger.template_kinds,
            "fsv_artifact_count": fsv_readback.artifact_count
        }
    });
    let report_readback = run.write_report(&report)?;
    if sha256_hex(&report_readback.bytes) != report_readback.sha256 {
        return Err("report sha256 readback mismatch".into());
    }
    println!(
        "ISSUE1054_LORE_TEMPLATES_FSV report={} bytes={} sha256={} state={} state_sha256={} template_ledger={} template_ledger_sha256={}",
        report_readback.path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        state_path.display(),
        state.sha256,
        template_ledger_path.display(),
        template_ledger.sha256
    );
    Ok(())
}

fn assert_issue1054(
    lore_entry_count: usize,
    link_count: usize,
    timeline_marker_count: usize,
    ledger: &litd_editor::mcp_story_tools::LoreTemplateLedgerReadback,
    artifacts: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if lore_entry_count != 9
        || link_count < 18
        || timeline_marker_count != 2
        || ledger.ledger.template_count != 4
        || ledger.ledger.generated_entry_count != 9
        || ledger.ledger.lowered_fingerprint_count != 9
        || artifacts != 3
    {
        return Err("issue1054 lore template FSV assertions failed".into());
    }
    Ok(())
}

fn edge_no_write(
    state_path: &Path,
    template_ledger_path: &Path,
    templates: Value,
    expected: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = read_mcp_story_state(state_path)?;
    let error = call_mcp_story_tool(
        GENERATE_LORE_TEMPLATES_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": template_ledger_path.with_file_name("edge-story-ledger-aster"),
            "template_ledger_path": template_ledger_path,
            "state_id": "issue1054_lore_template_state",
            "ledger_id": "issue1054_edge_story_ledger",
            "template_ledger_id": "issue1054_edge_template_ledger",
            "request_id": "issue1054_edge",
            "requested_by": "litd_editor_issue1054_fsv",
            "ward_policy": ward(),
            "templates": templates
        }),
    )
    .unwrap_err();
    let after = read_mcp_story_state(state_path)?;
    let actual_error = error.to_string();
    Ok(json!({
        "expected_error": expected,
        "actual_error": actual_error,
        "failed_closed": actual_error.contains(expected)
            && before.sha256 == after.sha256
            && !template_ledger_path.exists(),
        "state_sha256_before": before.sha256,
        "state_sha256_after": after.sha256,
        "template_ledger_exists_after": template_ledger_path.exists()
    }))
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
