use crate::{
    EngineError,
    animation::invalid_animation,
    math::{Quat, Transform, Vec3},
};

use super::{
    blend_support::{
        BLEND_EPSILON, find_skin, validate_name, validate_transform, validate_transform_parts,
    },
    blend_types::{
        AnimationBlendLayer, AnimationBlendPoseRequest, AnimationBlendPoseResult,
        AnimationBlendSpace1d, AnimationBlendSpace1dRequest, AnimationBlendSpace2d,
        AnimationBlendSpace2dRequest, AnimationNormalizedBlendLayer,
    },
    sample::{build_animation_pose_from_locals, sample_skeletal_animation},
    types::{AnimationPose, AnimationSampleRequest, SkeletalAnimationAsset},
};

pub fn blend_skeletal_animation_poses(
    asset: &SkeletalAnimationAsset,
    request: AnimationBlendPoseRequest,
) -> Result<AnimationBlendPoseResult, EngineError> {
    let skin = find_skin(asset, request.skin_index)?;
    if request.layers.is_empty() {
        return invalid_animation(
            "blend.layers",
            "blend request must contain at least one layer",
        );
    }
    let total_weight = request.layers.iter().try_fold(0.0, |total, layer| {
        validate_layer(layer)?;
        Ok::<f32, EngineError>(total + layer.weight)
    })?;
    if total_weight <= BLEND_EPSILON {
        return invalid_animation("blend.weight", "blend layer weights must sum above zero");
    }
    let normalized_layers = request
        .layers
        .iter()
        .map(|layer| AnimationNormalizedBlendLayer {
            clip_name: layer.clip_name.clone(),
            playback_time_ms: layer.playback_time_ms,
            weight: layer.weight / total_weight,
        })
        .collect::<Vec<_>>();
    let poses = request
        .layers
        .iter()
        .map(|layer| {
            sample_skeletal_animation(
                asset,
                AnimationSampleRequest {
                    clip_name: layer.clip_name.clone(),
                    skin_index: request.skin_index,
                    playback_time_ms: layer.playback_time_ms,
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_pose_shapes(&poses, asset)?;
    let node_local_transforms = blend_local_transforms(&poses, &normalized_layers)?;
    let pose = build_animation_pose_from_locals(
        asset,
        skin,
        &blend_clip_label(&normalized_layers),
        normalized_layers
            .iter()
            .map(|layer| layer.playback_time_ms)
            .max()
            .unwrap_or_default(),
        node_local_transforms,
    )?;
    Ok(AnimationBlendPoseResult {
        schema: "litd.animation_blend_pose.v1".to_string(),
        skin_index: request.skin_index,
        layer_count: normalized_layers.len(),
        normalized_layers,
        pose,
    })
}

pub fn evaluate_animation_blend_space_1d(
    asset: &SkeletalAnimationAsset,
    space: &AnimationBlendSpace1d,
    request: AnimationBlendSpace1dRequest,
) -> Result<AnimationBlendPoseResult, EngineError> {
    validate_blend_space_1d(space)?;
    if !request.parameter_value.is_finite() {
        return invalid_animation("blend_space.parameter", "parameter value must be finite");
    }
    let first = &space.points[0];
    let last = space
        .points
        .last()
        .expect("validated non-empty blend space");
    let layers = if request.parameter_value <= first.position || space.points.len() == 1 {
        vec![blend_space_layer(
            &first.clip_name,
            request.playback_time_ms,
            1.0,
        )]
    } else if request.parameter_value >= last.position {
        vec![blend_space_layer(
            &last.clip_name,
            request.playback_time_ms,
            1.0,
        )]
    } else {
        blend_space_1d_layers(space, request.parameter_value, request.playback_time_ms)?
    };
    blend_skeletal_animation_poses(
        asset,
        AnimationBlendPoseRequest {
            skin_index: request.skin_index,
            layers,
        },
    )
}

pub fn evaluate_animation_blend_space_2d(
    asset: &SkeletalAnimationAsset,
    space: &AnimationBlendSpace2d,
    request: AnimationBlendSpace2dRequest,
) -> Result<AnimationBlendPoseResult, EngineError> {
    validate_blend_space_2d(space)?;
    if !request.parameter_value.is_finite() {
        return invalid_animation("blend_space.parameter", "parameter value must be finite");
    }
    let mut exact = None;
    let mut weighted = Vec::new();
    let mut total = 0.0_f32;
    for point in &space.points {
        let distance_sq = (point.position - request.parameter_value).length_squared();
        if distance_sq <= BLEND_EPSILON {
            exact = Some(point.clip_name.clone());
            break;
        }
        let weight = 1.0 / distance_sq.max(BLEND_EPSILON);
        total += weight;
        weighted.push((point.clip_name.clone(), weight));
    }
    let layers = match exact {
        Some(clip_name) => vec![blend_space_layer(&clip_name, request.playback_time_ms, 1.0)],
        None => weighted
            .into_iter()
            .map(|(clip_name, weight)| {
                blend_space_layer(&clip_name, request.playback_time_ms, weight / total)
            })
            .collect(),
    };
    blend_skeletal_animation_poses(
        asset,
        AnimationBlendPoseRequest {
            skin_index: request.skin_index,
            layers,
        },
    )
}

fn blend_space_1d_layers(
    space: &AnimationBlendSpace1d,
    parameter_value: f32,
    playback_time_ms: u32,
) -> Result<Vec<AnimationBlendLayer>, EngineError> {
    for pair in space.points.windows(2) {
        let lower = &pair[0];
        let upper = &pair[1];
        if parameter_value >= lower.position && parameter_value <= upper.position {
            let span = upper.position - lower.position;
            if span <= BLEND_EPSILON {
                return invalid_animation(
                    "blend_space.points",
                    "blend-space point positions must be strictly increasing",
                );
            }
            let alpha = (parameter_value - lower.position) / span;
            return Ok(vec![
                blend_space_layer(&lower.clip_name, playback_time_ms, 1.0 - alpha),
                blend_space_layer(&upper.clip_name, playback_time_ms, alpha),
            ]);
        }
    }
    invalid_animation("blend_space.parameter", "no blend-space segment matched")
}

fn blend_local_transforms(
    poses: &[AnimationPose],
    layers: &[AnimationNormalizedBlendLayer],
) -> Result<Vec<Transform>, EngineError> {
    let node_count = poses[0].node_local_transforms.len();
    let mut transforms = Vec::with_capacity(node_count);
    for node_index in 0..node_count {
        let mut translation = Vec3::ZERO;
        let mut scale = Vec3::ZERO;
        let mut rotation = Quat::IDENTITY;
        let mut rotation_weight = 0.0_f32;
        for (pose, layer) in poses.iter().zip(layers) {
            let transform = pose.node_local_transforms[node_index];
            validate_transform(transform, "blend.pose")?;
            translation += transform.translation * layer.weight;
            scale += transform.scale * layer.weight;
            if layer.weight > BLEND_EPSILON {
                if rotation_weight <= BLEND_EPSILON {
                    rotation = transform.rotation;
                    rotation_weight = layer.weight;
                } else {
                    let alpha = layer.weight / (rotation_weight + layer.weight);
                    rotation = rotation.slerp(transform.rotation, alpha).normalize();
                    rotation_weight += layer.weight;
                }
            }
        }
        validate_transform_parts(translation, rotation, "blend.result")?;
        if !scale.is_finite() {
            return invalid_animation("blend.scale", "blended scale is not finite");
        }
        transforms.push(Transform::from_trs(translation, rotation, scale));
    }
    Ok(transforms)
}

fn validate_pose_shapes(
    poses: &[AnimationPose],
    asset: &SkeletalAnimationAsset,
) -> Result<(), EngineError> {
    for pose in poses {
        if pose.source_relative_path != asset.source_relative_path
            || pose.node_local_transforms.len() != asset.nodes.len()
        {
            return invalid_animation("blend.pose", "sampled pose does not match asset shape");
        }
    }
    Ok(())
}

fn validate_blend_space_1d(space: &AnimationBlendSpace1d) -> Result<(), EngineError> {
    validate_name("blend_space.name", &space.name)?;
    validate_name("blend_space.parameter_name", &space.parameter_name)?;
    if space.points.is_empty() {
        return invalid_animation("blend_space.points", "blend space needs at least one point");
    }
    let mut previous = None;
    for point in &space.points {
        validate_name("blend_space.point.clip_name", &point.clip_name)?;
        if !point.position.is_finite() {
            return invalid_animation("blend_space.point.position", "position must be finite");
        }
        if let Some(previous_position) = previous
            && point.position <= previous_position
        {
            return invalid_animation(
                "blend_space.point.position",
                "1D blend-space positions must be strictly increasing",
            );
        }
        previous = Some(point.position);
    }
    Ok(())
}

fn validate_blend_space_2d(space: &AnimationBlendSpace2d) -> Result<(), EngineError> {
    validate_name("blend_space.name", &space.name)?;
    validate_name("blend_space.parameter_x", &space.parameter_x)?;
    validate_name("blend_space.parameter_y", &space.parameter_y)?;
    if space.points.is_empty() {
        return invalid_animation("blend_space.points", "blend space needs at least one point");
    }
    for (index, point) in space.points.iter().enumerate() {
        validate_name("blend_space.point.clip_name", &point.clip_name)?;
        if !point.position.is_finite() {
            return invalid_animation("blend_space.point.position", "position must be finite");
        }
        for previous in &space.points[..index] {
            if (point.position - previous.position).length_squared() <= BLEND_EPSILON {
                return invalid_animation(
                    "blend_space.point.position",
                    "2D blend-space point positions must be unique",
                );
            }
        }
    }
    Ok(())
}

fn validate_layer(layer: &AnimationBlendLayer) -> Result<(), EngineError> {
    validate_name("blend.layer.clip_name", &layer.clip_name)?;
    if !layer.weight.is_finite() || layer.weight < 0.0 {
        return invalid_animation("blend.layer.weight", "layer weight must be finite and >= 0");
    }
    Ok(())
}

fn blend_space_layer(clip_name: &str, playback_time_ms: u32, weight: f32) -> AnimationBlendLayer {
    AnimationBlendLayer {
        clip_name: clip_name.to_string(),
        playback_time_ms,
        weight,
    }
}

fn blend_clip_label(layers: &[AnimationNormalizedBlendLayer]) -> String {
    let mut label = String::from("blend(");
    for (index, layer) in layers.iter().enumerate() {
        if index > 0 {
            label.push('+');
        }
        label.push_str(&format!(
            "{}@{}ms:{:.3}",
            layer.clip_name, layer.playback_time_ms, layer.weight
        ));
    }
    label.push(')');
    label
}
