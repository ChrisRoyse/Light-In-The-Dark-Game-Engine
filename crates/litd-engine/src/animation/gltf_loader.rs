use std::{collections::BTreeSet, path::Path};

use crate::{
    EngineError,
    animation::{
        AnimationChannel, AnimationChannelValues, AnimationClip, AnimationInterpolation,
        AnimationNode, AnimationProperty, AnimationSkin, MAX_SKINNING_JOINTS,
        SkeletalAnimationAsset, invalid_animation,
    },
    math::{Mat4, Quat, Transform, Vec3},
};

pub fn load_gltf_skeletal_animation_asset(
    source_path: impl AsRef<Path>,
    source_relative_path: impl Into<String>,
) -> Result<SkeletalAnimationAsset, EngineError> {
    let source_path = source_path.as_ref();
    let source_relative_path = source_relative_path.into();
    validate_source(source_path)?;
    let (document, buffers, _images) =
        ::gltf::import(source_path).map_err(|error| EngineError::AnimationInvalidConfig {
            field: "animation.gltf.import".to_string(),
            reason: format!("{} failed skeletal import: {error}", source_path.display()),
        })?;
    let nodes = import_nodes(&document)?;
    let skins = document
        .skins()
        .map(|skin| import_skin(skin, &buffers, nodes.len()))
        .collect::<Result<Vec<_>, _>>()?;
    if skins.is_empty() {
        return invalid_animation("skin_count", "skeletal animation asset must contain a skin");
    }
    let clips = document
        .animations()
        .map(|clip| import_clip(clip, &buffers, &source_relative_path, nodes.len()))
        .collect::<Result<Vec<_>, _>>()?;
    if clips.is_empty() {
        return invalid_animation(
            "clip_count",
            "skeletal animation asset must contain at least one animation clip",
        );
    }
    Ok(SkeletalAnimationAsset {
        schema: "litd.skeletal_animation_asset.v1".to_string(),
        source_relative_path,
        node_count: nodes.len(),
        skin_count: skins.len(),
        clip_count: clips.len(),
        nodes,
        skins,
        clips,
    })
}

fn validate_source(source_path: &Path) -> Result<(), EngineError> {
    let extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(extension.as_deref(), Some("gltf" | "glb")) {
        return invalid_animation("source.extension", "expected .gltf or .glb skeletal source");
    }
    Ok(())
}

fn import_nodes(document: &::gltf::Document) -> Result<Vec<AnimationNode>, EngineError> {
    let node_count = document.nodes().count();
    let mut nodes = vec![None; node_count];
    for node in document.nodes() {
        let node_index = node.index();
        if node_index >= node_count {
            return invalid_animation(
                "node.index",
                format!("node index {node_index} exceeds node count {node_count}"),
            );
        }
        nodes[node_index] = Some(AnimationNode {
            node_index,
            parent_index: None,
            base_transform: node_transform(node)?,
        });
    }
    let mut nodes = nodes
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| EngineError::AnimationInvalidConfig {
            field: "animation.nodes".to_string(),
            reason: "glTF document has sparse node indices".to_string(),
        })?;
    for parent in document.nodes() {
        for child in parent.children() {
            let child_node = nodes.get_mut(child.index()).ok_or_else(|| {
                EngineError::AnimationInvalidConfig {
                    field: "animation.node.parent".to_string(),
                    reason: format!("child node {} is outside node table", child.index()),
                }
            })?;
            if child_node.parent_index.replace(parent.index()).is_some() {
                return invalid_animation(
                    "node.parent",
                    format!("node {} has more than one parent", child.index()),
                );
            }
        }
    }
    Ok(nodes)
}

fn import_skin(
    skin: ::gltf::Skin<'_>,
    buffers: &[::gltf::buffer::Data],
    node_count: usize,
) -> Result<AnimationSkin, EngineError> {
    let joint_node_indices = skin.joints().map(|joint| joint.index()).collect::<Vec<_>>();
    if joint_node_indices.is_empty() {
        return invalid_animation(
            "skin.joints",
            format!("skin {} has no joints", skin.index()),
        );
    }
    if joint_node_indices.len() > MAX_SKINNING_JOINTS {
        return invalid_animation(
            "skin.joints",
            format!(
                "skin {} has {} joints; max supported GPU palette is {}",
                skin.index(),
                joint_node_indices.len(),
                MAX_SKINNING_JOINTS
            ),
        );
    }
    for joint_index in &joint_node_indices {
        if *joint_index >= node_count {
            return invalid_animation(
                "skin.joints",
                format!(
                    "skin {} references missing node {joint_index}",
                    skin.index()
                ),
            );
        }
    }
    let joint_names = skin
        .joints()
        .map(|joint| {
            joint
                .name()
                .map(str::to_string)
                .unwrap_or_else(|| format!("node{}", joint.index()))
        })
        .collect::<Vec<_>>();
    let inverse_bind_matrices = inverse_bind_matrices(&skin, buffers, joint_node_indices.len())?;
    Ok(AnimationSkin {
        skin_index: skin.index(),
        name: skin.name().map(str::to_string),
        skeleton_root_index: skin.skeleton().map(|node| node.index()),
        joint_node_indices,
        joint_names,
        inverse_bind_matrices,
    })
}

fn import_clip(
    animation: ::gltf::Animation<'_>,
    buffers: &[::gltf::buffer::Data],
    source_relative_path: &str,
    node_count: usize,
) -> Result<AnimationClip, EngineError> {
    let clip_index = animation.index();
    let name = animation
        .name()
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("animation_{clip_index}"));
    let mut channels = Vec::new();
    let mut seen_targets = BTreeSet::new();
    let mut duration_secs = 0.0_f32;
    let mut frame_count = 0_u32;
    for channel in animation.channels() {
        let property = import_property(channel.target().property())?;
        let target_node_index = channel.target().node().index();
        if target_node_index >= node_count {
            return invalid_animation(
                "clip.channel.target",
                format!("{source_relative_path} animation '{name}' targets missing node"),
            );
        }
        if !seen_targets.insert((target_node_index, property)) {
            return invalid_animation(
                "clip.channel.target",
                format!("{source_relative_path} animation '{name}' has duplicate target channel"),
            );
        }
        let interpolation = import_interpolation(channel.sampler().interpolation())?;
        let reader = channel.reader(|buffer| Some(&buffers[buffer.index()]));
        let times = read_times(reader.read_inputs(), source_relative_path, &name)?;
        duration_secs = duration_secs.max(*times.last().unwrap());
        frame_count = frame_count.max(times.len() as u32);
        let values = read_values(reader.read_outputs(), property, source_relative_path, &name)?;
        validate_sample_count(&times, &values, source_relative_path, &name)?;
        channels.push(AnimationChannel {
            target_node_index,
            property,
            interpolation,
            times,
            values,
        });
    }
    if channels.is_empty() {
        return invalid_animation(
            "clip.channels",
            format!("{source_relative_path} animation '{name}' has no channels"),
        );
    }
    Ok(AnimationClip {
        clip_index,
        name,
        duration_secs,
        duration_ms: (duration_secs * 1000.0).round() as u32,
        frame_count,
        channels,
    })
}

fn node_transform(node: ::gltf::Node<'_>) -> Result<Transform, EngineError> {
    let (translation, rotation, scale) = node.transform().decomposed();
    let translation = Vec3::from_array(translation);
    let rotation = normalize_quat(Quat::from_xyzw(
        rotation[0],
        rotation[1],
        rotation[2],
        rotation[3],
    ))?;
    let scale = Vec3::from_array(scale);
    if !translation.is_finite() || !scale.is_finite() {
        return invalid_animation(
            "node.transform",
            "node transform contains non-finite values",
        );
    }
    Ok(Transform::from_trs(translation, rotation, scale))
}

fn inverse_bind_matrices(
    skin: &::gltf::Skin<'_>,
    buffers: &[::gltf::buffer::Data],
    joint_count: usize,
) -> Result<Vec<Mat4>, EngineError> {
    let Some(read) = skin
        .reader(|buffer| Some(&buffers[buffer.index()]))
        .read_inverse_bind_matrices()
    else {
        return Ok(vec![Mat4::IDENTITY; joint_count]);
    };
    let matrices = read
        .map(|matrix| {
            let matrix = Mat4::from_cols_array_2d(&matrix);
            validate_mat4(matrix, "skin.inverse_bind_matrices")?;
            Ok(matrix)
        })
        .collect::<Result<Vec<_>, EngineError>>()?;
    if matrices.len() != joint_count {
        return invalid_animation(
            "skin.inverse_bind_matrices",
            format!(
                "skin {} inverse bind matrix count {} != joint count {}",
                skin.index(),
                matrices.len(),
                joint_count
            ),
        );
    }
    Ok(matrices)
}

fn import_property(
    property: ::gltf::animation::Property,
) -> Result<AnimationProperty, EngineError> {
    match property {
        ::gltf::animation::Property::Translation => Ok(AnimationProperty::Translation),
        ::gltf::animation::Property::Rotation => Ok(AnimationProperty::Rotation),
        ::gltf::animation::Property::Scale => Ok(AnimationProperty::Scale),
        ::gltf::animation::Property::MorphTargetWeights => invalid_animation(
            "clip.channel.property",
            "morph target animation is not part of skeletal skinning; see #1106",
        ),
    }
}

fn import_interpolation(
    interpolation: ::gltf::animation::Interpolation,
) -> Result<AnimationInterpolation, EngineError> {
    match interpolation {
        ::gltf::animation::Interpolation::Linear => Ok(AnimationInterpolation::Linear),
        ::gltf::animation::Interpolation::Step => Ok(AnimationInterpolation::Step),
        ::gltf::animation::Interpolation::CubicSpline => invalid_animation(
            "clip.channel.interpolation",
            "cubic spline interpolation is not implemented for skeletal sampling",
        ),
    }
}

fn read_times(
    inputs: Option<impl Iterator<Item = f32>>,
    source_relative_path: &str,
    clip_name: &str,
) -> Result<Vec<f32>, EngineError> {
    let mut previous = None;
    let mut times = Vec::new();
    for time in inputs.ok_or_else(|| EngineError::AnimationInvalidConfig {
        field: "animation.clip.inputs".to_string(),
        reason: format!("{source_relative_path} animation '{clip_name}' has no time input"),
    })? {
        if !time.is_finite() || time < 0.0 {
            return invalid_animation(
                "clip.time",
                format!("{source_relative_path} animation '{clip_name}' has invalid time"),
            );
        }
        if let Some(previous_time) = previous
            && time <= previous_time
        {
            return invalid_animation(
                "clip.time",
                format!("{source_relative_path} animation '{clip_name}' time input is not sorted"),
            );
        }
        previous = Some(time);
        times.push(time);
    }
    if times.is_empty() {
        return invalid_animation(
            "clip.time",
            format!("{source_relative_path} animation '{clip_name}' has no keyframes"),
        );
    }
    Ok(times)
}

fn read_values(
    outputs: Option<::gltf::animation::util::ReadOutputs<'_>>,
    property: AnimationProperty,
    source_relative_path: &str,
    clip_name: &str,
) -> Result<AnimationChannelValues, EngineError> {
    let outputs = outputs.ok_or_else(|| EngineError::AnimationInvalidConfig {
        field: "animation.clip.outputs".to_string(),
        reason: format!("{source_relative_path} animation '{clip_name}' has no output samples"),
    })?;
    match (property, outputs) {
        (
            AnimationProperty::Translation,
            ::gltf::animation::util::ReadOutputs::Translations(read),
        ) => collect_vec3(read, "translation").map(AnimationChannelValues::Translations),
        (AnimationProperty::Rotation, ::gltf::animation::util::ReadOutputs::Rotations(read)) => {
            let mut values = Vec::new();
            for rotation in read.into_f32() {
                values.push(normalize_quat(Quat::from_xyzw(
                    rotation[0],
                    rotation[1],
                    rotation[2],
                    rotation[3],
                ))?);
            }
            Ok(AnimationChannelValues::Rotations(values))
        }
        (AnimationProperty::Scale, ::gltf::animation::util::ReadOutputs::Scales(read)) => {
            collect_vec3(read, "scale").map(AnimationChannelValues::Scales)
        }
        _ => invalid_animation(
            "clip.outputs",
            format!("{source_relative_path} animation '{clip_name}' output type mismatches target"),
        ),
    }
}

fn collect_vec3(
    read: impl Iterator<Item = [f32; 3]>,
    field: &str,
) -> Result<Vec<Vec3>, EngineError> {
    let mut values = Vec::new();
    for value in read {
        let value = Vec3::from_array(value);
        if !value.is_finite() {
            return invalid_animation("clip.outputs", format!("{field} sample is invalid"));
        }
        values.push(value);
    }
    Ok(values)
}

fn validate_sample_count(
    times: &[f32],
    values: &AnimationChannelValues,
    source_relative_path: &str,
    clip_name: &str,
) -> Result<(), EngineError> {
    let value_count = match values {
        AnimationChannelValues::Translations(values) => values.len(),
        AnimationChannelValues::Rotations(values) => values.len(),
        AnimationChannelValues::Scales(values) => values.len(),
    };
    if times.len() != value_count {
        return invalid_animation(
            "clip.outputs",
            format!(
                "{source_relative_path} animation '{clip_name}' sample count {value_count} != input count {}",
                times.len()
            ),
        );
    }
    Ok(())
}

fn normalize_quat(rotation: Quat) -> Result<Quat, EngineError> {
    if rotation.to_array().iter().any(|value| !value.is_finite())
        || rotation.length() <= f32::EPSILON
    {
        return invalid_animation("rotation", "rotation quaternion is invalid");
    }
    Ok(rotation.normalize())
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
