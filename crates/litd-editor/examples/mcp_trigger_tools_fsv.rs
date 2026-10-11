use std::fs;

use litd_editor::{
    mcp_trigger_tools::{
        IMPORT_SOUND_TOOL, PREVIEW_SOUND_TOOL, READ_SOUND_EDITOR_TOOL, READ_TRIGGER_RUNTIME_TOOL,
        UPSERT_ECA_GRAPH_TOOL, UPSERT_RUNTIME_CUE_TOOL, UPSERT_SOUND_VARIABLE_TOOL,
        UPSERT_VARIABLE_TOOL, call_mcp_trigger_tool, read_mcp_sound_record, read_mcp_trigger_state,
    },
    sound_editor::{
        NamedSoundVariable, SoundChannel, SoundEditorRecord, SoundPlaybackParams, SoundSpatialMode,
    },
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue920Fsv {
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
        "LITD_MCP_TRIGGER_TOOLS_FSV_ROOT",
        "issue920_mcp_trigger_tools",
        "manual",
    )?;

    let state_path = root.join("trigger-state.json");
    let record_path = root.join("sound-record.ron");
    let runtime_path = root.join("sound-runtime.json");
    let report_path = root.join("mcp-trigger-tools-fsv-readback.json");
    let wav_path = root.join("imports").join("ashen_bell.wav");
    write_pcm_wav(&wav_path, 44_100, 1, 16, 4_410)?;
    let before = json!({
        "trigger": trigger_state(&state_path, &root.join("variable-ledger.json")),
        "sound": sound_state(&record_path, &root.join("import-ledger.json"))
    });

    let variable = call_mcp_trigger_tool(
        UPSERT_VARIABLE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": root.join("variable-ledger.json"),
            "state_id": "issue920-trigger-state",
            "ledger_id": "issue920-variable-ledger",
            "request_id": "issue920-variable",
            "requested_by": "litd-editor-issue920-fsv",
            "variable": {
                "name": "player_level",
                "value_type": "Integer",
                "scope": "Global",
                "is_array": false,
                "initial_value": "10"
            }
        }),
    )?;
    let boss_flag = call_mcp_trigger_tool(
        UPSERT_VARIABLE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": root.join("boss-variable-ledger.json"),
            "state_id": "issue920-trigger-state",
            "ledger_id": "issue920-boss-variable-ledger",
            "request_id": "issue920-boss-variable",
            "requested_by": "litd-editor-issue920-fsv",
            "variable": {
                "name": "boss_spawned",
                "value_type": "Boolean",
                "scope": "Global",
                "is_array": false,
                "initial_value": "false"
            }
        }),
    )?;
    let graph = call_mcp_trigger_tool(
        UPSERT_ECA_GRAPH_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": root.join("graph-ledger.json"),
            "state_id": "issue920-trigger-state",
            "ledger_id": "issue920-graph-ledger",
            "request_id": "issue920-graph",
            "requested_by": "litd-editor-issue920-fsv",
            "graph": boss_graph()
        }),
    )?;
    let cue = call_mcp_trigger_tool(
        UPSERT_RUNTIME_CUE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": root.join("cue-ledger.json"),
            "state_id": "issue920-trigger-state",
            "ledger_id": "issue920-cue-ledger",
            "request_id": "issue920-cue",
            "requested_by": "litd-editor-issue920-fsv",
            "cue": {
                "cue_id": "cue_boss_dialog",
                "trigger_id": "trg_boss_encounter",
                "kind": "Dialog",
                "properties": {
                    "speaker": "warden",
                    "text": "The ancient king awakens"
                }
            }
        }),
    )?;
    let import = call_mcp_trigger_tool(
        IMPORT_SOUND_TOOL,
        json!({
            "record_path": record_path,
            "ledger_path": root.join("import-ledger.json"),
            "ledger_id": "issue920-import-ledger",
            "request_id": "issue920-import",
            "requested_by": "litd-editor-issue920-fsv",
            "initial_record": empty_sound_record(),
            "source_path": wav_path,
            "category": "Imported",
            "asset_id": "snd_ashen_bell_asset"
        }),
    )?;
    let sound = call_mcp_trigger_tool(
        UPSERT_SOUND_VARIABLE_TOOL,
        json!({
            "record_path": record_path,
            "ledger_path": root.join("sound-variable-ledger.json"),
            "ledger_id": "issue920-sound-variable-ledger",
            "request_id": "issue920-sound-variable",
            "requested_by": "litd-editor-issue920-fsv",
            "initial_record": null,
            "sound": sample_sound_variable(),
            "trigger_binding": {
                "trigger_id": "trg_boss_encounter",
                "action_id": "act_play_bell",
                "variable_name": "snd_ashen_bell"
            }
        }),
    )?;
    let preview = call_mcp_trigger_tool(
        PREVIEW_SOUND_TOOL,
        json!({
            "record_path": record_path,
            "runtime_path": runtime_path,
            "ledger_path": root.join("preview-ledger.json"),
            "ledger_id": "issue920-preview-ledger",
            "request_id": "issue920-preview",
            "requested_by": "litd-editor-issue920-fsv",
            "variable_name": "snd_ashen_bell",
            "cursor_ms": 40
        }),
    )?;
    let trigger_read = call_mcp_trigger_tool(
        READ_TRIGGER_RUNTIME_TOOL,
        json!({ "state_path": state_path }),
    )?;
    let sound_read = call_mcp_trigger_tool(
        READ_SOUND_EDITOR_TOOL,
        json!({ "record_path": record_path, "runtime_path": runtime_path }),
    )?;
    let after = json!({
        "trigger": trigger_state(&state_path, &root.join("variable-ledger.json")),
        "sound": sound_state(&record_path, &root.join("import-ledger.json"))
    });
    let edges = vec![
        edge_missing_variable(&root, &state_path)?,
        edge_missing_trigger(&root, &state_path)?,
        edge_bad_sound_asset(&root, &record_path)?,
    ];
    let fsv = Issue920Fsv {
        issue: 920,
        source_of_truth: json!({
            "trigger_state": state_path,
            "sound_record": record_path,
            "sound_runtime": runtime_path
        }),
        states: json!({ "before": before, "after": after }),
        happy_path: json!({
            "upsert_variable": variable,
            "upsert_boss_flag": boss_flag,
            "upsert_eca_graph": graph,
            "upsert_runtime_cue": cue,
            "import_sound": import,
            "upsert_sound_variable": sound,
            "preview_sound": preview,
            "read_trigger_runtime": trigger_read,
            "read_sound_editor": sound_read
        }),
        edges,
    };
    let fsv = canonical_json_roundtrip(&fsv)?;
    write_json(&report_path, &fsv)?;
    if read_json::<Issue920Fsv>(&report_path)? != fsv {
        return Err(format!("report readback mismatch at {}", report_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue920 mcp_trigger_tools_fsv report={} report_sha256={} state_sha256={} sound_record_sha256={} sound_runtime_sha256={}",
        report_path.display(),
        report_readback.sha256,
        sha256_file(&state_path)?,
        sha256_file(&record_path)?,
        sha256_file(&runtime_path)?,
    );
    Ok(())
}

fn edge_missing_variable(
    root: &std::path::Path,
    state_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-missing-variable-ledger.json");
    let before = trigger_state(state_path, &ledger);
    let error = call_mcp_trigger_tool(
        UPSERT_ECA_GRAPH_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger,
            "state_id": "issue920-trigger-state",
            "ledger_id": "issue920-missing-ref",
            "request_id": "issue920-missing-ref",
            "requested_by": "litd-editor-issue920-fsv",
            "graph": missing_variable_graph()
        }),
    )
    .unwrap_err();
    let after = trigger_state(state_path, &ledger);
    Ok(
        json!({ "name": "missing_variable_ref", "before": before, "after": after, "error": error.to_string() }),
    )
}

fn edge_missing_trigger(
    root: &std::path::Path,
    state_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-missing-trigger-ledger.json");
    let before = trigger_state(state_path, &ledger);
    let error = call_mcp_trigger_tool(
        UPSERT_RUNTIME_CUE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": ledger,
            "state_id": "issue920-trigger-state",
            "ledger_id": "issue920-missing-trigger",
            "request_id": "issue920-missing-trigger",
            "requested_by": "litd-editor-issue920-fsv",
            "cue": {
                "cue_id": "cue_missing_trigger",
                "trigger_id": "trg_missing",
                "kind": "Dialog",
                "properties": { "text": "never written" }
            }
        }),
    )
    .unwrap_err();
    let after = trigger_state(state_path, &ledger);
    Ok(
        json!({ "name": "missing_runtime_trigger", "before": before, "after": after, "error": error.to_string() }),
    )
}

fn edge_bad_sound_asset(
    root: &std::path::Path,
    record_path: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ledger = root.join("edge-bad-sound-ledger.json");
    let before = sound_state(record_path, &ledger);
    let error = call_mcp_trigger_tool(
        UPSERT_SOUND_VARIABLE_TOOL,
        json!({
            "record_path": record_path,
            "ledger_path": ledger,
            "ledger_id": "issue920-bad-sound",
            "request_id": "issue920-bad-sound",
            "requested_by": "litd-editor-issue920-fsv",
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
    let after = sound_state(record_path, &ledger);
    Ok(
        json!({ "name": "missing_sound_asset", "before": before, "after": after, "error": error.to_string() }),
    )
}

fn boss_graph() -> Value {
    json!({
        "trigger_id": "trg_boss_encounter",
        "display_name": "Boss Encounter",
        "enabled": true,
        "folder_path": "Quests/Bosses",
        "events": [{ "node_id": "evt_enter_crypt", "operation": "unit_enters_region", "order": 1, "args": { "region": "crypt_entrance" }, "variable_refs": [] }],
        "conditions": [{ "node_id": "cond_level_gate", "operation": "integer_greater_equal", "order": 1, "args": { "left": "player_level", "right": "10" }, "variable_refs": ["player_level"] }],
        "actions": [{ "node_id": "act_spawn_boss", "operation": "spawn_unit", "order": 1, "args": { "unit_id": "unit_wraith_king", "region": "crypt_entrance" }, "variable_refs": ["boss_spawned"] }],
        "wasm_escape": { "script_id": "wasm_boss_bark", "wat_source": "(module (func (export \"run\")))", "wat_source_sha256": null, "entry_point": "run", "fuel_budget": 1000, "approved_reason": "uses deterministic trigger WASM for encounter bark", "gui_metadata_json": null, "artifact": null }
    })
}

fn missing_variable_graph() -> Value {
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

fn sample_playback() -> Value {
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

fn trigger_state(state_path: &std::path::Path, ledger_path: &std::path::Path) -> Value {
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

fn sound_state(record_path: &std::path::Path, ledger_path: &std::path::Path) -> Value {
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

fn write_json<T: Serialize>(
    path: &std::path::Path,
    value: &T,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(
    path: &std::path::Path,
) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn canonical_json_roundtrip<T>(value: &T) -> Result<T, Box<dyn std::error::Error>>
where
    T: Serialize + DeserializeOwned,
{
    Ok(serde_json::from_slice(&serde_json::to_vec_pretty(value)?)?)
}

fn sha256_file(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}

fn write_pcm_wav(
    path: &std::path::Path,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    sample_frames: u32,
) -> Result<(), Box<dyn std::error::Error>> {
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
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}
