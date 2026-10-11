use std::{
    f32::consts::PI,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use litd_editor::{
    asset_products::{AssetProduct, AssetProductKind},
    history::{EditorDocument, EditorObject, stable_fingerprint},
    spatial_audio_emitters::{
        SpatialAudioEmitterKind, SpatialAudioEmitterPlacementRequest,
        collect_spatial_audio_emitters, place_spatial_audio_emitter_in_document,
    },
};
use litd_engine::{
    audio::AudioBus,
    fsv_artifacts::begin_issue_run,
    game_package::{
        LITD_LOCK_IN_POLICY, LITD_QUICK_SHARE_SCHEMA, LITD_RUNTIME_ID, RuntimePackageAssetRef,
        RuntimePackageEndpoint, RuntimePackageHostMode, compute_runtime_manifest_fingerprint,
    },
    math::{Transform, Vec3},
};
use litd_hash::sha256_hex;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue869_spatial_audio_emitters")?;
    let audio_dir = run.run_dir().join("assets/audio");
    fs::create_dir_all(&audio_dir)?;
    let wav_path = audio_dir.join("issue869_chime.wav");
    write_test_wav(&wav_path, 329.63, 2_200)?;
    let wav_path = fs::canonicalize(&wav_path)?;

    let mut document = EditorDocument::new(
        "calyx://world/issue869-spatial-audio-emitters",
        "rev-issue869-fsv",
    )?;
    document.upsert_object(EditorObject::new(
        "world",
        "WorldRoot",
        None,
        Transform::from_translation(Vec3::ZERO),
    ))?;
    let product = audio_product(&wav_path)?;
    document
        .asset_products
        .insert(product.product_id.clone(), product.clone());

    let placement_reports = vec![
        place_spatial_audio_emitter_in_document(
            &mut document,
            &request(
                "audio.crypt_gate",
                "trg_crypt_gate",
                "snd_crypt_gate",
                SpatialAudioEmitterKind::PositionalSound,
                AudioBus::Sfx,
                Vec3::new(4.0, 0.0, -2.0),
                Vec3::new(0.0, 0.0, -6.0),
                0.2,
            ),
        )?,
        place_spatial_audio_emitter_in_document(
            &mut document,
            &request(
                "audio.low_score",
                "trg_low_score",
                "snd_low_score",
                SpatialAudioEmitterKind::Music,
                AudioBus::Music,
                Vec3::new(0.0, 0.0, -12.0),
                Vec3::ZERO,
                0.0,
            ),
        )?,
        place_spatial_audio_emitter_in_document(
            &mut document,
            &request(
                "audio.damp_cavern",
                "trg_damp_cavern",
                "snd_damp_cavern",
                SpatialAudioEmitterKind::Ambience,
                AudioBus::Sfx,
                Vec3::new(-7.0, 0.0, 3.0),
                Vec3::ZERO,
                0.35,
            ),
        )?,
    ];
    let emitters = collect_spatial_audio_emitters(&document)?;
    if emitters.len() != 3 {
        return Err(format!("expected 3 editor emitters, got {}", emitters.len()).into());
    }

    let package_path = run.run_dir().join("issue869-audio.litdgame.json");
    let state_path = run.run_dir().join("runtime-state.json");
    let report_path = run.run_dir().join("manual-fsv-report.json");
    let package = package_value("issue869audio", &document, &wav_path)?;
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

    let state_bytes = fs::read(&state_path)?;
    let state: serde_json::Value = serde_json::from_slice(&state_bytes)?;
    let audio = state
        .get("audio")
        .ok_or("runtime state did not contain audio")?;
    let report = json!({
        "schema_version": 1,
        "issue": 869,
        "source_of_truth": {
            "package_path": package_path,
            "runtime_state_path": state_path,
            "wav_path": wav_path,
            "litd_game_stdout": String::from_utf8_lossy(&output.stdout),
        },
        "editor_document": {
            "fingerprint": stable_fingerprint(&document)?,
            "object_count": document.objects.len(),
            "sound_emitter_count": emitters.len(),
            "sound_emitters": emitters,
        },
        "placement_reports": placement_reports,
        "observed_audio": audio,
        "assertions": {
            "runtime_triggered_count": audio["triggered_count"],
            "runtime_cue_count": audio["cue_count"],
            "runtime_spatial_snapshot_present": audio["spatial_snapshot"].is_object(),
            "no_code_sound_emitter_objects": true,
            "minimal_scene_no_textures": true
        }
    });
    let bytes = serde_json::to_vec_pretty(&report)?;
    fs::write(&report_path, &bytes)?;
    let readback: serde_json::Value = serde_json::from_slice(&fs::read(&report_path)?)?;
    if readback != report {
        return Err("report readback mismatch".into());
    }
    println!(
        "ISSUE869_SPATIAL_AUDIO_EMITTERS_FSV report={} bytes={} sha256={} state={} state_sha256={} wav={} wav_sha256={} stdout={}",
        report_path.display(),
        bytes.len(),
        sha256_hex(&bytes),
        state_path.display(),
        sha256_hex(&state_bytes),
        wav_path.display(),
        file_sha256(&wav_path),
        String::from_utf8_lossy(&output.stdout).trim()
    );
    Ok(())
}

// Positional fixture constructor for SpatialAudioEmitterPlacementRequest; struct-of-args would
// only re-spell the target struct's own fields.
#[allow(clippy::too_many_arguments)]
fn request(
    object_id: &str,
    trigger_id: &str,
    cue_id: &str,
    kind: SpatialAudioEmitterKind,
    bus: AudioBus,
    position: Vec3,
    velocity: Vec3,
    occlusion_factor: f32,
) -> SpatialAudioEmitterPlacementRequest {
    SpatialAudioEmitterPlacementRequest {
        object_id: object_id.to_string(),
        parent_id: None,
        product_id: "litd.asset.issue869.audio.chime".to_string(),
        trigger_id: trigger_id.to_string(),
        cue_id: cue_id.to_string(),
        kind,
        bus,
        position,
        velocity,
        direction: Vec3::new(0.0, 0.0, -1.0),
        cone_inner_degrees: 90.0,
        cone_outer_degrees: 180.0,
        min_distance_meters: 1.0,
        max_distance_meters: 32.0,
        spatialization_strength: 0.85,
        volume_db: -9.0,
        playback_rate: 1.0,
        occlusion_factor,
    }
}

fn audio_product(wav_path: &Path) -> Result<AssetProduct, Box<dyn std::error::Error>> {
    let bytes = fs::read(wav_path)?;
    let product = AssetProduct {
        product_id: "litd.asset.issue869.audio.chime".to_string(),
        product_kind: AssetProductKind::Audio,
        source: "issue869-fsv".to_string(),
        asset_id: "chime".to_string(),
        title: "Issue 869 Chime".to_string(),
        license: "CC0-1.0".to_string(),
        license_url: "https://creativecommons.org/publicdomain/zero/1.0/".to_string(),
        source_url: "local-fsv".to_string(),
        source_relative_path: "assets/audio/issue869_chime.wav".to_string(),
        source_absolute_path: wav_path.display().to_string(),
        source_sha256: sha256_hex(&bytes),
        source_bytes: bytes.len() as u64,
        builder_id: "litd.asset.vault_product_builder".to_string(),
        builder_fingerprint: "asset-products-v1".to_string(),
        deterministic_sub_id: "issue869chime".to_string(),
        dependencies: Vec::new(),
    };
    product.validate()?;
    Ok(product)
}

fn package_value(
    share_id: &str,
    document: &EditorDocument,
    wav_path: &Path,
) -> Result<serde_json::Value, litd_engine::EngineError> {
    let endpoint = RuntimePackageEndpoint {
        mode: RuntimePackageHostMode::Loopback,
        host: "127.0.0.1".to_string(),
        port: 8690,
        path: format!("/share/{share_id}"),
    };
    let assets = vec![RuntimePackageAssetRef {
        product_id: "litd.asset.issue869.audio.chime".to_string(),
        source_relative_path: "assets/audio/issue869_chime.wav".to_string(),
        source_sha256: file_sha256(wav_path),
        source_bytes: fs::metadata(wav_path).map(|meta| meta.len()).unwrap_or(0),
    }];
    let document_fingerprint = stable_fingerprint(document)?;
    let play_build_fingerprint = "0869beef0869beef";
    let manifest_fingerprint = compute_runtime_manifest_fingerprint(
        share_id,
        "agent.issue869",
        LITD_RUNTIME_ID,
        &endpoint,
        &document_fingerprint,
        play_build_fingerprint,
        &assets,
    )?;
    let host_uri = endpoint.uri();
    Ok(json!({
        "schema": LITD_QUICK_SHARE_SCHEMA,
        "share_id": share_id,
        "package_id": format!("litd.quick_share.{share_id}"),
        "created_by": "agent.issue869",
        "runtime_id": LITD_RUNTIME_ID,
        "lock_in_policy": LITD_LOCK_IN_POLICY,
        "endpoint": endpoint,
        "host_uri": host_uri,
        "document": document,
        "document_fingerprint": document_fingerprint,
        "play_build_fingerprint": play_build_fingerprint,
        "asset_count": assets.len(),
        "assets": assets,
        "manifest_fingerprint": manifest_fingerprint
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
