use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use litd_engine::{EngineError, game_package::RuntimePackageAssetRef};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{
    SoundChannel, SoundEditorRecord, SoundSpatialMode, build_sound_editor_runtime,
    invalid_sound_editor,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundEditorRuntimeAudioExport {
    pub runtime_audio: serde_json::Value,
    pub assets: Vec<RuntimePackageAssetRef>,
    pub cue_count: usize,
    pub trigger_ids: Vec<String>,
    pub source_fingerprint: String,
}

pub fn build_sound_editor_runtime_audio_export(
    record: &SoundEditorRecord,
    package_root: &Path,
) -> Result<SoundEditorRuntimeAudioExport, EngineError> {
    record.validate()?;
    if !package_root.is_absolute() {
        return invalid_sound_editor(
            "runtime_audio.package_root",
            "package root must be absolute",
        );
    }
    let runtime = build_sound_editor_runtime(record, None, 0)?;
    let library = record
        .library
        .iter()
        .map(|entry| (entry.asset_id.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut used_assets = BTreeSet::new();
    let mut assets = Vec::new();
    let mut cues = Vec::new();
    for binding in &runtime.resolved_trigger_bindings {
        let entry = library.get(binding.asset_id.as_str()).ok_or_else(|| {
            EngineError::UiRenderInvalidConfig {
                field: "editor.sound_editor.runtime_audio.asset_id".to_string(),
                reason: format!("missing asset {}", binding.asset_id),
            }
        })?;
        if used_assets.insert(entry.asset_id.as_str()) {
            assets.push(RuntimePackageAssetRef {
                product_id: entry.asset_id.clone(),
                source_relative_path: relative_audio_path(&entry.source_path, package_root)?,
                source_sha256: entry.sha256.clone(),
                source_bytes: entry.bytes_on_disk,
            });
        }
        cues.push(json!({
            "trigger_id": binding.trigger_id,
            "cue_id": binding.variable_name,
            "kind": cue_kind(binding.playback.channel),
            "asset_product_id": binding.asset_id,
            "bus": bus(binding.playback.channel),
            "position": position(&binding.playback.spatial),
            "velocity": [0.0, 0.0, 0.0],
            "direction": [0.0, 0.0, -1.0],
            "cone_inner_degrees": 360.0,
            "cone_outer_degrees": 360.0,
            "min_distance_meters": min_distance(&binding.playback.spatial),
            "max_distance_meters": max_distance(&binding.playback.spatial),
            "spatialization_strength": spatialization_strength(&binding.playback.spatial),
            "volume_db": linear_volume_to_db(binding.playback.volume),
            "playback_rate": f64::from(binding.playback.pitch),
            "occlusion": { "factor": 0.0 }
        }));
    }
    let trigger_ids = runtime
        .resolved_trigger_bindings
        .iter()
        .map(|binding| binding.trigger_id.clone())
        .collect::<Vec<_>>();
    Ok(SoundEditorRuntimeAudioExport {
        runtime_audio: json!({
            "listener": {
                "position": [0.0, 0.0, 0.0],
                "orientation_xyzw": [0.0, 0.0, 0.0, 1.0],
                "velocity": [0.0, 0.0, 0.0]
            },
            "cues": cues
        }),
        assets,
        cue_count: cues.len(),
        trigger_ids,
        source_fingerprint: runtime.source_fingerprint,
    })
}

fn relative_audio_path(source_path: &str, package_root: &Path) -> Result<String, EngineError> {
    let path = PathBuf::from(source_path);
    if !path.is_absolute() {
        return invalid_sound_editor(
            "runtime_audio.source_path",
            "audio source path must be absolute",
        );
    }
    let metadata = fs::metadata(&path).map_err(|error| EngineError::UiRenderInvalidConfig {
        field: "editor.sound_editor.runtime_audio.source_path".to_string(),
        reason: format!("{}: {error}", path.display()),
    })?;
    if !metadata.is_file() {
        return invalid_sound_editor(
            "runtime_audio.source_path",
            format!("{} is not a file", path.display()),
        );
    }
    let relative =
        path.strip_prefix(package_root)
            .map_err(|_| EngineError::UiRenderInvalidConfig {
                field: "editor.sound_editor.runtime_audio.source_path".to_string(),
                reason: format!(
                    "{} must be inside package root {}",
                    path.display(),
                    package_root.display()
                ),
            })?;
    Ok(relative.display().to_string())
}

fn cue_kind(channel: SoundChannel) -> &'static str {
    match channel {
        SoundChannel::Music => "music",
        SoundChannel::Ambient => "ambience",
        SoundChannel::Dialogue => "positional_sound",
        SoundChannel::Effects | SoundChannel::Interface => "positional_sound",
    }
}

fn bus(channel: SoundChannel) -> &'static str {
    match channel {
        SoundChannel::Music => "music",
        SoundChannel::Dialogue => "voice",
        SoundChannel::Effects | SoundChannel::Ambient | SoundChannel::Interface => "sfx",
    }
}

fn position(spatial: &SoundSpatialMode) -> [f32; 3] {
    match spatial {
        SoundSpatialMode::TwoD => [0.0, 0.0, 0.0],
        SoundSpatialMode::ThreeD { position, .. } => *position,
    }
}

fn min_distance(spatial: &SoundSpatialMode) -> f32 {
    match spatial {
        SoundSpatialMode::TwoD => 1.0,
        SoundSpatialMode::ThreeD { min_distance_m, .. } => *min_distance_m,
    }
}

fn max_distance(spatial: &SoundSpatialMode) -> f32 {
    match spatial {
        SoundSpatialMode::TwoD => 32.0,
        SoundSpatialMode::ThreeD { max_distance_m, .. } => *max_distance_m,
    }
}

fn spatialization_strength(spatial: &SoundSpatialMode) -> f32 {
    match spatial {
        SoundSpatialMode::TwoD => 0.0,
        SoundSpatialMode::ThreeD { .. } => 0.85,
    }
}

fn linear_volume_to_db(volume: f32) -> f32 {
    if volume <= 0.0 {
        -60.0
    } else {
        (20.0 * volume.log10()).clamp(-60.0, 12.0)
    }
}

#[cfg(test)]
#[path = "sound_editor_runtime_audio_tests.rs"]
mod tests;
