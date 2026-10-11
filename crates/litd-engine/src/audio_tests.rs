use std::{f32::consts::PI, path::Path};

use serde_json::json;

use crate::{
    audio::{AudioBus, AudioPlaybackRequest, AudioTween, EngineAudio, write_audio_snapshot},
    fsv_artifacts::begin_issue_run,
};

#[test]
fn issue1126_real_kira_mixer_routing_and_snapshot_readback() {
    let run = begin_issue_run("issue1126_kira_audio_core").unwrap();
    let snapshot_path = run.run_dir().join("audio-mixer-snapshot.json");
    let wav_path = run.run_dir().join("synthetic_440hz.wav");
    write_test_wav(&wav_path, 440.0, 800).unwrap();

    let happy_request = AudioPlaybackRequest {
        sound_id: "snd_synthetic_chime".to_string(),
        path: wav_path.display().to_string(),
        bus: AudioBus::Sfx,
        volume_db: -6.0,
        playback_rate: 1.25,
        fade_in: Some(AudioTween { duration_ms: 40 }),
    };
    happy_request.validate().unwrap();

    let mut audio = EngineAudio::new().unwrap();
    let before = audio.snapshot();
    audio
        .set_bus_volume(AudioBus::Sfx, -9.0, AudioTween::immediate())
        .unwrap();
    let _sound = audio.play_static(happy_request.clone()).unwrap();
    let after = audio.snapshot();
    let sfx = after
        .buses
        .iter()
        .find(|bus| bus.bus == AudioBus::Sfx)
        .unwrap();
    assert_eq!(before.played_sounds.len(), 0);
    assert_eq!(after.played_sounds.len(), 1);
    assert_eq!(sfx.volume_db, -9.0);
    assert_eq!(sfx.active_sounds, 1);
    assert!(
        after
            .buses
            .iter()
            .filter(|bus| bus.bus != AudioBus::Sfx)
            .all(|bus| bus.active_sounds == 0)
    );
    let readback = write_audio_snapshot(&snapshot_path, &after).unwrap();
    assert_eq!(readback, after);

    let empty_sound = invalid_request("", &wav_path, AudioBus::Sfx, -6.0, 1.0);
    let bad_volume = invalid_request("snd_bad_volume", &wav_path, AudioBus::Music, f32::NAN, 1.0);
    let bad_pitch = invalid_request("snd_bad_pitch", &wav_path, AudioBus::Voice, -6.0, 9.0);
    let bad_tween = AudioTween {
        duration_ms: 60_001,
    };

    let report = json!({
        "schema_version": 1,
        "issue": 1126,
        "source_of_truth": {
            "snapshot_path": snapshot_path,
            "wav_path": wav_path,
        },
        "happy_path": {
            "before": before,
            "request": happy_request,
            "after": readback,
        },
        "edges": {
            "empty_sound_id": {
                "before": "no request accepted",
                "after": error_code(empty_sound.validate().unwrap_err()),
            },
            "nan_volume": {
                "before": "no request accepted",
                "after": error_code(bad_volume.validate().unwrap_err()),
            },
            "invalid_playback_rate": {
                "before": "no request accepted",
                "after": error_code(bad_pitch.validate().unwrap_err()),
            },
            "overlong_tween": {
                "before": "tween duration 60001 ms",
                "after": error_code(bad_tween.validate().unwrap_err()),
            },
        }
    });
    let report_readback = run.write_report(&report).unwrap();
    assert_eq!(report_readback.json["issue"], 1126);
    assert_eq!(
        report_readback.json["happy_path"]["after"]["played_sounds"][0]["bus"],
        "sfx"
    );
    println!(
        "ISSUE1126_AUDIO_CORE_READBACK report={} bytes={} sha256={} snapshot={} wav={}",
        report_readback.path.display(),
        report_readback.bytes.len(),
        report_readback.sha256,
        snapshot_path.display(),
        wav_path.display()
    );
}

fn invalid_request(
    sound_id: &str,
    path: &Path,
    bus: AudioBus,
    volume_db: f32,
    playback_rate: f64,
) -> AudioPlaybackRequest {
    AudioPlaybackRequest {
        sound_id: sound_id.to_string(),
        path: path.display().to_string(),
        bus,
        volume_db,
        playback_rate,
        fade_in: None,
    }
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
        let sample = (t * frequency_hz * 2.0 * PI).sin() * 0.25;
        writer.write_sample((sample * i16::MAX as f32) as i16)?;
    }
    writer.finalize()
}

fn error_code(error: crate::EngineError) -> &'static str {
    error.code()
}
