use std::{io, path::Path};

use litd_engine::{
    EngineError,
    animation::{
        AnimationDriveRequest, AnimationDrivenClipBinding, AnimationDrivenClipMap,
        AnimationDrivenPose, AnimationDrivenState, AnimationPresentationSignal,
        AnimationPresentationSignalKind, drive_animation_from_sim_order_state,
        load_gltf_skeletal_animation_asset,
    },
    fsv_artifacts::begin_issue_run,
    math::FixedVec3,
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
    sim_replay::SimSnapshot,
};
use serde_json::{Value, json};

const STAN_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue876_animation_state_drive")?;
    let asset_path = workspace_asset_path(STAN_GLTF);
    let asset = load_gltf_skeletal_animation_asset(&asset_path, STAN_GLTF)?;
    let asset_sha256 = file_sha256(&asset_path)?;
    let clip_map = clip_map();

    let mut sim = FixedTickSim::<2>::new(SimConfig::new(64)?, 876)?;
    let unit = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0))?;
    let target = sim.spawn_body(v(3, 0, 0), v(0, 0, 0), v(0, 0, 0))?;

    let idle_hash = sim.state_hash();
    let idle = drive_pose(
        &asset,
        &SimSnapshot::capture(&sim),
        &clip_map,
        request(unit, 100),
    )?;
    ensure(
        idle.selected_state == AnimationDrivenState::Idle,
        "idle state",
    )?;
    ensure(idle.presentation_only, "idle presentation_only")?;
    ensure(
        sim.state_hash() == idle_hash,
        "idle must not mutate sim hash",
    )?;

    sim.set_body_velocity(unit, v(64, 0, 0))?;
    let walk_hash = sim.state_hash();
    let walk = drive_pose(
        &asset,
        &SimSnapshot::capture(&sim),
        &clip_map,
        request(unit, 125),
    )?;
    ensure(
        walk.selected_state == AnimationDrivenState::Walk,
        "walk state",
    )?;
    ensure(walk.presentation_only, "walk presentation_only")?;
    ensure(
        sim.state_hash() == walk_hash,
        "walk must not mutate sim hash",
    )?;

    let attack_snapshot = SimSnapshot::capture(&sim);
    let attack_order = SimOrder::new(
        attack_snapshot.tick + 1,
        SimOrderSource::human(7),
        1,
        target,
        SimOrderKind::ApplyAttack {
            attacker: unit,
            damage_fp: 1,
        },
    );
    let mut attack_request = request(unit, 200);
    attack_request.pending_orders.push(attack_order);
    let attack = drive_pose(&asset, &attack_snapshot, &clip_map, attack_request)?;
    ensure(
        attack.selected_state == AnimationDrivenState::Attack,
        "attack state",
    )?;
    ensure(
        sim.state_hash() == walk_hash,
        "attack must not mutate sim hash",
    )?;

    let cast_snapshot = SimSnapshot::capture(&sim);
    let mut cast_request = request(unit, 150);
    cast_request.presentation_signals.push(signal(
        unit,
        AnimationPresentationSignalKind::Cast,
        0,
        10,
    ));
    let cast = drive_pose(&asset, &cast_snapshot, &clip_map, cast_request)?;
    ensure(
        cast.selected_state == AnimationDrivenState::Cast,
        "cast state",
    )?;
    ensure(
        sim.state_hash() == walk_hash,
        "cast must not mutate sim hash",
    )?;

    let death_snapshot = SimSnapshot::capture(&sim);
    let mut death_request = request(unit, 150);
    death_request.presentation_signals.push(signal(
        unit,
        AnimationPresentationSignalKind::Cast,
        0,
        10,
    ));
    death_request.presentation_signals.push(signal(
        unit,
        AnimationPresentationSignalKind::Death,
        0,
        10,
    ));
    let death = drive_pose(&asset, &death_snapshot, &clip_map, death_request)?;
    ensure(
        death.selected_state == AnimationDrivenState::Death,
        "death state",
    )?;
    ensure(death.pose.joint_matrices.len() == 43, "death joint count")?;
    ensure(
        sim.state_hash() == walk_hash,
        "death must not mutate sim hash",
    )?;

    sim.step()?;
    let edge_snapshot = SimSnapshot::capture(&sim);
    let edges = edge_readbacks(&asset, &edge_snapshot, &clip_map, unit);
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    ensure(all_edges_matched, "edge readbacks")?;

    let states = [idle, walk, attack, cast, death];
    let state_names = states
        .iter()
        .map(|pose| format!("{:?}", pose.selected_state))
        .collect::<Vec<_>>();
    let clip_names = states
        .iter()
        .map(|pose| pose.selected_clip_name.clone())
        .collect::<Vec<_>>();
    let presentation_only = states.iter().all(|pose| pose.presentation_only);
    let state_hashes_match = states
        .iter()
        .all(|pose| pose.source_state_hash == pose_for_hash(pose, idle_hash, walk_hash));
    let report = json!({
        "schema": "litd.engine.issue876.animation_state_drive_fsv.v1",
        "issue": 876,
        "source_of_truth": {
            "asset_path": asset_path.display().to_string(),
            "asset_sha256": asset_sha256,
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "engine_function": "litd_engine::animation::drive_animation_from_sim_order_state",
            "sim_source": "FixedTickSim + SimSnapshot readback",
            "manual_gate": "manual Synapse/browser readback must inspect this persisted JSON and latest manifest"
        },
        "expected": {
            "states": ["Idle", "Walk", "Attack", "Cast", "Death"],
            "clips": ["Idle", "Walk", "SwordSlash", "Shoot", "Death"],
            "joint_count": 43,
            "presentation_only": true,
            "sim_hash_unchanged_after_drive": true
        },
        "actual": {
            "states": state_names,
            "clips": clip_names,
            "presentation_only_all": presentation_only,
            "state_hashes_match_snapshots": state_hashes_match,
            "idle_hash": format!("{idle_hash:016X}"),
            "walk_hash": format!("{walk_hash:016X}"),
            "final_sim_hash": format!("{:016X}", sim.state_hash()),
            "death_joint_count": states[4].pose.joint_matrices.len(),
            "selection_reasons": states.iter().map(|pose| pose.selection_reason.clone()).collect::<Vec<_>>()
        },
        "acceptance": {
            "states_match": state_names == ["Idle", "Walk", "Attack", "Cast", "Death"],
            "clips_match": clip_names == ["Idle", "Walk", "SwordSlash", "Shoot", "Death"],
            "presentation_only_all": presentation_only,
            "state_hashes_match_snapshots": state_hashes_match,
            "death_joint_count": states[4].pose.joint_matrices.len(),
            "all_edges_matched": all_edges_matched,
            "edge_count": edges.len()
        },
        "edges": edges,
        "sufficiency_argument": "The minimal known-I/O set exercises every #876 state transition class: idle from no movement/order/signal, walk from sim velocity, attack from a pending sim order, cast from a presentation signal, and death precedence over cast. The edge set covers invalid clip-map data, invalid signal interval, stale pending order, and missing sim body. Additional units would duplicate the same deterministic selection rules."
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE876_ANIMATION_STATE_DRIVE_FSV report={} sha256={} manifest={} run_id={} states={} clips={} edges={} asset_sha256={}",
        readback.path.display(),
        readback.sha256,
        readback.latest_manifest_path.display(),
        run.run_id(),
        state_names.join("|"),
        clip_names.join("|"),
        edges.len(),
        asset_sha256
    );
    Ok(())
}

fn drive_pose(
    asset: &litd_engine::animation::SkeletalAnimationAsset,
    snapshot: &SimSnapshot,
    clip_map: &AnimationDrivenClipMap,
    request: AnimationDriveRequest,
) -> Result<AnimationDrivenPose, EngineError> {
    drive_animation_from_sim_order_state(asset, snapshot, clip_map, request)
}

fn edge_readbacks(
    asset: &litd_engine::animation::SkeletalAnimationAsset,
    snapshot: &SimSnapshot,
    clip_map: &AnimationDrivenClipMap,
    unit: SimBodyId,
) -> Vec<Value> {
    let mut bad_map = clip_map.clone();
    bad_map.cast.clip_name.clear();

    let mut bad_signal = request(unit, 0);
    bad_signal
        .presentation_signals
        .push(signal(unit, AnimationPresentationSignalKind::Cast, 3, 2));

    let mut stale_order = request(unit, 0);
    stale_order.pending_orders.push(SimOrder::new(
        0,
        SimOrderSource::human(1),
        1,
        unit,
        SimOrderKind::SetVelocity(v(1, 0, 0)),
    ));

    vec![
        expect_animation_error(
            "invalid_clip_map_rejected",
            drive_pose(asset, snapshot, &bad_map, request(unit, 0)),
            "animation.drive.clip_map.cast.clip_name",
        ),
        expect_animation_error(
            "invalid_signal_ticks_rejected",
            drive_pose(asset, snapshot, clip_map, bad_signal),
            "animation.drive.signal.tick",
        ),
        expect_animation_error(
            "stale_pending_order_rejected",
            drive_pose(asset, snapshot, clip_map, stale_order),
            "animation.drive.pending_orders",
        ),
        expect_missing_body(
            "missing_sim_body_rejected",
            drive_pose(
                asset,
                snapshot,
                clip_map,
                request(SimBodyId::from_raw(99), 0),
            ),
        ),
    ]
}

fn expect_animation_error<T>(label: &str, result: Result<T, EngineError>, field: &str) -> Value {
    match result {
        Err(EngineError::AnimationInvalidConfig {
            field: actual,
            reason,
        }) => json!({
            "label": label,
            "matched": actual == field,
            "field": actual,
            "reason": reason,
        }),
        Err(error) => json!({
            "label": label,
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": label,
            "matched": false,
            "unexpected": "operation succeeded",
        }),
    }
}

fn expect_missing_body<T>(label: &str, result: Result<T, EngineError>) -> Value {
    match result {
        Err(EngineError::SimBodyMissing { id }) => json!({
            "label": label,
            "matched": id == 99,
            "id": id,
        }),
        Err(error) => json!({
            "label": label,
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": label,
            "matched": false,
            "unexpected": "operation succeeded",
        }),
    }
}

fn request(body_id: SimBodyId, playback_time_ms: u32) -> AnimationDriveRequest {
    AnimationDriveRequest {
        body_id,
        skin_index: 0,
        playback_time_ms,
        pending_orders: Vec::new(),
        presentation_signals: Vec::new(),
    }
}

fn signal(
    body_id: SimBodyId,
    kind: AnimationPresentationSignalKind,
    start_tick: u64,
    end_tick: u64,
) -> AnimationPresentationSignal {
    AnimationPresentationSignal {
        body_id,
        kind,
        start_tick,
        end_tick,
    }
}

fn clip_map() -> AnimationDrivenClipMap {
    AnimationDrivenClipMap {
        idle: binding("Idle", true),
        walk: binding("Walk", true),
        attack: binding("SwordSlash", false),
        cast: binding("Shoot", false),
        death: binding("Death", false),
        movement_threshold_raw: 0,
    }
}

fn binding(clip_name: &str, looped: bool) -> AnimationDrivenClipBinding {
    AnimationDrivenClipBinding {
        clip_name: clip_name.to_string(),
        looped,
    }
}

fn pose_for_hash(pose: &AnimationDrivenPose, idle_hash: u64, walk_hash: u64) -> u64 {
    match pose.selected_state {
        AnimationDrivenState::Idle => idle_hash,
        AnimationDrivenState::Walk
        | AnimationDrivenState::Attack
        | AnimationDrivenState::Cast
        | AnimationDrivenState::Death => walk_hash,
    }
}

// #1543: RUNTIME vault resolution; see `litd_engine::asset_vault_root`.
fn workspace_asset_path(relative_path: &str) -> std::path::PathBuf {
    litd_engine::asset_vault_root::local_asset_vault_path(relative_path).unwrap()
}

fn file_sha256(path: &Path) -> io::Result<String> {
    Ok(litd_hash::sha256_hex(&std::fs::read(path)?))
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

fn ensure(condition: bool, message: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}
