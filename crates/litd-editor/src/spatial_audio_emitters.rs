use litd_engine::{
    EngineError,
    audio::AudioBus,
    math::{Transform, Vec3},
};
use serde::{Deserialize, Serialize};

use crate::{
    asset_products::AssetProductKind,
    history::{EditorDocument, EditorObject, stable_fingerprint},
};

pub const SOUND_EMITTER_KIND: &str = "SoundEmitter";
pub const AUDIO_PRODUCT_ID_PROPERTY: &str = "audio.product_id";
pub const AUDIO_TRIGGER_ID_PROPERTY: &str = "audio.trigger_id";
pub const AUDIO_CUE_ID_PROPERTY: &str = "audio.cue_id";
pub const AUDIO_KIND_PROPERTY: &str = "audio.kind";
pub const AUDIO_BUS_PROPERTY: &str = "audio.bus";
pub const AUDIO_VELOCITY_PROPERTY: &str = "audio.velocity";
pub const AUDIO_DIRECTION_PROPERTY: &str = "audio.direction";
pub const AUDIO_CONE_INNER_PROPERTY: &str = "audio.cone_inner_degrees";
pub const AUDIO_CONE_OUTER_PROPERTY: &str = "audio.cone_outer_degrees";
pub const AUDIO_MIN_DISTANCE_PROPERTY: &str = "audio.min_distance_meters";
pub const AUDIO_MAX_DISTANCE_PROPERTY: &str = "audio.max_distance_meters";
pub const AUDIO_SPATIALIZATION_PROPERTY: &str = "audio.spatialization_strength";
pub const AUDIO_VOLUME_PROPERTY: &str = "audio.volume_db";
pub const AUDIO_PLAYBACK_RATE_PROPERTY: &str = "audio.playback_rate";
pub const AUDIO_OCCLUSION_PROPERTY: &str = "audio.occlusion_factor";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialAudioEmitterKind {
    PositionalSound,
    Music,
    Ambience,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialAudioEmitterPlacementRequest {
    pub object_id: String,
    pub parent_id: Option<String>,
    pub product_id: String,
    pub trigger_id: String,
    pub cue_id: String,
    pub kind: SpatialAudioEmitterKind,
    pub bus: AudioBus,
    pub position: Vec3,
    pub velocity: Vec3,
    pub direction: Vec3,
    pub cone_inner_degrees: f32,
    pub cone_outer_degrees: f32,
    pub min_distance_meters: f32,
    pub max_distance_meters: f32,
    pub spatialization_strength: f32,
    pub volume_db: f32,
    pub playback_rate: f32,
    pub occlusion_factor: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialAudioEmitterPlacementReport {
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub object_id: String,
    pub object_count: usize,
    pub readback: SpatialAudioEmitterReadback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialAudioEmitterReadback {
    pub object_id: String,
    pub parent_id: Option<String>,
    pub product_id: String,
    pub trigger_id: String,
    pub cue_id: String,
    pub kind: SpatialAudioEmitterKind,
    pub bus: AudioBus,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub direction: [f32; 3],
    pub cone_inner_degrees: f32,
    pub cone_outer_degrees: f32,
    pub min_distance_meters: f32,
    pub max_distance_meters: f32,
    pub spatialization_strength: f32,
    pub volume_db: f32,
    pub playback_rate: f32,
    pub occlusion_factor: f32,
}

impl SpatialAudioEmitterKind {
    fn as_property(self) -> &'static str {
        match self {
            Self::PositionalSound => "positional_sound",
            Self::Music => "music",
            Self::Ambience => "ambience",
        }
    }
}

impl SpatialAudioEmitterPlacementRequest {
    pub fn validate(&self, document: &EditorDocument) -> Result<(), EngineError> {
        validate_symbol("object_id", &self.object_id)?;
        validate_symbol("trigger_id", &self.trigger_id)?;
        validate_symbol("cue_id", &self.cue_id)?;
        if let Some(parent) = &self.parent_id {
            if !document.objects.contains_key(parent) {
                return invalid_spatial_audio_emitter(
                    "parent_id",
                    format!("parent object {parent} does not exist"),
                );
            }
        } else if !document.objects.contains_key("world") {
            return invalid_spatial_audio_emitter(
                "parent_id",
                "document must contain world root when parent_id is omitted",
            );
        }
        let product = document
            .asset_products
            .get(&self.product_id)
            .ok_or_else(|| {
                invalid_spatial_audio_emitter_error(
                    "product_id",
                    format!("missing audio product {}", self.product_id),
                )
            })?;
        if product.product_kind != AssetProductKind::Audio {
            return invalid_spatial_audio_emitter(
                "product_kind",
                format!("{} is not an Audio product", self.product_id),
            );
        }
        product.validate()?;
        validate_vec3("position", self.position)?;
        validate_vec3("velocity", self.velocity)?;
        validate_vec3("direction", self.direction)?;
        let direction_len = vec3_len(self.direction);
        if !(0.999..=1.001).contains(&direction_len) {
            return invalid_spatial_audio_emitter("direction", "direction must be normalized");
        }
        validate_degrees("cone_inner_degrees", self.cone_inner_degrees)?;
        validate_degrees("cone_outer_degrees", self.cone_outer_degrees)?;
        if self.cone_outer_degrees < self.cone_inner_degrees {
            return invalid_spatial_audio_emitter(
                "cone_outer_degrees",
                "outer cone must be greater than or equal to inner cone",
            );
        }
        if !self.min_distance_meters.is_finite() || self.min_distance_meters < 0.0 {
            return invalid_spatial_audio_emitter(
                "min_distance_meters",
                "min distance must be finite and >= 0",
            );
        }
        if !self.max_distance_meters.is_finite()
            || self.max_distance_meters <= self.min_distance_meters
        {
            return invalid_spatial_audio_emitter(
                "max_distance_meters",
                "max distance must be finite and greater than min distance",
            );
        }
        validate_unit("spatialization_strength", self.spatialization_strength)?;
        validate_unit("occlusion_factor", self.occlusion_factor)?;
        if !self.volume_db.is_finite() || !(-60.0..=12.0).contains(&self.volume_db) {
            return invalid_spatial_audio_emitter(
                "volume_db",
                "volume must be finite in -60.0..=12.0 dB",
            );
        }
        if !self.playback_rate.is_finite() || !(0.125..=4.0).contains(&self.playback_rate) {
            return invalid_spatial_audio_emitter(
                "playback_rate",
                "playback rate must be finite in 0.125..=4.0",
            );
        }
        Ok(())
    }
}

pub fn place_spatial_audio_emitter_in_document(
    document: &mut EditorDocument,
    request: &SpatialAudioEmitterPlacementRequest,
) -> Result<SpatialAudioEmitterPlacementReport, EngineError> {
    document.validate()?;
    request.validate(document)?;
    let before_fingerprint = stable_fingerprint(document)?;
    let mut next = document.clone();
    let parent = request
        .parent_id
        .clone()
        .or_else(|| Some("world".to_string()));
    let mut object = EditorObject::new(
        request.object_id.clone(),
        SOUND_EMITTER_KIND,
        parent,
        Transform::from_translation(request.position),
    );
    object.properties.insert(
        AUDIO_PRODUCT_ID_PROPERTY.to_string(),
        request.product_id.clone(),
    );
    object.properties.insert(
        AUDIO_TRIGGER_ID_PROPERTY.to_string(),
        request.trigger_id.clone(),
    );
    object
        .properties
        .insert(AUDIO_CUE_ID_PROPERTY.to_string(), request.cue_id.clone());
    object.properties.insert(
        AUDIO_KIND_PROPERTY.to_string(),
        request.kind.as_property().to_string(),
    );
    object.properties.insert(
        AUDIO_BUS_PROPERTY.to_string(),
        request.bus.as_str().to_string(),
    );
    object.properties.insert(
        AUDIO_VELOCITY_PROPERTY.to_string(),
        vec3_property(request.velocity),
    );
    object.properties.insert(
        AUDIO_DIRECTION_PROPERTY.to_string(),
        vec3_property(request.direction),
    );
    insert_f32(
        &mut object,
        AUDIO_CONE_INNER_PROPERTY,
        request.cone_inner_degrees,
    );
    insert_f32(
        &mut object,
        AUDIO_CONE_OUTER_PROPERTY,
        request.cone_outer_degrees,
    );
    insert_f32(
        &mut object,
        AUDIO_MIN_DISTANCE_PROPERTY,
        request.min_distance_meters,
    );
    insert_f32(
        &mut object,
        AUDIO_MAX_DISTANCE_PROPERTY,
        request.max_distance_meters,
    );
    insert_f32(
        &mut object,
        AUDIO_SPATIALIZATION_PROPERTY,
        request.spatialization_strength,
    );
    insert_f32(&mut object, AUDIO_VOLUME_PROPERTY, request.volume_db);
    insert_f32(
        &mut object,
        AUDIO_PLAYBACK_RATE_PROPERTY,
        request.playback_rate,
    );
    insert_f32(
        &mut object,
        AUDIO_OCCLUSION_PROPERTY,
        request.occlusion_factor,
    );

    next.upsert_object(object)?;
    next.validate()?;
    let readback = readback_for(&next, &request.object_id)?;
    let after_fingerprint = stable_fingerprint(&next)?;
    *document = next;
    Ok(SpatialAudioEmitterPlacementReport {
        before_fingerprint,
        after_fingerprint,
        object_id: request.object_id.clone(),
        object_count: document.objects.len(),
        readback,
    })
}

pub fn collect_spatial_audio_emitters(
    document: &EditorDocument,
) -> Result<Vec<SpatialAudioEmitterReadback>, EngineError> {
    document.validate()?;
    document
        .objects
        .values()
        .filter(|object| object.kind == SOUND_EMITTER_KIND)
        .map(|object| readback_for(document, &object.id))
        .collect()
}

fn readback_for(
    document: &EditorDocument,
    object_id: &str,
) -> Result<SpatialAudioEmitterReadback, EngineError> {
    let object = document.objects.get(object_id).ok_or_else(|| {
        invalid_spatial_audio_emitter_error(
            "readback.object",
            format!("placed emitter {object_id} missing"),
        )
    })?;
    if object.kind != SOUND_EMITTER_KIND {
        return invalid_spatial_audio_emitter(
            "readback.kind",
            format!("{object_id} is not a {SOUND_EMITTER_KIND}"),
        );
    }
    Ok(SpatialAudioEmitterReadback {
        object_id: object.id.clone(),
        parent_id: object.parent.clone(),
        product_id: required_property(object, AUDIO_PRODUCT_ID_PROPERTY)?,
        trigger_id: required_property(object, AUDIO_TRIGGER_ID_PROPERTY)?,
        cue_id: required_property(object, AUDIO_CUE_ID_PROPERTY)?,
        kind: parse_kind(&required_property(object, AUDIO_KIND_PROPERTY)?)?,
        bus: parse_bus(&required_property(object, AUDIO_BUS_PROPERTY)?)?,
        position: object.transform.translation.to_array(),
        velocity: parse_vec3(&required_property(object, AUDIO_VELOCITY_PROPERTY)?)?,
        direction: parse_vec3(&required_property(object, AUDIO_DIRECTION_PROPERTY)?)?,
        cone_inner_degrees: parse_f32(object, AUDIO_CONE_INNER_PROPERTY)?,
        cone_outer_degrees: parse_f32(object, AUDIO_CONE_OUTER_PROPERTY)?,
        min_distance_meters: parse_f32(object, AUDIO_MIN_DISTANCE_PROPERTY)?,
        max_distance_meters: parse_f32(object, AUDIO_MAX_DISTANCE_PROPERTY)?,
        spatialization_strength: parse_f32(object, AUDIO_SPATIALIZATION_PROPERTY)?,
        volume_db: parse_f32(object, AUDIO_VOLUME_PROPERTY)?,
        playback_rate: parse_f32(object, AUDIO_PLAYBACK_RATE_PROPERTY)?,
        occlusion_factor: parse_f32(object, AUDIO_OCCLUSION_PROPERTY)?,
    })
}

fn insert_f32(object: &mut EditorObject, key: &str, value: f32) {
    object.properties.insert(key.to_string(), value.to_string());
}

fn required_property(object: &EditorObject, key: &str) -> Result<String, EngineError> {
    object
        .properties
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| {
            invalid_spatial_audio_emitter_error(
                "readback.property",
                format!("{} missing {key}", object.id),
            )
        })
}

fn parse_kind(value: &str) -> Result<SpatialAudioEmitterKind, EngineError> {
    match value {
        "positional_sound" => Ok(SpatialAudioEmitterKind::PositionalSound),
        "music" => Ok(SpatialAudioEmitterKind::Music),
        "ambience" => Ok(SpatialAudioEmitterKind::Ambience),
        other => invalid_spatial_audio_emitter("kind", format!("unsupported kind {other}")),
    }
}

fn parse_bus(value: &str) -> Result<AudioBus, EngineError> {
    match value {
        "sfx" => Ok(AudioBus::Sfx),
        "music" => Ok(AudioBus::Music),
        "voice" => Ok(AudioBus::Voice),
        other => invalid_spatial_audio_emitter("bus", format!("unsupported bus {other}")),
    }
}

fn parse_vec3(value: &str) -> Result<[f32; 3], EngineError> {
    let parts = value
        .split(',')
        .map(|part| part.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| invalid_spatial_audio_emitter_error("vec3", error.to_string()))?;
    if parts.len() != 3 || parts.iter().any(|value| !value.is_finite()) {
        return invalid_spatial_audio_emitter("vec3", "vector must have three finite components");
    }
    Ok([parts[0], parts[1], parts[2]])
}

fn parse_f32(object: &EditorObject, key: &str) -> Result<f32, EngineError> {
    required_property(object, key)?
        .parse::<f32>()
        .map_err(|error| invalid_spatial_audio_emitter_error(key, error.to_string()))
}

fn vec3_property(value: Vec3) -> String {
    format!("{},{},{}", value.x, value.y, value.z)
}

fn validate_symbol(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_spatial_audio_emitter(field, "identifier must be non-empty");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.')
    {
        return invalid_spatial_audio_emitter(
            field,
            format!("{value:?} must contain only ASCII alphanumeric, underscore, or dot"),
        );
    }
    Ok(())
}

fn validate_vec3(field: &str, value: Vec3) -> Result<(), EngineError> {
    if value.is_finite() {
        Ok(())
    } else {
        invalid_spatial_audio_emitter(field, "vector components must be finite")
    }
}

fn validate_degrees(field: &str, value: f32) -> Result<(), EngineError> {
    if value.is_finite() && (0.0..=360.0).contains(&value) {
        Ok(())
    } else {
        invalid_spatial_audio_emitter(field, "angle must be finite in 0.0..=360.0")
    }
}

fn validate_unit(field: &str, value: f32) -> Result<(), EngineError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        invalid_spatial_audio_emitter(field, "value must be finite in 0.0..=1.0")
    }
}

fn vec3_len(value: Vec3) -> f32 {
    (value.x * value.x + value.y * value.y + value.z * value.z).sqrt()
}

fn invalid_spatial_audio_emitter<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_spatial_audio_emitter_error(field, reason))
}

fn invalid_spatial_audio_emitter_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.spatial_audio_emitter.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "spatial_audio_emitters_tests.rs"]
mod tests;
