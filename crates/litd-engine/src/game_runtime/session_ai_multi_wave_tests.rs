//! #1858 multi-wave schedule tests, split from the #1786 macro-AI suite so both
//! cohesive test modules stay below the #800 line-count ratchet.

use crate::game_runtime::{
    LoweredAiProfile, MatchRuntimeSession, SessionOrder,
    ai_profile::{AiBuildStep, AiWaveStep, lower_ai_profile_with_waves},
};

use super::{ai_manifest, kinds};

fn multi_wave_profile() -> LoweredAiProfile {
    // Three night-raid style pulses: wave 0 at tick 0, then 1 and 2 after regroup.
    lower_ai_profile_with_waves(
        1,
        "Raider",
        1,
        "gold_mine",
        1,
        vec![AiBuildStep {
            struct_id: "town_hall".to_string(),
            at_gold: 0,
            cell_x: 10,
            cell_z: 10,
        }],
        "soldier",
        1, // legacy wave_size unused when schedule present
        vec![
            AiWaveStep {
                at_tick: 0,
                size: 1,
                regroup_ticks: 2,
            },
            AiWaveStep {
                at_tick: 0,
                size: 1,
                regroup_ticks: 2,
            },
            AiWaveStep {
                at_tick: 0,
                size: 1,
                regroup_ticks: 2,
            },
        ],
        0,
        3,
        1000,
    )
    .expect("lower multi-wave profile")
}

#[test]
fn issue1858_multi_wave_schedule_fires_wave_advance() {
    let manifest = ai_manifest(multi_wave_profile());
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    // Run until all three waves have been sent (not just until two advances).
    for _ in 0..100 {
        session.apply(SessionOrder::Tick).expect("tick");
        if kinds(&session)
            .iter()
            .filter(|k| **k == "WAVE_SENT")
            .count()
            >= 3
        {
            break;
        }
    }
    let ks = kinds(&session);
    let wave_sents = ks.iter().filter(|k| **k == "WAVE_SENT").count();
    let advances = ks.iter().filter(|k| **k == "WAVE_ADVANCE").count();
    assert!(
        wave_sents >= 3,
        "three scheduled waves must each WAVE_SENT, got {wave_sents}: {ks:?}"
    );
    assert!(
        advances >= 2,
        "must advance from wave0->1 and wave1->2, got {advances}: {ks:?}"
    );
    // WAVE_SENT details must name distinct wave indices 0,1,2.
    let wave_details: Vec<&str> = session
        .ai()
        .timeline()
        .iter()
        .filter(|e| e.kind == "WAVE_SENT")
        .map(|e| e.detail.as_str())
        .collect();
    assert!(
        wave_details.iter().any(|d| d.contains("wave 0")),
        "missing wave 0: {wave_details:?}"
    );
    assert!(
        wave_details.iter().any(|d| d.contains("wave 1")),
        "missing wave 1: {wave_details:?}"
    );
    assert!(
        wave_details.iter().any(|d| d.contains("wave 2")),
        "missing wave 2: {wave_details:?}"
    );
}

#[test]
fn issue1858_decreasing_at_tick_fails_loud() {
    let err = lower_ai_profile_with_waves(
        1,
        "Bad",
        1,
        "mine",
        1,
        vec![AiBuildStep {
            struct_id: "hall".into(),
            at_gold: 0,
            cell_x: 0,
            cell_z: 0,
        }],
        "soldier",
        1,
        vec![
            AiWaveStep {
                at_tick: 10,
                size: 1,
                regroup_ticks: 0,
            },
            AiWaveStep {
                at_tick: 5,
                size: 1,
                regroup_ticks: 0,
            },
        ],
        0,
        3,
        1000,
    )
    .expect_err("decreasing at_tick must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("non-decreasing") || msg.contains("at_tick"),
        "error must name schedule order: {msg}"
    );
}

#[test]
fn issue1858_zero_wave_size_fails_loud() {
    let err = lower_ai_profile_with_waves(
        1,
        "Bad",
        1,
        "mine",
        1,
        vec![AiBuildStep {
            struct_id: "hall".into(),
            at_gold: 0,
            cell_x: 0,
            cell_z: 0,
        }],
        "soldier",
        1,
        vec![AiWaveStep {
            at_tick: 0,
            size: 0,
            regroup_ticks: 0,
        }],
        0,
        3,
        1000,
    )
    .expect_err("zero wave size must fail");
    assert!(
        err.to_string().contains("size"),
        "error must name size: {err}"
    );
}
