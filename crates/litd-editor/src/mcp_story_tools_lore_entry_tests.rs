use std::{fs, path::Path};

use serde_json::json;

use super::*;

#[test]
fn issue1049_lore_entries_author_world_bible_and_read_real_state() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1049_lore_entries", "unit").unwrap();
    reset_root(&root);
    let state_path = root.join("story-state-aster");
    let ledger_path = root.join("lore-entry-ledger-aster");

    let output = call_mcp_story_tool(
        UPSERT_LORE_ENTRY_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger_path,
            "state_id": "issue1049-world-bible-state",
            "ledger_id": "issue1049-lore-entry-ledger",
            "request_id": "issue1049-world-bible",
            "requested_by": "litd-editor-issue1049-test",
            "entries": sample_world_bible_entries()
        }),
    )
    .unwrap();
    assert_eq!(output.tool_name, UPSERT_LORE_ENTRY_TOOL);

    let read = call_mcp_story_tool(READ_STORY_TOOL, json!({ "state_path": state_path })).unwrap();
    let state = &read.result["state_readback"];
    assert_eq!(state["lore_entry_count"], 10);
    assert_eq!(state["lore_entry_link_count"], 9);
    assert_eq!(state["lore_timeline_marker_count"], 2);
    assert_eq!(state["lore_kind_counts"]["deity"], 1);
    assert_eq!(state["lore_kind_counts"]["timeline"], 1);
    assert_eq!(
        state["state"]["lore_entries"][0]["body_rich_text"],
        "## Vhael\nA quiet star-deity whose ember laws shape oaths in the Ashen Reach."
    );
    let ledger = read_mcp_story_tool_ledger(&ledger_path).unwrap();
    assert_eq!(ledger.ledger.records[0].tool_name, UPSERT_LORE_ENTRY_TOOL);
    assert_eq!(ledger.ledger.records[0].lore_entry_count, 10);
    assert_eq!(ledger.ledger.records[0].lore_entry_link_count, 9);
}

#[test]
fn issue1049_lore_entries_edges_fail_closed_without_writing() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1049_lore_entries", "edges").unwrap();
    reset_root(&root);
    let state_path = root.join("story-state-aster");
    let seed_ledger = root.join("seed-ledger-aster");
    seed_world_bible(&state_path, &seed_ledger);
    let edge_ledger = root.join("edge-ledger-aster");

    assert_edge_no_write(
        &state_path,
        &edge_ledger,
        json!([]),
        "at least one lore entry",
    );
    let mut dangling = sample_world_bible_entries();
    dangling[0]["links"][0]["target_entry_id"] = json!("place_missing");
    assert_edge_no_write(
        &state_path,
        &edge_ledger,
        dangling,
        "links to missing lore entry",
    );
    let mut active = sample_world_bible_entries();
    active[0]["body_rich_text"] = json!("## Broken\n<script>alert(1)</script>");
    assert_edge_no_write(&state_path, &edge_ledger, active, "active rich-text");
    let mut timeline = sample_world_bible_entries();
    timeline[8]["timeline_markers"] = json!([]);
    assert_edge_no_write(&state_path, &edge_ledger, timeline, "timeline and history");
}

fn seed_world_bible(state_path: &Path, ledger_path: &Path) {
    call_mcp_story_tool(
        UPSERT_LORE_ENTRY_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger_path,
            "state_id": "issue1049-world-bible-state",
            "ledger_id": "issue1049-seed-ledger",
            "request_id": "issue1049-seed",
            "requested_by": "litd-editor-issue1049-test",
            "entries": sample_world_bible_entries()
        }),
    )
    .unwrap();
}

fn assert_edge_no_write(
    state_path: &Path,
    ledger_path: &Path,
    entries: serde_json::Value,
    expected: &str,
) {
    let before = state(state_path, ledger_path);
    let error = call_mcp_story_tool(
        UPSERT_LORE_ENTRY_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger_path,
            "state_id": "issue1049-world-bible-state",
            "ledger_id": "issue1049-edge-ledger",
            "request_id": "issue1049-edge",
            "requested_by": "litd-editor-issue1049-test",
            "entries": entries
        }),
    )
    .unwrap_err();
    let after = state(state_path, ledger_path);
    assert_eq!(before, after);
    assert!(error.to_string().contains(expected), "{error}");
}

fn sample_world_bible_entries() -> serde_json::Value {
    json!([
        entry(EntryArgs {
            entry_id: "deity_vhael",
            kind: "deity",
            title: "Vhael",
            summary: "Star-deity of ember oaths.",
            body_rich_text: "## Vhael\nA quiet star-deity whose ember laws shape oaths in the Ashen Reach.",
            meta_key: "domain",
            meta_value: "embers",
            links: [link(
                "place_ashen_reach",
                "watches",
                "Vhael watches the Reach through ash-stars."
            )],
            timeline_markers: json!([]),
        }),
        entry(EntryArgs {
            entry_id: "cabal_sable_choir",
            kind: "cabal",
            title: "Sable Choir",
            summary: "A hidden cabal of ash-singers.",
            body_rich_text: "## Sable Choir\nThe Choir records forbidden hymns in smoke-glass vaults.",
            meta_key: "agenda",
            meta_value: "memory",
            links: [link(
                "deity_vhael",
                "serves",
                "The Choir claims its songs preserve Vhael's law."
            )],
            timeline_markers: json!([]),
        }),
        entry(EntryArgs {
            entry_id: "faction_ember_crown",
            kind: "faction",
            title: "Ember Crown",
            summary: "A claimant faction seeking ancient regalia.",
            body_rich_text: "## Ember Crown\nCrownward houses gather relic taxes from road settlements.",
            meta_key: "stance",
            meta_value: "claimant",
            links: [link(
                "artifact_cinder_astrolabe",
                "seeks",
                "The Crown wants the astrolabe as proof of mandate."
            )],
            timeline_markers: json!([]),
        }),
        entry(EntryArgs {
            entry_id: "race_glassborn",
            kind: "race",
            title: "Glassborn",
            summary: "A people marked by translucent ash-veins.",
            body_rich_text: "## Glassborn\nGlassborn artisans read stormlight trapped inside cinder panes.",
            meta_key: "origin",
            meta_value: "ashfall",
            links: [link(
                "place_ashen_reach",
                "inhabits",
                "Glassborn enclaves sit along the Reach's broken roads."
            )],
            timeline_markers: json!([]),
        }),
        entry(EntryArgs {
            entry_id: "place_ashen_reach",
            kind: "place",
            title: "Ashen Reach",
            summary: "A wind-scoured frontier of cinder roads.",
            body_rich_text: "## Ashen Reach\nBlack rain and ember roads divide the last bright settlements.",
            meta_key: "region",
            meta_value: "frontier",
            links: json!([]),
            timeline_markers: json!([]),
        }),
        entry(EntryArgs {
            entry_id: "event_first_ashen_rain",
            kind: "event",
            title: "First Ashen Rain",
            summary: "The first skyfall that changed the Reach.",
            body_rich_text: "## First Ashen Rain\nThe rain made maps unreliable and turned temple bells black.",
            meta_key: "era",
            meta_value: "rainfall",
            links: [link(
                "culture_roadwake",
                "shaped",
                "Roadwake rites began after the first black rain."
            )],
            timeline_markers: json!([]),
        }),
        entry(EntryArgs {
            entry_id: "artifact_cinder_astrolabe",
            kind: "artifact",
            title: "Cinder Astrolabe",
            summary: "A relic that maps ember constellations.",
            body_rich_text: "## Cinder Astrolabe\nIts rings click only when a sworn oath is broken nearby.",
            meta_key: "material",
            meta_value: "smoke_bronze",
            links: [link(
                "deity_vhael",
                "attuned_to",
                "The astrolabe answers Vhael's ember laws."
            )],
            timeline_markers: json!([]),
        }),
        entry(EntryArgs {
            entry_id: "culture_roadwake",
            kind: "culture",
            title: "Roadwake",
            summary: "Funerary road culture of caravan kin.",
            body_rich_text: "## Roadwake\nRoadwake families burn cedar wheels for the dead at dawn.",
            meta_key: "rite",
            meta_value: "wheel_pyres",
            links: [link(
                "place_ashen_reach",
                "belongs_to",
                "Roadwake rites mark safe paths through the Reach."
            )],
            timeline_markers: json!([]),
        }),
        entry(EntryArgs {
            entry_id: "timeline_fall_of_day",
            kind: "timeline",
            title: "Fall of Day",
            summary: "Timeline of the Reach's dimming.",
            body_rich_text: "## Fall of Day\nA living chronology used by scribes to align ruins and reigns.",
            meta_key: "span",
            meta_value: "three_eras",
            links: [link(
                "event_first_ashen_rain",
                "contains",
                "The first ash rain begins the dimming calendar."
            )],
            timeline_markers: [
                marker("marker_rain", "First Ashen Rain", 1),
                marker("marker_crown", "Ember Crown Rises", 2)
            ],
        }),
        entry(EntryArgs {
            entry_id: "entity_orren_mapless",
            kind: "entity",
            title: "Orren Mapless",
            summary: "A guide who remembers roads by scent.",
            body_rich_text: "## Orren Mapless\nOrren carries no charts and refuses Crown coin.",
            meta_key: "profession",
            meta_value: "guide",
            links: [link(
                "culture_roadwake",
                "raised_by",
                "Roadwake caravans taught Orren the mourning roads."
            )],
            timeline_markers: json!([]),
        })
    ])
}

struct EntryArgs<'a, T0: serde::Serialize, T1: serde::Serialize> {
    entry_id: &'a str,
    kind: &'a str,
    title: &'a str,
    summary: &'a str,
    body_rich_text: &'a str,
    meta_key: &'a str,
    meta_value: &'a str,
    links: T0,
    timeline_markers: T1,
}

fn entry(args: EntryArgs<'_, impl serde::Serialize, impl serde::Serialize>) -> serde_json::Value {
    let EntryArgs {
        entry_id,
        kind,
        title,
        summary,
        body_rich_text,
        meta_key,
        meta_value,
        links,
        timeline_markers,
    } = args;
    json!({
        "entry_id": entry_id,
        "kind": kind,
        "title": title,
        "summary": summary,
        "body_rich_text": body_rich_text,
        "metadata": { meta_key: meta_value },
        "source_refs": ["issue_1048", "issue_1049"],
        "tags": ["ashen_reach", "world_bible"],
        "links": links,
        "timeline_markers": timeline_markers
    })
}

fn link(target_entry_id: &str, relation: &str, description: &str) -> serde_json::Value {
    json!({
        "target_entry_id": target_entry_id,
        "relation": relation,
        "description": description
    })
}

fn marker(marker_id: &str, label: &str, order: i32) -> serde_json::Value {
    json!({
        "marker_id": marker_id,
        "label": label,
        "order": order,
        "description": format!("{label} chronology marker")
    })
}

fn state(state_path: &Path, ledger_path: &Path) -> serde_json::Value {
    let readback = if state_path.exists() {
        Some(read_mcp_story_state(state_path).unwrap())
    } else {
        None
    };
    json!({
        "state_exists": state_path.exists(),
        "state_bytes": fs::metadata(state_path).map(|metadata| metadata.len()).unwrap_or(0),
        "lore_entry_count": readback.as_ref().map(|readback| readback.lore_entry_count).unwrap_or(0),
        "lore_entry_link_count": readback.as_ref().map(|readback| readback.lore_entry_link_count).unwrap_or(0),
        "lore_timeline_marker_count": readback.as_ref().map(|readback| readback.lore_timeline_marker_count).unwrap_or(0),
        "ledger_exists": ledger_path.exists(),
        "ledger_bytes": fs::metadata(ledger_path).map(|metadata| metadata.len()).unwrap_or(0)
    })
}

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
