use crate::{EngineError, animation::invalid_animation};

use super::{
    blend_support::{clip_duration_ms, validate_name},
    blend_types::{AnimationMontage, AnimationMontageRuntime, AnimationMontageStep},
    sample::sample_skeletal_animation,
    types::{AnimationSampleRequest, SkeletalAnimationAsset},
};

pub fn advance_animation_montage(
    asset: &SkeletalAnimationAsset,
    montage: &AnimationMontage,
    runtime: &AnimationMontageRuntime,
    delta_ms: u32,
    skin_index: usize,
) -> Result<AnimationMontageStep, EngineError> {
    validate_montage(asset, montage)?;
    if runtime.finished {
        return invalid_animation(
            "montage.runtime",
            "finished montage cannot be advanced again",
        );
    }
    let span_ms = montage.end_time_ms - montage.start_time_ms;
    let elapsed = runtime
        .elapsed_time_ms
        .checked_add(delta_ms)
        .ok_or_else(|| EngineError::AnimationInvalidConfig {
            field: "animation.montage.elapsed_time_ms".to_string(),
            reason: "montage elapsed time overflowed".to_string(),
        })?;
    let clamped_elapsed = elapsed.min(span_ms);
    let finished = elapsed >= span_ms;
    let playback_time_ms = montage.start_time_ms + clamped_elapsed;
    let weight = if finished {
        0.0
    } else {
        montage_weight(montage, clamped_elapsed)
    };
    let pose = sample_skeletal_animation(
        asset,
        AnimationSampleRequest {
            clip_name: montage.clip_name.clone(),
            skin_index,
            playback_time_ms,
        },
    )?;
    Ok(AnimationMontageStep {
        schema: "litd.animation_montage_step.v1".to_string(),
        montage_name: montage.name.clone(),
        clip_name: montage.clip_name.clone(),
        playback_time_ms,
        weight,
        runtime: AnimationMontageRuntime {
            elapsed_time_ms: clamped_elapsed,
            finished,
        },
        pose,
    })
}

fn validate_montage(
    asset: &SkeletalAnimationAsset,
    montage: &AnimationMontage,
) -> Result<(), EngineError> {
    validate_name("montage.name", &montage.name)?;
    validate_name("montage.clip_name", &montage.clip_name)?;
    let duration_ms = clip_duration_ms(asset, &montage.clip_name)?;
    if montage.start_time_ms >= montage.end_time_ms || montage.end_time_ms > duration_ms {
        return invalid_animation(
            "montage.time",
            "montage time range must fit inside the clip",
        );
    }
    let span_ms = montage.end_time_ms - montage.start_time_ms;
    if montage.blend_in_ms.saturating_add(montage.blend_out_ms) > span_ms {
        return invalid_animation("montage.blend", "blend windows cannot exceed montage span");
    }
    Ok(())
}

fn montage_weight(montage: &AnimationMontage, elapsed_ms: u32) -> f32 {
    let span_ms = montage.end_time_ms - montage.start_time_ms;
    let mut weight = 1.0_f32;
    if montage.blend_in_ms > 0 {
        weight = weight.min(elapsed_ms as f32 / montage.blend_in_ms as f32);
    }
    if montage.blend_out_ms > 0 {
        weight = weight.min((span_ms - elapsed_ms) as f32 / montage.blend_out_ms as f32);
    }
    weight.clamp(0.0, 1.0)
}
