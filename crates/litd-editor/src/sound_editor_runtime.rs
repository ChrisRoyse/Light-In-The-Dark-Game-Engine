use std::collections::BTreeMap;

use litd_engine::EngineError;

use super::{
    PreviewPlaybackState, PreviewTransportState, ResolvedTriggerSound, SoundEditorRecord,
    SoundEditorRuntime, SoundLibraryRow, TriggerSoundBinding, invalid_sound_editor,
};

pub fn build_sound_editor_runtime(
    record: &SoundEditorRecord,
    preview_variable_name: Option<&str>,
    preview_cursor_ms: u64,
) -> Result<SoundEditorRuntime, EngineError> {
    record.validate()?;
    let assets = record
        .library
        .iter()
        .map(|entry| (entry.asset_id.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let named = record
        .named_sounds
        .iter()
        .map(|sound| (sound.variable_name.as_str(), sound))
        .collect::<BTreeMap<_, _>>();
    let mut variable_by_asset = BTreeMap::<&str, Vec<String>>::new();
    for sound in &record.named_sounds {
        variable_by_asset
            .entry(sound.asset_id.as_str())
            .or_default()
            .push(sound.variable_name.clone());
    }
    let library_rows = record
        .library
        .iter()
        .map(|entry| SoundLibraryRow {
            category: entry.category,
            file_name: entry.file_name.clone(),
            variable_names: variable_by_asset
                .get(entry.asset_id.as_str())
                .cloned()
                .unwrap_or_default(),
            length_ms: entry.length_ms,
        })
        .collect();
    let mut resolved_trigger_bindings = Vec::new();
    for binding in &record.trigger_bindings {
        resolved_trigger_bindings.push(resolve_trigger_sound(record, binding)?);
    }
    let preview = match preview_variable_name {
        Some(variable_name) => {
            let sound =
                named
                    .get(variable_name)
                    .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                        field: "editor.sound_editor.preview.variable_name".to_string(),
                        reason: format!("missing preview sound variable {variable_name}"),
                    })?;
            let entry = assets.get(sound.asset_id.as_str()).ok_or_else(|| {
                EngineError::UiRenderInvalidConfig {
                    field: "editor.sound_editor.preview.asset_id".to_string(),
                    reason: format!("missing preview asset {}", sound.asset_id),
                }
            })?;
            if preview_cursor_ms > entry.length_ms {
                return invalid_sound_editor(
                    "preview.cursor_ms",
                    format!(
                        "cursor {preview_cursor_ms} exceeds sound length {}",
                        entry.length_ms
                    ),
                );
            }
            Some(PreviewTransportState {
                variable_name: sound.variable_name.clone(),
                state: PreviewPlaybackState::Playing,
                cursor_ms: preview_cursor_ms,
                length_ms: entry.length_ms,
                playback: sound.playback.clone(),
                source_asset_id: entry.asset_id.clone(),
            })
        }
        None => None,
    };
    Ok(SoundEditorRuntime {
        library_rows,
        resolved_trigger_bindings,
        preview,
        source_fingerprint: record.stable_fingerprint()?,
    })
}

pub fn stop_preview(
    record: &SoundEditorRecord,
    variable_name: &str,
) -> Result<PreviewTransportState, EngineError> {
    let runtime = build_sound_editor_runtime(record, Some(variable_name), 0)?;
    let mut preview = runtime
        .preview
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.sound_editor.preview".to_string(),
            reason: "preview state was not produced".to_string(),
        })?;
    preview.state = PreviewPlaybackState::Stopped;
    Ok(preview)
}

fn resolve_trigger_sound(
    record: &SoundEditorRecord,
    binding: &TriggerSoundBinding,
) -> Result<ResolvedTriggerSound, EngineError> {
    let sound = record
        .named_sounds
        .iter()
        .find(|sound| sound.variable_name == binding.variable_name)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.sound_editor.trigger_bindings.variable_name".to_string(),
            reason: format!("missing sound variable {}", binding.variable_name),
        })?;
    let entry = record
        .library
        .iter()
        .find(|entry| entry.asset_id == sound.asset_id)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.sound_editor.trigger_bindings.asset_id".to_string(),
            reason: format!("missing sound asset {}", sound.asset_id),
        })?;
    Ok(ResolvedTriggerSound {
        trigger_id: binding.trigger_id.clone(),
        action_id: binding.action_id.clone(),
        variable_name: sound.variable_name.clone(),
        asset_id: entry.asset_id.clone(),
        file_name: entry.file_name.clone(),
        length_ms: entry.length_ms,
        playback: sound.playback.clone(),
    })
}
