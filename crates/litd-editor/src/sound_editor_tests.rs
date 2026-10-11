use std::{fs, path::PathBuf};

use super::*;
use crate::{asset_vault::AssetVaultCatalog, history::to_stable_ron};

#[test]
fn sound_editor_imports_real_wav_resolves_trigger_and_preview() {
    let (_temp_root, wav_path) = temp_wav_path("happy");
    write_pcm_wav(&wav_path, 44_100, 1, 16, 4_410);
    let entry =
        import_sound_file(&wav_path, SoundCategory::Imported, "snd_ashen_bell_asset").unwrap();
    let record = SoundEditorRecord::new(
        vec![entry],
        vec![
            NamedSoundVariable {
                variable_name: "snd_ashen_bell".to_string(),
                asset_id: "snd_ashen_bell_asset".to_string(),
                playback: two_d_params(),
            },
            NamedSoundVariable {
                variable_name: "snd_spatial_gate".to_string(),
                asset_id: "snd_ashen_bell_asset".to_string(),
                playback: three_d_params(),
            },
        ],
        vec![TriggerSoundBinding {
            trigger_id: "trg_intro".to_string(),
            action_id: "act_play_bell".to_string(),
            variable_name: "snd_ashen_bell".to_string(),
        }],
    )
    .unwrap();
    let runtime = build_sound_editor_runtime(&record, Some("snd_ashen_bell"), 40).unwrap();
    let stable = to_stable_ron(&runtime).unwrap();

    assert_eq!(record.library[0].length_ms, 100);
    assert_eq!(record.library[0].codec, SoundAudioCodec::PcmWav);
    assert_eq!(record.library[0].sample_rate_hz, 44_100);
    assert_eq!(record.library[0].channels, 1);
    assert_eq!(record.library[0].bits_per_sample, Some(16));
    assert_eq!(
        runtime.library_rows[0].variable_names,
        vec!["snd_ashen_bell".to_string(), "snd_spatial_gate".to_string()]
    );
    assert_eq!(
        runtime.resolved_trigger_bindings[0].variable_name,
        "snd_ashen_bell"
    );
    assert_eq!(runtime.resolved_trigger_bindings[0].length_ms, 100);
    let preview = runtime.preview.unwrap();
    assert_eq!(preview.state, PreviewPlaybackState::Playing);
    assert_eq!(preview.cursor_ms, 40);
    assert!(stable.contains("snd_ashen_bell"));
}

#[test]
fn sound_editor_projects_real_kenney_ogg_vault_records() {
    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    let footstep = real_record(
        &catalog,
        "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg",
    );
    let click = real_record(
        &catalog,
        "extracted/kenney/audio/interface-sounds/Audio/click_001.ogg",
    );
    let ambient = real_record(&catalog, "extracted/kenney/audio/rpg-audio/Preview.ogg");
    let footstep_entry = sound_library_entry_from_vault_record(&footstep).unwrap();
    let click_entry = sound_library_entry_from_vault_record(&click).unwrap();
    let ambient_entry = sound_library_entry_from_vault_record(&ambient).unwrap();

    assert_eq!(footstep_entry.codec, SoundAudioCodec::OggVorbis);
    assert_eq!(footstep_entry.category, SoundCategory::Units);
    assert_eq!(click_entry.category, SoundCategory::Interface);
    assert_eq!(ambient_entry.category, SoundCategory::Ambient);
    assert!(footstep_entry.length_ms > 0);
    assert!(footstep_entry.sample_rate_hz > 0);
    assert_eq!(footstep_entry.bits_per_sample, None);

    let record = SoundEditorRecord::new(
        vec![footstep_entry.clone(), click_entry],
        vec![NamedSoundVariable {
            variable_name: "snd_unit_footstep".to_string(),
            asset_id: footstep_entry.asset_id.clone(),
            playback: two_d_params(),
        }],
        vec![TriggerSoundBinding {
            trigger_id: "trg_unit_step".to_string(),
            action_id: "act_play_step".to_string(),
            variable_name: "snd_unit_footstep".to_string(),
        }],
    )
    .unwrap();
    let runtime = build_sound_editor_runtime(&record, Some("snd_unit_footstep"), 0).unwrap();

    assert_eq!(runtime.library_rows.len(), 2);
    assert_eq!(
        runtime.resolved_trigger_bindings[0].asset_id,
        footstep_entry.asset_id
    );
    assert_eq!(
        runtime.preview.unwrap().source_asset_id,
        footstep_entry.asset_id
    );
}

#[test]
fn three_d_sound_carries_position_params_into_trigger_resolution() {
    let record = fixture_record("three_d");
    let runtime = build_sound_editor_runtime(&record, Some("snd_spatial_gate"), 0).unwrap();
    let resolved = runtime
        .resolved_trigger_bindings
        .iter()
        .find(|binding| binding.variable_name == "snd_spatial_gate")
        .unwrap();

    assert_eq!(
        resolved.playback.spatial,
        SoundSpatialMode::ThreeD {
            position: [12.0, 3.5, -8.0],
            min_distance_m: 2.0,
            max_distance_m: 48.0,
        }
    );
    assert_eq!(
        runtime.preview.unwrap().playback.spatial,
        SoundSpatialMode::ThreeD {
            position: [12.0, 3.5, -8.0],
            min_distance_m: 2.0,
            max_distance_m: 48.0,
        }
    );
}

#[test]
fn missing_file_invalid_variable_and_seek_edges_fail_loud() {
    // #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
    let _temp_root = litd_engine::test_temp::TestTempDir::new("issue1277-missing");
    let missing = _temp_root.path().join("litd_issue1277_missing.wav");
    let error = import_sound_file(&missing, SoundCategory::Imported, "snd_missing")
        .unwrap_err()
        .to_string();
    assert!(error.contains("editor.sound_editor.import.read"));

    let mut record = fixture_record("invalid_name");
    record.named_sounds[0].variable_name = "bad-name".to_string();
    let error = record.validate().unwrap_err().to_string();
    assert!(error.contains("editor.sound_editor.named_sounds.variable_name"));

    let record = fixture_record("seek");
    let error = build_sound_editor_runtime(&record, Some("snd_ashen_bell"), 101)
        .unwrap_err()
        .to_string();
    assert!(error.contains("editor.sound_editor.preview.cursor_ms"));

    let catalog = AssetVaultCatalog::load_default_cached().unwrap();
    let non_audio = real_record(
        &catalog,
        "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf",
    );
    let error = sound_library_entry_from_vault_record(&non_audio)
        .unwrap_err()
        .to_string();
    assert!(error.contains("editor.sound_editor.vault.asset_type"));
}

fn fixture_record(label: &str) -> SoundEditorRecord {
    let (_temp_root, wav_path) = temp_wav_path(label);
    write_pcm_wav(&wav_path, 44_100, 1, 16, 4_410);
    let entry =
        import_sound_file(&wav_path, SoundCategory::Imported, "snd_ashen_bell_asset").unwrap();
    SoundEditorRecord::new(
        vec![entry],
        vec![
            NamedSoundVariable {
                variable_name: "snd_ashen_bell".to_string(),
                asset_id: "snd_ashen_bell_asset".to_string(),
                playback: two_d_params(),
            },
            NamedSoundVariable {
                variable_name: "snd_spatial_gate".to_string(),
                asset_id: "snd_ashen_bell_asset".to_string(),
                playback: three_d_params(),
            },
        ],
        vec![
            TriggerSoundBinding {
                trigger_id: "trg_intro".to_string(),
                action_id: "act_play_bell".to_string(),
                variable_name: "snd_ashen_bell".to_string(),
            },
            TriggerSoundBinding {
                trigger_id: "trg_ambush".to_string(),
                action_id: "act_play_spatial_gate".to_string(),
                variable_name: "snd_spatial_gate".to_string(),
            },
        ],
    )
    .unwrap()
}

fn two_d_params() -> SoundPlaybackParams {
    SoundPlaybackParams {
        volume: 0.75,
        pitch: 1.0,
        channel: SoundChannel::Effects,
        spatial: SoundSpatialMode::TwoD,
        looping: false,
        fade_in_ms: 0,
        fade_out_ms: 120,
    }
}

fn three_d_params() -> SoundPlaybackParams {
    SoundPlaybackParams {
        volume: 0.55,
        pitch: 0.95,
        channel: SoundChannel::Ambient,
        spatial: SoundSpatialMode::ThreeD {
            position: [12.0, 3.5, -8.0],
            min_distance_m: 2.0,
            max_distance_m: 48.0,
        },
        looping: true,
        fade_in_ms: 250,
        fade_out_ms: 500,
    }
}

// #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
fn temp_wav_path(label: &str) -> (litd_engine::test_temp::TestTempDir, PathBuf) {
    let temp = litd_engine::test_temp::TestTempDir::new(&format!("issue1277-sound-{label}"));
    let path = temp.path().join("litd_issue1277_sound_editor.wav");
    (temp, path)
}

fn write_pcm_wav(
    path: &PathBuf,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    sample_frames: u32,
) {
    let bytes_per_sample = u32::from(bits_per_sample / 8);
    let block_align = u32::from(channels) * bytes_per_sample;
    let data_bytes = sample_frames * block_align;
    let byte_rate = sample_rate * block_align;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&(block_align as u16).to_le_bytes());
    bytes.extend_from_slice(&bits_per_sample.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    bytes.resize(bytes.len() + data_bytes as usize, 0);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

fn real_record(
    catalog: &AssetVaultCatalog,
    relative_path: &str,
) -> crate::asset_vault::AssetVaultRecord {
    catalog
        .records()
        .iter()
        .find(|record| record.relative_path == relative_path)
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "{}",
                crate::asset_vault_fixture::missing_fixture(relative_path)
            )
        })
}
