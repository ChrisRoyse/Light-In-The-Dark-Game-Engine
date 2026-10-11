//! #1855: unit coverage for the runtime-effect session orders (quests #839, timers
//! #841, dialogs #842, floating text #843, victory/defeat #838). Every test is
//! mutation-falsifiable: each effect changes a subsystem folded into
//! `combined_state_hash`, and applying it moves that hash; a rejected effect fails
//! loud with a typed error.

use super::session_order_test_support::effect_manifest;
use crate::game_runtime::hud::{
    HudColor, QuestProgress, presentation::floating_text_damage_numbers,
};
use crate::game_runtime::victory::PlayerResult;
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

fn session() -> MatchRuntimeSession {
    MatchRuntimeSession::from_manifest(&effect_manifest()).expect("build session")
}

#[test]
fn quest_lifecycle_discovers_completes_and_hashes() {
    let mut s = session();
    let before = s.combined_state_hash();
    s.apply(SessionOrder::QuestCreate {
        quest: 1,
        title: "Hold the line".to_string(),
        description: "Defend Ashford Crossing".to_string(),
        icon: "banner".to_string(),
        required: true,
        required_flags: vec![7],
    })
    .expect("create quest");
    // Folded into the hash: a created quest changes combined_state_hash.
    assert_ne!(before, s.combined_state_hash(), "quest create must fold");

    s.apply(SessionOrder::QuestDiscover { quest: 1 })
        .expect("discover");
    assert!(s.hud().quests.quest(1).expect("quest").discovered);

    // Completion before the required flag is satisfied fails loud.
    let err = s
        .apply(SessionOrder::QuestComplete { quest: 1 })
        .expect_err("complete without flag must fail");
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_SIM");

    s.apply(SessionOrder::QuestSatisfyFlag { flag: 7 })
        .expect("satisfy");
    s.apply(SessionOrder::QuestComplete { quest: 1 })
        .expect("complete");
    assert_eq!(
        s.hud().quests.quest(1).expect("quest").progress,
        QuestProgress::Completed
    );
}

#[test]
fn duplicate_quest_id_fails_loud() {
    let mut s = session();
    let create = |q: u32| SessionOrder::QuestCreate {
        quest: q,
        title: "T".to_string(),
        description: "D".to_string(),
        icon: String::new(),
        required: false,
        required_flags: Vec::new(),
    };
    s.apply(create(1)).expect("first");
    let err = s.apply(create(1)).expect_err("duplicate must fail");
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_SIM");
}

#[test]
fn timer_start_pause_resume_and_window() {
    let mut s = session();
    s.apply(SessionOrder::StartTimer {
        one_shot: false,
        period_ticks: 4,
    })
    .expect("start");
    assert_eq!(s.hud().timers.timers().len(), 1);
    let timer = s.hud().timers.timers()[0].id;
    s.apply(SessionOrder::PauseTimer { timer }).expect("pause");
    assert!(!s.hud().timers.timer(timer).expect("timer").running);
    s.apply(SessionOrder::ResumeTimer { timer })
        .expect("resume");
    assert!(s.hud().timers.timer(timer).expect("timer").running);
    s.apply(SessionOrder::CreateTimerWindow {
        timer,
        title: "Reinforcements".to_string(),
    })
    .expect("window");
    assert_eq!(s.hud().timers.windows().len(), 1);
    // A window on a nonexistent timer fails loud.
    let err = s
        .apply(SessionOrder::CreateTimerWindow {
            timer: 999,
            title: "x".to_string(),
        })
        .expect_err("bad timer window");
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_SIM");
}

#[test]
fn dialog_create_button_show_hide() {
    let mut s = session();
    s.apply(SessionOrder::CreateDialog {
        title: "Parley?".to_string(),
    })
    .expect("create");
    let dialog = s.hud().dialogs.dialogs()[0].id;
    s.apply(SessionOrder::AddDialogButton {
        dialog,
        text: "Yield".to_string(),
        hotkey: 0,
    })
    .expect("button");
    s.apply(SessionOrder::ShowDialog {
        dialog,
        players: 0b1,
    })
    .expect("show");
    assert!(s.hud().dialogs.is_shown_to(dialog, 0));
    s.apply(SessionOrder::HideDialog { dialog }).expect("hide");
    assert!(!s.hud().dialogs.is_shown_to(dialog, 0));
}

#[test]
fn floating_text_spawns_and_culls_by_lifespan() {
    let mut s = session();
    s.apply(SessionOrder::SpawnFloatingText {
        text: "-5".to_string(),
        position_mm: [1_000, 0, 2_000],
        velocity_mm_per_tick: [0, 0, 40],
        color: [255, 64, 64, 255],
        lifespan_ticks: 2,
        fade_start_ticks: 1,
    })
    .expect("spawn");
    assert_eq!(s.hud().floating.active_count(), 1);
    // Culled after its lifespan elapses (exact-tick lifetime).
    s.apply(SessionOrder::Tick).expect("t1");
    s.apply(SessionOrder::Tick).expect("t2");
    assert_eq!(s.hud().floating.active_count(), 0);

    // A zero-lifespan tag is rejected loud.
    let err = s
        .apply(SessionOrder::SpawnFloatingText {
            text: "x".to_string(),
            position_mm: [0, 0, 0],
            velocity_mm_per_tick: [0, 0, 0],
            color: [0, 0, 0, 0],
            lifespan_ticks: 0,
            fade_start_ticks: 0,
        })
        .expect_err("zero lifespan");
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_SIM");
}

#[test]
fn issue882_apply_damage_spawns_one_exact_renderable_number() {
    let mut s = session();
    let before_hash = s.combined_state_hash();
    let before_hp = s.match_state().hp_milli(10).expect("target HP");
    let target = s.field_unit(10).expect("target position");

    s.apply(SessionOrder::ApplyDamage {
        target: 10,
        amount_milli: 250,
    })
    .expect("damage");

    assert_eq!(
        s.match_state().hp_milli(10).expect("target HP"),
        before_hp - 250
    );
    let entries = s.hud().floating.texts();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.text, "250");
    assert_eq!(
        entry.position_mm,
        [
            i64::from(target.cell_x) * 1_000,
            0,
            i64::from(target.cell_z) * 1_000,
        ]
    );
    assert_eq!(entry.velocity_mm_per_tick, [0, 0, 40]);
    assert_eq!(entry.color, HudColor::new(255, 96, 80, 255));
    assert_eq!((entry.spawn_tick, entry.lifespan_ticks), (0, 4));
    assert_eq!(entry.fade_start_ticks, 2);

    let dto = floating_text_damage_numbers(&s.hud().floating, s.hud().tick(), s.ticks_per_second());
    assert_eq!(dto.len(), 1);
    assert_eq!(dto[0].text, "250");
    assert_eq!(
        dto[0].world_position,
        glam::Vec3::new(target.cell_x as f32, 0.0, target.cell_z as f32)
    );
    assert_ne!(before_hash, s.combined_state_hash());
}

#[test]
fn issue882_damage_uses_clamped_amount_and_structure_base_position() {
    let mut s = session();
    s.apply(SessionOrder::ApplyDamage {
        target: 2,
        amount_milli: u32::MAX,
    })
    .expect("overkill structure");

    let entry = s.hud().floating.text(0).expect("damage number");
    assert_eq!(entry.text, "3000", "must display amount_applied");
    assert_eq!(entry.position_mm, [12_000, 0, 13_000]);
    assert_eq!(s.hud().floating.active_count(), 1);
}

#[test]
fn issue882_rejected_damage_never_spawns_or_duplicates_number() {
    let mut s = session();
    let hp = s.match_state().hp_milli(10).expect("target HP");

    s.apply(SessionOrder::ApplyDamage {
        target: 10,
        amount_milli: 0,
    })
    .expect_err("zero damage");
    assert_eq!(s.match_state().hp_milli(10).expect("target HP"), hp);
    assert_eq!(s.hud().floating.active_count(), 0);

    s.apply(SessionOrder::ApplyDamage {
        target: 999,
        amount_milli: 100,
    })
    .expect_err("missing target");
    assert_eq!(s.hud().floating.active_count(), 0);

    s.apply(SessionOrder::ApplyDamage {
        target: 10,
        amount_milli: u32::MAX,
    })
    .expect("lethal damage");
    assert_eq!(s.hud().floating.active_count(), 1);
    s.apply(SessionOrder::ApplyDamage {
        target: 10,
        amount_milli: 100,
    })
    .expect_err("dead target");
    assert_eq!(
        s.hud().floating.active_count(),
        1,
        "failed repeat must not duplicate the number"
    );
}

#[test]
fn defeat_player_forces_defeat_and_cascades_victory() {
    let mut s = session();
    assert_eq!(
        s.match_state().result(0).expect("p0"),
        PlayerResult::Playing
    );
    s.apply(SessionOrder::DefeatPlayer { player: 1 })
        .expect("defeat p1");
    assert_eq!(s.match_state().result(1).expect("p1"), PlayerResult::Defeat);
    // Melee cascade: with the only enemy defeated, player 0 wins.
    assert_eq!(
        s.match_state().result(0).expect("p0"),
        PlayerResult::Victory
    );
    // Idempotent: defeating an already-defeated player is a deterministic no-op.
    let hash = s.combined_state_hash();
    s.apply(SessionOrder::DefeatPlayer { player: 1 })
        .expect("defeat again");
    assert_eq!(hash, s.combined_state_hash());
    // Unknown player fails loud.
    let err = s
        .apply(SessionOrder::DefeatPlayer { player: 9 })
        .expect_err("bad player");
    assert_eq!(err.code(), "LITD_GAME_RUNTIME_SIM");
}

/// An identical effect-order stream reproduces an identical combined_state_hash
/// stream across two independent sessions (#85 lockstep) and survives save/load.
#[test]
fn effect_stream_is_replay_deterministic_and_save_load_stable() {
    let stream = vec![
        SessionOrder::QuestCreate {
            quest: 1,
            title: "Hold".to_string(),
            description: "Defend".to_string(),
            icon: String::new(),
            required: true,
            required_flags: vec![3],
        },
        SessionOrder::QuestDiscover { quest: 1 },
        SessionOrder::StartTimer {
            one_shot: true,
            period_ticks: 3,
        },
        SessionOrder::Tick,
        SessionOrder::SpawnFloatingText {
            text: "for the crossing".to_string(),
            position_mm: [5_000, 0, 5_000],
            velocity_mm_per_tick: [0, 0, 40],
            color: [255, 255, 255, 255],
            lifespan_ticks: 10,
            fade_start_ticks: 5,
        },
        SessionOrder::QuestSatisfyFlag { flag: 3 },
        SessionOrder::Tick,
        SessionOrder::QuestComplete { quest: 1 },
        SessionOrder::Tick,
    ];

    let run = |manifest: &_| -> Vec<u64> {
        let mut s = MatchRuntimeSession::from_manifest(manifest).expect("session");
        stream
            .iter()
            .map(|o| {
                s.apply(o.clone()).expect("apply");
                s.combined_state_hash()
            })
            .collect()
    };
    let manifest = effect_manifest();
    assert_eq!(run(&manifest), run(&manifest), "replay must be identical");

    // Save/load acid: capture mid-stream, replay the tail on a reloaded session.
    let mid = stream.len() / 2;
    let mut original = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    for o in &stream[..mid] {
        original.apply(o.clone()).expect("apply head");
    }
    let save = original.capture_save().expect("save");
    for o in &stream[mid..] {
        original.apply(o.clone()).expect("apply tail");
    }
    let mut reloaded = MatchRuntimeSession::restore_from_save(&manifest, &save).expect("restore");
    for o in &stream[mid..] {
        reloaded.apply(o.clone()).expect("replay tail");
    }
    assert_eq!(
        original.combined_state_hash(),
        reloaded.combined_state_hash(),
        "save/load acid must match"
    );
}
