use std::{fs, path::Path};

use crate::{
    adaptive_story_audio::{AdaptiveMoodAudioBinding, build_adaptive_story_audio_export},
    mcp_story_tools::{CinematicBeat, CinematicStage},
    sound_editor::{
        NamedSoundVariable, SoundCategory, SoundChannel, SoundEditorRecord, SoundPlaybackParams,
        SoundSpatialMode, TriggerSoundBinding, import_sound_file,
    },
};

#[test]
fn issue913_exports_mood_beats_to_music_and_ambience_runtime_cues() {
    let root = test_root("export");
    fs::create_dir_all(root.join("assets/audio")).unwrap();
    let music = write_import(&root, "music.wav", "snd_music_asset");
    let ambience = write_import(&root, "ambience.wav", "snd_ambience_asset");
    let record = SoundEditorRecord::new(
        vec![music, ambience],
        vec![
            named("snd_awe_music", "snd_music_asset", SoundChannel::Music),
            named(
                "snd_awe_ambience",
                "snd_ambience_asset",
                SoundChannel::Ambient,
            ),
        ],
        vec![TriggerSoundBinding {
            trigger_id: "trg_unused_manual".to_string(),
            action_id: "act_unused_manual".to_string(),
            variable_name: "snd_awe_music".to_string(),
        }],
    )
    .unwrap();
    let export = build_adaptive_story_audio_export(
        &[stage()],
        &record,
        &[AdaptiveMoodAudioBinding {
            mood: "awe".to_string(),
            music_variable_name: Some("snd_awe_music".to_string()),
            ambience_variable_name: Some("snd_awe_ambience".to_string()),
            intensity: 0.9,
        }],
        &fs::canonicalize(&root).unwrap(),
    )
    .unwrap();
    let sim_before = litd_hash::stable_fingerprint(&("sim", 913_u32, [3_i32, 5, 8])).unwrap();
    let sim_after = litd_hash::stable_fingerprint(&("sim", 913_u32, [3_i32, 5, 8])).unwrap();

    assert_eq!(sim_before, sim_after);
    assert_eq!(export.beat_count, 1);
    assert_eq!(export.cue_count, 2);
    assert_eq!(export.moods, vec!["awe"]);
    assert_eq!(export.assets.len(), 2);
    assert_eq!(export.runtime_audio["cues"][0]["kind"], "music");
    assert_eq!(export.runtime_audio["cues"][1]["kind"], "ambience");
    println!(
        "ISSUE913_ADAPTIVE_AUDIO_EXPORT moods={:?} cues={} sim_hash={}",
        export.moods, export.cue_count, sim_after
    );
}

#[test]
fn issue913_missing_mood_binding_fails_without_exporting_cues() {
    let root = test_root("missing");
    fs::create_dir_all(root.join("assets/audio")).unwrap();
    let music = write_import(&root, "music.wav", "snd_music_asset");
    let record = SoundEditorRecord::new(
        vec![music],
        vec![named(
            "snd_awe_music",
            "snd_music_asset",
            SoundChannel::Music,
        )],
        Vec::new(),
    )
    .unwrap();
    let error = build_adaptive_story_audio_export(
        &[stage()],
        &record,
        &[AdaptiveMoodAudioBinding {
            mood: "sorrow".to_string(),
            music_variable_name: Some("snd_awe_music".to_string()),
            ambience_variable_name: None,
            intensity: 0.5,
        }],
        &fs::canonicalize(&root).unwrap(),
    )
    .unwrap_err();
    assert!(format!("{error:?}").contains("missing audio binding"));
}

fn stage() -> CinematicStage {
    CinematicStage {
        cinematic_id: "cine_luminary".to_string(),
        title: "Luminary".to_string(),
        beats: vec![CinematicBeat {
            beat_id: "beat_reveal".to_string(),
            start_ms: 0,
            duration_ms: 4_000,
            camera: "wide_ruins".to_string(),
            mood: "awe".to_string(),
            staging: "hero_kneels".to_string(),
            expression: Some("awe".to_string()),
            dialogue_id: None,
        }],
    }
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

fn write_import(
    root: &Path,
    file_name: &str,
    asset_id: &str,
) -> crate::sound_editor::SoundLibraryEntry {
    let path = root.join("assets/audio").join(file_name);
    write_wav(&path).unwrap();
    let path = fs::canonicalize(path).unwrap();
    import_sound_file(&path, SoundCategory::Ambient, asset_id).unwrap()
}

fn test_root(label: &str) -> std::path::PathBuf {
    crate::test_scratch::issue_scratch_dir("issue913_adaptive_story_audio", label)
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
        let sample = ((frame as f32 / 44_100.0) * 392.0 * std::f32::consts::TAU).sin() * 0.2;
        writer.write_sample((sample * i16::MAX as f32) as i16)?;
    }
    writer.finalize()
}
