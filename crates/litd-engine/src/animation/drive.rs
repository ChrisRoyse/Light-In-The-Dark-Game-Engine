use crate::{
    EngineError, animation::invalid_animation, math::FixedVec3, sim::SimBodyState,
    sim_order::SimOrderKind, sim_replay::SimSnapshot,
};

use super::{
    blend_support::{clip_duration_ms, validate_name},
    drive_types::{
        AnimationDriveRequest, AnimationDrivenClipBinding, AnimationDrivenClipMap,
        AnimationDrivenPose, AnimationDrivenState, AnimationPresentationSignalKind,
    },
    sample::sample_skeletal_animation,
    types::{AnimationSampleRequest, SkeletalAnimationAsset},
};

pub fn drive_animation_from_sim_order_state(
    asset: &SkeletalAnimationAsset,
    snapshot: &SimSnapshot,
    clip_map: &AnimationDrivenClipMap,
    request: AnimationDriveRequest,
) -> Result<AnimationDrivenPose, EngineError> {
    validate_clip_map(clip_map)?;
    validate_request(snapshot, &request)?;
    let body = snapshot
        .bodies
        .iter()
        .find(|body| body.id == request.body_id)
        .ok_or(EngineError::SimBodyMissing {
            id: request.body_id.index(),
        })?;
    let (selected_state, selection_reason) = select_state(snapshot, body, clip_map, &request);
    let binding = binding_for_state(clip_map, selected_state);
    let playback_time_ms = resolve_playback_time(asset, binding, request.playback_time_ms)?;
    let pose = sample_skeletal_animation(
        asset,
        AnimationSampleRequest {
            clip_name: binding.clip_name.clone(),
            skin_index: request.skin_index,
            playback_time_ms,
        },
    )?;
    Ok(AnimationDrivenPose {
        schema: "litd.animation_driven_pose.v1".to_string(),
        presentation_only: true,
        body_id: request.body_id,
        source_tick: snapshot.tick,
        source_state_hash: snapshot.state_hash,
        selected_state,
        selected_clip_name: binding.clip_name.clone(),
        selection_reason,
        playback_time_ms,
        pose,
    })
}

fn select_state(
    snapshot: &SimSnapshot,
    body: &SimBodyState,
    clip_map: &AnimationDrivenClipMap,
    request: &AnimationDriveRequest,
) -> (AnimationDrivenState, String) {
    if has_active_signal(snapshot, request, AnimationPresentationSignalKind::Death) {
        return (
            AnimationDrivenState::Death,
            "active presentation death signal".to_string(),
        );
    }
    if has_active_signal(snapshot, request, AnimationPresentationSignalKind::Cast) {
        return (
            AnimationDrivenState::Cast,
            "active presentation cast signal".to_string(),
        );
    }
    if has_attack(snapshot, request) {
        return (
            AnimationDrivenState::Attack,
            "current attack event or pending attack order".to_string(),
        );
    }
    if velocity_exceeds_threshold(body.velocity, clip_map.movement_threshold_raw) {
        return (
            AnimationDrivenState::Walk,
            "body velocity exceeds movement threshold".to_string(),
        );
    }
    (
        AnimationDrivenState::Idle,
        "body has no active presentation signal, attack, or movement".to_string(),
    )
}

fn has_active_signal(
    snapshot: &SimSnapshot,
    request: &AnimationDriveRequest,
    kind: AnimationPresentationSignalKind,
) -> bool {
    request.presentation_signals.iter().any(|signal| {
        signal.body_id == request.body_id
            && signal.kind == kind
            && snapshot.tick >= signal.start_tick
            && snapshot.tick <= signal.end_tick
    })
}

fn has_attack(snapshot: &SimSnapshot, request: &AnimationDriveRequest) -> bool {
    snapshot
        .attack_events
        .iter()
        .any(|event| event.tick == snapshot.tick && event.attacker == request.body_id)
        || request.pending_orders.iter().any(|order| match order.kind {
            SimOrderKind::ApplyAttack { attacker, .. } => attacker == request.body_id,
            _ => false,
        })
}

fn velocity_exceeds_threshold(velocity: FixedVec3, threshold_raw: i64) -> bool {
    let speed_sq =
        square_raw(velocity.x.raw()) + square_raw(velocity.y.raw()) + square_raw(velocity.z.raw());
    speed_sq > square_raw(threshold_raw)
}

fn resolve_playback_time(
    asset: &SkeletalAnimationAsset,
    binding: &AnimationDrivenClipBinding,
    requested_ms: u32,
) -> Result<u32, EngineError> {
    let duration_ms = clip_duration_ms(asset, &binding.clip_name)?;
    if duration_ms == 0 {
        return invalid_animation("drive.clip.duration", "selected clip duration is zero");
    }
    if binding.looped {
        Ok(requested_ms % duration_ms)
    } else {
        Ok(requested_ms.min(duration_ms))
    }
}

fn validate_clip_map(clip_map: &AnimationDrivenClipMap) -> Result<(), EngineError> {
    for (field, binding) in [
        ("idle", &clip_map.idle),
        ("walk", &clip_map.walk),
        ("attack", &clip_map.attack),
        ("cast", &clip_map.cast),
        ("death", &clip_map.death),
    ] {
        validate_name(
            &format!("drive.clip_map.{field}.clip_name"),
            &binding.clip_name,
        )?;
    }
    if clip_map.movement_threshold_raw < 0 {
        return invalid_animation(
            "drive.movement_threshold_raw",
            "movement threshold must be non-negative",
        );
    }
    Ok(())
}

fn validate_request(
    snapshot: &SimSnapshot,
    request: &AnimationDriveRequest,
) -> Result<(), EngineError> {
    for signal in &request.presentation_signals {
        if signal.start_tick > signal.end_tick {
            return invalid_animation("drive.signal.tick", "signal start_tick must be <= end_tick");
        }
    }
    for order in &request.pending_orders {
        if order.execute_tick < snapshot.tick {
            return invalid_animation(
                "drive.pending_orders",
                "pending order execute_tick is older than the snapshot tick",
            );
        }
    }
    Ok(())
}

fn binding_for_state(
    clip_map: &AnimationDrivenClipMap,
    state: AnimationDrivenState,
) -> &AnimationDrivenClipBinding {
    match state {
        AnimationDrivenState::Idle => &clip_map.idle,
        AnimationDrivenState::Walk => &clip_map.walk,
        AnimationDrivenState::Attack => &clip_map.attack,
        AnimationDrivenState::Cast => &clip_map.cast,
        AnimationDrivenState::Death => &clip_map.death,
    }
}

fn square_raw(value: i64) -> i128 {
    let value = value as i128;
    value * value
}
