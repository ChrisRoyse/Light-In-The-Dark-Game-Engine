use std::{fs, path::Path};

use serde_json::{Value, json};

use super::*;

#[test]
fn issue1051_lore_surfaces_cover_world_codex_and_dialogue_readback() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1051_lore_surface", "unit").unwrap();
    reset_root(&root);
    let state_path = root.join("story-state-aster");
    let seed_ledger = root.join("seed-ledger-aster");
    let surface_ledger = root.join("surface-ledger-aster");
    seed_lore(&state_path, &seed_ledger);

    let output = call_mcp_story_tool(
        UPSERT_LORE_SURFACE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": surface_ledger,
            "state_id": "issue1051-lore-surface-state",
            "ledger_id": "issue1051-lore-surface-ledger",
            "request_id": "issue1051-lore-surface",
            "requested_by": "litd-editor-issue1051-test",
            "surfaces": sample_surfaces()
        }),
    )
    .unwrap();
    assert_eq!(output.tool_name, UPSERT_LORE_SURFACE_TOOL);

    let read = call_mcp_story_tool(READ_STORY_TOOL, json!({ "state_path": state_path })).unwrap();
    let state = &read.result["state_readback"];
    assert_eq!(state["lore_entry_count"], 4);
    assert_eq!(state["lore_surface_count"], 7);
    assert_eq!(state["lore_surface_lore_ref_count"], 11);
    assert_eq!(state["lore_surface_codex_unlock_count"], 2);
    for kind in [
        "book",
        "note",
        "inscription",
        "npc_dialogue",
        "codex_entry",
        "region_description",
        "environmental_prop",
    ] {
        assert_eq!(state["lore_surface_kind_counts"][kind], 1);
    }
    assert_eq!(
        state["state"]["lore_surfaces"][3]["world_ref"],
        "dialogue://elder_cairn/opening"
    );
    assert_eq!(
        state["state"]["lore_surfaces"][4]["codex_unlock_id"],
        "codex_black_bell_year"
    );

    let ledger = read_mcp_story_tool_ledger(&surface_ledger).unwrap();
    let record = &ledger.ledger.records[0];
    assert_eq!(record.tool_name, UPSERT_LORE_SURFACE_TOOL);
    assert_eq!(record.lore_surface_count, 7);
    assert_eq!(record.lore_surface_lore_ref_count, 11);
    assert_eq!(record.lore_surface_codex_unlock_count, 2);
}

#[test]
fn issue1051_lore_surface_edges_fail_closed_without_writing() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1051_lore_surface", "edges").unwrap();
    reset_root(&root);
    let state_path = root.join("story-state-aster");
    let seed_ledger = root.join("seed-ledger-aster");
    seed_lore(&state_path, &seed_ledger);

    assert_no_surface_write(
        &state_path,
        &root.join("empty-surface-ledger-aster"),
        json!([]),
        "at least one lore surface",
    );
    let mut unknown = sample_surfaces();
    unknown[0]["lore_entry_ids"][0] = json!("lore_missing");
    assert_no_surface_write(
        &state_path,
        &root.join("unknown-lore-ledger-aster"),
        unknown,
        "references missing lore entry",
    );
    let mut active = sample_surfaces();
    active[1]["body_rich_text"] = json!("## Broken\n<script>alert(1)</script>");
    assert_no_surface_write(
        &state_path,
        &root.join("active-content-ledger-aster"),
        active,
        "active rich-text",
    );
    let mut codex = sample_surfaces();
    codex[4]["codex_unlock_id"] = json!(null);
    assert_no_surface_write(
        &state_path,
        &root.join("codex-ledger-aster"),
        codex,
        "codex_entry surfaces require",
    );
}

fn seed_lore(state_path: &Path, ledger_path: &Path) {
    call_mcp_story_tool(
        UPSERT_LORE_ENTRY_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger_path,
            "state_id": "issue1051-lore-surface-state",
            "ledger_id": "issue1051-seed-ledger",
            "request_id": "issue1051-seed",
            "requested_by": "litd-editor-issue1051-test",
            "entries": lore_entries()
        }),
    )
    .unwrap();
}

fn assert_no_surface_write(state_path: &Path, ledger_path: &Path, surfaces: Value, expected: &str) {
    let before = read_mcp_story_state(state_path).unwrap();
    let error = call_mcp_story_tool(
        UPSERT_LORE_SURFACE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger_path,
            "state_id": "issue1051-lore-surface-state",
            "ledger_id": "issue1051-edge-ledger",
            "request_id": "issue1051-edge",
            "requested_by": "litd-editor-issue1051-test",
            "surfaces": surfaces
        }),
    )
    .unwrap_err();
    let after = read_mcp_story_state(state_path).unwrap();
    assert_eq!(before.sha256, after.sha256);
    assert!(!ledger_path.exists());
    assert!(error.to_string().contains(expected), "{error}");
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

fn sample_surfaces() -> Value {
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

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
