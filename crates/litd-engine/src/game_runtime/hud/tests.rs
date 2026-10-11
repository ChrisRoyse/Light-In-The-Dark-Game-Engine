//! Unit coverage for the WC3 runtime HUD systems (#839-#843): lifecycle,
//! per-player visibility, tick-exact timers/floating text, order-driven dialog
//! clicks, and replay determinism (identical order streams -> identical hash).

use super::boards::{Leaderboard, Multiboard};
use super::dialogs::DialogSet;
use super::floating_text::{FloatingTextSet, FloatingTextSpawn};
use super::quests::{QuestLog, QuestNotificationKind, QuestProgress};
use super::timers::{TimerKind, TimerSet};
use super::{GameHudWorld, HudColor, HudOrder, HudOrderKind, PlayerMask};

fn seed_dialog_world() -> (GameHudWorld, u32, u32) {
    let mut world = GameHudWorld::new();
    let dialog = world.dialogs.create_dialog("Choose a path").unwrap();
    let attack = world.dialogs.add_button(dialog, "Attack", 0).unwrap();
    let _flee = world.dialogs.add_button(dialog, "Flee", 0).unwrap();
    world
        .dialogs
        .show(dialog, PlayerMask::single(1).unwrap())
        .unwrap();
    (world, dialog, attack)
}

#[test]
fn quest_lifecycle_emits_notifications() {
    let mut log = QuestLog::new();
    log.create_quest(
        1,
        "Find the Flame",
        "Seek the ember",
        "icon",
        true,
        &[10, 20],
    )
    .unwrap();
    log.discover_quest(1, 5).unwrap();
    // Completion blocked until both flags satisfied.
    assert!(log.try_complete(1, 6).is_err());
    log.satisfy_flag(10);
    assert!(log.try_complete(1, 7).is_err());
    log.satisfy_flag(20);
    log.try_complete(1, 8).unwrap();
    assert_eq!(log.quest(1).unwrap().progress, QuestProgress::Completed);
    assert_eq!(log.quest(1).unwrap().completed_tick, Some(8));
    let kinds: Vec<_> = log.notifications().iter().map(|n| n.kind).collect();
    assert_eq!(
        kinds,
        vec![
            QuestNotificationKind::Discovered,
            QuestNotificationKind::Completed
        ]
    );
}

#[test]
fn quest_sabotage_fails_loud() {
    let mut log = QuestLog::new();
    // Empty title (sabotaged data) is rejected.
    assert!(
        log.create_quest(1, "  ", "desc", "icon", false, &[])
            .is_err()
    );
    log.create_quest(2, "Ok", "desc", "icon", false, &[])
        .unwrap();
    // Duplicate id rejected.
    assert!(
        log.create_quest(2, "Dup", "desc", "icon", false, &[])
            .is_err()
    );
    // Failing a completed quest is rejected.
    log.try_complete(2, 1).unwrap();
    assert!(log.fail_quest(2, 2).is_err());
}

#[test]
fn leaderboard_updates_on_value_change() {
    let mut board = Leaderboard::new("Gold");
    board.set_row("Red", 100);
    board.set_row("Blue", 250);
    board.set_row("Red", 400); // a resource change bumps the value
    board.sort_descending();
    assert_eq!(board.rows[0].label, "Red");
    assert_eq!(board.rows[0].value, 400);
}

#[test]
fn multiboard_dimensions_and_visibility() {
    assert!(Multiboard::new("bad", 0, 3).is_err());
    let mut board = Multiboard::new("Scores", 2, 3).unwrap();
    board.set_cell(1, 2, "42", 42).unwrap();
    assert_eq!(board.cell(1, 2).unwrap().value, 42);
    assert!(board.set_cell(2, 0, "x", 1).is_err()); // out of bounds
    board.visible_to = PlayerMask::single(3).unwrap();
    assert!(board.visible_to.contains(3));
    assert!(!board.visible_to.contains(4));
}

#[test]
fn periodic_timer_fires_exact_count() {
    let mut timers = TimerSet::new(64).unwrap();
    assert!(timers.start_timer(TimerKind::Periodic, 0, 0).is_err()); // zero period rejected
    let id = timers.start_timer(TimerKind::Periodic, 5, 0).unwrap();
    // Over 32 ticks a period-5 timer fires floor(32/5) = 6 times.
    let mut fires = 0;
    for tick in 1..=32 {
        fires += timers.advance_to(tick).unwrap().len();
    }
    assert_eq!(fires, 6);
    assert_eq!(timers.timer(id).unwrap().fire_count, 6);
}

#[test]
fn timer_deadline_overflow_preserves_timers_and_next_id() {
    let mut timers = TimerSet::new(64).unwrap();
    assert_eq!(timers.start_timer(TimerKind::Periodic, 5, 0).unwrap(), 0);
    let before = timers.timers().to_vec();

    let error = timers
        .start_timer(TimerKind::OneShot, 2, u64::MAX)
        .expect_err("overflowing absolute deadline must reject");
    assert!(matches!(
        error,
        crate::EngineError::SimInvalidConfig { field, reason }
            if field == "timer.next_fire_tick" && reason == "next fire tick overflowed"
    ));
    assert_eq!(timers.timers(), before.as_slice());

    assert_eq!(
        timers.start_timer(TimerKind::OneShot, 1, 0).unwrap(),
        1,
        "rejected timer must not consume the next id"
    );
}

#[test]
fn one_shot_timer_fires_once_and_formats() {
    let mut timers = TimerSet::new(64).unwrap();
    let id = timers.start_timer(TimerKind::OneShot, 128, 0).unwrap();
    timers.create_window(id, "Reinforcements").unwrap();
    // 128 ticks / 64 tps = 2 seconds remaining at tick 0.
    assert_eq!(timers.remaining_formatted(id, 0).unwrap(), "00:02");
    let mut fires = 0;
    for tick in 1..=300 {
        fires += timers.advance_to(tick).unwrap().len();
    }
    assert_eq!(fires, 1);
    assert!(!timers.timer(id).unwrap().running);
}

#[test]
fn floating_text_expires_tick_exact() {
    let mut set = FloatingTextSet::new();
    let id = set
        .spawn(
            FloatingTextSpawn {
                text: "123".to_string(),
                position_mm: [0, 0, 0],
                velocity_mm_per_tick: [0, 0, 40],
                color: HudColor::WHITE,
                lifespan_ticks: 10,
                fade_start_ticks: 5,
            },
            100,
        )
        .unwrap();
    // Alive at tick 109, gone at 110 (spawn 100 + lifespan 10).
    assert_eq!(set.advance_to(109), 0);
    assert_eq!(set.active_count(), 1);
    // Position drifts +z deterministically; alpha begins fading after tick 105.
    let text = set.text(id).unwrap();
    assert_eq!(text.position_at(105)[2], 200);
    assert_eq!(text.alpha_at(104), 255);
    assert!(text.alpha_at(107) < 255 && text.alpha_at(107) > 0);
    assert_eq!(set.advance_to(110), 1);
    assert_eq!(set.active_count(), 0);
}

#[test]
fn floating_text_zero_lifespan_rejected() {
    let mut set = FloatingTextSet::new();
    let err = set.spawn(
        FloatingTextSpawn {
            text: "x".to_string(),
            position_mm: [0, 0, 0],
            velocity_mm_per_tick: [0, 0, 0],
            color: HudColor::WHITE,
            lifespan_ticks: 0,
            fade_start_ticks: 0,
        },
        0,
    );
    assert!(err.is_err());
}

#[test]
fn dialog_click_requires_visibility_and_button() {
    let mut dialogs = DialogSet::new();
    let dialog = dialogs.create_dialog("Q").unwrap();
    let button = dialogs.add_button(dialog, "Yes", 0).unwrap();
    // Not shown to player 1 yet.
    assert!(dialogs.resolve_click(dialog, button, 1, 3).is_err());
    dialogs
        .show(dialog, PlayerMask::single(1).unwrap())
        .unwrap();
    // Unknown button rejected.
    assert!(dialogs.resolve_click(dialog, 99, 1, 3).is_err());
    let record = dialogs.resolve_click(dialog, button, 1, 3).unwrap();
    assert_eq!(record.button, button);
    // Dialog dismissed for player 1 after the click.
    assert!(!dialogs.is_shown_to(dialog, 1));
}

#[test]
fn dialog_click_order_resolves_in_replay_deterministically() {
    // Same seed + same click tick MUST reproduce the same final state hash.
    let run = |extra: u64| -> u64 {
        let (mut world, dialog, attack) = seed_dialog_world();
        world
            .enqueue_order(HudOrder::new(
                3,
                extra,
                HudOrderKind::DialogClick {
                    dialog,
                    button: attack,
                    player: 1,
                },
            ))
            .unwrap();
        world.run_ticks(6).unwrap();
        world.state_hash()
    };
    let first = run(0);
    let second = run(0);
    assert_eq!(first, second, "identical order streams must match");
    assert_eq!(1, {
        let (mut world, dialog, attack) = seed_dialog_world();
        world
            .enqueue_order(HudOrder::new(
                3,
                0,
                HudOrderKind::DialogClick {
                    dialog,
                    button: attack,
                    player: 1,
                },
            ))
            .unwrap();
        world.run_ticks(6).unwrap();
        world.dialogs.clicks().len()
    });
}

#[test]
fn past_due_hud_order_rejected() {
    let mut world = GameHudWorld::new();
    world.run_ticks(5).unwrap();
    let (_w, dialog, attack) = seed_dialog_world();
    // execute_tick 2 <= current tick 5 -> rejected.
    let err = world.enqueue_order(HudOrder::new(
        2,
        0,
        HudOrderKind::DialogClick {
            dialog,
            button: attack,
            player: 1,
        },
    ));
    assert!(err.is_err());
}

#[test]
fn full_world_replay_determinism() {
    let build = || -> u64 {
        let mut world = GameHudWorld::new();
        world
            .quests
            .create_quest(1, "T", "D", "i", true, &[7])
            .unwrap();
        world.quests.discover_quest(1, 0).unwrap();
        let mut lb = Leaderboard::new("Gold");
        lb.set_row("Red", 300);
        world.boards.set_leaderboard(lb);
        let _t = world.timers.start_timer(TimerKind::Periodic, 4, 0).unwrap();
        world
            .floating
            .spawn_damage_number(50, [1000, 0, 0], HudColor::WHITE, 8, 0)
            .unwrap();
        let dialog = world.dialogs.create_dialog("Pick").unwrap();
        let btn = world.dialogs.add_button(dialog, "Go", 0).unwrap();
        world
            .dialogs
            .show(dialog, PlayerMask::single(2).unwrap())
            .unwrap();
        world
            .enqueue_order(HudOrder::new(
                5,
                0,
                HudOrderKind::DialogClick {
                    dialog,
                    button: btn,
                    player: 2,
                },
            ))
            .unwrap();
        world.run_ticks(20).unwrap();
        world.state_hash()
    };
    assert_eq!(build(), build());
}
