use crate::{EngineError, animation::invalid_animation};

use super::{
    blend_support::{find_skin, validate_quat, validate_transform_parts},
    blend_types::{AnimationRootMotionDelta, AnimationRootMotionRequest},
    sample::sample_skeletal_animation,
    types::{AnimationSampleRequest, SkeletalAnimationAsset},
};

pub fn extract_root_motion(
    asset: &SkeletalAnimationAsset,
    request: AnimationRootMotionRequest,
) -> Result<AnimationRootMotionDelta, EngineError> {
    if request.from_time_ms > request.to_time_ms {
        return invalid_animation("root_motion.time", "from_time_ms must be <= to_time_ms");
    }
    let skin = find_skin(asset, request.skin_index)?;
    let root_node_index = request
        .root_node_index
        .or(skin.skeleton_root_index)
        .ok_or_else(|| EngineError::AnimationInvalidConfig {
            field: "animation.root_motion.root_node_index".to_string(),
            reason: "root node must be explicit or supplied by glTF skin.skeleton".to_string(),
        })?;
    if root_node_index >= asset.nodes.len() {
        return invalid_animation(
            "root_motion.root_node_index",
            "root node is outside node table",
        );
    }
    let from = sample_skeletal_animation(
        asset,
        AnimationSampleRequest {
            clip_name: request.clip_name.clone(),
            skin_index: request.skin_index,
            playback_time_ms: request.from_time_ms,
        },
    )?;
    let to = sample_skeletal_animation(
        asset,
        AnimationSampleRequest {
            clip_name: request.clip_name.clone(),
            skin_index: request.skin_index,
            playback_time_ms: request.to_time_ms,
        },
    )?;
    let (_, from_rotation, from_translation) =
        from.node_global_matrices[root_node_index].to_scale_rotation_translation();
    let (_, to_rotation, to_translation) =
        to.node_global_matrices[root_node_index].to_scale_rotation_translation();
    validate_transform_parts(from_translation, from_rotation, "root_motion.from")?;
    validate_transform_parts(to_translation, to_rotation, "root_motion.to")?;
    let rotation_delta = (to_rotation * from_rotation.inverse()).normalize();
    validate_quat(rotation_delta, "root_motion.rotation_delta")?;
    Ok(AnimationRootMotionDelta {
        schema: "litd.animation_root_motion_delta.v1".to_string(),
        source_relative_path: asset.source_relative_path.clone(),
        clip_name: request.clip_name,
        root_node_index,
        from_time_ms: request.from_time_ms,
        to_time_ms: request.to_time_ms,
        from_global_translation: from_translation,
        to_global_translation: to_translation,
        translation_delta: to_translation - from_translation,
        rotation_delta,
    })
}
