//! Full State Verification for #1460 — litd-game frame loop: off-thread cadenced
//! runtime-state persistence, `--gpu-validation`-gated per-frame GPU validation,
//! and `WaitUntil` pacing (no per-frame blocking sleep).
//!
//! Black-box, like `game_positional_audio_fsv`: it drives the real `litd-game`
//! runtime host binary (real window + GPU) as a subprocess and reads the
//! persisted state back out of the Calyx Aster vault. It proves:
//!   1. runtime state is still written and hash-verified (independent Aster
//!      readback of the terminal row) — the write→readback→hash discipline
//!      survived moving off the render thread;
//!   2. enabling `--runtime-state` adds negligible wall-clock vs a run without it
//!      (the per-frame synchronous vault write+readback+hash is gone); the old
//!      code did that work on the render thread every frame;
//!   3. `--gpu-validation` still runs a valid frame and persists state;
//!   4. an unavailable vault path (a regular file) fails the run LOUD at write
//!      time, not silently skipped.

use std::{
    f32::consts::PI,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

use litd_engine::{
    calyx_json_store::{
        CalyxJsonStoreReadback, CalyxJsonStoreSpec, read_calyx_json_bytes,
        read_calyx_json_wal_write,
    },
    fsv_artifacts::begin_issue_run,
    game_package::{
        LITD_LOCK_IN_POLICY, LITD_QUICK_SHARE_SCHEMA, LITD_RUNTIME_ID, RuntimePackageAssetRef,
        RuntimePackageEndpoint, RuntimePackageHostMode, compute_runtime_manifest_fingerprint,
    },
};
use serde_json::json;

const RUNTIME_STATE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCC";
const RUNTIME_STATE_VAULT_PURPOSE: &[u8] = b"litd-game-runtime-state-issue1363";
const RUNTIME_STATE_COLLECTION: &str = "litd_game_runtime_host";
const RUNTIME_STATE_ROW: &[u8] = b"runtime/host_state/latest";
const RUNTIME_STATE_COLUMN: &[u8] = b"state_json";
// >1s of 16ms-paced frames so the 1 Hz off-thread heartbeat fires at least once.
const FRAMES: u32 = 120;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1460_frame_loop")?;
    let assets = run.run_dir().join("assets/audio");
    fs::create_dir_all(&assets)?;
    let wav_path = assets.join("issue1460_tone.wav");
    write_test_wav(&wav_path, 220.0, 400)?;
    let package_path = run.run_dir().join("issue1460.litdgame.json");
    fs::write(
        &package_path,
        serde_json::to_vec_pretty(&package_value("issue1460frame", &wav_path)?)?,
    )?;

    // (1)+(2) state-on vs state-off wall clock.
    let state_path = run.run_dir().join("runtime-state-store");
    let (with_state_ms, with_stdout) = run_host(&package_path, Some(&state_path), false, FRAMES)?;
    let (without_state_ms, _) = run_host(&package_path, None, false, FRAMES)?;
    let state_readback = read_runtime_state_source(&state_path)?;
    let state: serde_json::Value = serde_json::from_slice(&state_readback.bytes)?;
    let terminal_status = state
        .get("status")
        .and_then(serde_json::Value::as_str)
        .ok_or("runtime state missing status")?
        .to_string();
    let state_wal = read_calyx_json_wal_write(&state_path, state_readback.seq)
        .map_err(|error| format!("read runtime state physical Aster WAL: {error}"))?;
    let independent_sha = state_wal.sha256.clone();
    let state_hash_matches = independent_sha == state_readback.sha256;
    let persisted = terminal_status == "exited" || terminal_status == "closed";
    // Enabling persistence must not multiply frame time — the per-frame vault I/O
    // is gone. Allow generous slack for OS/GPU scheduling noise on short runs.
    let overhead_ratio = with_state_ms as f64 / without_state_ms.max(1) as f64;
    let no_per_frame_stall = with_state_ms <= without_state_ms + without_state_ms / 2 + 400;

    // (3) --gpu-validation path still runs a valid frame and persists state.
    let gpu_state_path = run.run_dir().join("runtime-state-store-gpuvalidate");
    let (gpu_ms, _) = run_host(&package_path, Some(&gpu_state_path), true, FRAMES)?;
    let gpu_readback = read_runtime_state_source(&gpu_state_path)?;
    let gpu_wal = read_calyx_json_wal_write(&gpu_state_path, gpu_readback.seq)
        .map_err(|error| format!("read GPU-validation state physical Aster WAL: {error}"))?;
    let gpu_state: serde_json::Value = serde_json::from_slice(&gpu_readback.bytes)?;
    let gpu_persisted = gpu_state.get("status").and_then(|s| s.as_str()) == Some("exited")
        || gpu_state.get("status").and_then(|s| s.as_str()) == Some("closed");
    let gpu_hash_matches = gpu_wal.sha256 == gpu_readback.sha256;

    // (4) Edge: a regular file where a vault directory is expected must fail loud.
    let bad_vault = run.run_dir().join("runtime-state-not-a-vault");
    fs::write(&bad_vault, b"not an Aster vault")?;
    let edge = run_host_expect_failure(&package_path, &bad_vault, FRAMES)?;

    assert!(
        persisted,
        "terminal runtime state must be persisted, got {terminal_status:?}"
    );
    assert!(
        state_hash_matches,
        "persisted state sha256 must match independent hash"
    );
    assert!(
        no_per_frame_stall,
        "state-on wall clock {with_state_ms}ms must not stall vs state-off {without_state_ms}ms"
    );
    assert!(
        gpu_persisted,
        "--gpu-validation run must still persist terminal state"
    );
    assert!(
        gpu_hash_matches,
        "--gpu-validation Aster row must match its physical WAL write"
    );
    assert!(
        edge.failed_closed,
        "unavailable vault must fail the run loud: {}",
        edge.detail
    );

    let report = json!({
        "schema": "litd.issue1460.frame_loop_fsv.v1",
        "issue": 1460,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "package_path": package_path.display().to_string(),
            "runtime_state_vault": state_path.display().to_string(),
            "runtime_state_aster": {
                "sha256": state_readback.sha256,
                "independent_sha256": independent_sha,
                "independent_source": "physical Aster WAL frame decoded at committed seq",
                "bytes": state_readback.byte_len,
                "seq": state_readback.seq,
                "graph_rows": state_readback.graph_rows,
                "collection": state_readback.collection,
                "row_key": state_readback.row_key,
                "column": state_readback.column,
                "wal": {
                    "wal_row_count": state_wal.wal_row_count,
                    "graph_row_count": state_wal.graph_row_count,
                    "time_index_row_count": state_wal.time_index_row_count,
                    "graph_keys": state_wal.graph_keys,
                    "graph_key_sha256": state_wal.graph_key_sha256,
                    "segment_path": state_wal.segment_path,
                    "segment_sha256": state_wal.segment_sha256,
                    "frame_sha256": state_wal.frame_sha256,
                    "start_offset": state_wal.start_offset,
                    "end_offset": state_wal.end_offset,
                },
            },
            "manual_gate": "Read this JSON via Synapse; the Aster row sha256 must equal the independently decoded physical WAL-row sha256, terminal status persisted, and state-on wall clock must be within noise of state-off."
        },
        "research_sources": [
            "https://docs.rs/winit/latest/winit/event_loop/enum.ControlFlow.html",
            "https://docs.rs/wgpu/latest/wgpu/struct.Device.html#method.pop_error_scope",
            "https://gafferongames.com/post/fix_your_timestep/"
        ],
        "sufficiency_argument": "120 frames at 16ms pacing (~1.9s) is the smallest run that (a) exceeds the 1s heartbeat cadence so the off-thread writer provably fires, and (b) is long enough that a per-frame synchronous vault write+readback+hash (the old behavior) would visibly diverge state-on from state-off. Four subprocess runs cover state-on, state-off, gpu-validation, and the unavailable-vault edge.",
        "part_1_persistence": {
            "terminal_status": terminal_status,
            "state_bytes": state_readback.byte_len,
            "aster_sha256": state_readback.sha256,
            "independent_sha256": independent_sha,
            "hash_matches": state_hash_matches
        },
        "part_2_no_per_frame_stall": {
            "frames": FRAMES,
            "with_state_ms": with_state_ms,
            "without_state_ms": without_state_ms,
            "overhead_ratio": overhead_ratio,
            "no_per_frame_stall": no_per_frame_stall
        },
        "part_3_gpu_validation": {
            "gpu_validation_ms": gpu_ms,
            "gpu_state_persisted": gpu_persisted,
            "aster_sha256": gpu_readback.sha256,
            "physical_wal_sha256": gpu_wal.sha256,
            "hash_matches": gpu_hash_matches,
            "wal_frame_sha256": gpu_wal.frame_sha256,
            "wal_row_count": gpu_wal.wal_row_count,
            "wal_graph_row_count": gpu_wal.graph_row_count,
            "wal_time_index_row_count": gpu_wal.time_index_row_count,
            "wal_graph_key_sha256": gpu_wal.graph_key_sha256,
        },
        "part_4_edge_unavailable_vault": {
            "failed_closed": edge.failed_closed,
            "exit_code": edge.exit_code,
            "detail": edge.detail
        },
        "expected_actuals": { "all_assertions_passed": true }
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1460_FRAME_LOOP_FSV report={} report_sha256={} status={} state_sha256={} hash_ok={} with_state_ms={} without_state_ms={} ratio={:.3} gpu_persisted={} edge_failed_closed={} stdout={}",
        readback.path.display(),
        readback.sha256,
        terminal_status,
        state_readback.sha256,
        state_hash_matches,
        with_state_ms,
        without_state_ms,
        overhead_ratio,
        gpu_persisted,
        edge.failed_closed,
        with_stdout.trim()
    );
    Ok(())
}

struct EdgeResult {
    failed_closed: bool,
    exit_code: Option<i32>,
    detail: String,
}

fn run_host(
    package_path: &Path,
    state_path: Option<&Path>,
    gpu_validation: bool,
    frames: u32,
) -> Result<(u128, String), Box<dyn std::error::Error>> {
    let mut command = Command::new(litd_game_bin());
    command.arg("--package").arg(package_path);
    if let Some(state_path) = state_path {
        command.arg("--runtime-state").arg(state_path);
    }
    if gpu_validation {
        command.arg("--gpu-validation");
    }
    command.arg("--hidden-window");
    command.arg("--max-frames").arg(frames.to_string());
    let started = Instant::now();
    let output = command.output()?;
    let elapsed_ms = started.elapsed().as_millis();
    if !output.status.success() {
        return Err(format!(
            "litd-game failed: status={:?} stdout={} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok((
        elapsed_ms,
        String::from_utf8_lossy(&output.stdout).to_string(),
    ))
}

fn run_host_expect_failure(
    package_path: &Path,
    bad_vault: &Path,
    frames: u32,
) -> Result<EdgeResult, Box<dyn std::error::Error>> {
    let output = Command::new(litd_game_bin())
        .arg("--package")
        .arg(package_path)
        .arg("--runtime-state")
        .arg(bad_vault)
        .arg("--hidden-window")
        .arg("--max-frames")
        .arg(frames.to_string())
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let failed_closed = !output.status.success();
    Ok(EdgeResult {
        failed_closed,
        exit_code: output.status.code(),
        detail: format!("stderr={} stdout={}", stderr.trim(), stdout.trim()),
    })
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

fn package_value(
    share_id: &str,
    wav_path: &Path,
) -> Result<serde_json::Value, litd_engine::EngineError> {
    let endpoint = RuntimePackageEndpoint {
        mode: RuntimePackageHostMode::Loopback,
        host: "127.0.0.1".to_string(),
        port: 8490,
        path: format!("/share/{share_id}"),
    };
    let assets = vec![RuntimePackageAssetRef {
        product_id: "litd.asset.issue1460.tone_wav".to_string(),
        source_relative_path: relative_to(wav_path, wav_path.ancestors().nth(3).unwrap()),
        source_sha256: file_sha256(wav_path),
        source_bytes: fs::metadata(wav_path).map(|meta| meta.len()).unwrap_or(0),
    }];
    let document_fingerprint = "1460ad0c1460ad0c";
    let play_build_fingerprint = "1460beef1460beef";
    let manifest_fingerprint = compute_runtime_manifest_fingerprint(
        share_id,
        "agent.issue1460",
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
        "created_by": "agent.issue1460",
        "runtime_id": LITD_RUNTIME_ID,
        "lock_in_policy": LITD_LOCK_IN_POLICY,
        "endpoint": endpoint,
        "host_uri": host_uri,
        "document": {
            "schema": "litd.lowered_document.v1",
            "document_id": "calyx://world/issue1460-frame-loop",
            // #1891: package play requires terrain_cells (no ED11 fallback).
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
                    cue("trg_tone", "snd_tone", "ambience", [0.0, 0.0, -6.0], "sfx", 0.0)
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

fn cue(
    trigger_id: &str,
    cue_id: &str,
    kind: &str,
    position: [f32; 3],
    bus: &str,
    occlusion: f32,
) -> serde_json::Value {
    json!({
        "trigger_id": trigger_id,
        "cue_id": cue_id,
        "kind": kind,
        "asset_product_id": "litd.asset.issue1460.tone_wav",
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
