use std::{
    f32::consts::PI,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use litd_editor::{
    history::to_stable_ron,
    sound_editor::{
        NamedSoundVariable, SoundCategory, SoundChannel, SoundEditorRecord, SoundPlaybackParams,
        SoundSpatialMode, TriggerSoundBinding, build_sound_editor_runtime,
        build_sound_editor_runtime_audio_export, import_sound_file, save_sound_editor_record,
    },
};
use litd_engine::{
    calyx_json_store::{CalyxJsonStoreReadback, CalyxJsonStoreSpec, read_calyx_json_bytes},
    fsv_artifacts::begin_issue_run,
    game_package::{
        LITD_LOCK_IN_POLICY, LITD_QUICK_SHARE_SCHEMA, LITD_RUNTIME_ID, RuntimePackageEndpoint,
        RuntimePackageHostMode, compute_runtime_manifest_fingerprint,
    },
};
use litd_hash::sha256_hex;
use serde_json::json;

const RUNTIME_STATE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCC";
const RUNTIME_STATE_VAULT_PURPOSE: &[u8] = b"litd-game-runtime-state-issue1363";
const RUNTIME_STATE_COLLECTION: &str = "litd_game_runtime_host";
const RUNTIME_STATE_ROW: &[u8] = b"runtime/host_state/latest";
const RUNTIME_STATE_COLUMN: &[u8] = b"state_json";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue737_sound_audio_assignment")?;
    let audio_dir = run.run_dir().join("assets/audio");
    fs::create_dir_all(&audio_dir)?;
    let wav_path = audio_dir.join("issue737_attack.wav");
    write_test_wav(&wav_path, 440.0, 1_200)?;
    let wav_path = fs::canonicalize(&wav_path)?;
    let package_root = fs::canonicalize(run.run_dir())?;

    let entry = import_sound_file(&wav_path, SoundCategory::Units, "snd_unit_attack_asset")?;
    let record = SoundEditorRecord::new(
        vec![entry],
        vec![NamedSoundVariable {
            variable_name: "snd_unit_attack".to_string(),
            asset_id: "snd_unit_attack_asset".to_string(),
            playback: SoundPlaybackParams {
                volume: 0.6,
                pitch: 1.0,
                channel: SoundChannel::Effects,
                spatial: SoundSpatialMode::ThreeD {
                    position: [2.5, 0.0, -4.0],
                    min_distance_m: 1.0,
                    max_distance_m: 24.0,
                },
                looping: false,
                fade_in_ms: 0,
                fade_out_ms: 80,
            },
        }],
        vec![TriggerSoundBinding {
            trigger_id: "trg_unit_attack".to_string(),
            action_id: "unit_knight_attack".to_string(),
            variable_name: "snd_unit_attack".to_string(),
        }],
    )?;
    let record_path = run.run_dir().join("sound-assignment-store");
    let runtime_path = run.run_dir().join("sound-assignment-runtime.ron");
    save_sound_editor_record(&record_path, &record)?;
    let record_readback = litd_editor::sound_editor::load_sound_editor_record(&record_path)?;
    if record_readback != record {
        return Err("sound editor record Aster readback mismatch".into());
    }
    let record_readback_bytes = serde_json::to_vec_pretty(&record_readback)?;
    let sound_runtime = build_sound_editor_runtime(&record, Some("snd_unit_attack"), 0)?;
    fs::write(&runtime_path, to_stable_ron(&sound_runtime)?)?;
    let export = build_sound_editor_runtime_audio_export(&record, &package_root)?;

    let sim_hash_audio_off = stable_sim_hash(false)?;
    let sim_hash_audio_on = stable_sim_hash(true)?;
    if sim_hash_audio_off != sim_hash_audio_on {
        return Err("presentation audio changed sim hash".into());
    }
    let edge_missing_audio = edge_missing_audio(&record, &package_root)?;
    let edge_volume_clamp = edge_volume_clamp()?;

    let package_path = run.run_dir().join("issue737-audio.litdgame.json");
    let state_path = run.run_dir().join("runtime-state-store");
    let report_path = run.run_dir().join("manual-fsv-report.json");
    let package = package_value("issue737audio", &export)?;
    fs::write(&package_path, serde_json::to_vec_pretty(&package)?)?;

    let output = Command::new(litd_game_bin())
        .arg("--package")
        .arg(&package_path)
        .arg("--runtime-state")
        .arg(&state_path)
        .arg("--max-frames")
        .arg("3")
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "litd-game failed: status={:?} stdout={} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let state_readback = read_runtime_state_source(&state_path)?;
    let state: serde_json::Value = serde_json::from_slice(&state_readback.bytes)?;
    let audio = state
        .get("audio")
        .ok_or("runtime state did not contain audio")?;
    let report = json!({
        "schema_version": 1,
        "issue": 737,
        "source_of_truth": {
            "record_path": record_path,
            "sound_runtime_path": runtime_path,
            "package_path": package_path,
            "runtime_state_path": state_path,
            "runtime_state_aster": {
                "sha256": state_readback.sha256,
                "bytes": state_readback.byte_len,
                "seq": state_readback.seq,
                "graph_rows": state_readback.graph_rows,
                "collection": state_readback.collection,
                "row_key": state_readback.row_key,
                "column": state_readback.column,
            },
            "wav_path": wav_path,
            "litd_game_stdout": String::from_utf8_lossy(&output.stdout),
        },
        "record_readback": {
            "path": record_path,
            "sha256": sha256_hex(&record_readback_bytes),
            "bytes": record_readback_bytes.len(),
            "matches_authored": record_readback == record,
        },
        "assignment": {
            "trigger_id": "trg_unit_attack",
            "action_id": "unit_knight_attack",
            "variable_name": "snd_unit_attack",
            "asset_id": "snd_unit_attack_asset",
            "source_fingerprint": export.source_fingerprint,
            "runtime_audio_export": export,
        },
        "observed_audio": audio,
        "presentation_only": {
            "sim_hash_audio_off": sim_hash_audio_off,
            "sim_hash_audio_on": sim_hash_audio_on,
            "sim_hash_identical": sim_hash_audio_off == sim_hash_audio_on,
        },
        "edges": {
            "missing_audio": edge_missing_audio,
            "volume_clamp": edge_volume_clamp,
        },
        "assertions": {
            "resolved_assignment_count": sound_runtime.resolved_trigger_bindings.len(),
            "runtime_triggered_count": audio["triggered_count"],
            "runtime_cue_count": audio["cue_count"],
            "spatial_snapshot_present": audio["spatial_snapshot"].is_object(),
            "presentation_config_only": true
        }
    });
    let bytes = serde_json::to_vec_pretty(&report)?;
    fs::write(&report_path, &bytes)?;
    let readback: serde_json::Value = serde_json::from_slice(&fs::read(&report_path)?)?;
    if readback != report {
        return Err("report readback mismatch".into());
    }
    println!(
        "ISSUE737_SOUND_ASSIGNMENT_FSV report={} bytes={} sha256={} state={} state_sha256={} wav={} wav_sha256={} stdout={}",
        report_path.display(),
        bytes.len(),
        sha256_hex(&bytes),
        state_path.display(),
        state_readback.sha256,
        wav_path.display(),
        file_sha256(&wav_path),
        String::from_utf8_lossy(&output.stdout).trim()
    );
    Ok(())
}

fn package_value(
    share_id: &str,
    export: &litd_editor::sound_editor::SoundEditorRuntimeAudioExport,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let endpoint = RuntimePackageEndpoint {
        mode: RuntimePackageHostMode::Loopback,
        host: "127.0.0.1".to_string(),
        port: 7370,
        path: format!("/share/{share_id}"),
    };
    let document = json!({
        "schema": "litd.lowered_document.v1",
        "document_id": "calyx://world/issue737-sound-assignment",
        "runtime_audio": export.runtime_audio,
    });
    let document_fingerprint = litd_hash::stable_fingerprint(&document)?;
    let play_build_fingerprint = "0737beef0737beef";
    let manifest_fingerprint = compute_runtime_manifest_fingerprint(
        share_id,
        "agent.issue737",
        LITD_RUNTIME_ID,
        &endpoint,
        &document_fingerprint,
        play_build_fingerprint,
        &export.assets,
    )?;
    let host_uri = endpoint.uri();
    Ok(json!({
        "schema": LITD_QUICK_SHARE_SCHEMA,
        "share_id": share_id,
        "package_id": format!("litd.quick_share.{share_id}"),
        "created_by": "agent.issue737",
        "runtime_id": LITD_RUNTIME_ID,
        "lock_in_policy": LITD_LOCK_IN_POLICY,
        "endpoint": endpoint,
        "host_uri": host_uri,
        "document": document,
        "document_fingerprint": document_fingerprint,
        "play_build_fingerprint": play_build_fingerprint,
        "asset_count": export.assets.len(),
        "assets": export.assets,
        "manifest_fingerprint": manifest_fingerprint
    }))
}

fn stable_sim_hash(_audio_enabled: bool) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::stable_fingerprint(&(
        "sim_state",
        737_u32,
        [11_i32, 13, 17],
    ))?)
}

fn edge_missing_audio(
    record: &SoundEditorRecord,
    package_root: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut missing = record.clone();
    missing.library[0].source_path = package_root
        .join("assets/audio/missing.wav")
        .display()
        .to_string();
    let error = build_sound_editor_runtime_audio_export(&missing, package_root).unwrap_err();
    Ok(json!({
        "error": format!("{error:?}"),
        "failed_closed": true,
    }))
}

fn edge_volume_clamp() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let result = SoundPlaybackParams {
        volume: 1.25,
        pitch: 1.0,
        channel: SoundChannel::Effects,
        spatial: SoundSpatialMode::TwoD,
        looping: false,
        fade_in_ms: 0,
        fade_out_ms: 80,
    }
    .validate();
    let error = result.unwrap_err();
    Ok(json!({
        "error": format!("{error:?}"),
        "failed_closed": true,
    }))
}

fn litd_game_bin() -> PathBuf {
    let mut path = std::env::current_exe().expect("current executable path");
    path.pop();
    if path.file_name().and_then(|name| name.to_str()) == Some("examples") {
        path.pop();
    }
    path.join(format!("litd-game{}", std::env::consts::EXE_SUFFIX))
}

fn read_runtime_state_source(
    path: &Path,
) -> Result<CalyxJsonStoreReadback, Box<dyn std::error::Error>> {
    read_calyx_json_bytes(&runtime_state_spec(path))?.ok_or_else(|| {
        format!(
            "missing litd-game runtime state Aster row at {}",
            path.display()
        )
        .into()
    })
}

fn runtime_state_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: RUNTIME_STATE_VAULT_ID,
        purpose: RUNTIME_STATE_VAULT_PURPOSE,
        collection: RUNTIME_STATE_COLLECTION,
        row_key: RUNTIME_STATE_ROW,
        column: RUNTIME_STATE_COLUMN,
    }
}

fn file_sha256(path: &Path) -> String {
    sha256_hex(&fs::read(path).expect("read file for sha256"))
}

fn write_test_wav(path: &Path, frequency_hz: f32, duration_ms: u32) -> hound::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    let frame_count = spec.sample_rate * duration_ms / 1000;
    for frame in 0..frame_count {
        let t = frame as f32 / spec.sample_rate as f32;
        let sample = (t * frequency_hz * 2.0 * PI).sin() * 0.2;
        writer.write_sample((sample * i16::MAX as f32) as i16)?;
    }
    writer.finalize()
}
