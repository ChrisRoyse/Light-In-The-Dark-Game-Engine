use litd_engine::{
    EngineError,
    audio::{AudioBus, AudioOcclusion},
};

use super::{
    RuntimeAudioCueDocument, RuntimeAudioCueKind, audio_error, default_cone_inner_degrees,
    default_cone_outer_degrees, default_direction,
};

pub(super) fn sound_emitters_from_editor_document(
    document: &serde_json::Value,
) -> Result<Vec<RuntimeAudioCueDocument>, EngineError> {
    let Some(objects) = document
        .get("objects")
        .and_then(serde_json::Value::as_object)
    else {
        return Ok(Vec::new());
    };
    let mut cues = Vec::new();
    for (object_id, object) in objects {
        if object.get("kind").and_then(serde_json::Value::as_str) != Some("SoundEmitter") {
            continue;
        }
        let properties = object
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| {
                audio_error(
                    "object.properties",
                    format!("{object_id} has no properties"),
                )
            })?;
        let transform = object.get("transform").ok_or_else(|| {
            audio_error("object.transform", format!("{object_id} has no transform"))
        })?;
        let position = transform
            .get("translation")
            .and_then(array3)
            .ok_or_else(|| {
                audio_error(
                    "object.transform.translation",
                    format!("{object_id} has no translation"),
                )
            })?;
        cues.push(RuntimeAudioCueDocument {
            trigger_id: property(properties, "audio.trigger_id", object_id)?,
            cue_id: property(properties, "audio.cue_id", object_id)?,
            kind: parse_kind(&property(properties, "audio.kind", object_id)?)?,
            asset_product_id: property(properties, "audio.product_id", object_id)?,
            bus: Some(parse_bus(&property(properties, "audio.bus", object_id)?)?),
            position,
            velocity: parse_vec3_property(properties, "audio.velocity", [0.0, 0.0, 0.0])?,
            direction: parse_vec3_property(properties, "audio.direction", default_direction())?,
            cone_inner_degrees: parse_f32_property(
                properties,
                "audio.cone_inner_degrees",
                default_cone_inner_degrees(),
            )?,
            cone_outer_degrees: parse_f32_property(
                properties,
                "audio.cone_outer_degrees",
                default_cone_outer_degrees(),
            )?,
            min_distance_meters: parse_f32_property(properties, "audio.min_distance_meters", 1.0)?,
            max_distance_meters: parse_f32_property(properties, "audio.max_distance_meters", 32.0)?,
            spatialization_strength: parse_f32_property(
                properties,
                "audio.spatialization_strength",
                0.85,
            )?,
            volume_db: parse_f32_property(properties, "audio.volume_db", -9.0)?,
            playback_rate: f64::from(parse_f32_property(properties, "audio.playback_rate", 1.0)?),
            reverb_zone: None,
            occlusion: Some(AudioOcclusion {
                factor: parse_f32_property(properties, "audio.occlusion_factor", 0.0)?,
            }),
        });
    }
    Ok(cues)
}

fn parse_kind(value: &str) -> Result<RuntimeAudioCueKind, EngineError> {
    match value {
        "positional_sound" => Ok(RuntimeAudioCueKind::PositionalSound),
        "music" => Ok(RuntimeAudioCueKind::Music),
        "ambience" => Ok(RuntimeAudioCueKind::Ambience),
        other => Err(audio_error(
            "object.audio.kind",
            format!("unsupported kind {other}"),
        )),
    }
}

fn parse_bus(value: &str) -> Result<AudioBus, EngineError> {
    match value {
        "sfx" => Ok(AudioBus::Sfx),
        "music" => Ok(AudioBus::Music),
        "voice" => Ok(AudioBus::Voice),
        other => Err(audio_error(
            "object.audio.bus",
            format!("unsupported bus {other}"),
        )),
    }
}

fn property(
    properties: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    object_id: &str,
) -> Result<String, EngineError> {
    properties
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| audio_error("object.property", format!("{object_id} missing {key}")))
}

fn parse_vec3_property(
    properties: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    default: [f32; 3],
) -> Result<[f32; 3], EngineError> {
    let Some(value) = properties.get(key).and_then(serde_json::Value::as_str) else {
        return Ok(default);
    };
    let parts = value
        .split(',')
        .map(|part| part.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| audio_error(key, format!("invalid vector: {error}")))?;
    if parts.len() != 3 || parts.iter().any(|value| !value.is_finite()) {
        return Err(audio_error(key, "vector must have three finite components"));
    }
    Ok([parts[0], parts[1], parts[2]])
}

fn parse_f32_property(
    properties: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    default: f32,
) -> Result<f32, EngineError> {
    let Some(value) = properties.get(key).and_then(serde_json::Value::as_str) else {
        return Ok(default);
    };
    value
        .parse::<f32>()
        .map_err(|error| audio_error(key, format!("invalid number: {error}")))
}

fn array3(value: &serde_json::Value) -> Option<[f32; 3]> {
    let values = value.as_array()?;
    if values.len() != 3 {
        return None;
    }
    Some([
        values[0].as_f64()? as f32,
        values[1].as_f64()? as f32,
        values[2].as_f64()? as f32,
    ])
}
