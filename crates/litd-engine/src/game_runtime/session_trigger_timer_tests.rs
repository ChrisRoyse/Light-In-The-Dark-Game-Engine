//! #804 trigger-authored timer windows use the deterministic HUD timer orders.

use crate::game_runtime::trigger_graph::{
    LoweredTriggerGraph, TriggerAction, TriggerEvent, lower_trigger_graph,
};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

use super::session_trigger_tests::{manifest_with, on_trigger};

fn timer_graph(actions: Vec<TriggerAction>) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_reinforcement_timer",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            actions,
        )],
    )
    .expect("lower timer graph")
}

#[test]
fn issue804_map_init_builds_timer_window_deterministically() {
    let manifest = manifest_with(Some(timer_graph(vec![
        TriggerAction::StartTimer {
            one_shot: false,
            period_ticks: 4,
        },
        TriggerAction::CreateTimerWindow {
            timer: 0,
            title: "Reinforcements".to_string(),
        },
    ])));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");

        session.apply(SessionOrder::Tick).expect("timer tick");

        let timers = session.hud().timers.timers();
        assert_eq!(timers.len(), 1);
        assert_eq!((timers[0].id, timers[0].period_ticks), (0, 4));
        let windows = session.hud().timers.windows();
        assert_eq!(windows.len(), 1);
        assert_eq!(
            (
                windows[0].timer,
                windows[0].title.as_str(),
                windows[0].visible,
            ),
            (0, "Reinforcements", true)
        );
        assert_eq!(
            session.triggers().fire_count("trg_reinforcement_timer"),
            Some(1)
        );
        session.combined_state_hash()
    };
    assert_eq!(run(), run(), "identical timer actions must hash equally");
}

#[test]
fn issue804_empty_timer_window_title_rejected_before_play() {
    let manifest = manifest_with(Some(timer_graph(vec![TriggerAction::CreateTimerWindow {
        timer: 0,
        title: "  ".to_string(),
    }])));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("empty timer title must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("create_timer_window title for timer 0 must be non-empty"),
        "unexpected diagnostic: {diagnostic}"
    );
}

#[test]
fn issue804_missing_timer_rejected_without_window() {
    let manifest = manifest_with(Some(timer_graph(vec![TriggerAction::CreateTimerWindow {
        timer: 9,
        title: "Reinforcements".to_string(),
    }])));
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");

    let error = session
        .apply(SessionOrder::Tick)
        .expect_err("missing runtime timer must fail loud");
    assert!(
        error.to_string().contains("timer 9 does not exist"),
        "unexpected diagnostic: {error}"
    );
    assert!(session.hud().timers.windows().is_empty());
}

#[test]
fn issue804_second_window_action_retitles_without_duplication() {
    let manifest = manifest_with(Some(timer_graph(vec![
        TriggerAction::StartTimer {
            one_shot: true,
            period_ticks: 8,
        },
        TriggerAction::CreateTimerWindow {
            timer: 0,
            title: "First wave".to_string(),
        },
        TriggerAction::CreateTimerWindow {
            timer: 0,
            title: "Final wave".to_string(),
        },
    ])));
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");

    session.apply(SessionOrder::Tick).expect("retitle tick");

    let windows = session.hud().timers.windows();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].title, "Final wave");
}
