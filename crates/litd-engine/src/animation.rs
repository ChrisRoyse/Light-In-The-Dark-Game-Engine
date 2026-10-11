//! Skeletal animation loading, sampling, and GPU joint-palette upload support.

#[path = "animation/blend.rs"]
mod blend;
#[path = "animation/blend_support.rs"]
mod blend_support;
#[path = "animation/blend_types.rs"]
mod blend_types;
#[path = "animation/drive.rs"]
mod drive;
#[path = "animation/drive_types.rs"]
mod drive_types;
#[path = "animation/gltf_loader.rs"]
mod gltf_loader;
#[path = "animation/gpu.rs"]
mod gpu;
#[path = "animation/montage.rs"]
mod montage;
#[path = "animation/morph.rs"]
mod morph;
#[path = "animation/morph_gpu.rs"]
mod morph_gpu;
#[path = "animation/morph_types.rs"]
mod morph_types;
#[path = "animation/root_motion.rs"]
mod root_motion;
#[path = "animation/sample.rs"]
mod sample;
#[path = "animation/state_machine.rs"]
mod state_machine;
#[path = "animation/types.rs"]
mod types;

pub use blend::{
    blend_skeletal_animation_poses, evaluate_animation_blend_space_1d,
    evaluate_animation_blend_space_2d,
};
pub use blend_types::{
    AnimationBlendLayer, AnimationBlendPoseRequest, AnimationBlendPoseResult,
    AnimationBlendSpace1d, AnimationBlendSpace1dRequest, AnimationBlendSpace2d,
    AnimationBlendSpace2dRequest, AnimationBlendSpacePoint1d, AnimationBlendSpacePoint2d,
    AnimationGraphState, AnimationMontage, AnimationMontageRuntime, AnimationMontageStep,
    AnimationNormalizedBlendLayer, AnimationRootMotionDelta, AnimationRootMotionRequest,
    AnimationStateMachine, AnimationStateMachineInputs, AnimationStateMachineRuntime,
    AnimationStateMachineStep, AnimationStateTransition, AnimationTransitionCondition,
    AnimationTransitionRuntime,
};
pub use drive::drive_animation_from_sim_order_state;
pub use drive_types::{
    AnimationDriveRequest, AnimationDrivenClipBinding, AnimationDrivenClipMap, AnimationDrivenPose,
    AnimationDrivenState, AnimationPresentationSignal, AnimationPresentationSignalKind,
};
pub use gltf_loader::load_gltf_skeletal_animation_asset;
pub use gpu::{
    GpuJointMatrix, SKINNING_PALETTE_BIND_GROUP, SKINNING_PALETTE_BINDING, SKINNING_SHADER_ID,
    SkinningPaletteUpload, SkinningVertexInfluences, build_skinning_palette_upload,
    create_skinning_palette_buffer,
};
pub use montage::advance_animation_montage;
pub use morph::{load_gltf_morph_target_animation_asset, sample_morph_target_weights};
pub use morph_gpu::{
    MORPH_DELTA_BINDING, MORPH_SHADER_ID, MORPH_WEIGHT_BIND_GROUP, MORPH_WEIGHT_BINDING,
    MorphWeightUpload, build_morph_weight_upload, create_morph_weight_buffer,
};
pub use morph_types::{
    MorphMesh, MorphPrimitiveSummary, MorphSampleRequest, MorphTargetAnimationAsset,
    MorphWeightChannel, MorphWeightClip, MorphWeightSample,
};
pub use root_motion::extract_root_motion;
pub use sample::sample_skeletal_animation;
pub use state_machine::advance_animation_state_machine;
pub use types::{
    AnimationChannel, AnimationChannelValues, AnimationClip, AnimationInterpolation, AnimationNode,
    AnimationPose, AnimationProperty, AnimationSampleRequest, AnimationSkin, MAX_SKINNING_JOINTS,
    SkeletalAnimationAsset,
};

pub(crate) fn invalid_animation<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, crate::EngineError> {
    Err(crate::EngineError::AnimationInvalidConfig {
        field: format!("animation.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "animation/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "animation/morph_tests.rs"]
mod morph_tests;

#[cfg(test)]
#[path = "animation/blend_tests.rs"]
mod blend_tests;

#[cfg(test)]
#[path = "animation/drive_tests.rs"]
mod drive_tests;
