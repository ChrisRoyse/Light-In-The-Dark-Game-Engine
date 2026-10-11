use serde_json::json;

use crate::{
    audio::{
        AudioBus, AudioListenerPose, AudioOcclusion, AudioReverbZone, SpatialAudioPlaybackRequest,
        SpatialAudioSnapshot, SpatialEmitterSnapshot, compute_spatial_emitter_snapshot_for_test,
        write_spatial_audio_snapshot,
    },
    fsv_artifacts::begin_issue_run,
};

#[test]
fn issue1127_spatial_audio_state_and_snapshot_readback() {
    let run = begin_issue_run("issue1127_spatial_audio_3d").unwrap();
    let snapshot_path = run.run_dir().join("spatial-audio-snapshot.json");
    let listener = AudioListenerPose {
        position: [0.0, 0.0, 0.0],
        orientation_xyzw: [0.0, 0.0, 0.0, 1.0],
        velocity: [0.0, 0.0, 0.0],
    };
    let request = SpatialAudioPlaybackRequest {
        sound_id: "snd_spatial_bell".to_string(),
        path: "target/fsv/issue1127/synthetic.wav".to_string(),
        bus: AudioBus::Sfx,
        position: [10.0, 0.0, 0.0],
        velocity: [-20.0, 0.0, 0.0],
        min_distance_meters: 1.0,
        max_distance_meters: 31.0,
        spatialization_strength: 0.85,
        volume_db: -5.0,
        playback_rate: 1.0,
        fade_in: None,
        reverb_zone: Some(AudioReverbZone {
            zone_id: "stone_room".to_string(),
            center: [10.0, 0.0, 0.0],
            radius_meters: 12.0,
            wet_mix: 0.4,
            feedback: 0.6,
            damping: 0.2,
        }),
        occlusion: Some(AudioOcclusion { factor: 0.5 }),
    };

    let emitter = compute_spatial_emitter_snapshot_for_test(&request, &listener).unwrap();
    assert!(emitter.doppler_factor > 1.0);
    assert_eq!(emitter.reverb_zone_id.as_deref(), Some("stone_room"));
    assert_eq!(emitter.occlusion_cutoff_hz, 10600.0);
    assert!((emitter.attenuation_db - -30.0).abs() < f32::EPSILON);

    let snapshot = SpatialAudioSnapshot {
        schema_version: 1,
        backend: "kira.default.cpal".to_string(),
        listener: listener.clone(),
        emitters: vec![emitter],
        captured_at_unix_ms: 1127,
    };
    let readback = write_spatial_audio_snapshot(&snapshot_path, &snapshot).unwrap();
    assert_eq!(readback, snapshot);

    let invalid_distance = SpatialAudioPlaybackRequest {
        max_distance_meters: 1.0,
        min_distance_meters: 1.0,
        ..request.clone()
    };
    let invalid_occlusion = SpatialAudioPlaybackRequest {
        occlusion: Some(AudioOcclusion { factor: 1.5 }),
        ..request.clone()
    };
    let invalid_listener = AudioListenerPose {
        orientation_xyzw: [0.0, 0.0, 0.0, 0.5],
        ..listener
    };

    let report = json!({
        "schema_version": 1,
        "issue": 1127,
        "source_of_truth": {
            "snapshot_path": snapshot_path,
        },
        "happy_path": {
            "before": "listener at origin, emitter 10m right moving toward listener",
            "after": readback,
        },
        "edges": {
            "invalid_distance": error_code(invalid_distance.validate().unwrap_err()),
            "invalid_occlusion": error_code(invalid_occlusion.validate().unwrap_err()),
            "invalid_listener_orientation": error_code(invalid_listener.validate().unwrap_err()),
        }
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(report_readback.json["issue"], 1127);
    assert_eq!(
        report_readback.json["happy_path"]["after"]["emitters"][0]["bus"],
        "sfx"
    );
    println!(
        "ISSUE1127_SPATIAL_AUDIO_READBACK report={} bytes={} sha256={} snapshot={}",
        report_readback.path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        snapshot_path.display()
    );
}

fn error_code(error: crate::EngineError) -> &'static str {
    error.code()
}

#[allow(dead_code)]
fn _assert_snapshot_is_send_sync(snapshot: SpatialEmitterSnapshot) -> SpatialEmitterSnapshot {
    snapshot
}
