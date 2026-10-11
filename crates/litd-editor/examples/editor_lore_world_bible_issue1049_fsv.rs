use std::path::Path;

use litd_editor::{
    mcp_fsv_tools::{READ_STORY_RENDER_STATE_TOOL, call_mcp_fsv_tool},
    mcp_story_tools::{
        READ_STORY_TOOL, UPSERT_LORE_ENTRY_TOOL, call_mcp_story_tool, read_mcp_story_state,
        read_mcp_story_tool_ledger,
    },
};
use litd_engine::fsv_artifacts::begin_issue_run;
use litd_hash::sha256_hex;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let run = begin_issue_run("issue1049_lore_world_bible")?;
    let state_path = run.run_dir().join("story-state-aster");
    let ledger_path = run.run_dir().join("story-ledger-aster");

    let entries = world_bible_entries();
    let upsert = call_mcp_story_tool(
        UPSERT_LORE_ENTRY_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger_path,
            "state_id": "issue1049-world-bible-state",
            "ledger_id": "issue1049-lore-world-bible-ledger",
            "request_id": "issue1049-lore-world-bible",
            "requested_by": "litd-editor-issue1049-fsv",
            "entries": entries
        }),
    )?;
    let story_read = call_mcp_story_tool(READ_STORY_TOOL, json!({ "state_path": state_path }))?;
    let state = read_mcp_story_state(&state_path)?;
    let ledger = read_mcp_story_tool_ledger(&ledger_path)?;
    let fsv_readback = call_mcp_fsv_tool(
        READ_STORY_RENDER_STATE_TOOL,
        json!({
            "artifacts": [
                { "label": "issue1049_story_state", "kind": "story_state", "path": state_path },
                { "label": "issue1049_story_ledger", "kind": "story_ledger", "path": ledger_path }
            ]
        }),
    )?;
    let edge_dangling = edge_no_write(
        &state_path,
        &run.run_dir().join("edge-dangling-ledger-aster"),
        dangling_link_entries(),
        "links to missing lore entry",
    )?;
    let edge_active = edge_no_write(
        &state_path,
        &run.run_dir().join("edge-active-ledger-aster"),
        active_content_entries(),
        "active rich-text",
    )?;

    let requested_kinds = [
        "entity", "deity", "cabal", "faction", "race", "place", "event", "artifact", "timeline",
        "history", "culture",
    ];
    let all_kinds_present = requested_kinds
        .iter()
        .all(|kind| state.lore_kind_counts.get(*kind).copied().unwrap_or(0) == 1);
    let report = json!({
        "schema_version": 1,
        "issue": 1049,
        "source_of_truth": {
            "state_path": state_path,
            "ledger_path": ledger_path,
            "state_sha256": state.sha256,
            "ledger_sha256": ledger.sha256,
            "fsv_aggregate_sha256": fsv_readback.aggregate_sha256,
            "research_refs": [
                "https://www.gamedeveloper.com/design/building-a-basic-story-bible-for-your-game",
                "https://cleardemon.com/organising-data-in-a-narrative-video-game-6697c99a4f13",
                "https://book.leveldesignbook.com/process/preproduction/worldbuilding",
                "https://twinery.org/"
            ],
            "readme_refs": [
                "docs/readme/02-storytelling.png",
                "docs/readme/GP10-dialogue.png",
                "docs/readme/GP11-quest.png",
                "docs/readme/S16-quest-designer.png"
            ]
        },
        "happy_path": {
            "tool_name": upsert.tool_name,
            "operation_id": upsert.operation_id,
            "state_readback": story_read.result["state_readback"],
            "fsv_readback": fsv_readback,
            "ledger_record": ledger.ledger.records[0],
        },
        "edges": {
            "dangling_link": edge_dangling,
            "active_content": edge_active,
        },
        "assertions": {
            "all_requested_kinds_present": all_kinds_present,
            "lore_entry_count": state.lore_entry_count,
            "lore_entry_link_count": state.lore_entry_link_count,
            "lore_timeline_marker_count": state.lore_timeline_marker_count,
            "ledger_record_count": ledger.ledger.record_count,
            "fsv_artifact_count": fsv_readback.artifact_count,
        }
    });
    if !all_kinds_present
        || state.lore_entry_count != 11
        || state.lore_entry_link_count != 10
        || state.lore_timeline_marker_count != 3
        || ledger.ledger.record_count != 1
        || fsv_readback.artifact_count != 2
    {
        return Err(format!("issue1049 FSV assertions failed: {report}").into());
    }
    let report_readback = run.write_report(&report)?;
    let report_sha256 = sha256_hex(&report_readback.bytes);
    if report_sha256 != report_readback.sha256 {
        return Err("report sha256 readback mismatch".into());
    }
    println!(
        "ISSUE1049_LORE_WORLD_BIBLE_FSV report={} bytes={} sha256={} state={} state_sha256={} ledger={} ledger_sha256={} fsv_aggregate={}",
        report_readback.path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        state_path.display(),
        state.sha256,
        ledger_path.display(),
        ledger.sha256,
        fsv_readback.aggregate_sha256
    );
    Ok(())
}

fn edge_no_write(
    state_path: &Path,
    ledger_path: &Path,
    entries: Value,
    expected_error: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before = read_mcp_story_state(state_path)?;
    let error = call_mcp_story_tool(
        UPSERT_LORE_ENTRY_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger_path,
            "state_id": "issue1049-world-bible-state",
            "ledger_id": "issue1049-edge-ledger",
            "request_id": "issue1049-edge",
            "requested_by": "litd-editor-issue1049-fsv",
            "entries": entries
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
        "ledger_exists_after": ledger_path.exists(),
    }))
}

fn dangling_link_entries() -> Value {
    let mut entries = world_bible_entries();
    entries[0]["links"][0]["target_entry_id"] = json!("place_missing");
    entries
}

fn active_content_entries() -> Value {
    let mut entries = world_bible_entries();
    entries[0]["body_rich_text"] = json!("## Broken\n<script>alert(1)</script>");
    entries
}

fn world_bible_entries() -> Value {
    json!([
        entry(
            "deity_vhael",
            "deity",
            "Vhael",
            "Star-deity of ember oaths.",
            "## Vhael\nA quiet star-deity whose ember laws shape oaths in the Ashen Reach.",
            "domain",
            "embers",
            [link(
                "place_ashen_reach",
                "watches",
                "Vhael watches the Reach through ash-stars."
            )],
            json!([])
        ),
        entry(
            "cabal_sable_choir",
            "cabal",
            "Sable Choir",
            "A hidden cabal of ash-singers.",
            "## Sable Choir\nThe Choir records forbidden hymns in smoke-glass vaults.",
            "agenda",
            "memory",
            [link(
                "deity_vhael",
                "serves",
                "The Choir claims its songs preserve Vhael's law."
            )],
            json!([])
        ),
        entry(
            "faction_ember_crown",
            "faction",
            "Ember Crown",
            "A claimant faction seeking ancient regalia.",
            "## Ember Crown\nCrownward houses gather relic taxes from road settlements.",
            "stance",
            "claimant",
            [link(
                "artifact_cinder_astrolabe",
                "seeks",
                "The Crown wants the astrolabe as proof of mandate."
            )],
            json!([])
        ),
        entry(
            "race_glassborn",
            "race",
            "Glassborn",
            "A people marked by translucent ash-veins.",
            "## Glassborn\nGlassborn artisans read stormlight trapped inside cinder panes.",
            "origin",
            "ashfall",
            [link(
                "place_ashen_reach",
                "inhabits",
                "Glassborn enclaves sit along the Reach's broken roads."
            )],
            json!([])
        ),
        entry(
            "place_ashen_reach",
            "place",
            "Ashen Reach",
            "A wind-scoured frontier of cinder roads.",
            "## Ashen Reach\nBlack rain and ember roads divide the last bright settlements.",
            "region",
            "frontier",
            json!([]),
            json!([])
        ),
        entry(
            "event_first_ashen_rain",
            "event",
            "First Ashen Rain",
            "The first skyfall that changed the Reach.",
            "## First Ashen Rain\nThe rain made maps unreliable and turned temple bells black.",
            "era",
            "rainfall",
            [link(
                "culture_roadwake",
                "shaped",
                "Roadwake rites began after the first black rain."
            )],
            json!([])
        ),
        entry(
            "artifact_cinder_astrolabe",
            "artifact",
            "Cinder Astrolabe",
            "A relic that maps ember constellations.",
            "## Cinder Astrolabe\nIts rings click only when a sworn oath is broken nearby.",
            "material",
            "smoke_bronze",
            [link(
                "deity_vhael",
                "attuned_to",
                "The astrolabe answers Vhael's ember laws."
            )],
            json!([])
        ),
        entry(
            "timeline_fall_of_day",
            "timeline",
            "Fall of Day",
            "Timeline of the Reach's dimming.",
            "## Fall of Day\nA living chronology used by scribes to align ruins and reigns.",
            "span",
            "three_eras",
            [link(
                "event_first_ashen_rain",
                "contains",
                "The first ash rain begins the dimming calendar."
            )],
            [
                marker("marker_rain", "First Ashen Rain", 1),
                marker("marker_crown", "Ember Crown Rises", 2)
            ]
        ),
        entry(
            "history_black_bell_year",
            "history",
            "Black Bell Year",
            "A history entry for the year temple bells darkened.",
            "## Black Bell Year\nPriests marked the year when every bronze bell took ash into its voice.",
            "chronicle",
            "black_bell",
            [link(
                "timeline_fall_of_day",
                "belongs_to",
                "The Black Bell Year is part of the dimming chronology."
            )],
            [marker("marker_black_bell", "Black Bell Year", 1)]
        ),
        entry(
            "culture_roadwake",
            "culture",
            "Roadwake",
            "Funerary road culture of caravan kin.",
            "## Roadwake\nRoadwake families burn cedar wheels for the dead at dawn.",
            "rite",
            "wheel_pyres",
            [link(
                "place_ashen_reach",
                "belongs_to",
                "Roadwake rites mark safe paths through the Reach."
            )],
            json!([])
        ),
        entry(
            "entity_orren_mapless",
            "entity",
            "Orren Mapless",
            "A guide who remembers roads by scent.",
            "## Orren Mapless\nOrren carries no charts and refuses Crown coin.",
            "profession",
            "guide",
            [link(
                "culture_roadwake",
                "raised_by",
                "Roadwake caravans taught Orren the mourning roads."
            )],
            json!([])
        )
    ])
}

// Positional fixture constructor for a lore-bible entry Value; struct-of-args would duplicate fields.
#[allow(clippy::too_many_arguments)]
fn entry(
    entry_id: &str,
    kind: &str,
    title: &str,
    summary: &str,
    body_rich_text: &str,
    meta_key: &str,
    meta_value: &str,
    links: impl serde::Serialize,
    timeline_markers: impl serde::Serialize,
) -> Value {
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

fn link(target_entry_id: &str, relation: &str, description: &str) -> Value {
    json!({
        "target_entry_id": target_entry_id,
        "relation": relation,
        "description": description
    })
}

fn marker(marker_id: &str, label: &str, order: i32) -> Value {
    json!({
        "marker_id": marker_id,
        "label": label,
        "order": order,
        "description": format!("{label} chronology marker")
    })
}
