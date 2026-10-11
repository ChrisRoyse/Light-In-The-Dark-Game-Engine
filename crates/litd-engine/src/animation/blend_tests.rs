use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    animation::{
        AnimationBlendLayer, AnimationBlendPoseRequest, AnimationBlendSpace1d,
        AnimationBlendSpace1dRequest, AnimationBlendSpace2d, AnimationBlendSpace2dRequest,
        AnimationBlendSpacePoint1d, AnimationBlendSpacePoint2d, AnimationGraphState,
        AnimationMontage, AnimationMontageRuntime, AnimationRootMotionRequest,
        AnimationStateMachine, AnimationStateMachineInputs, AnimationStateMachineRuntime,
        AnimationStateTransition, AnimationTransitionCondition, advance_animation_montage,
        advance_animation_state_machine, blend_skeletal_animation_poses,
        evaluate_animation_blend_space_1d, evaluate_animation_blend_space_2d, extract_root_motion,
        load_gltf_skeletal_animation_asset,
    },
    math::Vec2,
};

const STAN_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";

#[test]
fn blends_real_stan_locomotion_and_blend_spaces() {
    let asset = load_stan();
    let blend = blend_skeletal_animation_poses(
        &asset,
        AnimationBlendPoseRequest {
            skin_index: 0,
            layers: vec![
                AnimationBlendLayer {
                    clip_name: "Idle".to_string(),
                    playback_time_ms: 100,
                    weight: 1.0,
                },
                AnimationBlendLayer {
                    clip_name: "Run".to_string(),
                    playback_time_ms: 100,
                    weight: 3.0,
                },
            ],
        },
    )
    .unwrap();

    assert_eq!(blend.schema, "litd.animation_blend_pose.v1");
    assert_eq!(blend.layer_count, 2);
    assert_eq!(blend.pose.joint_matrices.len(), 43);
    assert!((blend.normalized_layers[0].weight - 0.25).abs() < 0.001);
    assert!((blend.normalized_layers[1].weight - 0.75).abs() < 0.001);

    let space_1d = AnimationBlendSpace1d {
        name: "locomotion_speed".to_string(),
        parameter_name: "speed".to_string(),
        points: vec![
            AnimationBlendSpacePoint1d {
                position: 0.0,
                clip_name: "Idle".to_string(),
            },
            AnimationBlendSpacePoint1d {
                position: 0.5,
                clip_name: "Walk".to_string(),
            },
            AnimationBlendSpacePoint1d {
                position: 1.0,
                clip_name: "Run".to_string(),
            },
        ],
    };
    let speed = evaluate_animation_blend_space_1d(
        &asset,
        &space_1d,
        AnimationBlendSpace1dRequest {
            skin_index: 0,
            parameter_value: 0.75,
            playback_time_ms: 100,
        },
    )
    .unwrap();
    assert_eq!(speed.layer_count, 2);
    assert_eq!(speed.normalized_layers[0].clip_name, "Walk");
    assert_eq!(speed.normalized_layers[1].clip_name, "Run");
    assert!((speed.normalized_layers[0].weight - 0.5).abs() < 0.001);
    assert!((speed.normalized_layers[1].weight - 0.5).abs() < 0.001);

    let space_2d = AnimationBlendSpace2d {
        name: "move_aim".to_string(),
        parameter_x: "speed".to_string(),
        parameter_y: "aim".to_string(),
        points: vec![
            AnimationBlendSpacePoint2d {
                position: Vec2::new(0.0, 0.0),
                clip_name: "Idle".to_string(),
            },
            AnimationBlendSpacePoint2d {
                position: Vec2::new(1.0, 0.0),
                clip_name: "Run".to_string(),
            },
            AnimationBlendSpacePoint2d {
                position: Vec2::new(0.0, 1.0),
                clip_name: "Shoot".to_string(),
            },
        ],
    };
    let exact = evaluate_animation_blend_space_2d(
        &asset,
        &space_2d,
        AnimationBlendSpace2dRequest {
            skin_index: 0,
            parameter_value: Vec2::new(1.0, 0.0),
            playback_time_ms: 100,
        },
    )
    .unwrap();
    assert_eq!(exact.layer_count, 1);
    assert_eq!(exact.normalized_layers[0].clip_name, "Run");
    println!(
        "issue1107_blend source={} pose_clip={} joints={} speed_layers={} exact_2d={}",
        asset.source_relative_path,
        blend.pose.clip_name,
        blend.pose.joint_matrices.len(),
        speed.layer_count,
        exact.normalized_layers[0].clip_name
    );
}

#[test]
fn state_machine_transition_and_montage_progress_on_real_asset() {
    let asset = load_stan();
    let machine = locomotion_machine();
    let mut floats = BTreeMap::new();
    floats.insert("speed".to_string(), 1.0);
    let inputs = AnimationStateMachineInputs {
        floats,
        ..AnimationStateMachineInputs::default()
    };
    let runtime = AnimationStateMachineRuntime {
        active_state: "Idle".to_string(),
        state_time_ms: 0,
        transition: None,
    };

    let first =
        advance_animation_state_machine(&asset, &machine, &runtime, &inputs, 100, 0).unwrap();
    assert_eq!(first.active_state, "Run");
    assert_eq!(first.previous_state.as_deref(), Some("Idle"));
    assert!(first.runtime.transition.is_some());
    assert!((first.transition_alpha - 0.5).abs() < 0.001);

    let second =
        advance_animation_state_machine(&asset, &machine, &first.runtime, &inputs, 100, 0).unwrap();
    assert_eq!(second.active_state, "Run");
    assert!(second.runtime.transition.is_none());
    assert!((second.transition_alpha - 1.0).abs() < 0.001);

    let montage = AnimationMontage {
        name: "quick_slash".to_string(),
        clip_name: "SwordSlash".to_string(),
        start_time_ms: 0,
        end_time_ms: 600,
        blend_in_ms: 100,
        blend_out_ms: 100,
    };
    let montage_first = advance_animation_montage(
        &asset,
        &montage,
        &AnimationMontageRuntime {
            elapsed_time_ms: 0,
            finished: false,
        },
        50,
        0,
    )
    .unwrap();
    assert_eq!(montage_first.playback_time_ms, 50);
    assert!((montage_first.weight - 0.5).abs() < 0.001);
    let montage_done =
        advance_animation_montage(&asset, &montage, &montage_first.runtime, 600, 0).unwrap();
    assert!(montage_done.runtime.finished);
    assert_eq!(montage_done.weight, 0.0);
    println!(
        "issue1107_state_montage state={} alpha={:.3} montage={} done={} joints={}",
        second.active_state,
        second.transition_alpha,
        montage_done.montage_name,
        montage_done.runtime.finished,
        second.pose.joint_matrices.len()
    );
}

#[test]
fn root_motion_extracts_explicit_skeleton_root_delta() {
    let asset = load_stan();
    let root_node_index = asset.skins[0].joint_node_indices[0];
    let delta = extract_root_motion(
        &asset,
        AnimationRootMotionRequest {
            clip_name: "Run".to_string(),
            skin_index: 0,
            root_node_index: Some(root_node_index),
            from_time_ms: 0,
            to_time_ms: 250,
        },
    )
    .unwrap();

    assert_eq!(delta.root_node_index, root_node_index);
    assert!(delta.translation_delta.is_finite());
    assert!(
        delta
            .rotation_delta
            .to_array()
            .iter()
            .all(|value| value.is_finite())
    );
    println!(
        "issue1107_root_motion clip={} root={} delta_len={:.6}",
        delta.clip_name,
        delta.root_node_index,
        delta.translation_delta.length()
    );
}

#[test]
fn invalid_blend_graph_inputs_fail_loud() {
    let asset = load_stan();
    let blend_error = blend_skeletal_animation_poses(
        &asset,
        AnimationBlendPoseRequest {
            skin_index: 0,
            layers: vec![AnimationBlendLayer {
                clip_name: "Idle".to_string(),
                playback_time_ms: 0,
                weight: -1.0,
            }],
        },
    )
    .unwrap_err()
    .to_string();
    assert!(blend_error.contains("animation.blend.layer.weight"));

    let machine = locomotion_machine();
    let missing_input = advance_animation_state_machine(
        &asset,
        &machine,
        &AnimationStateMachineRuntime {
            active_state: "Idle".to_string(),
            state_time_ms: 0,
            transition: None,
        },
        &AnimationStateMachineInputs::default(),
        100,
        0,
    )
    .unwrap_err()
    .to_string();
    assert!(missing_input.contains("animation.state_machine.inputs.floats"));
}

fn locomotion_machine() -> AnimationStateMachine {
    AnimationStateMachine {
        name: "locomotion".to_string(),
        initial_state: "Idle".to_string(),
        states: vec![
            AnimationGraphState {
                name: "Idle".to_string(),
                clip_name: "Idle".to_string(),
                looped: true,
            },
            AnimationGraphState {
                name: "Run".to_string(),
                clip_name: "Run".to_string(),
                looped: true,
            },
        ],
        transitions: vec![AnimationStateTransition {
            from_state: "Idle".to_string(),
            to_state: "Run".to_string(),
            condition: AnimationTransitionCondition::FloatGreaterEqual {
                name: "speed".to_string(),
                threshold: 0.5,
            },
            blend_duration_ms: 200,
        }],
    }
}

fn load_stan() -> crate::animation::SkeletalAnimationAsset {
    load_gltf_skeletal_animation_asset(workspace_path(STAN_GLTF), STAN_GLTF).unwrap()
}

// #1543: RUNTIME vault resolution; see `crate::asset_vault_root`.
fn workspace_path(relative_path: &str) -> PathBuf {
    crate::asset_vault_root::local_asset_vault_path(relative_path).unwrap()
}
