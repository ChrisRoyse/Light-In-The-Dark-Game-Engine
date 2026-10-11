use crate::{
    EngineError,
    animation::invalid_animation,
    math::{Quat, Transform, Vec3},
};

use super::types::{AnimationSkin, SkeletalAnimationAsset};

pub(super) const BLEND_EPSILON: f32 = 0.000_001;

pub(super) fn find_skin(
    asset: &SkeletalAnimationAsset,
    skin_index: usize,
) -> Result<&AnimationSkin, EngineError> {
    asset
        .skins
        .iter()
        .find(|skin| skin.skin_index == skin_index)
        .ok_or_else(|| EngineError::AnimationInvalidConfig {
            field: "animation.skin_index".to_string(),
            reason: format!(
                "{} does not contain skin {skin_index}",
                asset.source_relative_path
            ),
        })
}

pub(super) fn clip_duration_ms(
    asset: &SkeletalAnimationAsset,
    clip_name: &str,
) -> Result<u32, EngineError> {
    asset
        .clips
        .iter()
        .find(|clip| clip.name == clip_name)
        .map(|clip| clip.duration_ms)
        .ok_or_else(|| EngineError::AnimationInvalidConfig {
            field: "animation.clip_name".to_string(),
            reason: format!(
                "{} does not contain animation clip '{clip_name}'",
                asset.source_relative_path
            ),
        })
}

pub(super) fn validate_name(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_animation(field, "value must be non-empty");
    }
    Ok(())
}

pub(super) fn validate_transform(transform: Transform, field: &str) -> Result<(), EngineError> {
    validate_transform_parts(transform.translation, transform.rotation, field)?;
    if !transform.scale.is_finite() {
        return invalid_animation(field, "scale contains non-finite values");
    }
    Ok(())
}

pub(super) fn validate_transform_parts(
    translation: Vec3,
    rotation: Quat,
    field: &str,
) -> Result<(), EngineError> {
    if !translation.is_finite() {
        return invalid_animation(field, "translation contains non-finite values");
    }
    validate_quat(rotation, field)
}

pub(super) fn validate_quat(rotation: Quat, field: &str) -> Result<(), EngineError> {
    if rotation.to_array().iter().any(|value| !value.is_finite())
        || rotation.length() <= f32::EPSILON
    {
        return invalid_animation(field, "rotation quaternion is invalid");
    }
    Ok(())
}
