use litd_engine::fsv_artifacts::env_path;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

use litd_editor::{
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
    history::{stable_fingerprint, to_stable_ron},
    sound_editor::{
        NamedSoundVariable, SoundCategory, SoundChannel, SoundEditorRecord, SoundPlaybackParams,
        SoundSpatialMode, TriggerSoundBinding, build_sound_editor_runtime, import_sound_file,
        sound_library_entry_from_vault_record,
    },
};
use serde_json::Value;

const FOOTSTEP_PATH: &str = "extracted/kenney/audio/rpg-audio/Audio/footstep00.ogg";
const CLICK_PATH: &str = "extracted/kenney/audio/interface-sounds/Audio/click_001.ogg";
const AMBIENT_PATH: &str = "extracted/kenney/audio/rpg-audio/Preview.ogg";
const NON_AUDIO_PATH: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let root = env_path(
        "LITD_EDITOR_AUDIO_VAULT_FSV_ROOT",
        "tmp/issue1279-audio-vault",
    );
    let stats_path = root.join("stats.txt");
    let runtime_path = root.join("sound_editor_audio_vault_runtime.ron");
    let summary_path = manifest_dir().join("kenney_audio_summary.json");

    let source_before = source_truth_state(&summary_path)?;
    let catalog = AssetVaultCatalog::load_default()?;
    let coverage = catalog.coverage_summary()?;
    let summary = read_summary(&summary_path)?;
    let footstep = real_record(&catalog, FOOTSTEP_PATH)?;
    let click = real_record(&catalog, CLICK_PATH)?;
    let ambient = real_record(&catalog, AMBIENT_PATH)?;
    let non_audio = real_record(&catalog, NON_AUDIO_PATH)?;
    let footstep_hash = verify_record_hash(&footstep)?;
    let click_hash = verify_record_hash(&click)?;
    let ambient_hash = verify_record_hash(&ambient)?;
    let footstep_entry = sound_library_entry_from_vault_record(&footstep)?;
    let click_entry = sound_library_entry_from_vault_record(&click)?;
    let ambient_entry = sound_library_entry_from_vault_record(&ambient)?;

    let record = SoundEditorRecord::new(
        vec![
            footstep_entry.clone(),
            click_entry.clone(),
            ambient_entry.clone(),
        ],
        vec![
            NamedSoundVariable {
                variable_name: "snd_unit_footstep".to_string(),
                asset_id: footstep_entry.asset_id.clone(),
                playback: two_d_params(SoundChannel::Effects),
            },
            NamedSoundVariable {
                variable_name: "snd_ui_click".to_string(),
                asset_id: click_entry.asset_id.clone(),
                playback: two_d_params(SoundChannel::Interface),
            },
            NamedSoundVariable {
                variable_name: "snd_rpg_preview_ambience".to_string(),
                asset_id: ambient_entry.asset_id.clone(),
                playback: ambient_params(),
            },
        ],
        vec![TriggerSoundBinding {
            trigger_id: "trg_unit_step".to_string(),
            action_id: "act_play_step_sound".to_string(),
            variable_name: "snd_unit_footstep".to_string(),
        }],
    )?;
    let runtime = build_sound_editor_runtime(&record, Some("snd_unit_footstep"), 0)?;
    write_text(&runtime_path, &to_stable_ron(&runtime)?)?;
    let runtime_readback = fs::read_to_string(&runtime_path)?;
    let runtime_fingerprint = stable_fingerprint(&runtime)?;
    let source_after_happy = source_truth_state(&summary_path)?;

    let missing_edge = edge_missing_file(&summary_path, &root.join("missing_kenney_sound.ogg"))?;
    let non_audio_edge = edge_non_audio_record(&summary_path, &non_audio)?;
    let corrupt_edge = edge_corrupt_ogg(&summary_path, &root.join("corrupt.ogg"))?;

    let source_final = source_truth_state(&summary_path)?;
    write_text(
        &stats_path,
        &format!(
            "issue=1279\n\
             source_of_truth={}\n\
             summary_file={}\n\
             download_manifest={}\n\
             extraction_manifest={}\n\
             source_before={}\n\
             source_after_happy={}\n\
             source_final={}\n\
             source_unchanged_after_happy={}\n\
             source_unchanged_after_all_edges={}\n\
             summary_pack_count={}\n\
             summary_audio_file_count={}\n\
             summary_appended_download_records={}\n\
             summary_appended_extraction_records={}\n\
             coverage_record_types={}\n\
             coverage_asset_types={}\n\
             coverage_licenses={}\n\
             footstep_path={}\n\
             footstep_hash={}\n\
             footstep_codec={:?}\n\
             footstep_category={:?}\n\
             footstep_length_ms={}\n\
             footstep_sample_rate_hz={}\n\
             footstep_channels={}\n\
             footstep_bits_per_sample={:?}\n\
             click_path={}\n\
             click_hash={}\n\
             click_codec={:?}\n\
             click_category={:?}\n\
             ambient_path={}\n\
             ambient_hash={}\n\
             ambient_codec={:?}\n\
             ambient_category={:?}\n\
             runtime_file={}\n\
             runtime_fingerprint={}\n\
             runtime_readback_contains_step_binding={}\n\
             runtime_readback_contains_preview={}\n\
             runtime_library_rows={}\n\
             runtime_trigger_bindings={}\n\
             edge_missing_file_error={}\n\
             edge_missing_file_before_state={}\n\
             edge_missing_file_action={}\n\
             edge_missing_file_after_state={}\n\
             edge_missing_file_source_unchanged={}\n\
             edge_non_audio_error={}\n\
             edge_non_audio_before_state={}\n\
             edge_non_audio_action={}\n\
             edge_non_audio_after_state={}\n\
             edge_non_audio_source_unchanged={}\n\
             edge_corrupt_ogg_error={}\n\
             edge_corrupt_ogg_before_state={}\n\
             edge_corrupt_ogg_action={}\n\
             edge_corrupt_ogg_after_state={}\n\
             edge_corrupt_ogg_source_unchanged={}\n",
            "local-asset-vault manifests plus extracted audio files",
            summary_path.display(),
            manifest_dir().join("asset_intake_manifest.jsonl").display(),
            manifest_dir()
                .join("asset_extraction_manifest.jsonl")
                .display(),
            source_before,
            source_after_happy,
            source_final,
            source_before == source_after_happy,
            source_before == source_final,
            summary["pack_count"].as_u64().unwrap_or(0),
            summary["audio_file_count"].as_u64().unwrap_or(0),
            summary["appended_download_records"].as_u64().unwrap_or(0),
            summary["appended_extraction_records"].as_u64().unwrap_or(0),
            coverage.record_types,
            coverage.asset_types,
            coverage.licenses,
            footstep.relative_path,
            footstep_hash,
            footstep_entry.codec,
            footstep_entry.category,
            footstep_entry.length_ms,
            footstep_entry.sample_rate_hz,
            footstep_entry.channels,
            footstep_entry.bits_per_sample,
            click.relative_path,
            click_hash,
            click_entry.codec,
            click_entry.category,
            ambient.relative_path,
            ambient_hash,
            ambient_entry.codec,
            ambient_entry.category,
            runtime_path.display(),
            runtime_fingerprint,
            runtime_readback.contains("trigger_id: \"trg_unit_step\""),
            runtime_readback.contains("state: Playing"),
            runtime.library_rows.len(),
            runtime.resolved_trigger_bindings.len(),
            missing_edge.error_code,
            missing_edge.before_state,
            missing_edge.action,
            missing_edge.after_state,
            missing_edge.source_unchanged(),
            non_audio_edge.error_code,
            non_audio_edge.before_state,
            non_audio_edge.action,
            non_audio_edge.after_state,
            non_audio_edge.source_unchanged(),
            corrupt_edge.error_code,
            corrupt_edge.before_state,
            corrupt_edge.action,
            corrupt_edge.after_state,
            corrupt_edge.source_unchanged(),
        ),
    )?;
    println!("wrote audio vault FSV stats to {}", stats_path.display());
    Ok(())
}

#[derive(Debug)]
struct EdgeAudit {
    error_code: String,
    before_state: String,
    action: String,
    after_state: String,
}

impl EdgeAudit {
    fn source_unchanged(&self) -> bool {
        self.before_state == self.after_state
    }
}

fn edge_missing_file(
    summary_path: &Path,
    missing_path: &Path,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before_state = source_truth_state(summary_path)?;
    let error_code = validation_error_code(
        import_sound_file(missing_path, SoundCategory::Imported, "snd_missing_audio").map(|_| ()),
        "missing audio unexpectedly imported",
    )?;
    let after_state = source_truth_state(summary_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        action: format!("import_missing_path={}", missing_path.display()),
        after_state,
    })
}

fn edge_non_audio_record(
    summary_path: &Path,
    record: &AssetVaultRecord,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let before_state = source_truth_state(summary_path)?;
    let error_code = validation_error_code(
        sound_library_entry_from_vault_record(record).map(|_| ()),
        "non-audio vault record unexpectedly projected",
    )?;
    let after_state = source_truth_state(summary_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        action: format!("project_non_audio_record={}", record.relative_path),
        after_state,
    })
}

fn edge_corrupt_ogg(
    summary_path: &Path,
    corrupt_path: &Path,
) -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    if let Some(parent) = corrupt_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(corrupt_path, b"OggS-corrupt")?;
    let before_state = source_truth_state(summary_path)?;
    let error_code = validation_error_code(
        import_sound_file(corrupt_path, SoundCategory::Imported, "snd_corrupt_audio").map(|_| ()),
        "corrupt OGG unexpectedly imported",
    )?;
    let after_state = source_truth_state(summary_path)?;
    Ok(EdgeAudit {
        error_code,
        before_state,
        action: format!("import_corrupt_ogg={}", corrupt_path.display()),
        after_state,
    })
}

fn source_truth_state(summary_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let catalog = AssetVaultCatalog::load_default()?;
    let coverage = catalog.coverage_summary()?;
    let summary = read_summary(summary_path)?;
    let footstep = real_record(&catalog, FOOTSTEP_PATH)?;
    let click = real_record(&catalog, CLICK_PATH)?;
    let ambient = real_record(&catalog, AMBIENT_PATH)?;
    Ok(format!(
        "records={};usable={};record_types={};asset_types={};licenses={};kenney_downloads={};audio={};summary_sha256={};summary_audio_count={};footstep={}:{}:{};click={}:{}:{};ambient={}:{}:{}",
        catalog.record_count(),
        catalog.usable_record_count(),
        coverage.record_types,
        coverage.asset_types,
        coverage.licenses,
        catalog.download_source_count("kenney"),
        catalog.asset_type_count("audio"),
        file_sha256(summary_path)?,
        summary["audio_file_count"].as_u64().unwrap_or(0),
        footstep.bytes_on_disk,
        footstep.sha256,
        verify_record_hash(&footstep)?,
        click.bytes_on_disk,
        click.sha256,
        verify_record_hash(&click)?,
        ambient.bytes_on_disk,
        ambient.sha256,
        verify_record_hash(&ambient)?,
    ))
}

fn read_summary(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

fn real_record(
    catalog: &AssetVaultCatalog,
    relative_path: &str,
) -> Result<AssetVaultRecord, Box<dyn std::error::Error>> {
    catalog
        .records()
        .iter()
        .find(|record| record.relative_path == relative_path)
        .cloned()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("missing real vault record {relative_path}"),
            )
            .into()
        })
}

fn verify_record_hash(record: &AssetVaultRecord) -> Result<String, Box<dyn std::error::Error>> {
    let hash = file_sha256(&record.absolute_path)?;
    if hash != record.sha256 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "hash mismatch for {} manifest={} actual={}",
                record.relative_path, record.sha256, hash
            ),
        )
        .into());
    }
    Ok(hash)
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

fn two_d_params(channel: SoundChannel) -> SoundPlaybackParams {
    SoundPlaybackParams {
        volume: 0.8,
        pitch: 1.0,
        channel,
        spatial: SoundSpatialMode::TwoD,
        looping: false,
        fade_in_ms: 0,
        fade_out_ms: 80,
    }
}

fn ambient_params() -> SoundPlaybackParams {
    SoundPlaybackParams {
        volume: 0.5,
        pitch: 1.0,
        channel: SoundChannel::Ambient,
        spatial: SoundSpatialMode::ThreeD {
            position: [0.0, 0.0, 0.0],
            min_distance_m: 4.0,
            max_distance_m: 80.0,
        },
        looping: true,
        fade_in_ms: 500,
        fade_out_ms: 500,
    }
}

// #1543: RUNTIME vault resolution; see `litd_engine::asset_vault_root`.
fn manifest_dir() -> PathBuf {
    litd_engine::asset_vault_root::local_asset_vault_path("manifests").unwrap()
}

fn file_sha256(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(stable_hash_sha256(&bytes))
}

fn stable_hash_sha256(bytes: &[u8]) -> String {
    litd_hash::sha256_hex(bytes)
}

fn write_text(path: &Path, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}
