use crate::{
    EngineError,
    animation::{
        AnimationChannelValues, AnimationClip, AnimationInterpolation, AnimationPose,
        AnimationSampleRequest, AnimationSkin, SkeletalAnimationAsset, invalid_animation,
    },
    math::{Mat4, Quat, Transform, Vec3},
};

pub fn sample_skeletal_animation(
    asset: &SkeletalAnimationAsset,
    request: AnimationSampleRequest,
) -> Result<AnimationPose, EngineError> {
    validate_asset(asset)?;
    let clip = asset
        .clips
        .iter()
        .find(|clip| clip.name == request.clip_name)
        .ok_or_else(|| EngineError::AnimationInvalidConfig {
            field: "animation.clip_name".to_string(),
            reason: format!(
                "{} does not contain animation clip '{}'",
                asset.source_relative_path, request.clip_name
            ),
        })?;
    if request.playback_time_ms > clip.duration_ms {
        return invalid_animation(
            "playback_time_ms",
            format!(
                "{}ms exceeds clip '{}' duration {}ms",
                request.playback_time_ms, clip.name, clip.duration_ms
            ),
        );
    }
    let skin = asset
        .skins
        .iter()
        .find(|skin| skin.skin_index == request.skin_index)
        .ok_or_else(|| EngineError::AnimationInvalidConfig {
            field: "animation.skin_index".to_string(),
            reason: format!(
                "{} does not contain skin {}",
                asset.source_relative_path, request.skin_index
            ),
        })?;
    let mut node_local_transforms = asset
        .nodes
        .iter()
        .map(|node| node.base_transform)
        .collect::<Vec<_>>();
    let time_secs = request.playback_time_ms as f32 / 1000.0;
    apply_clip(clip, &mut node_local_transforms, time_secs)?;
    build_animation_pose_from_locals(
        asset,
        skin,
        &clip.name,
        request.playback_time_ms,
        node_local_transforms,
    )
}

pub(crate) fn build_animation_pose_from_locals(
    asset: &SkeletalAnimationAsset,
    skin: &AnimationSkin,
    clip_name: &str,
    playback_time_ms: u32,
    node_local_transforms: Vec<Transform>,
) -> Result<AnimationPose, EngineError> {
    validate_asset(asset)?;
    if node_local_transforms.len() != asset.nodes.len() {
        return invalid_animation(
            "node_local_transforms",
            "local transform count does not match asset node count",
        );
    }
    if !asset
        .skins
        .iter()
        .any(|candidate| candidate.skin_index == skin.skin_index)
    {
        return invalid_animation("skin_index", "skin is not owned by the animation asset");
    }
    let node_global_matrices = compute_global_matrices(asset, &node_local_transforms)?;
    let joint_matrices = joint_matrices(skin, &node_global_matrices)?;
    Ok(AnimationPose {
        source_relative_path: asset.source_relative_path.clone(),
        clip_name: clip_name.to_string(),
        skin_index: skin.skin_index,
        playback_time_ms,
        node_local_transforms,
        node_global_matrices,
        joint_matrices,
    })
}

fn validate_asset(asset: &SkeletalAnimationAsset) -> Result<(), EngineError> {
    if asset.nodes.len() != asset.node_count {
        return invalid_animation("node_count", "node table length does not match metadata");
    }
    if asset.skins.len() != asset.skin_count || asset.clips.len() != asset.clip_count {
        return invalid_animation(
            "asset_counts",
            "skin or clip table length does not match metadata",
        );
    }
    Ok(())
}

fn apply_clip(
    clip: &AnimationClip,
    node_transforms: &mut [Transform],
    time_secs: f32,
) -> Result<(), EngineError> {
    for channel in &clip.channels {
        let transform = node_transforms
            .get_mut(channel.target_node_index)
            .ok_or_else(|| EngineError::AnimationInvalidConfig {
                field: "animation.channel.target".to_string(),
                reason: format!("clip '{}' targets a node outside the pose", clip.name),
            })?;
        match &channel.values {
            AnimationChannelValues::Translations(values) => {
                transform.translation =
                    sample_vec3(&channel.times, values, channel.interpolation, time_secs)?;
            }
            AnimationChannelValues::Rotations(values) => {
                transform.rotation =
                    sample_quat(&channel.times, values, channel.interpolation, time_secs)?;
            }
            AnimationChannelValues::Scales(values) => {
                transform.scale =
                    sample_vec3(&channel.times, values, channel.interpolation, time_secs)?;
            }
        }
    }
    Ok(())
}

fn compute_global_matrices(
    asset: &SkeletalAnimationAsset,
    local_transforms: &[Transform],
) -> Result<Vec<Mat4>, EngineError> {
    let mut resolved = vec![None; asset.nodes.len()];
    let mut visiting = vec![false; asset.nodes.len()];
    for index in 0..asset.nodes.len() {
        resolve_global(index, asset, local_transforms, &mut resolved, &mut visiting)?;
    }
    resolved
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| EngineError::AnimationInvalidConfig {
            field: "animation.node_global_matrices".to_string(),
            reason: "node global matrix resolution left a gap".to_string(),
        })
}

fn resolve_global(
    index: usize,
    asset: &SkeletalAnimationAsset,
    local_transforms: &[Transform],
    resolved: &mut [Option<Mat4>],
    visiting: &mut [bool],
) -> Result<Mat4, EngineError> {
    if let Some(matrix) = resolved[index] {
        return Ok(matrix);
    }
    if visiting[index] {
        return invalid_animation("node.parent", "node hierarchy contains a cycle");
    }
    visiting[index] = true;
    let node = asset.nodes[index];
    let local = local_transforms[index].matrix();
    let global = match node.parent_index {
        Some(parent) => {
            if parent >= asset.nodes.len() {
                return invalid_animation("node.parent", "parent index is outside the node table");
            }
            resolve_global(parent, asset, local_transforms, resolved, visiting)? * local
        }
        None => local,
    };
    validate_mat4(global, "node.global_matrix")?;
    resolved[index] = Some(global);
    visiting[index] = false;
    Ok(global)
}

fn joint_matrices(skin: &AnimationSkin, globals: &[Mat4]) -> Result<Vec<Mat4>, EngineError> {
    if skin.joint_node_indices.len() != skin.inverse_bind_matrices.len() {
        return invalid_animation(
            "skin.inverse_bind_matrices",
            "joint count does not match inverse bind matrix count",
        );
    }
    skin.joint_node_indices
        .iter()
        .zip(&skin.inverse_bind_matrices)
        .map(|(joint_node, inverse_bind)| {
            let global =
                *globals
                    .get(*joint_node)
                    .ok_or_else(|| EngineError::AnimationInvalidConfig {
                        field: "animation.skin.joint".to_string(),
                        reason: format!("joint node {joint_node} is outside the global pose"),
                    })?;
            let matrix = global * *inverse_bind;
            validate_mat4(matrix, "skin.joint_matrix")?;
            Ok(matrix)
        })
        .collect()
}

fn sample_vec3(
    times: &[f32],
    values: &[Vec3],
    interpolation: AnimationInterpolation,
    time_secs: f32,
) -> Result<Vec3, EngineError> {
    let window = sample_window(times, values.len(), time_secs)?;
    let value = match interpolation {
        AnimationInterpolation::Step => values[window.lower],
        AnimationInterpolation::Linear => {
            values[window.lower].lerp(values[window.upper], window.alpha)
        }
    };
    if !value.is_finite() {
        return invalid_animation("sample", "sampled vector is not finite");
    }
    Ok(value)
}

fn sample_quat(
    times: &[f32],
    values: &[Quat],
    interpolation: AnimationInterpolation,
    time_secs: f32,
) -> Result<Quat, EngineError> {
    let window = sample_window(times, values.len(), time_secs)?;
    let value = match interpolation {
        AnimationInterpolation::Step => values[window.lower],
        AnimationInterpolation::Linear => {
            values[window.lower].slerp(values[window.upper], window.alpha)
        }
    };
    if value
        .to_array()
        .iter()
        .any(|component| !component.is_finite())
        || value.length() <= f32::EPSILON
    {
        return invalid_animation("sample", "sampled rotation is invalid");
    }
    Ok(value.normalize())
}

#[derive(Debug, Clone, Copy)]
struct SampleWindow {
    lower: usize,
    upper: usize,
    alpha: f32,
}

fn sample_window(
    times: &[f32],
    value_count: usize,
    time_secs: f32,
) -> Result<SampleWindow, EngineError> {
    if times.is_empty() || times.len() != value_count {
        return invalid_animation("sample", "sample table is empty or mismatched");
    }
    if times.len() == 1 || time_secs <= times[0] {
        return Ok(SampleWindow {
            lower: 0,
            upper: 0,
            alpha: 0.0,
        });
    }
    for upper in 1..times.len() {
        if time_secs <= times[upper] {
            let lower = upper - 1;
            let span = times[upper] - times[lower];
            if span <= 0.0 {
                return invalid_animation("sample", "sample times are not increasing");
            }
            return Ok(SampleWindow {
                lower,
                upper,
                alpha: ((time_secs - times[lower]) / span).clamp(0.0, 1.0),
            });
        }
    }
    let last = times.len() - 1;
    Ok(SampleWindow {
        lower: last,
        upper: last,
        alpha: 0.0,
    })
}

fn validate_mat4(matrix: Mat4, field: &str) -> Result<(), EngineError> {
    if matrix
        .to_cols_array()
        .iter()
        .any(|value| !value.is_finite())
    {
        return invalid_animation(field, "matrix contains non-finite values");
    }
    Ok(())
}
