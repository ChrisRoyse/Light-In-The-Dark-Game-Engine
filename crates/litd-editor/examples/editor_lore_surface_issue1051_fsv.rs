use std::path::Path;

use litd_editor::{
    mcp_fsv_tools::{READ_STORY_RENDER_STATE_TOOL, call_mcp_fsv_tool},
    mcp_story_tools::{
        READ_STORY_TOOL, UPSERT_LORE_ENTRY_TOOL, UPSERT_LORE_SURFACE_TOOL, call_mcp_story_tool,
        read_mcp_story_state, read_mcp_story_tool_ledger,
    },
};
use litd_engine::fsv_artifacts::begin_issue_run;
use litd_hash::sha256_hex;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let run = begin_issue_run("issue1051_lore_surface")?;
    let state_path = run.run_dir().join("story-state-aster");
    let seed_ledger_path = run.run_dir().join("story-seed-ledger-aster");
    let surface_ledger_path = run.run_dir().join("story-surface-ledger-aster");

    seed_lore(&state_path, &seed_ledger_path)?;
    let surface_output = call_mcp_story_tool(
        UPSERT_LORE_SURFACE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": surface_ledger_path,
            "state_id": "issue1051-lore-surface-state",
            "ledger_id": "issue1051-lore-surface-ledger",
            "request_id": "issue1051-lore-surface",
            "requested_by": "litd-editor-issue1051-fsv",
            "surfaces": lore_surfaces()
        }),
    )?;
    let story_read = call_mcp_story_tool(READ_STORY_TOOL, json!({ "state_path": state_path }))?;
    let state = read_mcp_story_state(&state_path)?;
    let surface_ledger = read_mcp_story_tool_ledger(&surface_ledger_path)?;
    let fsv_readback = call_mcp_fsv_tool(
        READ_STORY_RENDER_STATE_TOOL,
        json!({
            "artifacts": [
                { "label": "issue1051_story_state", "kind": "story_state", "path": state_path },
                { "label": "issue1051_surface_ledger", "kind": "story_ledger", "path": surface_ledger_path }
            ]
        }),
    )?;

    let empty_batch = edge_no_write(
        &state_path,
        &run.run_dir().join("edge-empty-ledger-aster"),
        json!([]),
        "at least one lore surface",
    )?;
    let unknown_lore = {
        let mut surfaces = lore_surfaces();
        surfaces[0]["lore_entry_ids"][0] = json!("lore_missing");
        edge_no_write(
            &state_path,
            &run.run_dir().join("edge-unknown-ledger-aster"),
            surfaces,
            "references missing lore entry",
        )?
    };
    let active_content = {
        let mut surfaces = lore_surfaces();
        surfaces[1]["body_rich_text"] = json!("## Broken\n<script>alert(1)</script>");
        edge_no_write(
            &state_path,
            &run.run_dir().join("edge-active-ledger-aster"),
            surfaces,
            "active rich-text",
        )?
    };
    let missing_codex_unlock = {
        let mut surfaces = lore_surfaces();
        surfaces[4]["codex_unlock_id"] = json!(null);
        edge_no_write(
            &state_path,
            &run.run_dir().join("edge-codex-ledger-aster"),
            surfaces,
            "codex_entry surfaces require",
        )?
    };

    let expected_kinds = [
        "book",
        "note",
        "inscription",
        "npc_dialogue",
        "codex_entry",
        "region_description",
        "environmental_prop",
    ];
    let all_surface_kinds_present = expected_kinds.iter().all(|kind| {
        state
            .lore_surface_kind_counts
            .get(*kind)
            .copied()
            .unwrap_or(0)
            == 1
    });
    let report = json!({
        "schema_version": 1,
        "issue": 1051,
        "source_of_truth": {
            "state_path": state_path,
            "surface_ledger_path": surface_ledger_path,
            "state_sha256": state.sha256,
            "surface_ledger_sha256": surface_ledger.sha256,
            "fsv_aggregate_sha256": fsv_readback.aggregate_sha256,
            "research_refs": [
                "https://www.gamedeveloper.com/design/game-design-tools-narrative-graphs",
                "https://book.leveldesignbook.com/process/preproduction/worldbuilding"
            ],
            "readme_refs": [
                "docs/readme/02-storytelling.png",
                "docs/readme/GP10-dialogue.png",
                "docs/readme/GP11-quest.png",
                "docs/readme/S16-quest-designer.png"
            ]
        },
        "happy_path": {
            "tool_name": surface_output.tool_name,
            "operation_id": surface_output.operation_id,
            "state_readback": story_read.result["state_readback"],
            "surface_ledger_record": surface_ledger.ledger.records[0],
            "fsv_readback": fsv_readback,
            "surface_examples": {
                "book": "object://book/book_of_cinders",
                "npc_dialogue": "dialogue://elder_cairn/opening",
                "codex": "codex://ashen_reach/black_bell_year",
                "region": "region://ashen_reach/bell_road",
                "environment": "prop://black_bell/ash_marks"
            }
        },
        "edges": {
            "empty_batch": empty_batch,
            "unknown_lore": unknown_lore,
            "active_content": active_content,
            "missing_codex_unlock": missing_codex_unlock
        },
        "assertions": {
            "lore_entry_count": state.lore_entry_count,
            "lore_surface_count": state.lore_surface_count,
            "lore_surface_lore_ref_count": state.lore_surface_lore_ref_count,
            "lore_surface_codex_unlock_count": state.lore_surface_codex_unlock_count,
            "all_surface_kinds_present": all_surface_kinds_present,
            "surface_ledger_record_count": surface_ledger.ledger.record_count,
            "fsv_artifact_count": fsv_readback.artifact_count
        }
    });
    if state.lore_entry_count != 4
        || state.lore_surface_count != 7
        || state.lore_surface_lore_ref_count != 11
        || state.lore_surface_codex_unlock_count != 2
        || !all_surface_kinds_present
        || surface_ledger.ledger.record_count != 1
        || fsv_readback.artifact_count != 2
    {
        return Err(format!("issue1051 FSV assertions failed: {report}").into());
    }
    let report_readback = run.write_report(&report)?;
    if sha256_hex(&report_readback.bytes) != report_readback.sha256 {
        return Err("report sha256 readback mismatch".into());
    }
    println!(
        "ISSUE1051_LORE_SURFACE_FSV report={} bytes={} sha256={} state={} state_sha256={} surface_ledger={} surface_ledger_sha256={}",
        report_readback.path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        state_path.display(),
        state.sha256,
        surface_ledger_path.display(),
        surface_ledger.sha256
    );
    Ok(())
}

fn seed_lore(state_path: &Path, ledger_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    call_mcp_story_tool(
        UPSERT_LORE_ENTRY_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger_path,
            "state_id": "issue1051-lore-surface-state",
            "ledger_id": "issue1051-seed-ledger",
            "request_id": "issue1051-seed",
            "requested_by": "litd-editor-issue1051-fsv",
            "entries": lore_entries()
        }),
    )?;
    Ok(())
}

fn edge_no_write(
    state_path: &Path,
    ledger_path: &Path,
    surfaces: Value,
    expected_error: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = read_mcp_story_state(state_path)?;
    let error = call_mcp_story_tool(
        UPSERT_LORE_SURFACE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger_path,
            "state_id": "issue1051-lore-surface-state",
            "ledger_id": "issue1051-edge-ledger",
            "request_id": "issue1051-edge",
            "requested_by": "litd-editor-issue1051-fsv",
            "surfaces": surfaces
        }),
    )
    .unwrap_err();
    let after = read_mcp_story_state(state_path)?;
    let message = error.to_string();
    if before.sha256 != after.sha256 || ledger_path.exists() || !message.contains(expected_error) {
        return Err(format!("edge did not fail closed: {message}").into());
    }
    Ok(json!({
        "failed_closed": true,
        "expected_error": expected_error,
        "actual_error": message,
        "state_sha256_before": before.sha256,
        "state_sha256_after": after.sha256,
        "ledger_exists_after": ledger_path.exists()
    }))
}

fn lore_entries() -> Value {
    json!([
        entry(
            "deity_vhael",
            "deity",
            "Vhael",
            "Star-deity of ember oaths.",
            json!([])
        ),
        entry(
            "place_ashen_reach",
            "place",
            "Ashen Reach",
            "A frontier of cinder roads.",
            json!([])
        ),
        entry(
            "history_black_bell_year",
            "history",
            "Black Bell Year",
            "The year temple bells darkened.",
            json!([{
                "marker_id": "marker_black_bell",
                "label": "Black Bell Year",
                "order": 1,
                "description": "The bells darken across the Reach."
            }])
        ),
        entry(
            "entity_orren_mapless",
            "entity",
            "Orren Mapless",
            "A guide who remembers roads by scent.",
            json!([])
        )
    ])
}

fn lore_surfaces() -> Value {
    json!([
        surface(
            "surface_book_cinders",
            "book",
            "Book of Cinders",
            ["deity_vhael", "place_ashen_reach"],
            "object://book/book_of_cinders",
            Some("codex_vhael_oaths")
        ),
        surface(
            "surface_note_bell",
            "note",
            "Charred Bell Note",
            ["history_black_bell_year"],
            "object://note/charred_bell_note",
            None
        ),
        surface(
            "surface_inscription_gate",
            "inscription",
            "Gate Inscription",
            ["deity_vhael"],
            "object://inscription/cinder_gate",
            None
        ),
        surface(
            "surface_dialogue_elder",
            "npc_dialogue",
            "Elder Cairn Rumor",
            ["entity_orren_mapless", "deity_vhael"],
            "dialogue://elder_cairn/opening",
            None
        ),
        surface(
            "surface_codex_bell",
            "codex_entry",
            "Black Bell Codex",
            ["history_black_bell_year"],
            "codex://ashen_reach/black_bell_year",
            Some("codex_black_bell_year")
        ),
        surface(
            "surface_region_reach",
            "region_description",
            "Ashen Reach Region",
            ["place_ashen_reach", "history_black_bell_year"],
            "region://ashen_reach/bell_road",
            None
        ),
        surface(
            "surface_prop_black_bell",
            "environmental_prop",
            "Black Bell Prop Cue",
            ["deity_vhael", "history_black_bell_year"],
            "prop://black_bell/ash_marks",
            None
        )
    ])
}

fn entry(entry_id: &str, kind: &str, title: &str, summary: &str, timeline_markers: Value) -> Value {
    json!({
        "entry_id": entry_id,
        "kind": kind,
        "title": title,
        "summary": summary,
        "body_rich_text": format!("## {title}\n{summary}"),
        "metadata": { "issue": "1051" },
        "source_refs": ["issue_1051"],
        "tags": ["ashen_reach", "lore_surface"],
        "links": [],
        "timeline_markers": timeline_markers
    })
}

fn surface(
    surface_id: &str,
    kind: &str,
    title: &str,
    lore_entry_ids: impl serde::Serialize,
    world_ref: &str,
    codex_unlock_id: Option<&str>,
) -> Value {
    json!({
        "surface_id": surface_id,
        "kind": kind,
        "title": title,
        "body_rich_text": format!("## {title}\nA discovered Ashen Reach lore surface."),
        "excerpt": format!("{title} excerpt"),
        "lore_entry_ids": lore_entry_ids,
        "world_ref": world_ref,
        "codex_unlock_id": codex_unlock_id,
        "discovery_trigger": "inspect_or_read",
        "source_refs": ["issue_1051"],
        "tags": ["ashen_reach", "surfaced_lore"]
    })
}
