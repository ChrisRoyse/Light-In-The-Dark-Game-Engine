//! Sound editor authoring records, imports, trigger bindings, and preview transport.

use litd_hash::sha256_hex;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use serde::{Deserialize, Serialize};

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};

use crate::history::stable_fingerprint;

#[path = "sound_editor_audio.rs"]
mod sound_editor_audio;
#[path = "sound_editor_runtime.rs"]
mod sound_editor_runtime;
#[path = "sound_editor_runtime_audio.rs"]
mod sound_editor_runtime_audio;
#[path = "sound_editor_vault.rs"]
mod sound_editor_vault;

pub use sound_editor_runtime::{build_sound_editor_runtime, stop_preview};
pub use sound_editor_runtime_audio::{
    SoundEditorRuntimeAudioExport, build_sound_editor_runtime_audio_export,
};
pub use sound_editor_vault::{
    sound_category_for_vault_record, sound_library_entry_from_vault_record,
};

const SCHEMA_VERSION: u32 = 1;
const SOURCE_ISSUE: u32 = 1277;
const SOUND_EDITOR_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FC1";
const SOUND_EDITOR_VAULT_PURPOSE: &[u8] = b"litd-sound-editor-issue1360";
const SOUND_EDITOR_COLLECTION: &str = "litd_sound_editor";
const SOUND_EDITOR_RECORD_ROW: &[u8] = b"sound/editor/record/latest";
const SOUND_EDITOR_RECORD_COLUMN: &[u8] = b"record_json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundEditorRecord {
    pub schema_version: u32,
    pub source_issue: u32,
    pub library: Vec<SoundLibraryEntry>,
    pub named_sounds: Vec<NamedSoundVariable>,
    pub trigger_bindings: Vec<TriggerSoundBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoundLibraryEntry {
    pub asset_id: String,
    pub category: SoundCategory,
    pub file_name: String,
    pub source_path: String,
    pub sha256: String,
    pub codec: SoundAudioCodec,
    pub length_ms: u64,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub bits_per_sample: Option<u16>,
    pub bytes_on_disk: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SoundAudioCodec {
    PcmWav,
    OggVorbis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SoundCategory {
    Abilities,
    Buildings,
    Doodads,
    Units,
    Ambient,
    Dialogue,
    Interface,
    Imported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedSoundVariable {
    pub variable_name: String,
    pub asset_id: String,
    pub playback: SoundPlaybackParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundPlaybackParams {
    pub volume: f32,
    pub pitch: f32,
    pub channel: SoundChannel,
    pub spatial: SoundSpatialMode,
    pub looping: bool,
    pub fade_in_ms: u32,
    pub fade_out_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SoundChannel {
    Effects,
    Music,
    Ambient,
    Dialogue,
    Interface,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SoundSpatialMode {
    TwoD,
    ThreeD {
        position: [f32; 3],
        min_distance_m: f32,
        max_distance_m: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerSoundBinding {
    pub trigger_id: String,
    pub action_id: String,
    pub variable_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundEditorRuntime {
    pub library_rows: Vec<SoundLibraryRow>,
    pub resolved_trigger_bindings: Vec<ResolvedTriggerSound>,
    pub preview: Option<PreviewTransportState>,
    pub source_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoundLibraryRow {
    pub category: SoundCategory,
    pub file_name: String,
    pub variable_names: Vec<String>,
    pub length_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedTriggerSound {
    pub trigger_id: String,
    pub action_id: String,
    pub variable_name: String,
    pub asset_id: String,
    pub file_name: String,
    pub length_ms: u64,
    pub playback: SoundPlaybackParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewTransportState {
    pub variable_name: String,
    pub state: PreviewPlaybackState,
    pub cursor_ms: u64,
    pub length_ms: u64,
    pub playback: SoundPlaybackParams,
    pub source_asset_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewPlaybackState {
    Playing,
    Stopped,
}

impl SoundEditorRecord {
    pub fn new(
        library: Vec<SoundLibraryEntry>,
        named_sounds: Vec<NamedSoundVariable>,
        trigger_bindings: Vec<TriggerSoundBinding>,
    ) -> Result<Self, EngineError> {
        let record = Self {
            schema_version: SCHEMA_VERSION,
            source_issue: SOURCE_ISSUE,
            library,
            named_sounds,
            trigger_bindings,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema_version != SCHEMA_VERSION || self.source_issue != SOURCE_ISSUE {
            return invalid_sound_editor("schema", "unexpected sound editor record identity");
        }
        let mut assets = BTreeMap::new();
        let mut paths = BTreeSet::new();
        for entry in &self.library {
            entry.validate()?;
            if assets.insert(entry.asset_id.as_str(), entry).is_some() {
                return invalid_sound_editor(
                    "library.asset_id",
                    format!("duplicate asset {}", entry.asset_id),
                );
            }
            if !paths.insert(entry.source_path.as_str()) {
                return invalid_sound_editor(
                    "library.source_path",
                    format!("duplicate sound path {}", entry.source_path),
                );
            }
        }
        let mut named = BTreeSet::new();
        for sound in &self.named_sounds {
            sound.validate()?;
            if !assets.contains_key(sound.asset_id.as_str()) {
                return invalid_sound_editor(
                    "named_sounds.asset_id",
                    format!(
                        "sound variable {} references missing asset {}",
                        sound.variable_name, sound.asset_id
                    ),
                );
            }
            if !named.insert(sound.variable_name.as_str()) {
                return invalid_sound_editor(
                    "named_sounds.variable_name",
                    format!("duplicate sound variable {}", sound.variable_name),
                );
            }
        }
        for binding in &self.trigger_bindings {
            binding.validate()?;
            if !named.contains(binding.variable_name.as_str()) {
                return invalid_sound_editor(
                    "trigger_bindings.variable_name",
                    format!(
                        "trigger {} action {} references missing sound variable {}",
                        binding.trigger_id, binding.action_id, binding.variable_name
                    ),
                );
            }
        }
        Ok(())
    }

    pub fn stable_fingerprint(&self) -> Result<String, EngineError> {
        self.validate()?;
        stable_fingerprint(self)
    }
}

impl SoundLibraryEntry {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_symbol("library.asset_id", &self.asset_id)?;
        if self.file_name.trim().is_empty() || self.source_path.trim().is_empty() {
            return invalid_sound_editor("library.path", "file name and source path are required");
        }
        if !is_sha256_hex(&self.sha256) {
            return invalid_sound_editor("library.sha256", "must be 64 lowercase hex chars");
        }
        if self.length_ms == 0
            || self.sample_rate_hz == 0
            || self.channels == 0
            || self.bytes_on_disk == 0
        {
            return invalid_sound_editor(
                "library.metadata",
                "decoded length, sample rate, channels, and bytes are required",
            );
        }
        if self.codec == SoundAudioCodec::PcmWav && self.bits_per_sample.unwrap_or(0) == 0 {
            return invalid_sound_editor(
                "library.bits_per_sample",
                "PCM WAV bit depth is required",
            );
        }
        Ok(())
    }
}

impl NamedSoundVariable {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_symbol("named_sounds.variable_name", &self.variable_name)?;
        validate_symbol("named_sounds.asset_id", &self.asset_id)?;
        self.playback.validate()
    }
}

impl SoundPlaybackParams {
    pub fn validate(&self) -> Result<(), EngineError> {
        if !self.volume.is_finite() || !(0.0..=1.0).contains(&self.volume) {
            return invalid_sound_editor("playback.volume", "volume must be finite in 0.0..=1.0");
        }
        if !self.pitch.is_finite() || !(0.125..=4.0).contains(&self.pitch) {
            return invalid_sound_editor("playback.pitch", "pitch must be finite in 0.125..=4.0");
        }
        if self.fade_in_ms > 60_000 || self.fade_out_ms > 60_000 {
            return invalid_sound_editor("playback.fade", "fade durations must be <= 60000 ms");
        }
        self.spatial.validate()
    }
}

impl SoundSpatialMode {
    fn validate(&self) -> Result<(), EngineError> {
        match self {
            SoundSpatialMode::TwoD => Ok(()),
            SoundSpatialMode::ThreeD {
                position,
                min_distance_m,
                max_distance_m,
            } => {
                if position.iter().any(|value| !value.is_finite()) {
                    return invalid_sound_editor(
                        "playback.spatial.position",
                        "3D sound position must be finite",
                    );
                }
                if !min_distance_m.is_finite()
                    || !max_distance_m.is_finite()
                    || *min_distance_m <= 0.0
                    || *max_distance_m < *min_distance_m
                {
                    return invalid_sound_editor(
                        "playback.spatial.distance",
                        "3D sound distances must be finite and ordered",
                    );
                }
                Ok(())
            }
        }
    }
}

impl TriggerSoundBinding {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_symbol("trigger_bindings.trigger_id", &self.trigger_id)?;
        validate_symbol("trigger_bindings.action_id", &self.action_id)?;
        validate_symbol("trigger_bindings.variable_name", &self.variable_name)
    }
}

pub fn import_sound_file(
    path: &Path,
    category: SoundCategory,
    asset_id: impl Into<String>,
) -> Result<SoundLibraryEntry, EngineError> {
    let asset_id = asset_id.into();
    validate_symbol("import.asset_id", &asset_id)?;
    let extension = extension(path).ok_or_else(|| EngineError::UiRenderInvalidConfig {
        field: "editor.sound_editor.import.extension".to_string(),
        reason: format!("missing extension for {}", path.display()),
    })?;
    let bytes = fs::read(path).map_err(|error| io_sound_error("import.read", path, error))?;
    let metadata = sound_editor_audio::parse_audio_metadata(&extension, &bytes)?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.sound_editor.import.file_name".to_string(),
            reason: format!("invalid file name {}", path.display()),
        })?
        .to_string();
    let entry = SoundLibraryEntry {
        asset_id,
        category,
        file_name,
        source_path: path.display().to_string(),
        sha256: sha256_hex(&bytes),
        codec: metadata.codec,
        length_ms: metadata.length_ms,
        sample_rate_hz: metadata.sample_rate_hz,
        channels: metadata.channels,
        bits_per_sample: metadata.bits_per_sample,
        bytes_on_disk: bytes.len() as u64,
    };
    entry.validate()?;
    Ok(entry)
}

pub fn save_sound_editor_record(
    path: &Path,
    record: &SoundEditorRecord,
) -> Result<(), EngineError> {
    record.validate()?;
    let bytes =
        serde_json::to_vec_pretty(record).map_err(|error| EngineError::SerializationFailed {
            format: "sound-editor-record-json",
            reason: format!("serialize sound editor record: {error}"),
        })?;
    let readback = write_calyx_json_bytes(&sound_record_spec(path), &bytes)
        .map_err(|error| sound_store_error("save", path, error))?;
    if readback.bytes != bytes {
        return invalid_sound_editor(
            "save.readback",
            "Aster sound editor record bytes did not match serialized bytes",
        );
    }
    Ok(())
}

pub fn load_sound_editor_record(path: &Path) -> Result<SoundEditorRecord, EngineError> {
    Ok(read_sound_editor_record_source(path)?.record)
}

pub(crate) struct SoundEditorRecordAsterReadback {
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub record: SoundEditorRecord,
}

pub(crate) fn read_sound_editor_record_source(
    path: &Path,
) -> Result<SoundEditorRecordAsterReadback, EngineError> {
    let readback = read_calyx_json_bytes(&sound_record_spec(path))
        .map_err(|error| sound_store_error("load", path, error))?
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.sound_editor.load".to_string(),
            reason: format!(
                "missing Calyx Aster sound editor record row at {}",
                path.display()
            ),
        })?;
    let record: SoundEditorRecord = serde_json::from_slice(&readback.bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "sound-editor-record-json",
            reason: format!("parse sound editor record {}: {error}", path.display()),
        }
    })?;
    record.validate()?;
    Ok(SoundEditorRecordAsterReadback {
        sha256: readback.sha256,
        bytes: readback.bytes,
        record,
    })
}

fn validate_symbol(field: &str, value: &str) -> Result<(), EngineError> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return invalid_sound_editor(field, "symbol cannot be empty");
    };
    if !(first.is_ascii_alphabetic() || first == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return invalid_sound_editor(field, format!("{value:?} must be an ASCII identifier"));
    }
    Ok(())
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

use litd_hash::is_sha256_hex;

fn io_sound_error(field: &str, path: &Path, error: std::io::Error) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.sound_editor.{field}"),
        reason: format!("{}: {error}", path.display()),
    }
}

fn sound_store_error(field: &str, path: &Path, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.sound_editor.{field}"),
        reason: format!("{}: {}", path.display(), reason.into()),
    }
}

fn sound_record_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: SOUND_EDITOR_VAULT_ID,
        purpose: SOUND_EDITOR_VAULT_PURPOSE,
        collection: SOUND_EDITOR_COLLECTION,
        row_key: SOUND_EDITOR_RECORD_ROW,
        column: SOUND_EDITOR_RECORD_COLUMN,
    }
}

fn invalid_sound_editor<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.sound_editor.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "sound_editor_audio_tests.rs"]
mod audio_tests;
#[cfg(test)]
#[path = "sound_editor_tests.rs"]
mod tests;
