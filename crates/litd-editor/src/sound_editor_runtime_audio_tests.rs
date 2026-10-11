use std::{fs, path::Path};

use super::build_sound_editor_runtime_audio_export;
use crate::sound_editor::{
    NamedSoundVariable, SoundCategory, SoundChannel, SoundEditorRecord, SoundPlaybackParams,
    SoundSpatialMode, TriggerSoundBinding, import_sound_file,
};

#[test]
fn issue737_exports_trigger_sound_assignment_as_runtime_audio() {
    let root = test_root("export");
    fs::create_dir_all(root.join("assets/audio")).unwrap();
    let wav_path = root.join("assets/audio/attack.wav");
    write_wav(&wav_path).unwrap();
    let wav_path = fs::canonicalize(&wav_path).unwrap();
    let package_root = fs::canonicalize(&root).unwrap();
    let entry = import_sound_file(&wav_path, SoundCategory::Units, "snd_attack_asset").unwrap();
    let record = SoundEditorRecord::new(
        vec![entry],
        vec![NamedSoundVariable {
            variable_name: "snd_unit_attack".to_string(),
            asset_id: "snd_attack_asset".to_string(),
            playback: SoundPlaybackParams {
                volume: 0.5,
                pitch: 1.0,
                channel: SoundChannel::Effects,
                spatial: SoundSpatialMode::ThreeD {
                    position: [2.0, 0.0, -3.0],
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
    )
    .unwrap();
    let sim_hash_before = litd_hash::stable_fingerprint(&("sim", 7_u32, [1_i32, 2, 3])).unwrap();
    let export = build_sound_editor_runtime_audio_export(&record, &package_root).unwrap();
    let sim_hash_after = litd_hash::stable_fingerprint(&("sim", 7_u32, [1_i32, 2, 3])).unwrap();

    assert_eq!(sim_hash_before, sim_hash_after);
    assert_eq!(export.cue_count, 1);
    assert_eq!(export.trigger_ids, vec!["trg_unit_attack"]);
    assert_eq!(export.assets[0].product_id, "snd_attack_asset");
    assert_eq!(
        export.assets[0].source_relative_path,
        "assets\\audio\\attack.wav"
    );
    assert_eq!(export.runtime_audio["cues"][0]["cue_id"], "snd_unit_attack");
    assert_eq!(export.runtime_audio["cues"][0]["bus"], "sfx");
    assert_eq!(
        export.runtime_audio["cues"][0]["position"],
        serde_json::json!([2.0, 0.0, -3.0])
    );
    println!(
        "ISSUE737_SOUND_ASSIGNMENT_EXPORT trigger={} cue={} sim_hash={}",
        export.trigger_ids[0], export.runtime_audio["cues"][0]["cue_id"], sim_hash_after
    );
}

#[test]
fn issue737_export_fails_closed_when_audio_is_outside_package_root() {
    let package_root = test_root("package-root");
    fs::create_dir_all(&package_root).unwrap();
    let package_root = fs::canonicalize(package_root).unwrap();
    let outside_root = test_root("outside-root");
    fs::create_dir_all(outside_root.join("assets/audio")).unwrap();
    let wav_path = outside_root.join("assets/audio/attack.wav");
    write_wav(&wav_path).unwrap();
    let wav_path = fs::canonicalize(&wav_path).unwrap();
    let entry = import_sound_file(&wav_path, SoundCategory::Units, "snd_attack_asset").unwrap();
    let record = SoundEditorRecord::new(
        vec![entry],
        vec![NamedSoundVariable {
            variable_name: "snd_unit_attack".to_string(),
            asset_id: "snd_attack_asset".to_string(),
            playback: SoundPlaybackParams {
                volume: 0.5,
                pitch: 1.0,
                channel: SoundChannel::Effects,
                spatial: SoundSpatialMode::TwoD,
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
    )
    .unwrap();

    let error = build_sound_editor_runtime_audio_export(&record, &package_root).unwrap_err();
    assert!(format!("{error:?}").contains("must be inside package root"));
}

fn test_root(label: &str) -> std::path::PathBuf {
    crate::test_scratch::issue_scratch_dir("issue737_sound_assignment", label)
}

fn write_wav(path: &Path) -> hound::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for frame in 0..4_410 {
        let sample = ((frame as f32 / 44_100.0) * 523.25 * std::f32::consts::TAU).sin() * 0.2;
        writer.write_sample((sample * i16::MAX as f32) as i16)?;
    }
    writer.finalize()
}
