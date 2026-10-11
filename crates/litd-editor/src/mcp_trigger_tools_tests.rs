use std::{fs, path::Path};

use serde_json::json;

use crate::sound_editor::{
    NamedSoundVariable, SoundChannel, SoundEditorRecord, SoundPlaybackParams, SoundSpatialMode,
};

use super::*;

#[path = "mcp_trigger_tools_tests_part2.rs"]
mod part2;

#[test]
fn issue920_trigger_mcp_tools_edges_fail_closed_without_writing() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue920_mcp_trigger_tools", "edges").unwrap();
    reset_root(&root);
    let state_path = root.join("trigger-state.json");
    let record_path = root.join("sound-record.ron");
    seed_trigger_state(&root, &state_path);
    seed_sound_record(&root, &record_path);
    let edge_ledger = root.join("edge-ledger.json");

    let before_missing_ref = trigger_state(&state_path, &edge_ledger);
    let missing_ref = call_mcp_trigger_tool(
        UPSERT_ECA_GRAPH_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": edge_ledger,
            "state_id": "issue920-trigger-state",
            "ledger_id": "issue920-missing-ref",
            "request_id": "issue920-missing-ref",
            "requested_by": "litd-editor-issue920-test",
            "graph": missing_variable_graph()
        }),
    )
    .unwrap_err();
    let after_missing_ref = trigger_state(&state_path, &edge_ledger);
    println!(
        "issue920 missing_ref before={before_missing_ref} after={after_missing_ref} error={missing_ref}"
    );
    assert_eq!(before_missing_ref, after_missing_ref);
    assert!(missing_ref.to_string().contains("missing variable"));

    let before_missing_trigger = trigger_state(&state_path, &edge_ledger);
    let missing_trigger = call_mcp_trigger_tool(
        UPSERT_RUNTIME_CUE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": edge_ledger,
            "state_id": "issue920-trigger-state",
            "ledger_id": "issue920-missing-trigger",
            "request_id": "issue920-missing-trigger",
            "requested_by": "litd-editor-issue920-test",
            "cue": {
                "cue_id": "cue_missing_trigger",
                "trigger_id": "trg_missing",
                "kind": "Dialog",
                "properties": { "text": "never written" }
            }
        }),
    )
    .unwrap_err();
    let after_missing_trigger = trigger_state(&state_path, &edge_ledger);
    println!(
        "issue920 missing_trigger before={before_missing_trigger} after={after_missing_trigger} error={missing_trigger}"
    );
    assert_eq!(before_missing_trigger, after_missing_trigger);
    assert!(
        missing_trigger
            .to_string()
            .contains("missing trigger graph")
    );

    let before_bad_sound = sound_state(&record_path, &edge_ledger);
    let bad_sound = call_mcp_trigger_tool(
        UPSERT_SOUND_VARIABLE_TOOL,
        json!({
            "record_path": record_path,
            "ledger_path": edge_ledger,
            "ledger_id": "issue920-bad-sound",
            "request_id": "issue920-bad-sound",
            "requested_by": "litd-editor-issue920-test",
            "initial_record": null,
            "sound": {
                "variable_name": "snd_missing_asset",
                "asset_id": "snd_missing_asset",
                "playback": sample_playback()
            },
            "trigger_binding": null
        }),
    )
    .unwrap_err();
    let after_bad_sound = sound_state(&record_path, &edge_ledger);
    println!(
        "issue920 bad_sound before={before_bad_sound} after={after_bad_sound} error={bad_sound}"
    );
    assert_eq!(before_bad_sound, after_bad_sound);
    assert!(bad_sound.to_string().contains("references missing asset"));
}

fn seed_trigger_state(root: &Path, state_path: &Path) {
    call_mcp_trigger_tool(
        UPSERT_VARIABLE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": root.join("seed-variable-ledger.json"),
            "state_id": "issue920-trigger-state",
            "ledger_id": "issue920-seed-variable",
            "request_id": "issue920-seed-variable",
            "requested_by": "litd-editor-issue920-test",
            "variable": {
                "name": "player_level",
                "value_type": "Integer",
                "scope": "Global",
                "is_array": false,
                "initial_value": "10"
            }
        }),
    )
    .unwrap();
    call_mcp_trigger_tool(
        UPSERT_ECA_GRAPH_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": root.join("seed-graph-ledger.json"),
            "state_id": "issue920-trigger-state",
            "ledger_id": "issue920-seed-graph",
            "request_id": "issue920-seed-graph",
            "requested_by": "litd-editor-issue920-test",
            "graph": simple_graph()
        }),
    )
    .unwrap();
}

#[test]
fn issue1518_trigger_tool_rejects_out_of_root_paths_without_writing() {
    // #1518 confinement close gate for the trigger family: a poisoned state_path
    // (absolute-escape write sink) and a ..-traversal source_path (the sound-import source
    // read sink) are both rejected before any fs effect, leaving the filesystem untouched.
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_run, root) =
        crate::fsv_support::artifact_dir("issue1518_mcp_trigger_tools", "confine").unwrap();
    let ledger_path = root.join("issue1518-ledger.json");

    // --- EDGE 1: absolute state_path OUTSIDE the workspace root (UPSERT_VARIABLE) ---
    let _temp_root = litd_engine::test_temp::TestTempDir::new("issue1518-trigger");
    let escape_dir = _temp_root.path().to_path_buf();
    if escape_dir.exists() {
        fs::remove_dir_all(&escape_dir).ok();
    }
    let escape_state = escape_dir.join("trigger-state.json");
    let e1 = call_mcp_trigger_tool(
        UPSERT_VARIABLE_TOOL,
        json!({
            "state_path": escape_state,
            "ledger_path": ledger_path,
            "state_id": "issue1518-trigger-state",
            "ledger_id": "issue1518-ledger",
            "request_id": "issue1518-escape",
            "requested_by": "litd-editor-issue1518-test",
            "variable": {
                "name": "player_level",
                "value_type": "Integer",
                "scope": "Global",
                "is_array": false,
                "initial_value": "1"
            }
        }),
    )
    .unwrap_err()
    .to_string();
    println!("issue1518 trigger absolute-escape rejected: {e1}");
    assert!(
        e1.contains("outside the project root"),
        "expected outside-root rejection, got: {e1}"
    );
    assert!(!escape_dir.exists());

    // --- EDGE 2: ..-traversal source_path on IMPORT_SOUND (the source-file read sink) ---
    let record_path = root.join("issue1518-rec.ron");
    let e2 = call_mcp_trigger_tool(
        IMPORT_SOUND_TOOL,
        json!({
            "record_path": record_path,
            "ledger_path": ledger_path,
            "ledger_id": "issue1518-ledger",
            "request_id": "issue1518-traversal",
            "requested_by": "litd-editor-issue1518-test",
            "initial_record": empty_sound_record(),
            "source_path": "snd/../../litd-issue1518-trigger-traversal.wav",
            "category": "Imported",
            "asset_id": "snd_issue1518_asset"
        }),
    )
    .unwrap_err()
    .to_string();
    println!("issue1518 trigger traversal rejected: {e2}");
    assert!(
        e2.contains("parent-directory"),
        "expected traversal rejection, got: {e2}"
    );
    assert!(!Path::new("../litd-issue1518-trigger-traversal.wav").exists());
    assert!(
        !record_path.exists(),
        "confinement breach: sound record written despite a rejected source_path"
    );
}

fn seed_sound_record(root: &Path, record_path: &Path) {
    let wav_path = root.join("imports").join("edge_bell.wav");
    write_pcm_wav(&wav_path, 44_100, 1, 16, 4_410);
    call_mcp_trigger_tool(
        IMPORT_SOUND_TOOL,
        json!({
            "record_path": record_path,
            "ledger_path": root.join("seed-sound-ledger.json"),
            "ledger_id": "issue920-seed-sound",
            "request_id": "issue920-seed-sound",
            "requested_by": "litd-editor-issue920-test",
            "initial_record": empty_sound_record(),
            "source_path": wav_path,
            "category": "Imported",
            "asset_id": "snd_ashen_bell_asset"
        }),
    )
    .unwrap();
}

fn boss_graph() -> serde_json::Value {
    json!({
        "trigger_id": "trg_boss_encounter",
        "display_name": "Boss Encounter",
        "enabled": true,
        "folder_path": "Quests/Bosses",
        "events": [{
            "node_id": "evt_enter_crypt",
            "operation": "unit_enters_region",
            "order": 1,
            "args": { "region": "crypt_entrance" },
            "variable_refs": []
        }],
        "conditions": [{
            "node_id": "cond_level_gate",
            "operation": "integer_greater_equal",
            "order": 1,
            "args": { "left": "player_level", "right": "10" },
            "variable_refs": ["player_level"]
        }],
        "actions": [{
            "node_id": "act_spawn_boss",
            "operation": "spawn_unit",
            "order": 1,
            "args": { "unit_id": "unit_wraith_king", "region": "crypt_entrance" },
            "variable_refs": ["boss_spawned"]
        }],
        "wasm_escape": {
            "script_id": "wasm_boss_bark",
            "wat_source": r#"(module (func (export "run")))"#,
            "wat_source_sha256": null,
            "entry_point": "run",
            "fuel_budget": 1000,
            "approved_reason": "uses deterministic trigger WASM for encounter bark",
            "gui_metadata_json": null,
            "artifact": null
        }
    })
}

fn simple_graph() -> serde_json::Value {
    json!({
        "trigger_id": "trg_simple",
        "display_name": "Simple Trigger",
        "enabled": true,
        "folder_path": null,
        "events": [{
            "node_id": "evt_map_init",
            "operation": "map_initialization",
            "order": 1,
            "args": { "phase": "start" },
            "variable_refs": []
        }],
        "conditions": [],
        "actions": [{
            "node_id": "act_display_text",
            "operation": "display_text",
            "order": 1,
            "args": { "text": "ready" },
            "variable_refs": []
        }],
        "wasm_escape": null
    })
}

fn missing_variable_graph() -> serde_json::Value {
    let mut graph = boss_graph();
    graph["conditions"][0]["variable_refs"] = json!(["missing_level"]);
    graph
}

fn sample_sound_variable() -> NamedSoundVariable {
    NamedSoundVariable {
        variable_name: "snd_ashen_bell".to_string(),
        asset_id: "snd_ashen_bell_asset".to_string(),
        playback: sample_playback_struct(),
    }
}

fn sample_playback() -> serde_json::Value {
    json!(sample_playback_struct())
}

fn sample_playback_struct() -> SoundPlaybackParams {
    SoundPlaybackParams {
        volume: 0.75,
        pitch: 1.0,
        channel: SoundChannel::Effects,
        spatial: SoundSpatialMode::TwoD,
        looping: false,
        fade_in_ms: 0,
        fade_out_ms: 120,
    }
}

fn empty_sound_record() -> SoundEditorRecord {
    SoundEditorRecord::new(Vec::new(), Vec::new(), Vec::new()).unwrap()
}

fn trigger_state(state_path: &Path, ledger_path: &Path) -> serde_json::Value {
    let readback = if state_path.exists() {
        Some(read_mcp_trigger_state(state_path).unwrap())
    } else {
        None
    };
    json!({
        "state_exists": state_path.exists(),
        "state_bytes": fs::metadata(state_path).map(|metadata| metadata.len()).unwrap_or(0),
        "variable_count": readback.as_ref().map(|readback| readback.variable_count).unwrap_or(0),
        "graph_count": readback.as_ref().map(|readback| readback.graph_count).unwrap_or(0),
        "runtime_cue_count": readback.as_ref().map(|readback| readback.runtime_cue_count).unwrap_or(0),
        "ledger_exists": ledger_path.exists(),
        "ledger_bytes": fs::metadata(ledger_path).map(|metadata| metadata.len()).unwrap_or(0)
    })
}

fn sound_state(record_path: &Path, ledger_path: &Path) -> serde_json::Value {
    let readback = if record_path.exists() {
        Some(read_mcp_sound_record(record_path).unwrap())
    } else {
        None
    };
    json!({
        "record_exists": record_path.exists(),
        "record_bytes": fs::metadata(record_path).map(|metadata| metadata.len()).unwrap_or(0),
        "library_count": readback.as_ref().map(|readback| readback.library_count).unwrap_or(0),
        "named_sound_count": readback.as_ref().map(|readback| readback.named_sound_count).unwrap_or(0),
        "trigger_binding_count": readback.as_ref().map(|readback| readback.trigger_binding_count).unwrap_or(0),
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

fn write_pcm_wav(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    sample_frames: u32,
) {
    let bytes_per_sample = u32::from(bits_per_sample / 8);
    let block_align = u32::from(channels) * bytes_per_sample;
    let data_bytes = sample_frames * block_align;
    let byte_rate = sample_rate * block_align;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&(block_align as u16).to_le_bytes());
    bytes.extend_from_slice(&bits_per_sample.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    bytes.resize(bytes.len() + data_bytes as usize, 0);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}
