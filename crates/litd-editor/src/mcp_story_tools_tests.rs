use std::{fs, path::Path};

use serde_json::json;

use super::*;

#[test]
fn issue921_story_mcp_tools_author_story_and_read_real_state() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue921_mcp_story_tools", "unit").unwrap();
    reset_root(&root);
    let state_path = root.join("story-state.json");
    let dialogue_ledger = root.join("dialogue-ledger.json");
    let cinematic_ledger = root.join("cinematic-ledger.json");
    let lore_ledger = root.join("lore-ledger.json");
    let before = state(&state_path, &dialogue_ledger);
    println!("issue921 before {before}");

    let dialogue = call_mcp_story_tool(
        UPSERT_DIALOGUE_QUEST_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": dialogue_ledger,
            "state_id": "issue921-story-state",
            "ledger_id": "issue921-dialogue-ledger",
            "request_id": "issue921-dialogue",
            "requested_by": "litd-editor-issue921-test",
            "characters": sample_characters(),
            "dialogue": sample_dialogue(),
            "quests": [sample_quest()]
        }),
    )
    .unwrap();
    assert_eq!(dialogue.tool_name, UPSERT_DIALOGUE_QUEST_TOOL);
    let cinematic = call_mcp_story_tool(
        STAGE_CINEMATIC_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": cinematic_ledger,
            "state_id": "issue921-story-state",
            "ledger_id": "issue921-cinematic-ledger",
            "request_id": "issue921-cinematic",
            "requested_by": "litd-editor-issue921-test",
            "cinematic": sample_cinematic()
        }),
    )
    .unwrap();
    assert_eq!(cinematic.tool_name, STAGE_CINEMATIC_TOOL);
    let lore = call_mcp_story_tool(
        UPSERT_LORE_CONTEXT_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": lore_ledger,
            "state_id": "issue921-story-state",
            "ledger_id": "issue921-lore-ledger",
            "request_id": "issue921-lore",
            "requested_by": "litd-editor-issue921-test",
            "lore": sample_lore()
        }),
    )
    .unwrap();
    assert_eq!(lore.tool_name, UPSERT_LORE_CONTEXT_TOOL);

    let read = call_mcp_story_tool(READ_STORY_TOOL, json!({ "state_path": state_path })).unwrap();
    assert_eq!(read.result["state_readback"]["character_count"], 2);
    assert_eq!(read.result["state_readback"]["dialogue_count"], 1);
    assert_eq!(read.result["state_readback"]["quest_count"], 1);
    assert_eq!(read.result["state_readback"]["cinematic_count"], 1);
    assert_eq!(read.result["state_readback"]["lore_context_count"], 1);
    let after = state(&state_path, &dialogue_ledger);
    println!("issue921 after {after}");
    assert_eq!(after["character_count"], 2);
    assert_eq!(after["dialogue_node_count"], 2);
    assert_eq!(after["quest_objective_count"], 2);
    assert_eq!(after["cinematic_beat_count"], 2);
}

#[test]
fn issue921_story_mcp_tools_edges_fail_closed_without_writing() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue921_mcp_story_tools", "edges").unwrap();
    reset_root(&root);
    let state_path = root.join("story-state.json");
    seed_story_state(&root, &state_path);
    let edge_ledger = root.join("edge-ledger.json");

    let before_bad_dialogue = state(&state_path, &edge_ledger);
    let bad_dialogue = call_mcp_story_tool(
        UPSERT_DIALOGUE_QUEST_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": edge_ledger,
            "state_id": "issue921-story-state",
            "ledger_id": "issue921-bad-dialogue",
            "request_id": "issue921-bad-dialogue",
            "requested_by": "litd-editor-issue921-test",
            "characters": [],
            "dialogue": bad_dialogue_missing_speaker(),
            "quests": []
        }),
    )
    .unwrap_err();
    let after_bad_dialogue = state(&state_path, &edge_ledger);
    println!(
        "issue921 bad_dialogue before={before_bad_dialogue} after={after_bad_dialogue} error={bad_dialogue}"
    );
    assert_eq!(before_bad_dialogue, after_bad_dialogue);
    assert!(bad_dialogue.to_string().contains("missing character"));

    let before_bad_cinematic = state(&state_path, &edge_ledger);
    let bad_cinematic = call_mcp_story_tool(
        STAGE_CINEMATIC_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": edge_ledger,
            "state_id": "issue921-story-state",
            "ledger_id": "issue921-bad-cinematic",
            "request_id": "issue921-bad-cinematic",
            "requested_by": "litd-editor-issue921-test",
            "cinematic": bad_cinematic_missing_dialogue()
        }),
    )
    .unwrap_err();
    let after_bad_cinematic = state(&state_path, &edge_ledger);
    println!(
        "issue921 bad_cinematic before={before_bad_cinematic} after={after_bad_cinematic} error={bad_cinematic}"
    );
    assert_eq!(before_bad_cinematic, after_bad_cinematic);
    assert!(bad_cinematic.to_string().contains("missing dialogue"));

    let before_bad_lore = state(&state_path, &edge_ledger);
    let bad_lore = call_mcp_story_tool(
        UPSERT_LORE_CONTEXT_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": edge_ledger,
            "state_id": "issue921-story-state",
            "ledger_id": "issue921-bad-lore",
            "request_id": "issue921-bad-lore",
            "requested_by": "litd-editor-issue921-test",
            "lore": {
                "lore_id": "lore_bad",
                "title": "Bad Lore",
                "body": "No source refs.",
                "source_refs": [],
                "tags": ["cairn"]
            }
        }),
    )
    .unwrap_err();
    let after_bad_lore = state(&state_path, &edge_ledger);
    println!("issue921 bad_lore before={before_bad_lore} after={after_bad_lore} error={bad_lore}");
    assert_eq!(before_bad_lore, after_bad_lore);
    assert!(bad_lore.to_string().contains("source"));
}

fn seed_story_state(root: &Path, state_path: &Path) {
    call_mcp_story_tool(
        UPSERT_DIALOGUE_QUEST_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": root.join("seed-dialogue-ledger.json"),
            "state_id": "issue921-story-state",
            "ledger_id": "issue921-seed-dialogue",
            "request_id": "issue921-seed-dialogue",
            "requested_by": "litd-editor-issue921-test",
            "characters": sample_characters(),
            "dialogue": sample_dialogue(),
            "quests": [sample_quest()]
        }),
    )
    .unwrap();
}

#[test]
fn issue1518_story_tool_rejects_out_of_root_paths_without_writing() {
    // #1518 confinement close gate for the story family (non-ORT tool): a poisoned state_path
    // (absolute-escape) and a ..-traversal ledger_path are both rejected before any fs effect.
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue1518_mcp_story_tools", "confine").unwrap();
    reset_root(&root);
    let ledger_path = root.join("issue1518-ledger.json");

    // --- EDGE 1: absolute state_path OUTSIDE the workspace root ---
    // #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
    let _temp_root = litd_engine::test_temp::TestTempDir::new("issue1518-story");
    let escape_dir = _temp_root.path().to_path_buf();
    if escape_dir.exists() {
        fs::remove_dir_all(&escape_dir).ok();
    }
    let escape_state = escape_dir.join("story-state.json");
    let e1 = call_mcp_story_tool(
        UPSERT_DIALOGUE_QUEST_TOOL,
        json!({
            "state_path": escape_state,
            "ledger_path": ledger_path,
            "state_id": "issue1518-story-state",
            "ledger_id": "issue1518-ledger",
            "request_id": "issue1518-escape",
            "requested_by": "litd-editor-issue1518-test",
            "characters": sample_characters(),
            "dialogue": sample_dialogue(),
            "quests": [sample_quest()]
        }),
    )
    .unwrap_err()
    .to_string();
    println!("issue1518 story absolute-escape rejected: {e1}");
    assert!(
        e1.contains("outside the project root"),
        "expected outside-root rejection, got: {e1}"
    );
    assert!(!escape_dir.exists());

    // --- EDGE 2: ..-traversal ledger_path ---
    let e2 = call_mcp_story_tool(
        UPSERT_DIALOGUE_QUEST_TOOL,
        json!({
            "state_path": root.join("story-state.json"),
            "ledger_path": "led/../../litd-issue1518-story-traversal",
            "state_id": "issue1518-story-state",
            "ledger_id": "issue1518-ledger",
            "request_id": "issue1518-traversal",
            "requested_by": "litd-editor-issue1518-test",
            "characters": sample_characters(),
            "dialogue": sample_dialogue(),
            "quests": [sample_quest()]
        }),
    )
    .unwrap_err()
    .to_string();
    println!("issue1518 story traversal rejected: {e2}");
    assert!(
        e2.contains("parent-directory"),
        "expected traversal rejection, got: {e2}"
    );
    assert!(!Path::new("../litd-issue1518-story-traversal").exists());
    assert!(
        !root.join("story-state.json").exists(),
        "confinement breach: state written despite a rejected ledger_path"
    );
}

fn sample_characters() -> serde_json::Value {
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

fn sample_dialogue() -> serde_json::Value {
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

fn sample_quest() -> serde_json::Value {
    json!({
        "quest_id": "quest_lost_luminary",
        "title": "The Lost Luminary",
        "start_dialogue_id": "dlg_elder_cairn_intro",
        "completion_dialogue_id": null,
        "objectives": [
            { "objective_id": "find_elder", "description": "Locate Elder Morven in the Silverleaf Grove.", "target_ref": "npc_elder_morven", "required_count": 1 },
            { "objective_id": "collect_starshards", "description": "Collect five starshards.", "target_ref": "item_starshard", "required_count": 5 }
        ],
        "rewards": ["xp_250", "relic_hint"]
    })
}

fn sample_cinematic() -> serde_json::Value {
    json!({
        "cinematic_id": "cine_luminary_reveal",
        "title": "Luminary Reveal",
        "beats": [
            { "beat_id": "wide_ruins", "start_ms": 0, "duration_ms": 4000, "camera": "wide_ruins", "mood": "awe", "staging": "hero_kneels", "expression": "awe", "dialogue_id": "dlg_elder_cairn_intro" },
            { "beat_id": "spirit_close", "start_ms": 4000, "duration_ms": 3000, "camera": "spirit_close", "mood": "sorrow", "staging": "spirit_appears", "expression": "sorrow", "dialogue_id": "dlg_elder_cairn_intro" }
        ]
    })
}

fn sample_lore() -> serde_json::Value {
    json!({
        "lore_id": "lore_lost_luminary",
        "title": "The Lost Luminary",
        "body": "A moonlit relic remembered by Elder Cairn and sought in Silverleaf Grove.",
        "source_refs": ["source_elder_cairn_manual"],
        "tags": ["luminary", "silverleaf"]
    })
}

fn bad_dialogue_missing_speaker() -> serde_json::Value {
    let mut value = sample_dialogue();
    value["nodes"][0]["speaker_id"] = json!("char_missing");
    value
}

fn bad_cinematic_missing_dialogue() -> serde_json::Value {
    let mut value = sample_cinematic();
    value["beats"][0]["dialogue_id"] = json!("dlg_missing");
    value
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

fn reset_root(root: &Path) {
    if root.exists() {
        fs::remove_dir_all(root).unwrap();
    }
    fs::create_dir_all(root).unwrap();
}
