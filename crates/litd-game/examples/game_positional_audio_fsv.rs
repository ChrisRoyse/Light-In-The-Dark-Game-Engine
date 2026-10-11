//! Full State Verification for #849 — game positional sound / music / ambience.
//!
//! Drives the real `litd-game` binary (EngineAudio/Kira spatial path, real WAV)
//! and independently reads the Calyx Aster runtime-state row. Fail-closed edges:
//! missing asset, invalid cue, empty `runtime_audio`.

use std::{
    f32::consts::PI,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use litd_engine::{
    calyx_json_store::{CalyxJsonStoreReadback, CalyxJsonStoreSpec, read_calyx_json_bytes},
    fsv_artifacts::begin_issue_run,
    game_package::{
        LITD_LOCK_IN_POLICY, LITD_QUICK_SHARE_SCHEMA, LITD_RUNTIME_ID, RuntimePackageAssetRef,
        RuntimePackageEndpoint, RuntimePackageHostMode, compute_runtime_manifest_fingerprint,
    },
};
use serde_json::{Value, json};

const RUNTIME_STATE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCC";
const RUNTIME_STATE_VAULT_PURPOSE: &[u8] = b"litd-game-runtime-state-issue1363";
const RUNTIME_STATE_COLLECTION: &str = "litd_game_runtime_host";
const RUNTIME_STATE_ROW: &[u8] = b"runtime/host_state/latest";
const RUNTIME_STATE_COLUMN: &[u8] = b"state_json";
const MISSING_ASSET_ID: &str = "litd.asset.missing_wav";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue849_game_positional_audio")?;
    let assets = run.run_dir().join("assets/audio");
    fs::create_dir_all(&assets)?;
    let wav_path = assets.join("issue849_spatial.wav");
    write_test_wav(&wav_path, 261.63, 2_200)?;
    let package_path = run.run_dir().join("issue849-audio.litdgame.json");
    let state_path = run.run_dir().join("runtime-state-store");
    let report_path = run.run_dir().join("manual-fsv-report.json");
    let package = write_package(&package_path, "issue849audio", &wav_path, |_| {})?;

    let output = run_litd_game(&package_path, &state_path, 3)?;
    if !output.status.success() {
        return Err(host_failure("happy path", &output).into());
    }
    let state_readback = read_runtime_state_source(&state_path)?;
    let state: Value = serde_json::from_slice(&state_readback.bytes)?;
    let audio = state
        .get("audio")
        .ok_or("runtime state did not contain audio")?;
    let positional = audio_cue(audio, "snd_gate_open")?;
    let music = audio_cue(audio, "snd_low_music")?;
    let ambience = audio_cue(audio, "snd_forest_ambience")?;
    let assertions = json!({
        "triggered_count": audio["triggered_count"],
        "cue_count": audio["cue_count"],
        "status": audio["status"],
        "spatial_snapshot_present": audio["spatial_snapshot"].is_object(),
        "positional_cue_position": positional["position"],
        "positional_cue_kind": positional["kind"],
        "music_cue_position": music["position"],
        "ambience_cue_position": ambience["position"],
        "presentation_state_only": true
    });
    if assertions["triggered_count"] != 3
        || assertions["cue_count"] != 3
        || assertions["status"] != "playing"
        || assertions["spatial_snapshot_present"] != true
        || assertions["positional_cue_position"] != json!([4.0, 0.0, -2.0])
        || assertions["positional_cue_kind"] != "positional_sound"
        || assertions["music_cue_position"] != json!([0.0, 0.0, -10.0])
        || assertions["ambience_cue_position"] != json!([-8.0, 0.0, 3.0])
    {
        return Err("runtime audio state did not match expected positional data".into());
    }

    let missing_asset = edge_missing_asset(run.run_dir(), &wav_path)?;
    let invalid_cue = edge_invalid_cue(run.run_dir(), &wav_path)?;
    let empty_runtime_audio = edge_empty_runtime_audio(run.run_dir(), &wav_path)?;
    if missing_asset["failed_closed"] != true
        || invalid_cue["failed_closed"] != true
        || empty_runtime_audio["proved"] != true
    {
        return Err("runtime audio fail-closed edges did not match expected SoT".into());
    }

    let report = json!({
        "schema_version": 1,
        "issue": 849,
        "aster_issue": 1443,
        "source_of_truth": {
            "package_path": package_path,
            "package_share_id": package["share_id"],
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
        "observed_audio": audio,
        "assertions": assertions,
        "edges": {
            "missing_asset": missing_asset,
            "invalid_cue": invalid_cue,
            "empty_runtime_audio": empty_runtime_audio,
        }
    });
    let bytes = serde_json::to_vec_pretty(&report)?;
    fs::write(&report_path, &bytes)?;
    let readback: Value = serde_json::from_slice(&fs::read(&report_path)?)?;
    if readback != report {
        return Err("report readback mismatch".into());
    }
    println!(
        "ISSUE849_GAME_AUDIO_FSV report={} bytes={} sha256={} state={} state_sha256={} state_seq={} state_graph_rows={} wav={} stdout={}",
        report_path.display(),
        bytes.len(),
        litd_hash::sha256_hex(&bytes),
        state_path.display(),
        state_readback.sha256,
        state_readback.seq,
        state_readback.graph_rows,
        wav_path.display(),
        String::from_utf8_lossy(&output.stdout).trim()
    );
    Ok(())
}

fn edge_missing_asset(
    run_dir: &Path,
    wav_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let package_path = run_dir.join("missing-asset.litdgame.json");
    let state_path = run_dir.join("runtime-state-missing-asset");
    let before_package = write_package(&package_path, "issue849missing", wav_path, |value| {
        value["document"]["runtime_audio"]["cues"][0]["asset_product_id"] = json!(MISSING_ASSET_ID);
    })?;
    let before_asset_id =
        before_package["document"]["runtime_audio"]["cues"][0]["asset_product_id"]
            .as_str()
            .unwrap_or("")
            .to_string();
    let before_state = read_runtime_state_source(&state_path)
        .err()
        .map(|e| e.to_string());
    let output = run_litd_game(&package_path, &state_path, 3)?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let after_state = read_runtime_state_source(&state_path)
        .err()
        .map(|e| e.to_string());
    let failed_closed = !output.status.success()
        && stderr.contains("missing package audio asset")
        && stderr.contains(MISSING_ASSET_ID)
        && after_state.is_some();
    let edge = json!({
        "before": {
            "package_path": package_path,
            "cue_asset_product_id": before_asset_id,
            "runtime_state_error": before_state,
        },
        "after": {
            "exit_code": output.status.code(),
            "stderr": stderr.trim(),
            "runtime_state_error": after_state,
        },
        "failed_closed": failed_closed,
    });
    println!(
        "EDGE missing_asset before={} after={}",
        edge["before"], edge["after"]
    );
    Ok(edge)
}

fn edge_invalid_cue(run_dir: &Path, wav_path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let package_path = run_dir.join("invalid-cue.litdgame.json");
    let state_path = run_dir.join("runtime-state-invalid-cue");
    let before_package = write_package(&package_path, "issue849invalid", wav_path, |value| {
        value["document"]["runtime_audio"]["cues"][0]["cue_id"] = json!("");
    })?;
    let before_cue_id = before_package["document"]["runtime_audio"]["cues"][0]["cue_id"].clone();
    let before_state = read_runtime_state_source(&state_path)
        .err()
        .map(|e| e.to_string());
    let output = run_litd_game(&package_path, &state_path, 3)?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let after_state = read_runtime_state_source(&state_path)
        .err()
        .map(|e| e.to_string());
    let failed_closed = !output.status.success()
        && stderr.contains("symbol cannot be empty")
        && after_state.is_some();
    let edge = json!({
        "before": {
            "package_path": package_path,
            "cue_id": before_cue_id,
            "runtime_state_error": before_state,
        },
        "after": {
            "exit_code": output.status.code(),
            "stderr": stderr.trim(),
            "runtime_state_error": after_state,
        },
        "failed_closed": failed_closed,
    });
    println!(
        "EDGE invalid_cue before={} after={}",
        edge["before"], edge["after"]
    );
    Ok(edge)
}

fn edge_empty_runtime_audio(
    run_dir: &Path,
    wav_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let package_path = run_dir.join("empty-runtime-audio.litdgame.json");
    let state_path = run_dir.join("runtime-state-empty");
    let before_package = write_package(&package_path, "issue849empty", wav_path, |value| {
        value["document"]["runtime_audio"] = json!({ "cues": [] });
    })?;
    let before_cues = before_package["document"]["runtime_audio"]["cues"].clone();
    let before_state = read_runtime_state_source(&state_path)
        .err()
        .map(|e| e.to_string());
    let output = run_litd_game(&package_path, &state_path, 3)?;
    if !output.status.success() {
        return Err(host_failure("empty runtime_audio", &output).into());
    }
    let state_readback = read_runtime_state_source(&state_path)?;
    let state: Value = serde_json::from_slice(&state_readback.bytes)?;
    let audio = state
        .get("audio")
        .ok_or("empty runtime_audio run did not persist audio")?;
    let proved = audio["status"] == "no_audio_cues"
        && audio["cue_count"] == 0
        && audio["triggered_count"] == 0
        && audio["spatial_snapshot"].is_null()
        && before_cues == json!([]);
    let edge = json!({
        "before": {
            "package_path": package_path,
            "runtime_audio": before_package["document"]["runtime_audio"],
            "runtime_state_error": before_state,
        },
        "after": {
            "exit_code": output.status.code(),
            "stdout": String::from_utf8_lossy(&output.stdout).trim(),
            "audio_status": audio["status"],
            "cue_count": audio["cue_count"],
            "triggered_count": audio["triggered_count"],
            "spatial_snapshot": audio["spatial_snapshot"],
            "state_sha256": state_readback.sha256,
        },
        "proved": proved,
    });
    println!(
        "EDGE empty_runtime_audio before={} after={}",
        edge["before"], edge["after"]
    );
    Ok(edge)
}

fn write_package(
    path: &Path,
    share_id: &str,
    wav_path: &Path,
    transform: impl FnOnce(&mut Value),
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut value = package_value(share_id, wav_path)?;
    transform(&mut value);
    let bytes = serde_json::to_vec_pretty(&value)?;
    fs::write(path, &bytes)?;
    let readback: Value = serde_json::from_slice(&fs::read(path)?)?;
    if readback != value {
        return Err(format!("package readback mismatch at {}", path.display()).into());
    }
    Ok(readback)
}

fn run_litd_game(
    package_path: &Path,
    state_path: &Path,
    frames: u32,
) -> Result<Output, Box<dyn std::error::Error>> {
    Ok(Command::new(litd_game_bin()?)
        .arg("--package")
        .arg(package_path)
        .arg("--runtime-state")
        .arg(state_path)
        .arg("--hidden-window")
        .arg("--max-frames")
        .arg(frames.to_string())
        .output()?)
}

fn host_failure(label: &str, output: &Output) -> String {
    format!(
        "litd-game {label} failed: status={:?} stdout={} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn litd_game_bin() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut path = std::env::current_exe()?;
    path.pop();
    if path.file_name().and_then(|name| name.to_str()) == Some("examples") {
        path.pop();
    }
    let bin = path.join(format!("litd-game{}", std::env::consts::EXE_SUFFIX));
    if !bin.is_file() {
        return Err(format!(
            "litd-game binary not found at {} — cargo test --example does not build the package bin; run: cargo build -p litd-game --bin litd-game --features fsv",
            bin.display()
        )
        .into());
    }
    Ok(bin)
}

fn package_value(share_id: &str, wav_path: &Path) -> Result<Value, litd_engine::EngineError> {
    let endpoint = RuntimePackageEndpoint {
        mode: RuntimePackageHostMode::Loopback,
        host: "127.0.0.1".to_string(),
        port: 8490,
        path: format!("/share/{share_id}"),
    };
    let assets = vec![RuntimePackageAssetRef {
        product_id: "litd.asset.issue849.spatial_wav".to_string(),
        source_relative_path: relative_to(wav_path, wav_path.ancestors().nth(3).unwrap()),
        source_sha256: file_sha256(wav_path),
        source_bytes: fs::metadata(wav_path).map(|meta| meta.len()).unwrap_or(0),
    }];
    let document_fingerprint = "0849ad0c0849ad0c";
    let play_build_fingerprint = "0849beef0849beef";
    let manifest_fingerprint = compute_runtime_manifest_fingerprint(
        share_id,
        "agent.issue849",
        LITD_RUNTIME_ID,
        &endpoint,
        document_fingerprint,
        play_build_fingerprint,
        &assets,
    )?;
    let host_uri = endpoint.uri();
    Ok(json!({
        "schema": LITD_QUICK_SHARE_SCHEMA,
        "share_id": share_id,
        "package_id": format!("litd.quick_share.{share_id}"),
        "created_by": "agent.issue849",
        "runtime_id": LITD_RUNTIME_ID,
        "lock_in_policy": LITD_LOCK_IN_POLICY,
        "endpoint": endpoint,
        "host_uri": host_uri,
        "document": {
            "schema": "litd.lowered_document.v1",
            "document_id": "calyx://world/issue849-runtime-audio",
            "terrain_cells": {
                "0_0": { "tile_x": 0, "tile_z": 0, "height": 0.5, "cliff_level": 0 },
                "1_0": { "tile_x": 1, "tile_z": 0, "height": 0.5, "cliff_level": 0 },
                "0_1": { "tile_x": 0, "tile_z": 1, "height": 0.5, "cliff_level": 0 },
                "1_1": { "tile_x": 1, "tile_z": 1, "height": 0.8, "cliff_level": 0 }
            },
            "objects": {
                "start_player_one": {
                    "kind": "StartLocation",
                    "transform": { "translation": [0.5, 0.0, 0.5] }
                }
            },
            "runtime_audio": {
                "listener": {
                    "position": [0.0, 0.0, 0.0],
                    "orientation_xyzw": [0.0, 0.0, 0.0, 1.0],
                    "velocity": [0.0, 0.0, 0.0]
                },
                "cues": [
                    cue("trg_gate_open", "snd_gate_open", "positional_sound", [4.0, 0.0, -2.0], "sfx", 0.0),
                    cue("trg_music_start", "snd_low_music", "music", [0.0, 0.0, -10.0], "music", 0.0),
                    cue("trg_forest_ambience", "snd_forest_ambience", "ambience", [-8.0, 0.0, 3.0], "sfx", 0.25)
                ]
            }
        },
        "document_fingerprint": document_fingerprint,
        "play_build_fingerprint": play_build_fingerprint,
        "asset_count": assets.len(),
        "assets": assets,
        "manifest_fingerprint": manifest_fingerprint
    }))
}

fn read_runtime_state_source(
    path: &Path,
) -> Result<CalyxJsonStoreReadback, Box<dyn std::error::Error>> {
    read_calyx_json_bytes(&runtime_state_spec(path))
        .map_err(|error| format!("read runtime state Aster row: {error}"))?
        .ok_or_else(|| format!("missing runtime state Aster row at {}", path.display()).into())
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

fn audio_cue<'a>(audio: &'a Value, cue_id: &str) -> Result<&'a Value, Box<dyn std::error::Error>> {
    audio
        .get("cues")
        .and_then(Value::as_array)
        .and_then(|cues| cues.iter().find(|cue| cue["cue_id"] == cue_id))
        .ok_or_else(|| format!("missing runtime audio cue {cue_id}").into())
}

fn cue(
    trigger_id: &str,
    cue_id: &str,
    kind: &str,
    position: [f32; 3],
    bus: &str,
    occlusion: f32,
) -> Value {
    json!({
        "trigger_id": trigger_id,
        "cue_id": cue_id,
        "kind": kind,
        "asset_product_id": "litd.asset.issue849.spatial_wav",
        "bus": bus,
        "position": position,
        "velocity": [0.0, 0.0, 0.0],
        "min_distance_meters": 1.0,
        "max_distance_meters": 32.0,
        "spatialization_strength": 0.85,
        "volume_db": -9.0,
        "playback_rate": 1.0,
        "occlusion": { "factor": occlusion },
        "reverb_zone": {
            "zone_id": "forest_edge",
            "center": [0.0, 0.0, 0.0],
            "radius_meters": 20.0,
            "wet_mix": 0.35,
            "feedback": 0.55,
            "damping": 0.25
        }
    })
}

fn relative_to(path: &Path, root: &Path) -> String {
    path.strip_prefix(root).unwrap().display().to_string()
}

fn file_sha256(path: &Path) -> String {
    litd_hash::sha256_hex(&fs::read(path).expect("read wav for sha256"))
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
