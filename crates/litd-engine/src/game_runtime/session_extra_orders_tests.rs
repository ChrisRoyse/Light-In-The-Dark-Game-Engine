//! Unit coverage for #1857 fog, #1859 spawn, #1854 cinematic SimGating, and
//! #1861 weather + camera-follow session orders. Every test is mutation-
//! falsifiable: accepted orders move `combined_state_hash` (or presentation
//! read surfaces); rejected orders fail loud with a typed error.

use super::session_order_test_support::{FIELD_UNIT_P0, GRID, SPAWN_UNIT, effect_manifest};
use crate::game_runtime::camera::{CameraMode, CameraOp, CameraState};
use crate::game_runtime::fog::{ModifierKind, VisibilityState};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

fn session() -> MatchRuntimeSession {
    MatchRuntimeSession::from_manifest(&effect_manifest()).expect("build session")
}

#[test]
fn reveal_region_makes_cells_visible_and_hashes() {
    let mut s = session();
    let before = s.combined_state_hash();
    // Far corner, outside default unit vision.
    assert!(!s.fog().is_visible(0, (GRID as i32) - 1, (GRID as i32) - 1));
    s.apply(SessionOrder::RevealRegion {
        player: 0,
        min_x: (GRID as i32) - 2,
        min_y: (GRID as i32) - 2,
        max_x: (GRID as i32) - 1,
        max_y: (GRID as i32) - 1,
    })
    .expect("reveal");
    assert!(s.fog().is_visible(0, (GRID as i32) - 1, (GRID as i32) - 1));
    assert_ne!(before, s.combined_state_hash(), "reveal must fold");
    assert_eq!(s.fog_modifiers().len(), 1);
    assert_eq!(s.fog_modifiers()[0].kind, ModifierKind::Reveal);
}

#[test]
fn conceal_wins_over_reveal_on_same_cell() {
    let mut s = session();
    s.apply(SessionOrder::RevealRegion {
        player: 0,
        min_x: 0,
        min_y: 0,
        max_x: 2,
        max_y: 2,
    })
    .expect("reveal");
    s.apply(SessionOrder::ConcealRegion {
        player: 0,
        min_x: 1,
        min_y: 1,
        max_x: 1,
        max_y: 1,
    })
    .expect("conceal");
    assert_eq!(
        s.fog().grid(0).expect("grid").state(1, 1),
        VisibilityState::Masked
    );
    assert!(s.fog().is_visible(0, 0, 0));
}

#[test]
fn fog_modifier_rejects_bad_player_and_inverted_rect() {
    let mut s = session();
    let err = s
        .apply(SessionOrder::RevealRegion {
            player: 9,
            min_x: 0,
            min_y: 0,
            max_x: 1,
            max_y: 1,
        })
        .expect_err("bad player");
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_INVALID");
    let err = s
        .apply(SessionOrder::RevealRegion {
            player: 0,
            min_x: 3,
            min_y: 0,
            max_x: 1,
            max_y: 1,
        })
        .expect_err("inverted");
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_INVALID");
}

#[test]
fn reveal_is_deduped_and_replay_stable() {
    let mut s = session();
    let order = SessionOrder::RevealRegion {
        player: 0,
        min_x: 4,
        min_y: 4,
        max_x: 5,
        max_y: 5,
    };
    s.apply(order.clone()).expect("first");
    s.apply(order).expect("dup");
    assert_eq!(s.fog_modifiers().len(), 1, "dedupe");

    let stream = vec![
        SessionOrder::RevealRegion {
            player: 0,
            min_x: 1,
            min_y: 1,
            max_x: 2,
            max_y: 2,
        },
        SessionOrder::ConcealRegion {
            player: 0,
            min_x: 2,
            min_y: 2,
            max_x: 2,
            max_y: 2,
        },
        SessionOrder::Tick,
    ];
    let run = || {
        let mut a = session();
        for o in &stream {
            a.apply(o.clone()).expect("apply");
        }
        a.combined_state_hash()
    };
    assert_eq!(run(), run(), "replay must match");
}

#[test]
fn spawn_field_unit_validates_and_places() {
    let mut s = session();
    let before = s.combined_state_hash();
    assert!(s.field_unit(100).is_none());
    s.apply(SessionOrder::SpawnFieldUnit {
        unit_key: SPAWN_UNIT.to_string(),
        owner: 0,
        cell_x: 8,
        cell_z: 8,
    })
    .expect("spawn");
    // First spawn id is past blueprint field units (10, 11) -> 12.
    let view = s.field_unit(12).expect("spawned");
    assert_eq!(view.cell_x, 8);
    assert_eq!(view.cell_z, 8);
    assert_eq!(view.owner, 0);
    assert_ne!(before, s.combined_state_hash());
}

#[test]
fn spawn_rejects_empty_key_unknown_unit_and_oob_cell() {
    let mut s = session();
    let err = s
        .apply(SessionOrder::SpawnFieldUnit {
            unit_key: String::new(),
            owner: 0,
            cell_x: 1,
            cell_z: 1,
        })
        .expect_err("empty key");
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_INVALID");
    let err = s
        .apply(SessionOrder::SpawnFieldUnit {
            unit_key: "not-a-unit".to_string(),
            owner: 0,
            cell_x: 1,
            cell_z: 1,
        })
        .expect_err("unknown");
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_INVALID");
    let err = s
        .apply(SessionOrder::SpawnFieldUnit {
            unit_key: SPAWN_UNIT.to_string(),
            owner: 0,
            cell_x: GRID as i32,
            cell_z: 0,
        })
        .expect_err("oob");
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_INVALID");
}

#[test]
fn play_cinematic_gates_sim_and_skip_clears() {
    let mut s = session();
    assert!(s.should_advance_sim());
    let before = s.combined_state_hash();
    s.apply(SessionOrder::PlayCinematic {
        timeline_id: "m1_intro".to_string(),
        pauses_sim: true,
    })
    .expect("play");
    assert!(!s.should_advance_sim());
    assert_eq!(s.cinematic_timeline_id(), Some("m1_intro"));
    assert_ne!(before, s.combined_state_hash());
    s.apply(SessionOrder::SkipCinematic).expect("skip");
    assert!(s.should_advance_sim());
    assert_eq!(s.cinematic_timeline_id(), None);
    // Empty timeline fails loud.
    let err = s
        .apply(SessionOrder::PlayCinematic {
            timeline_id: String::new(),
            pauses_sim: true,
        })
        .expect_err("empty");
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_INVALID");
}

#[test]
fn play_cinematic_run_underneath_does_not_gate() {
    let mut s = session();
    s.apply(SessionOrder::PlayCinematic {
        timeline_id: "ambient".to_string(),
        pauses_sim: false,
    })
    .expect("play");
    assert!(s.should_advance_sim());
    assert_eq!(s.cinematic_timeline_id(), Some("ambient"));
}

#[test]
fn set_weather_updates_time_of_day_and_rejects_bad_intensity() {
    let mut s = session();
    let before = s.combined_state_hash();
    s.apply(SessionOrder::SetWeather {
        ordinal: 2,
        intensity_permille: 750,
    })
    .expect("weather");
    assert_eq!(s.time_of_day().weather_ordinal, 2);
    assert_eq!(s.time_of_day().weather_intensity_permille, 750);
    assert_ne!(before, s.combined_state_hash());
    let err = s
        .apply(SessionOrder::SetWeather {
            ordinal: 0,
            intensity_permille: 1001,
        })
        .expect_err("bad intensity");
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_INVALID");
}

#[test]
fn camera_follow_binds_and_rejects_missing_unit() {
    let mut s = session();
    s.apply(SessionOrder::CameraFollowUnit {
        unit: FIELD_UNIT_P0,
    })
    .expect("follow");
    assert_eq!(s.camera_follow_unit(), Some(FIELD_UNIT_P0));
    s.apply(SessionOrder::CameraClearFollow).expect("clear");
    assert_eq!(s.camera_follow_unit(), None);
    let err = s
        .apply(SessionOrder::CameraFollowUnit { unit: 999 })
        .expect_err("missing");
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_INVALID");

    // Presentation camera FollowUnit op binds mode + unit.
    let mut cam = CameraState::top_down_rts();
    assert!(cam.apply(&CameraOp::FollowUnit {
        unit: FIELD_UNIT_P0
    }));
    assert_eq!(cam.mode, CameraMode::Follow);
    assert_eq!(cam.follow_unit, Some(FIELD_UNIT_P0));
    cam.rebind_follow_target([10.0, 0.0, 20.0]);
    assert_eq!(cam.pose.target, [10.0, 0.0, 20.0]);
    assert!(cam.apply(&CameraOp::ClearFollow));
    assert_eq!(cam.mode, CameraMode::TopDown);
    assert_eq!(cam.follow_unit, None);
}
