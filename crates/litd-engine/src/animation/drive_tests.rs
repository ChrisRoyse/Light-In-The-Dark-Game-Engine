use std::path::PathBuf;

use crate::{
    animation::{
        AnimationDriveRequest, AnimationDrivenClipBinding, AnimationDrivenClipMap,
        AnimationDrivenState, AnimationPresentationSignal, AnimationPresentationSignalKind,
        drive_animation_from_sim_order_state, load_gltf_skeletal_animation_asset,
    },
    math::FixedVec3,
    sim::{FixedTickSim, SimConfig},
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
    sim_replay::SimSnapshot,
};

const STAN_GLTF: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";

#[test]
fn sim_order_state_drives_idle_walk_attack_cast_death_without_hash_feedback() {
    let asset = load_stan();
    let mut sim = FixedTickSim::<2>::new(SimConfig::new(64).unwrap(), 1108).unwrap();
    let unit = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let target = sim.spawn_body(v(3, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    let clip_map = clip_map();

    let idle_hash = sim.state_hash();
    let idle = drive_animation_from_sim_order_state(
        &asset,
        &SimSnapshot::capture(&sim),
        &clip_map,
        request(unit, 100, vec![], vec![]),
    )
    .unwrap();
    assert_eq!(idle.selected_state, AnimationDrivenState::Idle);
    assert_eq!(idle.selected_clip_name, "Idle");
    assert!(idle.presentation_only);
    assert_eq!(idle.source_state_hash, idle_hash);
    assert_eq!(sim.state_hash(), idle_hash);

    sim.set_body_velocity(unit, v(64, 0, 0)).unwrap();
    let walk_hash = sim.state_hash();
    let walk = drive_animation_from_sim_order_state(
        &asset,
        &SimSnapshot::capture(&sim),
        &clip_map,
        request(unit, 125, vec![], vec![]),
    )
    .unwrap();
    assert_eq!(walk.selected_state, AnimationDrivenState::Walk);
    assert_eq!(walk.selected_clip_name, "Walk");
    assert_eq!(sim.state_hash(), walk_hash);

    let attack_order = SimOrder::new(
        SimSnapshot::capture(&sim).tick + 1,
        SimOrderSource::human(7),
        1,
        target,
        SimOrderKind::ApplyAttack {
            attacker: unit,
            damage_fp: 1,
        },
    );
    let attack = drive_animation_from_sim_order_state(
        &asset,
        &SimSnapshot::capture(&sim),
        &clip_map,
        request(unit, 200, vec![attack_order], vec![]),
    )
    .unwrap();
    assert_eq!(attack.selected_state, AnimationDrivenState::Attack);
    assert_eq!(attack.selected_clip_name, "SwordSlash");

    let cast = drive_animation_from_sim_order_state(
        &asset,
        &SimSnapshot::capture(&sim),
        &clip_map,
        request(
            unit,
            150,
            vec![],
            vec![signal(unit, AnimationPresentationSignalKind::Cast, 0, 10)],
        ),
    )
    .unwrap();
    assert_eq!(cast.selected_state, AnimationDrivenState::Cast);
    assert_eq!(cast.selected_clip_name, "Shoot");

    let death = drive_animation_from_sim_order_state(
        &asset,
        &SimSnapshot::capture(&sim),
        &clip_map,
        request(
            unit,
            150,
            vec![],
            vec![
                signal(unit, AnimationPresentationSignalKind::Cast, 0, 10),
                signal(unit, AnimationPresentationSignalKind::Death, 0, 10),
            ],
        ),
    )
    .unwrap();
    assert_eq!(death.selected_state, AnimationDrivenState::Death);
    assert_eq!(death.selected_clip_name, "Death");
    assert_eq!(death.pose.joint_matrices.len(), 43);
    assert_eq!(sim.state_hash(), walk_hash);
    println!(
        "issue1108_drive states={:?},{:?},{:?},{:?},{:?} source_hash={:016X} joints={}",
        idle.selected_state,
        walk.selected_state,
        attack.selected_state,
        cast.selected_state,
        death.selected_state,
        death.source_state_hash,
        death.pose.joint_matrices.len()
    );
}

#[test]
fn driver_fails_loud_for_invalid_clip_map_signal_and_order_state() {
    let asset = load_stan();
    let mut sim = FixedTickSim::<1>::new(SimConfig::new(64).unwrap(), 7).unwrap();
    let unit = sim.spawn_body(v(0, 0, 0), v(0, 0, 0), v(0, 0, 0)).unwrap();
    sim.step().unwrap();
    let snapshot = SimSnapshot::capture(&sim);

    let mut bad_map = clip_map();
    bad_map.cast.clip_name.clear();
    let bad_clip = drive_animation_from_sim_order_state(
        &asset,
        &snapshot,
        &bad_map,
        request(unit, 0, vec![], vec![]),
    )
    .unwrap_err()
    .to_string();
    assert!(bad_clip.contains("animation.drive.clip_map.cast.clip_name"));

    let bad_signal = drive_animation_from_sim_order_state(
        &asset,
        &snapshot,
        &clip_map(),
        request(
            unit,
            0,
            vec![],
            vec![signal(unit, AnimationPresentationSignalKind::Cast, 3, 2)],
        ),
    )
    .unwrap_err()
    .to_string();
    assert!(bad_signal.contains("animation.drive.signal.tick"));

    let stale_order = SimOrder::new(
        0,
        SimOrderSource::human(1),
        1,
        unit,
        SimOrderKind::SetVelocity(v(1, 0, 0)),
    );
    let stale = drive_animation_from_sim_order_state(
        &asset,
        &snapshot,
        &clip_map(),
        request(unit, 0, vec![stale_order], vec![]),
    )
    .unwrap_err()
    .to_string();
    assert!(stale.contains("animation.drive.pending_orders"));
}

fn request(
    body_id: crate::sim::SimBodyId,
    playback_time_ms: u32,
    pending_orders: Vec<SimOrder>,
    presentation_signals: Vec<AnimationPresentationSignal>,
) -> AnimationDriveRequest {
    AnimationDriveRequest {
        body_id,
        skin_index: 0,
        playback_time_ms,
        pending_orders,
        presentation_signals,
    }
}

fn signal(
    body_id: crate::sim::SimBodyId,
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

fn load_stan() -> crate::animation::SkeletalAnimationAsset {
    load_gltf_skeletal_animation_asset(workspace_path(STAN_GLTF), STAN_GLTF).unwrap()
}

fn v(x: i32, y: i32, z: i32) -> FixedVec3 {
    FixedVec3::from_i32s(x, y, z)
}

// #1543: RUNTIME vault resolution; see `crate::asset_vault_root`.
fn workspace_path(relative_path: &str) -> PathBuf {
    crate::asset_vault_root::local_asset_vault_path(relative_path).unwrap()
}
