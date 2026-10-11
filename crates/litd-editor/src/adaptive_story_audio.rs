use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use litd_engine::{EngineError, game_package::RuntimePackageAssetRef};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    mcp_story_tools::CinematicStage,
    sound_editor::{SoundEditorRecord, SoundLibraryEntry, SoundSpatialMode},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveMoodAudioBinding {
    pub mood: String,
    pub music_variable_name: Option<String>,
    pub ambience_variable_name: Option<String>,
    pub intensity: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveStoryAudioExport {
    pub runtime_audio: serde_json::Value,
    pub assets: Vec<RuntimePackageAssetRef>,
    pub beat_count: usize,
    pub cue_count: usize,
    pub moods: Vec<String>,
    pub source_fingerprint: String,
}

pub fn build_adaptive_story_audio_export(
    stages: &[CinematicStage],
    sound_record: &SoundEditorRecord,
    mood_bindings: &[AdaptiveMoodAudioBinding],
    package_root: &Path,
) -> Result<AdaptiveStoryAudioExport, EngineError> {
    sound_record.validate()?;
    validate_package_root(package_root)?;
    if stages.is_empty() {
        return invalid_adaptive_audio("stages", "at least one cinematic stage is required");
    }
    let bindings = mood_bindings
        .iter()
        .map(|binding| {
            binding.validate()?;
            Ok((binding.mood.as_str(), binding))
        })
        .collect::<Result<BTreeMap<_, _>, EngineError>>()?;
    let named = sound_record
        .named_sounds
        .iter()
        .map(|sound| (sound.variable_name.as_str(), sound))
        .collect::<BTreeMap<_, _>>();
    let library = sound_record
        .library
        .iter()
        .map(|entry| (entry.asset_id.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut assets = Vec::new();
    let mut used_assets = BTreeSet::new();
    let mut cues = Vec::new();
    let mut moods = BTreeSet::new();
    let mut beat_count = 0;
    for stage in stages {
        for beat in &stage.beats {
            beat_count += 1;
            moods.insert(beat.mood.clone());
            let binding = bindings.get(beat.mood.as_str()).ok_or_else(|| {
                adaptive_audio_error(
                    "mood",
                    format!("missing audio binding for mood {}", beat.mood),
                )
            })?;
            if let Some(variable_name) = &binding.music_variable_name {
                push_cue(PushCueArgs {
                    cues: &mut cues,
                    assets: &mut assets,
                    used_assets: &mut used_assets,
                    package_root,
                    named: &named,
                    library: &library,
                    beat_id: beat.beat_id.as_str(),
                    variable_name,
                    layer: "music",
                    intensity: binding.intensity,
                })?;
            }
            if let Some(variable_name) = &binding.ambience_variable_name {
                push_cue(PushCueArgs {
                    cues: &mut cues,
                    assets: &mut assets,
                    used_assets: &mut used_assets,
                    package_root,
                    named: &named,
                    library: &library,
                    beat_id: beat.beat_id.as_str(),
                    variable_name,
                    layer: "ambience",
                    intensity: binding.intensity,
                })?;
            }
        }
    }
    if cues.is_empty() {
        return invalid_adaptive_audio("cues", "adaptive story audio produced no cues");
    }
    Ok(AdaptiveStoryAudioExport {
        runtime_audio: json!({
            "listener": {
                "position": [0.0, 0.0, 0.0],
                "orientation_xyzw": [0.0, 0.0, 0.0, 1.0],
                "velocity": [0.0, 0.0, 0.0]
            },
            "cues": cues
        }),
        assets,
        beat_count,
        cue_count: cues.len(),
        moods: moods.into_iter().collect(),
        source_fingerprint: sound_record.stable_fingerprint()?,
    })
}

impl AdaptiveMoodAudioBinding {
    fn validate(&self) -> Result<(), EngineError> {
        validate_symbol("mood", &self.mood)?;
        if self.music_variable_name.is_none() && self.ambience_variable_name.is_none() {
            return invalid_adaptive_audio("binding", "music or ambience variable is required");
        }
        if let Some(name) = &self.music_variable_name {
            validate_symbol("music_variable_name", name)?;
        }
        if let Some(name) = &self.ambience_variable_name {
            validate_symbol("ambience_variable_name", name)?;
        }
        if !self.intensity.is_finite() || !(0.0..=1.0).contains(&self.intensity) {
            return invalid_adaptive_audio("intensity", "intensity must be finite in 0.0..=1.0");
        }
        Ok(())
    }
}

struct PushCueArgs<'a> {
    cues: &'a mut Vec<serde_json::Value>,
    assets: &'a mut Vec<RuntimePackageAssetRef>,
    used_assets: &'a mut BTreeSet<String>,
    package_root: &'a Path,
    named: &'a BTreeMap<&'a str, &'a crate::sound_editor::NamedSoundVariable>,
    library: &'a BTreeMap<&'a str, &'a SoundLibraryEntry>,
    beat_id: &'a str,
    variable_name: &'a str,
    layer: &'a str,
    intensity: f32,
}

fn push_cue(args: PushCueArgs<'_>) -> Result<(), EngineError> {
    let PushCueArgs {
        cues,
        assets,
        used_assets,
        package_root,
        named,
        library,
        beat_id,
        variable_name,
        layer,
        intensity,
    } = args;
    let sound = named
        .get(variable_name)
        .ok_or_else(|| adaptive_audio_error("variable_name", format!("missing {variable_name}")))?;
    let entry = library
        .get(sound.asset_id.as_str())
        .ok_or_else(|| adaptive_audio_error("asset_id", format!("missing {}", sound.asset_id)))?;
    if used_assets.insert(entry.asset_id.clone()) {
        assets.push(RuntimePackageAssetRef {
            product_id: entry.asset_id.clone(),
            source_relative_path: relative_audio_path(&entry.source_path, package_root)?,
            source_sha256: entry.sha256.clone(),
            source_bytes: entry.bytes_on_disk,
        });
    }
    cues.push(json!({
        "trigger_id": format!("trg_{}_{}", beat_id, layer),
        "cue_id": format!("{}_{}", beat_id, variable_name),
        "kind": layer,
        "asset_product_id": entry.asset_id,
        "bus": if layer == "music" { "music" } else { "sfx" },
        "position": position(&sound.playback.spatial, layer),
        "velocity": [0.0, 0.0, 0.0],
        "direction": [0.0, 0.0, -1.0],
        "cone_inner_degrees": 360.0,
        "cone_outer_degrees": 360.0,
        "min_distance_meters": min_distance(&sound.playback.spatial),
        "max_distance_meters": max_distance(&sound.playback.spatial),
        "spatialization_strength": if layer == "music" { 0.0 } else { 0.85 },
        "volume_db": linear_volume_to_db(sound.playback.volume * (0.25 + 0.75 * intensity)),
        "playback_rate": f64::from(sound.playback.pitch),
        "occlusion": { "factor": 0.0 }
    }));
    Ok(())
}

fn validate_package_root(package_root: &Path) -> Result<(), EngineError> {
    if package_root.is_absolute() {
        Ok(())
    } else {
        invalid_adaptive_audio("package_root", "package root must be absolute")
    }
}

fn relative_audio_path(source_path: &str, package_root: &Path) -> Result<String, EngineError> {
    let path = PathBuf::from(source_path);
    let metadata = fs::metadata(&path).map_err(|error| {
        adaptive_audio_error("source_path", format!("{}: {error}", path.display()))
    })?;
    if !metadata.is_file() {
        return invalid_adaptive_audio("source_path", "audio source must be a file");
    }
    let relative = path.strip_prefix(package_root).map_err(|_| {
        adaptive_audio_error(
            "source_path",
            format!(
                "{} must be inside package root {}",
                path.display(),
                package_root.display()
            ),
        )
    })?;
    Ok(relative.display().to_string())
}

fn position(spatial: &SoundSpatialMode, layer: &str) -> [f32; 3] {
    match (spatial, layer) {
        (_, "music") => [0.0, 0.0, -8.0],
        (SoundSpatialMode::ThreeD { position, .. }, _) => *position,
        _ => [0.0, 0.0, 0.0],
    }
}

fn min_distance(spatial: &SoundSpatialMode) -> f32 {
    match spatial {
        SoundSpatialMode::ThreeD { min_distance_m, .. } => *min_distance_m,
        SoundSpatialMode::TwoD => 1.0,
    }
}

fn max_distance(spatial: &SoundSpatialMode) -> f32 {
    match spatial {
        SoundSpatialMode::ThreeD { max_distance_m, .. } => *max_distance_m,
        SoundSpatialMode::TwoD => 32.0,
    }
}

fn linear_volume_to_db(volume: f32) -> f32 {
    if volume <= 0.0 {
        -60.0
    } else {
        (20.0 * volume.log10()).clamp(-60.0, 12.0)
    }
}

fn validate_symbol(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        invalid_adaptive_audio(field, format!("{value:?} must be an ASCII identifier"))
    } else {
        Ok(())
    }
}

fn invalid_adaptive_audio<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(adaptive_audio_error(field, reason))
}

fn adaptive_audio_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.adaptive_story_audio.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "adaptive_story_audio_tests.rs"]
mod tests;
