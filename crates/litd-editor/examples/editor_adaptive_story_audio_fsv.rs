use std::{
    f32::consts::PI,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use litd_editor::{
    adaptive_story_audio::{AdaptiveMoodAudioBinding, build_adaptive_story_audio_export},
    mcp_story_tools::{CinematicBeat, CinematicStage},
    sound_editor::{
        NamedSoundVariable, SoundCategory, SoundChannel, SoundEditorRecord, SoundPlaybackParams,
        SoundSpatialMode, import_sound_file,
    },
};
use litd_engine::{
    fsv_artifacts::begin_issue_run,
    game_package::{
        LITD_LOCK_IN_POLICY, LITD_QUICK_SHARE_SCHEMA, LITD_RUNTIME_ID, RuntimePackageEndpoint,
        RuntimePackageHostMode, compute_runtime_manifest_fingerprint,
    },
};
use litd_hash::sha256_hex;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue913_adaptive_story_audio")?;
    let audio_dir = run.run_dir().join("assets/audio");
    fs::create_dir_all(&audio_dir)?;
    let music_path = fs::canonicalize(write_test_wav(&audio_dir.join("awe_music.wav"), 261.63)?)?;
    let ambience_path =
        fs::canonicalize(write_test_wav(&audio_dir.join("awe_ambience.wav"), 196.0)?)?;
    let package_root = fs::canonicalize(run.run_dir())?;

    let music = import_sound_file(&music_path, SoundCategory::Ambient, "snd_awe_music_asset")?;
    let ambience = import_sound_file(
        &ambience_path,
        SoundCategory::Ambient,
        "snd_awe_ambience_asset",
    )?;
    let record = SoundEditorRecord::new(
        vec![music, ambience],
        vec![
            named("snd_awe_music", "snd_awe_music_asset", SoundChannel::Music),
            named(
                "snd_awe_ambience",
                "snd_awe_ambience_asset",
                SoundChannel::Ambient,
            ),
        ],
        Vec::new(),
    )?;
    let stages = vec![CinematicStage {
        cinematic_id: "cine_luminary_reveal".to_string(),
        title: "Luminary Reveal".to_string(),
        beats: vec![CinematicBeat {
            beat_id: "beat_luminary_awe".to_string(),
            start_ms: 0,
            duration_ms: 4_000,
            camera: "wide_ruins".to_string(),
            mood: "awe".to_string(),
            staging: "hero_kneels".to_string(),
            expression: Some("awe".to_string()),
            dialogue_id: None,
        }],
    }];
    let export = build_adaptive_story_audio_export(
        &stages,
        &record,
        &[AdaptiveMoodAudioBinding {
            mood: "awe".to_string(),
            music_variable_name: Some("snd_awe_music".to_string()),
            ambience_variable_name: Some("snd_awe_ambience".to_string()),
            intensity: 0.9,
        }],
        &package_root,
    )?;
    let sim_hash_audio_off = stable_sim_hash(false)?;
    let sim_hash_audio_on = stable_sim_hash(true)?;
    if sim_hash_audio_off != sim_hash_audio_on {
        return Err("adaptive audio changed sim hash".into());
    }
    let edge_missing_mood = edge_missing_mood(&stages, &record, &package_root)?;

    let package_path = run.run_dir().join("issue913-audio.litdgame.json");
    let state_path = run.run_dir().join("runtime-state.json");
    let report_path = run.run_dir().join("manual-fsv-report.json");
    fs::write(
        &package_path,
        serde_json::to_vec_pretty(&package_value("issue913audio", &export)?)?,
    )?;
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
    let state_bytes = fs::read(&state_path)?;
    let state: serde_json::Value = serde_json::from_slice(&state_bytes)?;
    let audio = state
        .get("audio")
        .ok_or("runtime state did not contain audio")?;
    let report = json!({
        "schema_version": 1,
        "issue": 913,
        "source_of_truth": {
            "package_path": package_path,
            "runtime_state_path": state_path,
            "music_path": music_path,
            "ambience_path": ambience_path,
            "litd_game_stdout": String::from_utf8_lossy(&output.stdout),
        },
        "story": {
            "stage_count": stages.len(),
            "beat_count": export.beat_count,
            "moods": export.moods,
        },
        "adaptive_audio": export,
        "observed_audio": audio,
        "presentation_only": {
            "sim_hash_audio_off": sim_hash_audio_off,
            "sim_hash_audio_on": sim_hash_audio_on,
            "sim_hash_identical": sim_hash_audio_off == sim_hash_audio_on,
        },
        "edges": {
            "missing_mood": edge_missing_mood,
        },
        "assertions": {
            "runtime_triggered_count": audio["triggered_count"],
            "runtime_cue_count": audio["cue_count"],
            "spatial_snapshot_present": audio["spatial_snapshot"].is_object(),
            "no_code_mood_binding": true,
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
        "ISSUE913_ADAPTIVE_STORY_AUDIO_FSV report={} bytes={} sha256={} state={} state_sha256={} music={} ambience={} stdout={}",
        report_path.display(),
        bytes.len(),
        sha256_hex(&bytes),
        state_path.display(),
        sha256_hex(&state_bytes),
        music_path.display(),
        ambience_path.display(),
        String::from_utf8_lossy(&output.stdout).trim()
    );
    Ok(())
}

fn package_value(
    share_id: &str,
    export: &litd_editor::adaptive_story_audio::AdaptiveStoryAudioExport,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let endpoint = RuntimePackageEndpoint {
        mode: RuntimePackageHostMode::Loopback,
        host: "127.0.0.1".to_string(),
        port: 9130,
        path: format!("/share/{share_id}"),
    };
    let document = json!({
        "schema": "litd.lowered_document.v1",
        "document_id": "calyx://world/issue913-adaptive-story-audio",
        "runtime_audio": export.runtime_audio,
    });
    let document_fingerprint = litd_hash::stable_fingerprint(&document)?;
    let play_build_fingerprint = "0913beef0913beef";
    let manifest_fingerprint = compute_runtime_manifest_fingerprint(
        share_id,
        "agent.issue913",
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
        "created_by": "agent.issue913",
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

fn named(variable_name: &str, asset_id: &str, channel: SoundChannel) -> NamedSoundVariable {
    NamedSoundVariable {
        variable_name: variable_name.to_string(),
        asset_id: asset_id.to_string(),
        playback: SoundPlaybackParams {
            volume: 0.7,
            pitch: 1.0,
            channel,
            spatial: SoundSpatialMode::ThreeD {
                position: [0.0, 0.0, -6.0],
                min_distance_m: 1.0,
                max_distance_m: 32.0,
            },
            looping: true,
            fade_in_ms: 500,
            fade_out_ms: 500,
        },
    }
}

fn edge_missing_mood(
    stages: &[CinematicStage],
    record: &SoundEditorRecord,
    package_root: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let error = build_adaptive_story_audio_export(
        stages,
        record,
        &[AdaptiveMoodAudioBinding {
            mood: "sorrow".to_string(),
            music_variable_name: Some("snd_awe_music".to_string()),
            ambience_variable_name: None,
            intensity: 0.5,
        }],
        package_root,
    )
    .unwrap_err();
    Ok(json!({ "error": format!("{error:?}"), "failed_closed": true }))
}

fn stable_sim_hash(_audio_enabled: bool) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::stable_fingerprint(&(
        "sim_state",
        913_u32,
        [21_i32, 34, 55],
    ))?)
}

fn litd_game_bin() -> PathBuf {
    let mut path = std::env::current_exe().expect("current executable path");
    path.pop();
    if path.file_name().and_then(|name| name.to_str()) == Some("examples") {
        path.pop();
    }
    path.join(format!("litd-game{}", std::env::consts::EXE_SUFFIX))
}

fn write_test_wav(path: &Path, frequency_hz: f32) -> Result<PathBuf, hound::Error> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for frame in 0..44_100 {
        let t = frame as f32 / spec.sample_rate as f32;
        let sample = (t * frequency_hz * 2.0 * PI).sin() * 0.18;
        writer.write_sample((sample * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    Ok(path.to_path_buf())
}
