use litd_engine::EngineError;

use super::{
    GltfAnimationImport,
    support::{invalid_import, validate_vec3_iter},
};

pub(super) fn import_animation(
    animation: gltf::Animation<'_>,
    buffers: &[gltf::buffer::Data],
    source_relative_path: &str,
) -> Result<GltfAnimationImport, EngineError> {
    let clip_index = animation.index();
    let name = animation
        .name()
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("animation_{clip_index}"));
    let mut duration_secs = 0.0_f32;
    let mut targets = Vec::new();
    let mut node_transform_channel_count = 0;
    let mut morph_weight_channel_count = 0;
    let mut output_sample_count = 0;
    let channel_count = animation.channels().count();
    if channel_count == 0 {
        return invalid_import(
            "animation.channels",
            format!("{source_relative_path} animation '{name}' has no channels"),
        );
    }
    for channel in animation.channels() {
        let reader = channel.reader(|buffer| Some(&buffers[buffer.index()]));
        let inputs = reader
            .read_inputs()
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.gltf_import.animation.inputs".to_string(),
                reason: format!("{source_relative_path} animation '{name}' has no time input"),
            })?;
        let mut previous_time = None;
        let mut input_count = 0;
        for time in inputs {
            if !time.is_finite() || time < 0.0 {
                return invalid_import(
                    "animation.time",
                    format!("{source_relative_path} animation '{name}' has invalid time"),
                );
            }
            if let Some(previous) = previous_time
                && time < previous
            {
                return invalid_import(
                    "animation.time",
                    format!("{source_relative_path} animation '{name}' time input is not sorted"),
                );
            }
            previous_time = Some(time);
            duration_secs = duration_secs.max(time);
            input_count += 1;
        }
        if input_count == 0 {
            return invalid_import(
                "animation.time",
                format!("{source_relative_path} animation '{name}' has no keyframes"),
            );
        }
        let property = channel.target().property();
        match property {
            gltf::animation::Property::MorphTargetWeights => morph_weight_channel_count += 1,
            _ => node_transform_channel_count += 1,
        }
        let outputs = reader
            .read_outputs()
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.gltf_import.animation.outputs".to_string(),
                reason: format!("{source_relative_path} animation '{name}' has no output samples"),
            })?;
        output_sample_count += validate_animation_outputs(outputs, source_relative_path, &name)?;
        targets.push(format!(
            "node{}:{:?}:{:?}",
            channel.target().node().index(),
            property,
            channel.sampler().interpolation()
        ));
    }
    Ok(GltfAnimationImport {
        clip_index,
        name,
        duration_ms: (duration_secs * 1000.0).round() as u32,
        channel_count,
        sampler_count: animation.samplers().count(),
        node_transform_channel_count,
        morph_weight_channel_count,
        output_sample_count,
        target_summary: targets.join("|"),
    })
}

fn validate_animation_outputs(
    outputs: gltf::animation::util::ReadOutputs<'_>,
    source_relative_path: &str,
    name: &str,
) -> Result<usize, EngineError> {
    let count = match outputs {
        gltf::animation::util::ReadOutputs::Translations(read) => {
            validate_vec3_iter(read, "animation.outputs", source_relative_path)?
        }
        gltf::animation::util::ReadOutputs::Scales(read) => {
            validate_vec3_iter(read, "animation.outputs", source_relative_path)?
        }
        gltf::animation::util::ReadOutputs::Rotations(read) => {
            let mut count = 0;
            for rotation in read.into_f32() {
                if rotation.iter().any(|value| !value.is_finite())
                    || rotation.iter().all(|value| value.abs() <= f32::EPSILON)
                {
                    return invalid_import(
                        "animation.outputs",
                        format!("{source_relative_path} animation '{name}' has invalid rotation"),
                    );
                }
                count += 1;
            }
            count
        }
        gltf::animation::util::ReadOutputs::MorphTargetWeights(read) => {
            let mut count = 0;
            for weight in read.into_f32() {
                if !weight.is_finite() {
                    return invalid_import(
                        "animation.outputs",
                        format!(
                            "{source_relative_path} animation '{name}' has invalid morph weight"
                        ),
                    );
                }
                count += 1;
            }
            count
        }
    };
    if count == 0 {
        return invalid_import(
            "animation.outputs",
            format!("{source_relative_path} animation '{name}' has empty outputs"),
        );
    }
    Ok(count)
}
