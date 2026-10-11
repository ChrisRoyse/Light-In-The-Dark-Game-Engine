use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use litd_engine::EngineError;
use litd_hash::sha256_hex;
use serde_json::{Value, json};

use crate::{
    mcp_story_tools::{
        CinematicBeat, CinematicStage, DialogueChoice, DialogueNode, DialogueTree, LoreContext,
        QuestArc, QuestObjective, StoryCharacter,
        mcp_story_tools_store::{McpStoryState, write_mcp_story_state},
    },
    mcp_trigger_tools::{
        EcaGraph, EcaNode, RuntimeCue, RuntimeCueKind, TriggerValueType, TriggerVariable,
        TriggerVariableScope, mcp_trigger_tools_store::write_mcp_sound_record,
    },
    sound_editor::{
        NamedSoundVariable, SoundAudioCodec, SoundCategory, SoundChannel, SoundEditorRecord,
        SoundLibraryEntry, SoundPlaybackParams, SoundSpatialMode, TriggerSoundBinding,
    },
    temporal_content::TemporalContentItem,
    vfx_text_generation::{VfxTextGenerationRequest, generate_and_store_vfx_text_effect},
};

pub(crate) fn story_character() -> StoryCharacter {
    StoryCharacter {
        character_id: "char_elara".to_string(),
        display_name: "Elara".to_string(),
        role: "gate_guard".to_string(),
        voice_tags: vec!["calm".to_string()],
    }
}

pub(crate) fn dialogue_tree() -> DialogueTree {
    DialogueTree {
        dialogue_id: "dlg_gate".to_string(),
        title: "Gate Dialogue".to_string(),
        nodes: vec![DialogueNode {
            node_id: "start".to_string(),
            speaker_id: "char_elara".to_string(),
            line: "The gate opens when the signal fire rises.".to_string(),
            choices: vec![DialogueChoice {
                choice_id: "accept".to_string(),
                text: "Light it.".to_string(),
                target_node_id: None,
                conditions: Vec::new(),
                effects: vec!["quest_gate_start".to_string()],
            }],
            quest_effects: vec!["quest_gate_start".to_string()],
        }],
    }
}

pub(crate) fn quest_arc() -> QuestArc {
    QuestArc {
        quest_id: "quest_gate".to_string(),
        title: "Open the Gate".to_string(),
        start_dialogue_id: Some("dlg_gate".to_string()),
        completion_dialogue_id: Some("dlg_gate".to_string()),
        objectives: vec![QuestObjective {
            objective_id: "light_signal".to_string(),
            description: "Light the signal fire".to_string(),
            target_ref: "region.gate".to_string(),
            required_count: 1,
        }],
        rewards: vec!["gate_access".to_string()],
    }
}

pub(crate) fn cinematic_stage() -> CinematicStage {
    CinematicStage {
        cinematic_id: "cin_gate".to_string(),
        title: "Gate Opens".to_string(),
        beats: vec![CinematicBeat {
            beat_id: "beat_signal".to_string(),
            start_ms: 0,
            duration_ms: 1500,
            camera: "gate_wide".to_string(),
            mood: "urgent".to_string(),
            staging: "signal fire ignites".to_string(),
            expression: Some("relief".to_string()),
            dialogue_id: Some("dlg_gate".to_string()),
        }],
    }
}

pub(crate) fn lore_context() -> LoreContext {
    LoreContext {
        lore_id: "lore_gate".to_string(),
        title: "Gate Signal".to_string(),
        body: "Signal fires are authoritative gate unlock anchors.".to_string(),
        source_refs: vec!["design_issue1360_gate".to_string()],
        tags: vec!["gate".to_string(), "signal".to_string()],
    }
}

pub(crate) fn trigger_variable() -> TriggerVariable {
    TriggerVariable {
        name: "gate_open".to_string(),
        value_type: TriggerValueType::Boolean,
        scope: TriggerVariableScope::Global,
        is_array: false,
        initial_value: Some("false".to_string()),
        array_size: None,
        array_initial_values: Vec::new(),
    }
}

pub(crate) fn eca_graph() -> EcaGraph {
    EcaGraph {
        trigger_id: "trg_gate_open".to_string(),
        display_name: "Gate Open".to_string(),
        enabled: true,
        folder_path: Some("gates".to_string()),
        initially_on: true,
        comment: None,
        events: vec![EcaNode {
            node_id: "event_signal".to_string(),
            operation: "event.signal_lit".to_string(),
            order: 0,
            args: BTreeMap::new(),
            variable_refs: Vec::new(),
        }],
        conditions: vec![EcaNode {
            node_id: "cond_gate".to_string(),
            operation: "condition.boolean_equals".to_string(),
            order: 1,
            args: BTreeMap::from([("expected".to_string(), "false".to_string())]),
            variable_refs: vec!["gate_open".to_string()],
        }],
        actions: vec![EcaNode {
            node_id: "action_open".to_string(),
            operation: "action.set_boolean".to_string(),
            order: 2,
            args: BTreeMap::from([("value".to_string(), "true".to_string())]),
            variable_refs: vec!["gate_open".to_string()],
        }],
        wasm_escape: None,
    }
}

pub(crate) fn runtime_cue() -> RuntimeCue {
    RuntimeCue {
        cue_id: "cue_gate_open".to_string(),
        trigger_id: "trg_gate_open".to_string(),
        kind: RuntimeCueKind::Sound,
        properties: BTreeMap::from([("sound".to_string(), "snd_gate_open".to_string())]),
    }
}

pub(crate) fn sound_record() -> SoundEditorRecord {
    let bytes = b"issue1360-synthetic-sound-bytes";
    SoundEditorRecord::new(
        vec![SoundLibraryEntry {
            asset_id: "asset_gate_open".to_string(),
            category: SoundCategory::Interface,
            file_name: "gate_open.wav".to_string(),
            source_path: "synthetic://issue1360/gate_open.wav".to_string(),
            sha256: sha256_hex(bytes),
            codec: SoundAudioCodec::PcmWav,
            length_ms: 1000,
            sample_rate_hz: 44_100,
            channels: 2,
            bits_per_sample: Some(16),
            bytes_on_disk: bytes.len() as u64,
        }],
        vec![NamedSoundVariable {
            variable_name: "snd_gate_open".to_string(),
            asset_id: "asset_gate_open".to_string(),
            playback: SoundPlaybackParams {
                volume: 0.75,
                pitch: 1.0,
                channel: SoundChannel::Interface,
                spatial: SoundSpatialMode::TwoD,
                looping: false,
                fade_in_ms: 0,
                fade_out_ms: 250,
            },
        }],
        vec![TriggerSoundBinding {
            trigger_id: "trg_gate_open".to_string(),
            action_id: "action_open".to_string(),
            variable_name: "snd_gate_open".to_string(),
        }],
    )
    .unwrap()
}

pub(crate) fn temporal_item(
    item_id: &str,
    base_score_milli: u16,
    event_time_secs: Option<i64>,
    sequence_position: u64,
    sequence_total: u64,
) -> TemporalContentItem {
    TemporalContentItem {
        item_id: item_id.to_string(),
        title: item_id.replace('_', " "),
        base_score_milli,
        event_time_secs,
        sequence_position,
        sequence_total,
        provenance_refs: vec![format!("Aster://issue1360/{item_id}")],
    }
}

pub(crate) fn temporal_query() -> crate::temporal_content::TemporalContentQuery {
    use calyx_core::{
        BoostConfig, DecayFunction, FusionWeights, PeriodicOptions, SequenceOptions, TemporalPolicy,
    };
    crate::temporal_content::TemporalContentQuery {
        query_id: "issue1360-temporal-query".to_string(),
        query_time_secs: 1_704_204_000,
        tz_offset_secs: 0,
        policy: TemporalPolicy::new(
            true,
            DecayFunction::Exponential {
                half_life_secs: 3_600,
            },
            PeriodicOptions {
                target_hour: Some(14),
                target_day_of_week: Some(1),
                use_now: false,
            },
            SequenceOptions::default(),
            FusionWeights::default(),
            BoostConfig::default(),
            true,
        )
        .unwrap(),
        requested_by: "issue1360-fsv".to_string(),
    }
}

pub(crate) fn file_path_edge(root: &Path, story: &McpStoryState) -> Value {
    let path = root.join("edge-story-file-target.txt");
    fs::write(&path, b"not an Aster vault").unwrap();
    let before = file_state(&path);
    let before_hash = sha256_hex(&fs::read(&path).unwrap());
    let error = write_mcp_story_state(&path, story).unwrap_err();
    let after_hash = sha256_hex(&fs::read(&path).unwrap());
    let after = file_state(&path);
    assert_eq!(before_hash, after_hash);
    json!({
        "before": before,
        "after": after,
        "file_hash_before": before_hash,
        "file_hash_after": after_hash,
        "error": error_report(&error),
    })
}

pub(crate) fn invalid_sound_edge(root: &Path) -> Value {
    let path = root.join("edge-invalid-sound-aster");
    let before = file_state(&path);
    let mut record = sound_record();
    record.named_sounds[0].asset_id = "missing_asset".to_string();
    let error = write_mcp_sound_record(&path, &record).unwrap_err();
    let after = file_state(&path);
    assert!(!path.exists());
    json!({
        "before": before,
        "after": after,
        "error": error_report(&error),
    })
}

pub(crate) fn ungrounded_vfx_edge(root: &Path) -> Value {
    let legacy = root.join("edge-vfx-legacy.json");
    let aster = root.join("edge-vfx-aster");
    let store = root.join("edge-vfx-store");
    let before = json!({
        "legacy": file_state(&legacy),
        "aster": file_state(&aster),
        "store_files": physical_files(&store).len(),
    });
    let error = generate_and_store_vfx_text_effect(
        &legacy,
        &aster,
        &store,
        "issue1360-edge-vfx",
        VfxTextGenerationRequest {
            request_id: "issue1360-edge-vfx".to_string(),
            prompt: "make the thing nicer".to_string(),
            seed: 13_602,
            requested_by: "issue1360-fsv".to_string(),
        },
    )
    .unwrap_err();
    let after = json!({
        "legacy": file_state(&legacy),
        "aster": file_state(&aster),
        "store_files": physical_files(&store).len(),
    });
    assert!(!legacy.exists());
    assert!(!aster.exists());
    json!({
        "before": before,
        "after": after,
        "error": error_report(&error),
    })
}

pub(crate) fn physical_summary(stores: &[(impl AsRef<str>, PathBuf)]) -> Value {
    let mut total_files = 0usize;
    let mut rows = Vec::new();
    for (label, path) in stores {
        let files = physical_files(path);
        total_files += files.len();
        rows.push(json!({
            "label": label.as_ref(),
            "path": path,
            "file_count": files.len(),
            "files": files,
        }));
    }
    json!({"total_files": total_files, "stores": rows})
}

pub(crate) fn physical_files(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if root.exists() {
        collect_files(root, root, &mut files);
    }
    files.sort();
    files
}

pub(crate) fn collect_files(root: &Path, current: &Path, files: &mut Vec<String>) {
    for entry in fs::read_dir(current).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else if path.is_file() {
            files.push(relative_path(root, &path));
        }
    }
}

pub(crate) fn legacy_json_outputs(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_legacy(root, root, &mut files);
    files.sort();
    files
}

pub(crate) fn collect_legacy(root: &Path, current: &Path, files: &mut Vec<String>) {
    for entry in fs::read_dir(current).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_legacy(root, &path, files);
        } else if path.is_file()
            && path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| {
                    matches!(
                        name,
                        "story-state.json"
                            | "trigger-state.json"
                            | "sound-record.ron"
                            | "sound-runtime.json"
                            | "vfx-text-legacy.json"
                            | "vfx-library-legacy.json"
                            | "temporal-legacy.json"
                            | "edge-vfx-legacy.json"
                    )
                })
        {
            files.push(relative_path(root, &path));
        }
    }
}

pub(crate) fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
        .replace('\\', "/")
}

pub(crate) fn file_state(path: &Path) -> Value {
    let metadata = fs::metadata(path).ok();
    let sha256 = metadata
        .as_ref()
        .filter(|metadata| metadata.is_file())
        .map(|_| sha256_hex(&fs::read(path).unwrap()));
    json!({
        "path": path.display().to_string(),
        "exists": path.exists(),
        "is_dir": path.is_dir(),
        "is_file": path.is_file(),
        "len": metadata.as_ref().map(|metadata| metadata.len()),
        "sha256": sha256,
    })
}

pub(crate) fn error_report(error: &EngineError) -> Value {
    let report = error.report();
    json!({
        "code": report.code,
        "message": report.message,
        "remediation": report.remediation,
    })
}
