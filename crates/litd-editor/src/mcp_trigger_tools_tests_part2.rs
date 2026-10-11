use super::*;

#[test]
fn issue920_trigger_mcp_tools_author_graph_sound_and_read_real_state() {
    crate::mcp_project_tools::ensure_project_root_env_for_tests();
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue920_mcp_trigger_tools", "unit").unwrap();
    reset_root(&root);
    let state_path = root.join("trigger-state.json");
    let record_path = root.join("sound-record.ron");
    let runtime_path = root.join("sound-runtime.json");
    let variable_ledger = root.join("variable-ledger.json");
    let graph_ledger = root.join("graph-ledger.json");
    let cue_ledger = root.join("cue-ledger.json");
    let import_ledger = root.join("import-ledger.json");
    let sound_ledger = root.join("sound-variable-ledger.json");
    let preview_ledger = root.join("preview-ledger.json");
    let wav_path = root.join("imports").join("ashen_bell.wav");
    write_pcm_wav(&wav_path, 44_100, 1, 16, 4_410);
    let before = trigger_state(&state_path, &variable_ledger);
    println!("issue920 before {before}");

    call_mcp_trigger_tool(
        UPSERT_VARIABLE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": variable_ledger,
            "state_id": "issue920-trigger-state",
            "ledger_id": "issue920-variable-ledger",
            "request_id": "issue920-variable",
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
        UPSERT_VARIABLE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": root.join("boss-variable-ledger.json"),
            "state_id": "issue920-trigger-state",
            "ledger_id": "issue920-boss-variable-ledger",
            "request_id": "issue920-boss-variable",
            "requested_by": "litd-editor-issue920-test",
            "variable": {
                "name": "boss_spawned",
                "value_type": "Boolean",
                "scope": "Global",
                "is_array": false,
                "initial_value": "false"
            }
        }),
    )
    .unwrap();
    call_mcp_trigger_tool(
        UPSERT_ECA_GRAPH_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": graph_ledger,
            "state_id": "issue920-trigger-state",
            "ledger_id": "issue920-graph-ledger",
            "request_id": "issue920-graph",
            "requested_by": "litd-editor-issue920-test",
            "graph": boss_graph()
        }),
    )
    .unwrap();
    call_mcp_trigger_tool(
        UPSERT_RUNTIME_CUE_TOOL,
        json!({
            "state_path": state_path,
            "ledger_path": cue_ledger,
            "state_id": "issue920-trigger-state",
            "ledger_id": "issue920-cue-ledger",
            "request_id": "issue920-cue",
            "requested_by": "litd-editor-issue920-test",
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
    )
    .unwrap();
    let import = call_mcp_trigger_tool(
        IMPORT_SOUND_TOOL,
        json!({
            "record_path": record_path,
            "ledger_path": import_ledger,
            "ledger_id": "issue920-import-ledger",
            "request_id": "issue920-import",
            "requested_by": "litd-editor-issue920-test",
            "initial_record": empty_sound_record(),
            "source_path": wav_path,
            "category": "Imported",
            "asset_id": "snd_ashen_bell_asset"
        }),
    )
    .unwrap();
    assert_eq!(import.sound_readback.unwrap().library_count, 1);
    call_mcp_trigger_tool(
        UPSERT_SOUND_VARIABLE_TOOL,
        json!({
            "record_path": record_path,
            "ledger_path": sound_ledger,
            "ledger_id": "issue920-sound-variable-ledger",
            "request_id": "issue920-sound-variable",
            "requested_by": "litd-editor-issue920-test",
            "initial_record": null,
            "sound": sample_sound_variable(),
            "trigger_binding": {
                "trigger_id": "trg_boss_encounter",
                "action_id": "act_play_bell",
                "variable_name": "snd_ashen_bell"
            }
        }),
    )
    .unwrap();
    call_mcp_trigger_tool(
        PREVIEW_SOUND_TOOL,
        json!({
            "record_path": record_path,
            "runtime_path": runtime_path,
            "ledger_path": preview_ledger,
            "ledger_id": "issue920-preview-ledger",
            "request_id": "issue920-preview",
            "requested_by": "litd-editor-issue920-test",
            "variable_name": "snd_ashen_bell",
            "cursor_ms": 40
        }),
    )
    .unwrap();
    let trigger_read = call_mcp_trigger_tool(
        READ_TRIGGER_RUNTIME_TOOL,
        json!({ "state_path": state_path }),
    )
    .unwrap();
    let sound_read = call_mcp_trigger_tool(
        READ_SOUND_EDITOR_TOOL,
        json!({ "record_path": record_path, "runtime_path": runtime_path }),
    )
    .unwrap();
    assert_eq!(trigger_read.result["trigger_readback"]["variable_count"], 2);
    assert_eq!(trigger_read.result["trigger_readback"]["graph_count"], 1);
    assert_eq!(
        trigger_read.result["trigger_readback"]["wasm_escape_count"],
        1
    );
    let wasm = &trigger_read.result["trigger_readback"]["state"]["graphs"][0]["wasm_escape"];
    assert_eq!(wasm["artifact"]["module_id"], "wasm_boss_bark");
    assert_eq!(wasm["artifact"]["wasm_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(wasm["wat_source_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        trigger_read.result["trigger_readback"]["runtime_cue_count"],
        1
    );
    assert_eq!(sound_read.result["sound_readback"]["named_sound_count"], 1);
    assert_eq!(
        sound_read.result["runtime_readback"]["preview_active"],
        true
    );
    let after = trigger_state(&state_path, &variable_ledger);
    println!("issue920 after {after}");
    assert_eq!(after["variable_count"], 2);
    assert_eq!(after["graph_count"], 1);
    assert_eq!(after["runtime_cue_count"], 1);
}
