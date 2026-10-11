use std::fs;

use litd_editor::mcp_story_tools::{
    READ_STORY_TOOL, STAGE_CINEMATIC_TOOL, UPSERT_DIALOGUE_QUEST_TOOL, UPSERT_LORE_CONTEXT_TOOL,
    call_mcp_story_tool, read_mcp_story_state,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue921Fsv {
    issue: u32,
    source_of_truth: Value,
    states: Value,
    happy_path: Value,
    edges: Vec<Value>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_MCP_STORY_TOOLS_FSV_ROOT",
        "issue921_mcp_story_tools",
        "manual",
    )?;

    let state_path = root.join("story-state.json");
    let dialogue_ledger = root.join("dialogue-ledger.json");
    let cinematic_ledger = root.join("cinematic-ledger.json");
    let lore_ledger = root.join("lore-ledger.json");
    let report_path = root.join("mcp-story-tools-fsv-readback.json");
    let before = state(&state_path, &dialogue_ledger);

    let dialogue = call_mcp_story_tool(
        UPSERT_DIALOGUE_QUEST_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": dialogue_ledger,
            "state_id": "issue921-story-state",
            "ledger_id": "issue921-dialogue-ledger",
            "request_id": "issue921-dialogue",
            "requested_by": "litd-editor-issue921-fsv",
            "characters": sample_characters(),
            "dialogue": sample_dialogue(),
            "quests": [sample_quest()]
        }),
    )?;
    let cinematic = call_mcp_story_tool(
        STAGE_CINEMATIC_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": cinematic_ledger,
            "state_id": "issue921-story-state",
            "ledger_id": "issue921-cinematic-ledger",
            "request_id": "issue921-cinematic",
            "requested_by": "litd-editor-issue921-fsv",
            "cinematic": sample_cinematic()
        }),
    )?;
    let lore = call_mcp_story_tool(
        UPSERT_LORE_CONTEXT_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": lore_ledger,
            "state_id": "issue921-story-state",
            "ledger_id": "issue921-lore-ledger",
            "request_id": "issue921-lore",
            "requested_by": "litd-editor-issue921-fsv",
            "lore": sample_lore()
        }),
    )?;
    let read = call_mcp_story_tool(READ_STORY_TOOL, json!({ "state_path": state_path }))?;
    let after = state(&state_path, &dialogue_ledger);
    let edges = vec![
        edge_empty_dialogue_quest(&root, &state_path)?,
        edge_missing_dialogue_speaker(&root, &state_path)?,
        edge_missing_cinematic_dialogue(&root, &state_path)?,
        edge_ungrounded_lore(&root, &state_path)?,
    ];

    let fsv = Issue921Fsv {
        issue: 921,
        source_of_truth: json!({
            "story_state": state_path,
            "dialogue_ledger": dialogue_ledger,
            "cinematic_ledger": cinematic_ledger,
            "lore_ledger": lore_ledger
        }),
        states: json!({ "before": before, "after": after }),
        happy_path: json!({
            "upsert_dialogue_quest": dialogue,
            "stage_cinematic": cinematic,
            "upsert_lore_context": lore,
            "read_story": read
        }),
        edges,
    };
    let fsv = canonical_json_roundtrip(&fsv)?;
    write_json(&report_path, &fsv)?;
    if read_json::<Issue921Fsv>(&report_path)? != fsv {
        return Err(format!("report readback mismatch at {}", report_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue921 mcp_story_tools_fsv report={} report_sha256={} state_sha256={} dialogue_ledger_sha256={} cinematic_ledger_sha256={} lore_ledger_sha256={}",
        report_path.display(),
        report_readback.sha256,
        sha256_file(&state_path)?,
        sha256_file(&dialogue_ledger)?,
        sha256_file(&cinematic_ledger)?,
        sha256_file(&lore_ledger)?,
    );
    Ok(())
}

fn edge_empty_dialogue_quest(
    root: &std::path::Path,
    state_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-empty-dialogue-ledger.json");
    let before = state(state_path, &ledger);
    let error = call_mcp_story_tool(
        UPSERT_DIALOGUE_QUEST_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger,
            "state_id": "issue921-story-state",
            "ledger_id": "issue921-empty-dialogue",
            "request_id": "issue921-empty-dialogue",
            "requested_by": "litd-editor-issue921-fsv",
            "characters": [],
            "dialogue": null,
            "quests": []
        }),
    )
    .unwrap_err();
    edge_report(
        "empty_dialogue_quest",
        before,
        state(state_path, &ledger),
        error,
    )
}

fn edge_missing_dialogue_speaker(
    root: &std::path::Path,
    state_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-missing-speaker-ledger.json");
    let before = state(state_path, &ledger);
    let error = call_mcp_story_tool(
        UPSERT_DIALOGUE_QUEST_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger,
            "state_id": "issue921-story-state",
            "ledger_id": "issue921-missing-speaker",
            "request_id": "issue921-missing-speaker",
            "requested_by": "litd-editor-issue921-fsv",
            "characters": [],
            "dialogue": bad_dialogue_missing_speaker(),
            "quests": []
        }),
    )
    .unwrap_err();
    edge_report(
        "missing_dialogue_speaker",
        before,
        state(state_path, &ledger),
        error,
    )
}

fn edge_missing_cinematic_dialogue(
    root: &std::path::Path,
    state_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-missing-cinematic-dialogue-ledger.json");
    let before = state(state_path, &ledger);
    let error = call_mcp_story_tool(
        STAGE_CINEMATIC_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger,
            "state_id": "issue921-story-state",
            "ledger_id": "issue921-missing-cinematic-dialogue",
            "request_id": "issue921-missing-cinematic-dialogue",
            "requested_by": "litd-editor-issue921-fsv",
            "cinematic": bad_cinematic_missing_dialogue()
        }),
    )
    .unwrap_err();
    edge_report(
        "missing_cinematic_dialogue",
        before,
        state(state_path, &ledger),
        error,
    )
}

fn edge_ungrounded_lore(
    root: &std::path::Path,
    state_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-ungrounded-lore-ledger.json");
    let before = state(state_path, &ledger);
    let error = call_mcp_story_tool(
        UPSERT_LORE_CONTEXT_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger,
            "state_id": "issue921-story-state",
            "ledger_id": "issue921-ungrounded-lore",
            "request_id": "issue921-ungrounded-lore",
            "requested_by": "litd-editor-issue921-fsv",
            "lore": {
                "lore_id": "lore_bad",
                "title": "Bad Lore",
                "body": "A lore record without provenance should never persist.",
                "source_refs": [],
                "tags": ["luminary"]
            }
        }),
    )
    .unwrap_err();
    edge_report("ungrounded_lore", before, state(state_path, &ledger), error)
}

fn edge_report(
    name: &str,
    before: Value,
    after: Value,
    error: litd_engine::EngineError,
) -> Result<Value, Box<dyn std::error::Error>> {
    if before != after {
        return Err(format!("edge {name} changed source-of-truth state").into());
    }
    Ok(json!({
        "name": name,
        "state_unchanged": true,
        "before": before,
        "after": after,
        "error": error.to_string()
    }))
}

fn sample_characters() -> Value {
    json!([
        {
            "character_id": "char_elder_cairn",
            "display_name": "Elder Cairn",
            "role": "keeper",
            "voice_tags": ["ancient", "weary"]
        },
        {
            "character_id": "char_hero",
            "display_name": "Wayfarer",
            "role": "player",
            "voice_tags": ["direct", "curious"]
        }
    ])
}

fn sample_dialogue() -> Value {
    json!({
        "dialogue_id": "dlg_elder_cairn_intro",
        "title": "Elder Cairn Intro",
        "nodes": [
            {
                "node_id": "start",
                "speaker_id": "char_elder_cairn",
                "line": "The shadow deepens, traveler. What do you seek?",
                "choices": [
                    {
                        "choice_id": "ask_relic",
                        "text": "I search for the Lost Relic.",
                        "target_node_id": "relic",
                        "conditions": [],
                        "effects": ["quest_offer_lost_luminary"]
                    }
                ],
                "quest_effects": []
            },
            {
                "node_id": "relic",
                "speaker_id": "char_elder_cairn",
                "line": "Then follow the blue runes to the silver grove.",
                "choices": [],
                "quest_effects": ["quest_start_lost_luminary"]
            }
        ]
    })
}

fn sample_quest() -> Value {
    json!({
        "quest_id": "quest_lost_luminary",
        "title": "The Lost Luminary",
        "start_dialogue_id": "dlg_elder_cairn_intro",
        "completion_dialogue_id": null,
        "objectives": [
            {
                "objective_id": "find_elder",
                "description": "Locate Elder Morven in the Silverleaf Grove.",
                "target_ref": "npc_elder_morven",
                "required_count": 1
            },
            {
                "objective_id": "collect_starshards",
                "description": "Collect five starshards.",
                "target_ref": "item_starshard",
                "required_count": 5
            }
        ],
        "rewards": ["xp_250", "relic_hint"]
    })
}

fn sample_cinematic() -> Value {
    json!({
        "cinematic_id": "cine_luminary_reveal",
        "title": "Luminary Reveal",
        "beats": [
            {
                "beat_id": "wide_ruins",
                "start_ms": 0,
                "duration_ms": 4000,
                "camera": "wide_ruins",
                "mood": "awe",
                "staging": "hero_kneels",
                "expression": "awe",
                "dialogue_id": "dlg_elder_cairn_intro"
            },
            {
                "beat_id": "spirit_close",
                "start_ms": 4000,
                "duration_ms": 3000,
                "camera": "spirit_close",
                "mood": "sorrow",
                "staging": "spirit_appears",
                "expression": "sorrow",
                "dialogue_id": "dlg_elder_cairn_intro"
            }
        ]
    })
}

fn sample_lore() -> Value {
    json!({
        "lore_id": "lore_lost_luminary",
        "title": "The Lost Luminary",
        "body": "A moonlit relic remembered by Elder Cairn and sought in Silverleaf Grove.",
        "source_refs": ["source_elder_cairn_manual"],
        "tags": ["luminary", "silverleaf"]
    })
}

fn bad_dialogue_missing_speaker() -> Value {
    let mut value = sample_dialogue();
    value["nodes"][0]["speaker_id"] = json!("char_missing");
    value
}

fn bad_cinematic_missing_dialogue() -> Value {
    let mut value = sample_cinematic();
    value["beats"][0]["dialogue_id"] = json!("dlg_missing");
    value
}

fn state(state_path: &std::path::Path, ledger_path: &std::path::Path) -> Value {
    let readback = if state_path.exists() {
        Some(read_mcp_story_state(state_path).unwrap())
    } else {
        None
    };
    json!({
        "state_exists": state_path.exists(),
        "state_bytes": fs::metadata(state_path).map(|metadata| metadata.len()).unwrap_or(0),
        "character_count": readback.as_ref().map(|readback| readback.character_count).unwrap_or(0),
        "dialogue_count": readback.as_ref().map(|readback| readback.dialogue_count).unwrap_or(0),
        "dialogue_node_count": readback.as_ref().map(|readback| readback.dialogue_node_count).unwrap_or(0),
        "quest_count": readback.as_ref().map(|readback| readback.quest_count).unwrap_or(0),
        "quest_objective_count": readback.as_ref().map(|readback| readback.quest_objective_count).unwrap_or(0),
        "cinematic_count": readback.as_ref().map(|readback| readback.cinematic_count).unwrap_or(0),
        "cinematic_beat_count": readback.as_ref().map(|readback| readback.cinematic_beat_count).unwrap_or(0),
        "lore_context_count": readback.as_ref().map(|readback| readback.lore_context_count).unwrap_or(0),
        "ledger_exists": ledger_path.exists(),
        "ledger_bytes": fs::metadata(ledger_path).map(|metadata| metadata.len()).unwrap_or(0)
    })
}

fn canonical_json_roundtrip<T>(value: &T) -> Result<T, Box<dyn std::error::Error>>
where
    T: Serialize + DeserializeOwned,
{
    let bytes = serde_json::to_vec_pretty(value)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn write_json<T: Serialize>(
    path: &std::path::Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &std::path::Path) -> Result<T, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn sha256_file(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}
