//! Unit tests for the #844-#847 runtime presentation/perception systems.

use std::collections::BTreeSet;

use crate::cinematic_timeline::{
    CameraKeyframe, CinematicCameraPose, CinematicFade, CinematicTimeline, FadeKind,
};
use crate::math::{Fixed, FixedVec3, Vec3};

use super::camera::{CameraMode, CameraOp, CameraPose, CameraState};
use super::cinematic_player::{CinematicPlayer, PlayState, SimGating};
use super::effects::{
    EffectArtRegistry, EffectAttach, EffectRuntime, EffectSpawn, lower_effect_spawn,
};
use super::fog::{
    FogState, ModifierKind, NoSharedVision, SharedVisionTable, VisibilityModifier, VisibilityState,
    VisionSource, world_to_cell,
};

// ---- #847 fog ----------------------------------------------------------------

fn source(owner: u32, x: i32, y: i32, r: u32) -> VisionSource {
    VisionSource {
        owner,
        cell_x: x,
        cell_y: y,
        sight_radius_cells: r,
    }
}

#[test]
fn fog_reveals_disc_and_demotes_to_explored() {
    let mut fog = FogState::new(8, 8, &[1]).unwrap();
    fog.recompute(&[source(1, 4, 4, 2)], &[], &NoSharedVision);
    let (_, _, visible) = fog.counts(1).unwrap();
    assert!(visible > 0);
    assert!(fog.is_visible(1, 4, 4));
    // Move the unit far away: previously visible cells demote to Explored.
    fog.recompute(&[source(1, 0, 0, 1)], &[], &NoSharedVision);
    assert!(!fog.is_visible(1, 4, 4));
    assert_eq!(fog.grid(1).unwrap().state(4, 4), VisibilityState::Explored);
    assert_eq!(fog.grid(1).unwrap().state(0, 0), VisibilityState::Visible);
}

#[test]
fn fog_recompute_is_deterministic() {
    let sources = [source(1, 2, 2, 3), source(1, 5, 5, 1)];
    let mut a = FogState::new(8, 8, &[1]).unwrap();
    let mut b = FogState::new(8, 8, &[1]).unwrap();
    a.recompute(&sources, &[], &NoSharedVision);
    b.recompute(&sources, &[], &NoSharedVision);
    assert_eq!(a.state_hash(), b.state_hash());
    // Source order must not matter (disc union is commutative).
    let reversed = [sources[1], sources[0]];
    let mut c = FogState::new(8, 8, &[1]).unwrap();
    c.recompute(&reversed, &[], &NoSharedVision);
    assert_eq!(a.state_hash(), c.state_hash());
}

#[test]
fn fog_reveal_and_conceal_modifiers() {
    let mut fog = FogState::new(6, 6, &[1]).unwrap();
    let reveal = VisibilityModifier {
        player: 1,
        kind: ModifierKind::Reveal,
        min_x: 0,
        min_y: 0,
        max_x: 1,
        max_y: 1,
    };
    fog.recompute(&[], &[reveal], &NoSharedVision);
    assert!(fog.is_visible(1, 0, 0));
    // Conceal wins over a reveal on the same cell.
    let conceal = VisibilityModifier {
        player: 1,
        kind: ModifierKind::Conceal,
        min_x: 0,
        min_y: 0,
        max_x: 0,
        max_y: 0,
    };
    fog.recompute(&[], &[reveal, conceal], &NoSharedVision);
    assert_eq!(fog.grid(1).unwrap().state(0, 0), VisibilityState::Masked);
    assert!(fog.is_visible(1, 1, 1));
}

#[test]
fn fog_shared_vision_via_alliance() {
    let mut table = SharedVisionTable::new();
    table.ally(1, 2);
    let mut fog = FogState::new(6, 6, &[1, 2, 3]).unwrap();
    fog.recompute(&[source(2, 3, 3, 1)], &[], &table);
    // Player 1 is allied to 2 -> sees it; player 3 is not -> does not.
    assert!(fog.is_visible(1, 3, 3));
    assert!(fog.is_visible(2, 3, 3));
    assert!(!fog.is_visible(3, 3, 3));
}

#[test]
fn world_to_cell_floors_toward_negative() {
    let size = Fixed::from_i32(4).raw();
    assert_eq!(world_to_cell(Fixed::from_i32(9).raw(), 0, size).unwrap(), 2);
    assert_eq!(
        world_to_cell(Fixed::from_i32(-1).raw(), 0, size).unwrap(),
        -1
    );
    assert!(world_to_cell(0, 0, 0).is_err());
}

// ---- #846 camera -------------------------------------------------------------

#[test]
fn camera_default_is_top_down() {
    let cam = CameraState::default();
    assert_eq!(cam.mode, CameraMode::TopDown);
    assert_eq!(cam.pose.pitch_degrees, 70.0);
}

#[test]
fn camera_lock_makes_manual_ops_inert() {
    let mut cam = CameraState::top_down_rts();
    cam.apply(&CameraOp::Lock(true));
    assert!(!cam.apply(&CameraOp::Pan([1.0, 0.0, 0.0])));
    assert_eq!(cam.pose.target, [0.0, 0.0, 0.0]);
    // Scripted Apply still wins while locked.
    let scripted = CameraPose {
        target: [5.0, 0.0, 5.0],
        distance: 10.0,
        yaw_degrees: 0.0,
        pitch_degrees: 45.0,
        fov_degrees: 60.0,
    };
    assert!(cam.apply(&CameraOp::Apply(scripted)));
    assert_eq!(cam.pose.target, [5.0, 0.0, 5.0]);
    cam.apply(&CameraOp::Lock(false));
    assert!(cam.apply(&CameraOp::Pan([1.0, 0.0, 0.0])));
}

#[test]
fn camera_first_person_collapses_distance() {
    let mut cam = CameraState::top_down_rts();
    cam.apply(&CameraOp::SetMode(CameraMode::FirstPerson));
    assert_eq!(cam.mode, CameraMode::FirstPerson);
    assert_eq!(cam.pose.distance, 0.0);
}

#[test]
fn camera_adopts_cinematic_pose() {
    let mut cam = CameraState::top_down_rts();
    let pose = CinematicCameraPose {
        target: Vec3::new(1.0, 2.0, 3.0),
        distance: 12.0,
        yaw_degrees: 30.0,
        pitch_degrees: 40.0,
        fov_degrees: 55.0,
    };
    cam.apply_cinematic_pose(&pose);
    assert_eq!(cam.pose.target, [1.0, 2.0, 3.0]);
    assert_eq!(cam.pose.distance, 12.0);
}

// ---- #845 cinematic player ---------------------------------------------------

fn sample_timeline() -> CinematicTimeline {
    CinematicTimeline {
        timeline_id: "test.intro".to_string(),
        duration_seconds: 10.0,
        camera_track: vec![
            CameraKeyframe {
                time_seconds: 0.0,
                camera_ref: "cam_a".to_string(),
                pose: CinematicCameraPose {
                    target: Vec3::ZERO,
                    distance: 20.0,
                    yaw_degrees: 0.0,
                    pitch_degrees: 60.0,
                    fov_degrees: 60.0,
                },
            },
            CameraKeyframe {
                time_seconds: 10.0,
                camera_ref: "cam_b".to_string(),
                pose: CinematicCameraPose {
                    target: Vec3::new(10.0, 0.0, 0.0),
                    distance: 20.0,
                    yaw_degrees: 90.0,
                    pitch_degrees: 60.0,
                    fov_degrees: 60.0,
                },
            },
        ],
        transmissions: Vec::new(),
        fades: vec![CinematicFade {
            start_seconds: 0.0,
            duration_seconds: 2.0,
            kind: FadeKind::In,
            color: [0.0, 0.0, 0.0, 1.0],
        }],
        subtitles: Vec::new(),
        cues: Vec::new(),
    }
}

#[test]
fn cinematic_player_pauses_sim_then_finishes() {
    let mut player = CinematicPlayer::start(sample_timeline(), SimGating::Pause).unwrap();
    assert!(player.letterbox_active());
    assert!(!player.should_advance_sim());
    player.advance(5.0);
    assert_eq!(player.state(), PlayState::Playing);
    let mid = player.sample().unwrap();
    assert!(mid.fade.alpha.abs() < f32::EPSILON); // fade-in done by t=5
    player.advance(6.0);
    assert_eq!(player.state(), PlayState::Finished);
    assert!(!player.letterbox_active());
    assert!(player.should_advance_sim());
}

#[test]
fn cinematic_player_skip_finishes_immediately() {
    let mut player = CinematicPlayer::start(sample_timeline(), SimGating::Pause).unwrap();
    player.skip();
    assert!(player.was_skipped());
    assert_eq!(player.state(), PlayState::Finished);
    assert!(player.should_advance_sim());
}

#[test]
fn cinematic_player_rejects_sabotaged_timeline() {
    let mut bad = sample_timeline();
    bad.duration_seconds = -1.0;
    assert!(CinematicPlayer::start(bad, SimGating::Pause).is_err());
}

#[test]
fn cinematic_player_run_underneath_keeps_sim_running() {
    let player = CinematicPlayer::start(sample_timeline(), SimGating::RunUnderneath).unwrap();
    assert!(player.is_active());
    assert!(player.should_advance_sim());
}

// ---- #844 effects ------------------------------------------------------------

fn registry() -> EffectArtRegistry {
    EffectArtRegistry::new(&["fx.blood", "fx.flame"])
}

#[test]
fn effect_lowering_rejects_dangling_ref() {
    let spawn = EffectSpawn {
        effect_ref: "fx.missing".to_string(),
        attach: EffectAttach::Point(FixedVec3::ZERO),
        lifetime_ticks: Some(5),
        destroy_on_owner_death: false,
    };
    let err = lower_effect_spawn(&spawn, &registry()).unwrap_err();
    assert_eq!(err.code(), "EFFECT_ART_REF_DANGLING");
}

#[test]
fn effect_dies_with_owner() {
    let mut runtime = EffectRuntime::new();
    let spawn = EffectSpawn {
        effect_ref: "fx.flame".to_string(),
        attach: EffectAttach::Unit {
            owner: 7,
            attach_point: "hand_left".to_string(),
        },
        lifetime_ticks: None,
        destroy_on_owner_death: true,
    };
    let lowered = lower_effect_spawn(&spawn, &registry()).unwrap();
    let id = runtime.spawn(&lowered, 0).expect("spawn effect");
    let mut alive: BTreeSet<u32> = BTreeSet::new();
    alive.insert(7);
    assert!(runtime.tick(1, &alive).is_empty());
    assert!(runtime.contains(id));
    // Owner dies -> effect destroyed.
    alive.remove(&7);
    let destroyed = runtime.tick(2, &alive);
    assert_eq!(destroyed, vec![id]);
    assert_eq!(runtime.active_count(), 0);
}

#[test]
fn effect_expires_at_lifetime_deadline() {
    let mut runtime = EffectRuntime::new();
    let spawn = EffectSpawn {
        effect_ref: "fx.blood".to_string(),
        attach: EffectAttach::Point(FixedVec3::from_i32s(1, 0, 2)),
        lifetime_ticks: Some(3),
        destroy_on_owner_death: false,
    };
    let lowered = lower_effect_spawn(&spawn, &registry()).unwrap();
    runtime.spawn(&lowered, 10).expect("spawn effect");
    let alive = BTreeSet::new();
    assert!(runtime.tick(12, &alive).is_empty());
    assert_eq!(runtime.tick(13, &alive).len(), 1);
}

#[test]
fn effect_id_exhaustion_rejects_without_collision_or_mutation() {
    let mut runtime = EffectRuntime::new();
    let lowered = lower_effect_spawn(
        &EffectSpawn {
            effect_ref: "fx.blood".to_string(),
            attach: EffectAttach::Point(FixedVec3::ZERO),
            lifetime_ticks: None,
            destroy_on_owner_death: false,
        },
        &registry(),
    )
    .unwrap();
    runtime.set_next_id_for_test(u64::from(u32::MAX));
    assert_eq!(runtime.spawn(&lowered, 0).unwrap(), u32::MAX);
    let before = runtime.instances().to_vec();
    let error = runtime
        .spawn(&lowered, 0)
        .expect_err("exhausted id space must reject");
    assert_eq!(error.code(), "EFFECT_ID_EXHAUSTED");
    assert_eq!(runtime.instances(), before);
    assert_eq!(runtime.active_ids(), vec![u32::MAX]);
}

#[test]
fn effect_deadline_overflow_rejects_before_id_or_instance_mutation() {
    let mut runtime = EffectRuntime::new();
    let overflowing = lower_effect_spawn(
        &EffectSpawn {
            effect_ref: "fx.flame".to_string(),
            attach: EffectAttach::Point(FixedVec3::ZERO),
            lifetime_ticks: Some(2),
            destroy_on_owner_death: false,
        },
        &registry(),
    )
    .unwrap();
    let error = runtime
        .spawn(&overflowing, u64::MAX)
        .expect_err("deadline overflow must reject");
    assert_eq!(error.code(), "EFFECT_DEADLINE_OVERFLOW");
    assert!(runtime.instances().is_empty());

    let valid = lower_effect_spawn(
        &EffectSpawn {
            effect_ref: "fx.flame".to_string(),
            attach: EffectAttach::Point(FixedVec3::ZERO),
            lifetime_ticks: None,
            destroy_on_owner_death: false,
        },
        &registry(),
    )
    .unwrap();
    assert_eq!(runtime.spawn(&valid, u64::MAX).unwrap(), 0);
}
