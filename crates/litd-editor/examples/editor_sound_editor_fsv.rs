use litd_engine::fsv_artifacts::env_path;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use litd_editor::{
    history::{stable_fingerprint, to_stable_ron},
    sound_editor::{
        NamedSoundVariable, SoundCategory, SoundChannel, SoundEditorRecord, SoundPlaybackParams,
        SoundSpatialMode, TriggerSoundBinding, build_sound_editor_runtime, import_sound_file,
        load_sound_editor_record, save_sound_editor_record, stop_preview,
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let root = env_path("LITD_EDITOR_SOUND_FSV_ROOT", "tmp/issue1277-sound-editor");
    let wav_path = root.join("imports").join("ashen_bell.wav");
    let record_path = root.join("sound_editor_record.ron");
    let runtime_path = root.join("sound_editor_runtime.ron");
    let stats_path = root.join("stats.txt");

    write_pcm_wav(&wav_path, 44_100, 1, 16, 4_410)?;
    let entry = import_sound_file(&wav_path, SoundCategory::Imported, "snd_ashen_bell_asset")?;
    let record = fixture_record(entry)?;
    let before_fingerprint = record.stable_fingerprint()?;
    save_sound_editor_record(&record_path, &record)?;
    let readback = load_sound_editor_record(&record_path)?;
    let readback_fingerprint = readback.stable_fingerprint()?;
    let runtime = build_sound_editor_runtime(&readback, Some("snd_ashen_bell"), 40)?;
    write_text(&runtime_path, &to_stable_ron(&runtime)?)?;
    let runtime_text = fs::read_to_string(&runtime_path)?;
    let runtime_fingerprint = stable_fingerprint(&runtime)?;
    let stopped = stop_preview(&readback, "snd_ashen_bell")?;

    let edge_missing_file =
        edge_missing_file(&record_path, &root.join("imports").join("missing.wav"))?;
    let edge_invalid_variable = edge_invalid_variable(&record_path)?;
    let edge_seek_past_end = edge_seek_past_end(&record_path)?;

    let bell_binding = runtime
        .resolved_trigger_bindings
        .iter()
        .find(|binding| binding.variable_name == "snd_ashen_bell")
        .expect("fixture resolves bell binding");
    let spatial_binding = runtime
        .resolved_trigger_bindings
        .iter()
        .find(|binding| binding.variable_name == "snd_spatial_gate")
        .expect("fixture resolves spatial binding");
    let preview = runtime.preview.as_ref().expect("fixture starts preview");
    let wav_hash = file_sha256(&wav_path)?;

    write_text(
        &stats_path,
        &format!(
            "issue=1277\n\
             wav_file={}\n\
             wav_sha256={}\n\
             record_file={}\n\
             runtime_file={}\n\
             record_before_fingerprint={}\n\
             record_readback_fingerprint={}\n\
             record_readback_matches={}\n\
             library_count={}\n\
             named_sound_count={}\n\
             trigger_binding_count={}\n\
             decoded_length_ms={}\n\
             decoded_sample_rate_hz={}\n\
             decoded_channels={}\n\
             decoded_bits_per_sample={:?}\n\
             decoded_bytes_on_disk={}\n\
             library_row_file={}\n\
             library_row_variables={}\n\
             trigger_bell_resolution={}:{}:{}:{}:{}\n\
             trigger_spatial_resolution={}:{}:{}:{}:{}\n\
             preview_state={:?}\n\
             preview_cursor_ms={}\n\
             preview_length_ms={}\n\
             stop_preview_state={:?}\n\
             spatial_position={}\n\
             spatial_distance={}\n\
             runtime_readback_contains_bell_binding={}\n\
             runtime_readback_contains_preview={}\n\
             runtime_readback_contains_3d_position={}\n\
             runtime_readback_fingerprint={}\n\
             edge_missing_file_error={}\n\
             edge_missing_file_before_state={}\n\
             edge_missing_file_attempted_state={}\n\
             edge_missing_file_after_state={}\n\
             edge_missing_file_before_fingerprint={}\n\
             edge_missing_file_after_fingerprint={}\n\
             edge_missing_file_record_unchanged={}\n\
             edge_invalid_variable_error={}\n\
             edge_invalid_variable_before_state={}\n\
             edge_invalid_variable_attempted_state={}\n\
             edge_invalid_variable_after_state={}\n\
             edge_invalid_variable_before_fingerprint={}\n\
             edge_invalid_variable_after_fingerprint={}\n\
             edge_invalid_variable_record_unchanged={}\n\
             edge_seek_past_end_error={}\n\
             edge_seek_past_end_before_state={}\n\
             edge_seek_past_end_attempted_state={}\n\
             edge_seek_past_end_after_state={}\n\
             edge_seek_past_end_before_fingerprint={}\n\
             edge_seek_past_end_after_fingerprint={}\n\
             edge_seek_past_end_record_unchanged={}\n",
            wav_path.display(),
            wav_hash,
            record_path.display(),
            runtime_path.display(),
            before_fingerprint,
            readback_fingerprint,
            before_fingerprint == readback_fingerprint,
            readback.library.len(),
            readback.named_sounds.len(),
            readback.trigger_bindings.len(),
            readback.library[0].length_ms,
            readback.library[0].sample_rate_hz,
            readback.library[0].channels,
            readback.library[0].bits_per_sample,
            readback.library[0].bytes_on_disk,
            runtime.library_rows[0].file_name,
            runtime.library_rows[0].variable_names.join("|"),
            bell_binding.trigger_id,
            bell_binding.action_id,
            bell_binding.variable_name,
            bell_binding.file_name,
            bell_binding.length_ms,
            spatial_binding.trigger_id,
            spatial_binding.action_id,
            spatial_binding.variable_name,
            spatial_binding.file_name,
            spatial_binding.length_ms,
            preview.state,
            preview.cursor_ms,
            preview.length_ms,
            stopped.state,
            spatial_position(spatial_binding),
            spatial_distance(spatial_binding),
            runtime_text.contains("variable_name: \"snd_ashen_bell\""),
            runtime_text.contains("state: Playing"),
            runtime_text.contains("position: (12.0, 3.5, -8.0)"),
            runtime_fingerprint,
            edge_missing_file.error_code,
            edge_missing_file.before_state,
            edge_missing_file.attempted_state,
            edge_missing_file.after_state,
            edge_missing_file.before_fingerprint,
            edge_missing_file.after_fingerprint,
            edge_missing_file.record_unchanged(),
            edge_invalid_variable.error_code,
            edge_invalid_variable.before_state,
            edge_invalid_variable.attempted_state,
            edge_invalid_variable.after_state,
            edge_invalid_variable.before_fingerprint,
            edge_invalid_variable.after_fingerprint,
            edge_invalid_variable.record_unchanged(),
            edge_seek_past_end.error_code,
            edge_seek_past_end.before_state,
            edge_seek_past_end.attempted_state,
            edge_seek_past_end.after_state,
            edge_seek_past_end.before_fingerprint,
            edge_seek_past_end.after_fingerprint,
            edge_seek_past_end.record_unchanged(),
        ),
    )?;
    println!("wrote sound editor FSV stats to {}", stats_path.display());
    Ok(())
}

fn fixture_record(
    entry: litd_editor::sound_editor::SoundLibraryEntry,
) -> Result<SoundEditorRecord, litd_engine::EngineError> {
    SoundEditorRecord::new(
        vec![entry],
        vec![
            NamedSoundVariable {
                variable_name: "snd_ashen_bell".to_string(),
                asset_id: "snd_ashen_bell_asset".to_string(),
                playback: SoundPlaybackParams {
                    volume: 0.75,
                    pitch: 1.0,
                    channel: SoundChannel::Effects,
                    spatial: SoundSpatialMode::TwoD,
                    looping: false,
                    fade_in_ms: 0,
                    fade_out_ms: 120,
                },
            },
            NamedSoundVariable {
                variable_name: "snd_spatial_gate".to_string(),
                asset_id: "snd_ashen_bell_asset".to_string(),
                playback: SoundPlaybackParams {
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
                },
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
}

#[derive(Debug)]
struct EdgeAudit {
    error_code: String,
    before_state: String,
    attempted_state: String,
    after_state: String,
    before_fingerprint: String,
    after_fingerprint: String,
}

impl EdgeAudit {
    fn record_unchanged(&self) -> bool {
        self.before_fingerprint == self.after_fingerprint && self.before_state == self.after_state
    }
}

fn edge_missing_file(
    record_path: &Path,
    missing_path: &Path,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before = load_sound_editor_record(record_path)?;
    let before_state = record_state(&before);
    let before_fingerprint = before.stable_fingerprint()?;
    let error_code = validation_error_code(
        import_sound_file(missing_path, SoundCategory::Imported, "snd_missing").map(|_| ()),
        "missing file unexpectedly imported",
    )?;
    let after = load_sound_editor_record(record_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        attempted_state: format!("missing_path={}", missing_path.display()),
        after_state: record_state(&after),
        before_fingerprint,
        after_fingerprint: after.stable_fingerprint()?,
    })
}

fn edge_invalid_variable(record_path: &Path) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before = load_sound_editor_record(record_path)?;
    let before_state = sound_names(&before);
    let before_fingerprint = before.stable_fingerprint()?;
    let mut candidate = before.clone();
    candidate.named_sounds[0].variable_name = "bad-name".to_string();
    let attempted_state = sound_names(&candidate);
    let error_code =
        validation_error_code(candidate.validate(), "invalid variable unexpectedly passed")?;
    let after = load_sound_editor_record(record_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        attempted_state,
        after_state: sound_names(&after),
        before_fingerprint,
        after_fingerprint: after.stable_fingerprint()?,
    })
}

fn edge_seek_past_end(record_path: &Path) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before = load_sound_editor_record(record_path)?;
    let before_runtime = build_sound_editor_runtime(&before, Some("snd_ashen_bell"), 40)?;
    let before_state = preview_state(&before_runtime);
    let before_fingerprint = before.stable_fingerprint()?;
    let attempted_state = "cursor=101:length=100".to_string();
    let error_code = validation_error_code(
        build_sound_editor_runtime(&before, Some("snd_ashen_bell"), 101).map(|_| ()),
        "seek past end unexpectedly passed",
    )?;
    let after = load_sound_editor_record(record_path)?;
    let after_runtime = build_sound_editor_runtime(&after, Some("snd_ashen_bell"), 40)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        attempted_state,
        after_state: preview_state(&after_runtime),
        before_fingerprint,
        after_fingerprint: after.stable_fingerprint()?,
    })
}

fn record_state(record: &SoundEditorRecord) -> String {
    format!(
        "library={};names={};bindings={}",
        record.library.len(),
        sound_names(record),
        record
            .trigger_bindings
            .iter()
            .map(|binding| format!(
                "{}:{}:{}",
                binding.trigger_id, binding.action_id, binding.variable_name
            ))
            .collect::<Vec<_>>()
            .join("|")
    )
}

fn sound_names(record: &SoundEditorRecord) -> String {
    record
        .named_sounds
        .iter()
        .map(|sound| sound.variable_name.clone())
        .collect::<Vec<_>>()
        .join("|")
}

fn preview_state(runtime: &litd_editor::sound_editor::SoundEditorRuntime) -> String {
    let preview = runtime.preview.as_ref().expect("fixture starts preview");
    format!(
        "state={:?};cursor={};length={};variable={}",
        preview.state, preview.cursor_ms, preview.length_ms, preview.variable_name
    )
}

fn spatial_position(binding: &litd_editor::sound_editor::ResolvedTriggerSound) -> String {
    match &binding.playback.spatial {
        SoundSpatialMode::ThreeD { position, .. } => {
            format!("{},{},{}", position[0], position[1], position[2])
        }
        SoundSpatialMode::TwoD => "2d".to_string(),
    }
}

fn spatial_distance(binding: &litd_editor::sound_editor::ResolvedTriggerSound) -> String {
    match &binding.playback.spatial {
        SoundSpatialMode::ThreeD {
            min_distance_m,
            max_distance_m,
            ..
        } => format!("{}..{}", min_distance_m, max_distance_m),
        SoundSpatialMode::TwoD => "2d".to_string(),
    }
}

fn validation_error_code(
    result: Result<(), litd_engine::EngineError>,
    message: &'static str,
) -> Result<String, Box<dyn std::error::Error>> {
    match result {
        Ok(()) => Err(io::Error::new(io::ErrorKind::InvalidData, message).into()),
        Err(error) => Ok(error.code().to_string()),
    }
}

fn file_sha256(path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(stable_hash_sha256(&bytes))
}

fn stable_hash_sha256(bytes: &[u8]) -> String {
    litd_hash::sha256_hex(bytes)
}

fn write_text(path: &PathBuf, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

fn write_pcm_wav(
    path: &PathBuf,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    sample_frames: u32,
) -> Result<(), Box<dyn std::error::Error>> {
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
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}
