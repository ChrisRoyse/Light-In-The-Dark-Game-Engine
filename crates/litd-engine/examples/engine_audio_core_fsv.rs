use std::{f32::consts::PI, path::Path, thread, time::Duration};

use litd_engine::{
    EngineError,
    audio::{AudioBus, AudioPlaybackRequest, AudioTween, EngineAudio, write_audio_snapshot},
    fsv_artifacts::begin_issue_run,
};
use serde_json::json;

fn main() -> Result<(), EngineError> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1126_kira_audio_core").map_err(io_audio_error)?;
    let root = run.run_dir();
    let wav_path = root.join("issue1126_synthetic_chord.wav");
    let snapshot_path = root.join("live-audio-mixer-snapshot.json");
    write_test_wav(&wav_path, 523.25, 900).map_err(|error| EngineError::AudioOperationFailed {
        op: "fsv.write_wav".to_string(),
        reason: error.to_string(),
    })?;

    let mut audio = EngineAudio::new()?;
    let before = audio.snapshot();
    let requests = [
        playback("snd_fsv_sfx", &wav_path, AudioBus::Sfx, -6.0, 1.0, 25),
        playback("snd_fsv_music", &wav_path, AudioBus::Music, -12.0, 0.75, 80),
        playback("snd_fsv_voice", &wav_path, AudioBus::Voice, -9.0, 1.25, 40),
    ];
    for request in requests {
        audio.play_static(request)?;
    }
    audio.set_bus_volume(AudioBus::Sfx, -3.0, AudioTween { duration_ms: 30 })?;
    audio.set_bus_volume(AudioBus::Music, -18.0, AudioTween { duration_ms: 120 })?;
    audio.set_bus_volume(AudioBus::Voice, -6.0, AudioTween { duration_ms: 60 })?;
    thread::sleep(Duration::from_millis(350));
    let after = audio.snapshot();
    let snapshot_readback = write_audio_snapshot(&snapshot_path, &after)?;

    let report = json!({
        "schema_version": 1,
        "issue": 1126,
        "source_of_truth": {
            "wav_path": wav_path,
            "snapshot_path": snapshot_path,
            "backend": snapshot_readback.backend,
        },
        "happy_path": {
            "before": before,
            "after": snapshot_readback,
        },
        "manual_observation": {
            "expected": "three short synthetic tones routed through sfx/music/voice buses with bus tweens applied",
            "heard_by_agent": "pending Synapse/manual FSV"
        }
    });
    let readback = run.write_report(&report).map_err(io_audio_error)?;
    println!(
        "ISSUE1126_REAL_AUDIO_FSV report={} bytes={} sha256={} snapshot={} wav={}",
        readback.path.display(),
        readback.bytes.len(),
        readback.sha256,
        snapshot_path.display(),
        wav_path.display()
    );
    Ok(())
}

fn playback(
    sound_id: &str,
    path: &Path,
    bus: AudioBus,
    volume_db: f32,
    playback_rate: f64,
    fade_ms: u64,
) -> AudioPlaybackRequest {
    AudioPlaybackRequest {
        sound_id: sound_id.to_string(),
        path: path.display().to_string(),
        bus,
        volume_db,
        playback_rate,
        fade_in: Some(AudioTween {
            duration_ms: fade_ms,
        }),
    }
}

fn write_test_wav(path: &Path, frequency_hz: f32, duration_ms: u32) -> hound::Result<()> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    let frame_count = spec.sample_rate * duration_ms / 1000;
    for frame in 0..frame_count {
        let t = frame as f32 / spec.sample_rate as f32;
        let left = (t * frequency_hz * 2.0 * PI).sin() * 0.22;
        let right = (t * frequency_hz * 1.5 * 2.0 * PI).sin() * 0.18;
        writer.write_sample((left * i16::MAX as f32) as i16)?;
        writer.write_sample((right * i16::MAX as f32) as i16)?;
    }
    writer.finalize()
}

fn io_audio_error(error: std::io::Error) -> EngineError {
    EngineError::AudioOperationFailed {
        op: "fsv.artifact".to_string(),
        reason: error.to_string(),
    }
}
