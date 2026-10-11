use std::{collections::BTreeMap, path::Path};

use crate::{
    EngineError,
    animation::{
        AnimationInterpolation, MorphMesh, MorphPrimitiveSummary, MorphSampleRequest,
        MorphTargetAnimationAsset, MorphWeightChannel, MorphWeightClip, MorphWeightSample,
        invalid_animation,
    },
};

pub fn load_gltf_morph_target_animation_asset(
    source_path: impl AsRef<Path>,
    source_relative_path: impl Into<String>,
) -> Result<MorphTargetAnimationAsset, EngineError> {
    let source_path = source_path.as_ref();
    let source_relative_path = source_relative_path.into();
    validate_source(source_path)?;
    let (document, buffers, _images) =
        ::gltf::import(source_path).map_err(|error| EngineError::AnimationInvalidConfig {
            field: "animation.morph.gltf.import".to_string(),
            reason: format!("{} failed morph import: {error}", source_path.display()),
        })?;
    let node_mesh_indices = node_mesh_indices(&document);
    let meshes = import_morph_meshes(&document, &buffers)?;
    if meshes.is_empty() {
        return invalid_animation("morph.meshes", "asset contains no morph target meshes");
    }
    let mesh_targets = meshes
        .iter()
        .map(|mesh| (mesh.mesh_index, mesh.target_count))
        .collect::<BTreeMap<_, _>>();
    let clips = document
        .animations()
        .map(|clip| import_morph_clip(clip, &buffers, &node_mesh_indices, &mesh_targets))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if clips.is_empty() {
        return invalid_animation(
            "morph.clips",
            "asset contains morph targets but no morph weight animation channels",
        );
    }
    Ok(MorphTargetAnimationAsset {
        schema: "litd.morph_target_animation_asset.v1".to_string(),
        source_relative_path,
        mesh_count: meshes.len(),
        node_count: node_mesh_indices.len(),
        clip_count: clips.len(),
        meshes,
        clips,
    })
}

pub fn sample_morph_target_weights(
    asset: &MorphTargetAnimationAsset,
    request: MorphSampleRequest,
) -> Result<MorphWeightSample, EngineError> {
    let clip = asset
        .clips
        .iter()
        .find(|clip| clip.name == request.clip_name)
        .ok_or_else(|| EngineError::AnimationInvalidConfig {
            field: "animation.morph.clip_name".to_string(),
            reason: format!(
                "{} does not contain morph clip '{}'",
                asset.source_relative_path, request.clip_name
            ),
        })?;
    if request.playback_time_ms > clip.duration_ms {
        return invalid_animation(
            "morph.playback_time_ms",
            format!(
                "{}ms exceeds clip '{}' duration {}ms",
                request.playback_time_ms, clip.name, clip.duration_ms
            ),
        );
    }
    let channel = clip
        .channels
        .iter()
        .find(|channel| channel.target_node_index == request.target_node_index)
        .ok_or_else(|| EngineError::AnimationInvalidConfig {
            field: "animation.morph.target_node_index".to_string(),
            reason: format!(
                "clip '{}' has no morph channel for node {}",
                clip.name, request.target_node_index
            ),
        })?;
    let weights = sample_weights(
        &channel.times,
        &channel.weights,
        channel.interpolation,
        request.playback_time_ms as f32 / 1000.0,
    )?;
    Ok(MorphWeightSample {
        source_relative_path: asset.source_relative_path.clone(),
        clip_name: clip.name.clone(),
        target_node_index: request.target_node_index,
        mesh_index: channel.mesh_index,
        playback_time_ms: request.playback_time_ms,
        weights,
    })
}

fn validate_source(source_path: &Path) -> Result<(), EngineError> {
    let extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(extension.as_deref(), Some("gltf" | "glb")) {
        return invalid_animation(
            "morph.source.extension",
            "expected .gltf or .glb morph source",
        );
    }
    Ok(())
}

fn node_mesh_indices(document: &::gltf::Document) -> Vec<Option<usize>> {
    let mut nodes = vec![None; document.nodes().count()];
    for node in document.nodes() {
        nodes[node.index()] = node.mesh().map(|mesh| mesh.index());
    }
    nodes
}

fn import_morph_meshes(
    document: &::gltf::Document,
    buffers: &[::gltf::buffer::Data],
) -> Result<Vec<MorphMesh>, EngineError> {
    let mut meshes = Vec::new();
    for mesh in document.meshes() {
        let primitives = mesh
            .primitives()
            .map(|primitive| import_morph_primitive(primitive, buffers))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|primitive| primitive.target_count > 0)
            .collect::<Vec<_>>();
        let target_count = primitives
            .iter()
            .map(|primitive| primitive.target_count)
            .max()
            .unwrap_or(0);
        if target_count == 0 {
            continue;
        }
        let default_weights = default_weights(&mesh, target_count)?;
        meshes.push(MorphMesh {
            mesh_index: mesh.index(),
            name: mesh.name().map(str::to_string),
            target_count,
            default_weights,
            primitives,
        });
    }
    Ok(meshes)
}

fn import_morph_primitive(
    primitive: ::gltf::Primitive<'_>,
    buffers: &[::gltf::buffer::Data],
) -> Result<MorphPrimitiveSummary, EngineError> {
    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
    let vertex_count = reader
        .read_positions()
        .ok_or_else(|| EngineError::AnimationInvalidConfig {
            field: "animation.morph.primitive.positions".to_string(),
            reason: format!("primitive {} has morphs but no POSITION", primitive.index()),
        })?
        .count();
    let mut target_count = 0;
    let mut position_delta_count = 0;
    let mut normal_delta_count = 0;
    let mut tangent_delta_count = 0;
    for (positions, normals, tangents) in reader.read_morph_targets() {
        target_count += 1;
        if let Some(read) = positions {
            position_delta_count += validate_vec3_count(read, vertex_count, "position")?;
        }
        if let Some(read) = normals {
            normal_delta_count += validate_vec3_count(read, vertex_count, "normal")?;
        }
        if let Some(read) = tangents {
            tangent_delta_count += validate_vec3_count(read, vertex_count, "tangent")?;
        }
    }
    Ok(MorphPrimitiveSummary {
        primitive_index: primitive.index(),
        vertex_count,
        target_count,
        position_delta_count,
        normal_delta_count,
        tangent_delta_count,
    })
}

fn default_weights(mesh: &::gltf::Mesh<'_>, target_count: usize) -> Result<Vec<f32>, EngineError> {
    let weights = mesh
        .weights()
        .map(|weights| weights.to_vec())
        .unwrap_or_else(|| vec![0.0; target_count]);
    if weights.len() != target_count || weights.iter().any(|weight| !weight.is_finite()) {
        return invalid_animation(
            "morph.mesh.weights",
            format!(
                "mesh {} default morph weights must be finite and match target count {}",
                mesh.index(),
                target_count
            ),
        );
    }
    Ok(weights)
}

fn import_morph_clip(
    animation: ::gltf::Animation<'_>,
    buffers: &[::gltf::buffer::Data],
    node_mesh_indices: &[Option<usize>],
    mesh_targets: &BTreeMap<usize, usize>,
) -> Result<Option<MorphWeightClip>, EngineError> {
    let clip_index = animation.index();
    let name = animation
        .name()
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("animation_{clip_index}"));
    let mut channels = Vec::new();
    let mut non_morph_channel_count = 0;
    let mut duration_secs = 0.0_f32;
    let mut frame_count = 0_u32;
    for channel in animation.channels() {
        if channel.target().property() != ::gltf::animation::Property::MorphTargetWeights {
            non_morph_channel_count += 1;
            continue;
        }
        let target_node_index = channel.target().node().index();
        let mesh_index = node_mesh_indices
            .get(target_node_index)
            .and_then(|mesh| *mesh)
            .ok_or_else(|| EngineError::AnimationInvalidConfig {
                field: "animation.morph.channel.target".to_string(),
                reason: format!("animation '{name}' targets node without a mesh"),
            })?;
        let target_count =
            *mesh_targets
                .get(&mesh_index)
                .ok_or_else(|| EngineError::AnimationInvalidConfig {
                    field: "animation.morph.channel.mesh".to_string(),
                    reason: format!("animation '{name}' targets mesh {mesh_index} without morphs"),
                })?;
        let interpolation = import_interpolation(channel.sampler().interpolation())?;
        let reader = channel.reader(|buffer| Some(&buffers[buffer.index()]));
        let times = read_times(reader.read_inputs(), &name)?;
        duration_secs = duration_secs.max(*times.last().unwrap());
        frame_count = frame_count.max(times.len() as u32);
        let weights = read_weight_frames(reader.read_outputs(), target_count, times.len(), &name)?;
        channels.push(MorphWeightChannel {
            target_node_index,
            mesh_index,
            target_count,
            interpolation,
            times,
            weights,
        });
    }
    if channels.is_empty() {
        return Ok(None);
    }
    Ok(Some(MorphWeightClip {
        clip_index,
        name,
        duration_secs,
        duration_ms: (duration_secs * 1000.0).round() as u32,
        frame_count,
        non_morph_channel_count,
        channels,
    }))
}

fn import_interpolation(
    interpolation: ::gltf::animation::Interpolation,
) -> Result<AnimationInterpolation, EngineError> {
    match interpolation {
        ::gltf::animation::Interpolation::Linear => Ok(AnimationInterpolation::Linear),
        ::gltf::animation::Interpolation::Step => Ok(AnimationInterpolation::Step),
        ::gltf::animation::Interpolation::CubicSpline => invalid_animation(
            "morph.channel.interpolation",
            "cubic spline interpolation is not implemented for morph weights",
        ),
    }
}

fn read_times(
    inputs: Option<impl Iterator<Item = f32>>,
    clip_name: &str,
) -> Result<Vec<f32>, EngineError> {
    let mut previous = None;
    let mut times = Vec::new();
    for time in inputs.ok_or_else(|| EngineError::AnimationInvalidConfig {
        field: "animation.morph.inputs".to_string(),
        reason: format!("animation '{clip_name}' has no morph time input"),
    })? {
        if !time.is_finite() || time < 0.0 {
            return invalid_animation("morph.time", "morph keyframe time is invalid");
        }
        if let Some(previous_time) = previous
            && time <= previous_time
        {
            return invalid_animation("morph.time", "morph keyframe times are not sorted");
        }
        previous = Some(time);
        times.push(time);
    }
    if times.is_empty() {
        return invalid_animation("morph.time", "morph channel has no keyframes");
    }
    Ok(times)
}

fn read_weight_frames(
    outputs: Option<::gltf::animation::util::ReadOutputs<'_>>,
    target_count: usize,
    frame_count: usize,
    clip_name: &str,
) -> Result<Vec<Vec<f32>>, EngineError> {
    let outputs = outputs.ok_or_else(|| EngineError::AnimationInvalidConfig {
        field: "animation.morph.outputs".to_string(),
        reason: format!("animation '{clip_name}' has no morph weight outputs"),
    })?;
    let ::gltf::animation::util::ReadOutputs::MorphTargetWeights(read) = outputs else {
        return invalid_animation("morph.outputs", "morph channel output type mismatch");
    };
    let values = read.into_f32().collect::<Vec<_>>();
    if values.len() != target_count * frame_count {
        return invalid_animation(
            "morph.outputs",
            format!(
                "morph output count {} != target_count {} * frame_count {}",
                values.len(),
                target_count,
                frame_count
            ),
        );
    }
    if values.iter().any(|weight| !weight.is_finite()) {
        return invalid_animation("morph.outputs", "morph output weight is not finite");
    }
    Ok(values
        .chunks(target_count)
        .map(|weights| weights.to_vec())
        .collect())
}

fn sample_weights(
    times: &[f32],
    frames: &[Vec<f32>],
    interpolation: AnimationInterpolation,
    time_secs: f32,
) -> Result<Vec<f32>, EngineError> {
    let window = sample_window(times, frames.len(), time_secs)?;
    let values = match interpolation {
        AnimationInterpolation::Step => frames[window.lower].clone(),
        AnimationInterpolation::Linear => frames[window.lower]
            .iter()
            .zip(&frames[window.upper])
            .map(|(left, right)| left + (right - left) * window.alpha)
            .collect::<Vec<_>>(),
    };
    if values.iter().any(|weight| !weight.is_finite()) {
        return invalid_animation("morph.sample", "sampled morph weight is not finite");
    }
    Ok(values)
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
        return invalid_animation("morph.sample", "sample table is empty or mismatched");
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
                return invalid_animation("morph.sample", "sample times are not increasing");
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

fn validate_vec3_count(
    read: impl Iterator<Item = [f32; 3]>,
    expected: usize,
    field: &str,
) -> Result<usize, EngineError> {
    let mut count = 0;
    for value in read {
        if value.iter().any(|component| !component.is_finite()) {
            return invalid_animation("morph.primitive", format!("{field} delta is invalid"));
        }
        count += 1;
    }
    if count != expected {
        return invalid_animation(
            "morph.primitive",
            format!("{field} morph delta count {count} != vertex count {expected}"),
        );
    }
    Ok(count)
}
