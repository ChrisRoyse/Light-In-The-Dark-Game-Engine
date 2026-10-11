use std::collections::BTreeSet;

use crate::{EngineError, animation::invalid_animation};

use super::{
    blend::blend_skeletal_animation_poses,
    blend_support::{clip_duration_ms, validate_name},
    blend_types::{
        AnimationBlendLayer, AnimationBlendPoseRequest, AnimationGraphState, AnimationStateMachine,
        AnimationStateMachineInputs, AnimationStateMachineRuntime, AnimationStateMachineStep,
        AnimationStateTransition, AnimationTransitionCondition, AnimationTransitionRuntime,
    },
    sample::sample_skeletal_animation,
    types::{AnimationPose, AnimationSampleRequest, SkeletalAnimationAsset},
};

pub fn advance_animation_state_machine(
    asset: &SkeletalAnimationAsset,
    machine: &AnimationStateMachine,
    runtime: &AnimationStateMachineRuntime,
    inputs: &AnimationStateMachineInputs,
    delta_ms: u32,
    skin_index: usize,
) -> Result<AnimationStateMachineStep, EngineError> {
    validate_state_machine(machine)?;
    if let Some(transition) = &runtime.transition {
        return advance_active_transition(asset, machine, transition, delta_ms, skin_index);
    }
    let state = find_state(machine, &runtime.active_state)?;
    let state_time_ms = advance_state_time(asset, state, runtime.state_time_ms, delta_ms)?;
    if let Some(transition) = first_matching_transition(machine, state, inputs, state_time_ms)? {
        return start_transition(
            asset,
            machine,
            state,
            transition,
            state_time_ms,
            delta_ms,
            skin_index,
        );
    }
    let pose = sample_state_pose(asset, state, state_time_ms, skin_index)?;
    Ok(state_step(
        state,
        None,
        1.0,
        None,
        AnimationStateMachineRuntime {
            active_state: state.name.clone(),
            state_time_ms,
            transition: None,
        },
        pose,
    ))
}

fn start_transition(
    asset: &SkeletalAnimationAsset,
    machine: &AnimationStateMachine,
    from_state: &AnimationGraphState,
    transition: &AnimationStateTransition,
    from_time_ms: u32,
    delta_ms: u32,
    skin_index: usize,
) -> Result<AnimationStateMachineStep, EngineError> {
    let to_state = find_state(machine, &transition.to_state)?;
    let to_time_ms = advance_state_time(asset, to_state, 0, delta_ms)?;
    if transition.blend_duration_ms == 0 {
        let pose = sample_state_pose(asset, to_state, to_time_ms, skin_index)?;
        return Ok(state_step(
            to_state,
            Some(from_state.name.clone()),
            1.0,
            Some(transition.clone()),
            AnimationStateMachineRuntime {
                active_state: to_state.name.clone(),
                state_time_ms: to_time_ms,
                transition: None,
            },
            pose,
        ));
    }
    let elapsed_ms = delta_ms.min(transition.blend_duration_ms);
    let alpha = elapsed_ms as f32 / transition.blend_duration_ms as f32;
    if alpha >= 1.0 {
        let pose = sample_state_pose(asset, to_state, to_time_ms, skin_index)?;
        return Ok(state_step(
            to_state,
            Some(from_state.name.clone()),
            1.0,
            Some(transition.clone()),
            AnimationStateMachineRuntime {
                active_state: to_state.name.clone(),
                state_time_ms: to_time_ms,
                transition: None,
            },
            pose,
        ));
    }
    let transition_runtime = AnimationTransitionRuntime {
        from_state: from_state.name.clone(),
        to_state: to_state.name.clone(),
        elapsed_ms,
        duration_ms: transition.blend_duration_ms,
        from_state_time_ms: from_time_ms,
        to_state_time_ms: to_time_ms,
    };
    let pose = blend_state_poses(
        asset,
        from_state,
        from_time_ms,
        to_state,
        to_time_ms,
        alpha,
        skin_index,
    )?;
    Ok(state_step(
        to_state,
        Some(from_state.name.clone()),
        alpha,
        Some(transition.clone()),
        AnimationStateMachineRuntime {
            active_state: to_state.name.clone(),
            state_time_ms: to_time_ms,
            transition: Some(transition_runtime),
        },
        pose,
    ))
}

fn advance_active_transition(
    asset: &SkeletalAnimationAsset,
    machine: &AnimationStateMachine,
    runtime: &AnimationTransitionRuntime,
    delta_ms: u32,
    skin_index: usize,
) -> Result<AnimationStateMachineStep, EngineError> {
    let from_state = find_state(machine, &runtime.from_state)?;
    let to_state = find_state(machine, &runtime.to_state)?;
    let elapsed_ms = runtime.elapsed_ms.checked_add(delta_ms).ok_or_else(|| {
        EngineError::AnimationInvalidConfig {
            field: "animation.state_machine.transition.elapsed_ms".to_string(),
            reason: "transition elapsed time overflowed".to_string(),
        }
    })?;
    let from_time_ms = advance_state_time(asset, from_state, runtime.from_state_time_ms, delta_ms)?;
    let to_time_ms = advance_state_time(asset, to_state, runtime.to_state_time_ms, delta_ms)?;
    if elapsed_ms >= runtime.duration_ms {
        let pose = sample_state_pose(asset, to_state, to_time_ms, skin_index)?;
        return Ok(state_step(
            to_state,
            Some(from_state.name.clone()),
            1.0,
            None,
            AnimationStateMachineRuntime {
                active_state: to_state.name.clone(),
                state_time_ms: to_time_ms,
                transition: None,
            },
            pose,
        ));
    }
    let alpha = elapsed_ms as f32 / runtime.duration_ms as f32;
    let pose = blend_state_poses(
        asset,
        from_state,
        from_time_ms,
        to_state,
        to_time_ms,
        alpha,
        skin_index,
    )?;
    Ok(state_step(
        to_state,
        Some(from_state.name.clone()),
        alpha,
        None,
        AnimationStateMachineRuntime {
            active_state: to_state.name.clone(),
            state_time_ms: to_time_ms,
            transition: Some(AnimationTransitionRuntime {
                from_state: runtime.from_state.clone(),
                to_state: runtime.to_state.clone(),
                elapsed_ms,
                duration_ms: runtime.duration_ms,
                from_state_time_ms: from_time_ms,
                to_state_time_ms: to_time_ms,
            }),
        },
        pose,
    ))
}

fn blend_state_poses(
    asset: &SkeletalAnimationAsset,
    from_state: &AnimationGraphState,
    from_time_ms: u32,
    to_state: &AnimationGraphState,
    to_time_ms: u32,
    alpha: f32,
    skin_index: usize,
) -> Result<AnimationPose, EngineError> {
    let result = blend_skeletal_animation_poses(
        asset,
        AnimationBlendPoseRequest {
            skin_index,
            layers: vec![
                AnimationBlendLayer {
                    clip_name: from_state.clip_name.clone(),
                    playback_time_ms: from_time_ms,
                    weight: 1.0 - alpha,
                },
                AnimationBlendLayer {
                    clip_name: to_state.clip_name.clone(),
                    playback_time_ms: to_time_ms,
                    weight: alpha,
                },
            ],
        },
    )?;
    Ok(result.pose)
}

fn state_step(
    state: &AnimationGraphState,
    previous_state: Option<String>,
    transition_alpha: f32,
    triggered_transition: Option<AnimationStateTransition>,
    runtime: AnimationStateMachineRuntime,
    pose: AnimationPose,
) -> AnimationStateMachineStep {
    AnimationStateMachineStep {
        schema: "litd.animation_state_machine_step.v1".to_string(),
        active_state: state.name.clone(),
        previous_state,
        transition_alpha,
        triggered_transition,
        runtime,
        pose,
    }
}

fn first_matching_transition<'a>(
    machine: &'a AnimationStateMachine,
    state: &AnimationGraphState,
    inputs: &AnimationStateMachineInputs,
    state_time_ms: u32,
) -> Result<Option<&'a AnimationStateTransition>, EngineError> {
    for transition in machine
        .transitions
        .iter()
        .filter(|transition| transition.from_state == state.name)
    {
        if condition_matches(&transition.condition, inputs, state_time_ms)? {
            return Ok(Some(transition));
        }
    }
    Ok(None)
}

fn condition_matches(
    condition: &AnimationTransitionCondition,
    inputs: &AnimationStateMachineInputs,
    state_time_ms: u32,
) -> Result<bool, EngineError> {
    match condition {
        AnimationTransitionCondition::Trigger { name } => Ok(inputs.triggers.contains(name)),
        AnimationTransitionCondition::BoolEquals { name, value } => {
            let actual =
                inputs
                    .bools
                    .get(name)
                    .ok_or_else(|| EngineError::AnimationInvalidConfig {
                        field: "animation.state_machine.inputs.bools".to_string(),
                        reason: format!("missing bool input '{name}'"),
                    })?;
            Ok(actual == value)
        }
        AnimationTransitionCondition::FloatGreaterEqual { name, threshold } => {
            let actual =
                inputs
                    .floats
                    .get(name)
                    .ok_or_else(|| EngineError::AnimationInvalidConfig {
                        field: "animation.state_machine.inputs.floats".to_string(),
                        reason: format!("missing float input '{name}'"),
                    })?;
            Ok(*actual >= *threshold)
        }
        AnimationTransitionCondition::FloatLessThan { name, threshold } => {
            let actual =
                inputs
                    .floats
                    .get(name)
                    .ok_or_else(|| EngineError::AnimationInvalidConfig {
                        field: "animation.state_machine.inputs.floats".to_string(),
                        reason: format!("missing float input '{name}'"),
                    })?;
            Ok(*actual < *threshold)
        }
        AnimationTransitionCondition::ElapsedAtLeast { threshold_ms } => {
            Ok(state_time_ms >= *threshold_ms)
        }
    }
}

fn advance_state_time(
    asset: &SkeletalAnimationAsset,
    state: &AnimationGraphState,
    current_ms: u32,
    delta_ms: u32,
) -> Result<u32, EngineError> {
    let duration_ms = clip_duration_ms(asset, &state.clip_name)?;
    let next =
        current_ms
            .checked_add(delta_ms)
            .ok_or_else(|| EngineError::AnimationInvalidConfig {
                field: "animation.state_machine.state_time_ms".to_string(),
                reason: "state playback time overflowed".to_string(),
            })?;
    if state.looped {
        if duration_ms == 0 {
            return invalid_animation(
                "state_machine.state.duration",
                "looped clip duration is zero",
            );
        }
        Ok(next % duration_ms)
    } else {
        Ok(next.min(duration_ms))
    }
}

fn sample_state_pose(
    asset: &SkeletalAnimationAsset,
    state: &AnimationGraphState,
    playback_time_ms: u32,
    skin_index: usize,
) -> Result<AnimationPose, EngineError> {
    sample_skeletal_animation(
        asset,
        AnimationSampleRequest {
            clip_name: state.clip_name.clone(),
            skin_index,
            playback_time_ms,
        },
    )
}

fn validate_state_machine(machine: &AnimationStateMachine) -> Result<(), EngineError> {
    validate_name("state_machine.name", &machine.name)?;
    validate_name("state_machine.initial_state", &machine.initial_state)?;
    if machine.states.is_empty() {
        return invalid_animation(
            "state_machine.states",
            "state machine needs at least one state",
        );
    }
    let mut state_names = BTreeSet::new();
    for state in &machine.states {
        validate_name("state_machine.state.name", &state.name)?;
        validate_name("state_machine.state.clip_name", &state.clip_name)?;
        if !state_names.insert(state.name.clone()) {
            return invalid_animation("state_machine.state.name", "duplicate state name");
        }
    }
    if !state_names.contains(&machine.initial_state) {
        return invalid_animation(
            "state_machine.initial_state",
            "initial state is not defined",
        );
    }
    for transition in &machine.transitions {
        if !state_names.contains(&transition.from_state)
            || !state_names.contains(&transition.to_state)
        {
            return invalid_animation(
                "state_machine.transition",
                "transition references unknown state",
            );
        }
        validate_condition(&transition.condition)?;
    }
    Ok(())
}

fn validate_condition(condition: &AnimationTransitionCondition) -> Result<(), EngineError> {
    match condition {
        AnimationTransitionCondition::Trigger { name }
        | AnimationTransitionCondition::BoolEquals { name, .. }
        | AnimationTransitionCondition::FloatGreaterEqual { name, .. }
        | AnimationTransitionCondition::FloatLessThan { name, .. } => {
            validate_name("state_machine.condition.name", name)?;
        }
        AnimationTransitionCondition::ElapsedAtLeast { .. } => {}
    }
    match condition {
        AnimationTransitionCondition::FloatGreaterEqual { threshold, .. }
        | AnimationTransitionCondition::FloatLessThan { threshold, .. }
            if !threshold.is_finite() =>
        {
            invalid_animation(
                "state_machine.condition.threshold",
                "threshold must be finite",
            )
        }
        _ => Ok(()),
    }
}

fn find_state<'a>(
    machine: &'a AnimationStateMachine,
    state_name: &str,
) -> Result<&'a AnimationGraphState, EngineError> {
    machine
        .states
        .iter()
        .find(|state| state.name == state_name)
        .ok_or_else(|| EngineError::AnimationInvalidConfig {
            field: "animation.state_machine.active_state".to_string(),
            reason: format!("state '{state_name}' is not defined"),
        })
}
