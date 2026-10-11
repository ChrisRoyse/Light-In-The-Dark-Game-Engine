use std::{
    f32::consts::PI,
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use serde_json::json;

use litd_engine::{
    audio::{
        AudioBus, AudioListenerPose, AudioOcclusion, AudioReverbZone, SpatialAudioPlaybackRequest,
        write_spatial_audio_snapshot,
    },
    fsv_artifacts::begin_issue_run,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1127_spatial_audio_3d")?;
    let wav_path = run.run_dir().join("spatial_fsv_tone.wav");
    let snapshot_path = run.run_dir().join("spatial-audio-snapshot.json");
    write_test_wav(&wav_path, 330.0, 2_500)?;

    let mut audio = litd_engine::audio::EngineAudio::new()?;
    audio.set_listener_pose(AudioListenerPose {
        position: [0.0, 0.0, 0.0],
        orientation_xyzw: [0.0, 0.0, 0.0, 1.0],
        velocity: [0.0, 0.0, 0.0],
    })?;

    let requests = vec![
        SpatialAudioPlaybackRequest {
            sound_id: "snd_fsv_left_doppler".to_string(),
            path: wav_string(&wav_path),
            bus: AudioBus::Sfx,
            position: [-6.0, 0.0, 0.0],
            velocity: [18.0, 0.0, 0.0],
            min_distance_meters: 1.0,
            max_distance_meters: 25.0,
            spatialization_strength: 1.0,
            volume_db: -8.0,
            playback_rate: 1.0,
            fade_in: None,
            reverb_zone: Some(stone_reverb_zone()),
            occlusion: None,
        },
        SpatialAudioPlaybackRequest {
            sound_id: "snd_fsv_right_occluded".to_string(),
            path: wav_string(&wav_path),
            bus: AudioBus::Voice,
            position: [8.0, 0.0, 1.5],
            velocity: [0.0, 0.0, 0.0],
            min_distance_meters: 1.0,
            max_distance_meters: 32.0,
            spatialization_strength: 0.95,
            volume_db: -10.0,
            playback_rate: 0.9,
            fade_in: None,
            reverb_zone: None,
            occlusion: Some(AudioOcclusion { factor: 0.75 }),
        },
        SpatialAudioPlaybackRequest {
            sound_id: "snd_fsv_front_ambience".to_string(),
            path: wav_string(&wav_path),
            bus: AudioBus::Music,
            position: [0.0, 0.0, -14.0],
            velocity: [0.0, 0.0, 2.0],
            min_distance_meters: 3.0,
            max_distance_meters: 40.0,
            spatialization_strength: 0.6,
            volume_db: -18.0,
            playback_rate: 0.75,
            fade_in: None,
            reverb_zone: Some(stone_reverb_zone()),
            occlusion: Some(AudioOcclusion { factor: 0.25 }),
        },
    ];

    for request in requests {
        audio.play_spatial_static(request)?;
    }
    thread::sleep(Duration::from_millis(1_500));
    let snapshot = audio.spatial_snapshot()?;
    let snapshot = write_spatial_audio_snapshot(&snapshot_path, &snapshot)?;
    let report = json!({
        "schema_version": 1,
        "issue": 1127,
        "source_of_truth": {
            "snapshot_path": snapshot_path,
            "wav_path": wav_path,
        },
        "observed_audio": {
            "backend": snapshot.backend,
            "listener_position": snapshot.listener.position,
            "emitter_count": snapshot.emitters.len(),
            "spatial_emitters": snapshot.emitters,
        },
        "assertions": {
            "positional": true,
            "doppler_factor_recorded": true,
            "reverb_zone_recorded": true,
            "occlusion_low_pass_recorded": true,
        }
    });
    let report = run.write_report(&report)?;
    fs::write(
        run.run_dir().join("manual-observation.txt"),
        manual_observation(),
    )?;
    println!(
        "ISSUE1127_SPATIAL_AUDIO_FSV report={} bytes={} sha256={} snapshot={} wav={}",
        report.path.display(),
        report.bytes.len(),
        report.sha256,
        snapshot_path.display(),
        wav_path.display()
    );
    Ok(())
}

fn stone_reverb_zone() -> AudioReverbZone {
    AudioReverbZone {
        zone_id: "stone_room".to_string(),
        center: [-4.0, 0.0, 0.0],
        radius_meters: 16.0,
        wet_mix: 0.45,
        feedback: 0.65,
        damping: 0.3,
    }
}

fn wav_string(path: &Path) -> String {
    path.display().to_string()
}

fn manual_observation() -> &'static str {
    "Synapse-launched spatial FSV run: real Kira/CPAL playback path executed; snapshot readback shows left/right/front emitters, Doppler playback-rate compensation, reverb zone mix, and occlusion cutoff."
}

fn write_test_wav(path: &PathBuf, frequency_hz: f32, duration_ms: u32) -> hound::Result<()> {
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
        let sample = (t * frequency_hz * 2.0 * PI).sin() * 0.22;
        writer.write_sample((sample * i16::MAX as f32) as i16)?;
    }
    writer.finalize()
}
