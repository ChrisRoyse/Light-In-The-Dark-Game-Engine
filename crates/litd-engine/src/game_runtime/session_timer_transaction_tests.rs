//! #804 timer-start arithmetic and session rollback transaction coverage.

use crate::EngineError;
use crate::game_runtime::{GameRuntimeError, MatchRuntimeSession, SessionOrder};

use super::session_order_test_support::manifest_with;

#[test]
fn issue804_timer_deadline_overflow_rolls_back_exact_session_state() {
    let manifest = manifest_with(None);
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    session
        .apply(SessionOrder::Tick)
        .expect("advance to tick one");

    let before_applied_tick = session.applied_tick();
    let before_economy = session.economy().state_hash();
    let before_match = session.match_state().state_hash();
    let before_fog = session.fog().state_hash();
    let before_time_of_day = session.time_of_day().state_hash();
    let before_hud = session.hud().state_hash();
    let before_kv = session.kv().state_hash();
    let before_combined = session.combined_state_hash();
    let before_orders = session.order_log().to_vec();

    let error = session
        .apply(SessionOrder::StartTimer {
            one_shot: true,
            period_ticks: u64::MAX,
        })
        .expect_err("tick one plus maximum period must overflow");
    assert!(matches!(
        error,
        GameRuntimeError::Sim(EngineError::SimInvalidConfig { field, reason })
            if field == "timer.next_fire_tick" && reason == "next fire tick overflowed"
    ));

    assert_eq!(session.applied_tick(), before_applied_tick);
    assert_eq!(session.economy().state_hash(), before_economy);
    assert_eq!(session.match_state().state_hash(), before_match);
    assert_eq!(session.fog().state_hash(), before_fog);
    assert_eq!(session.time_of_day().state_hash(), before_time_of_day);
    assert_eq!(session.hud().state_hash(), before_hud);
    assert_eq!(session.kv().state_hash(), before_kv);
    assert_eq!(session.combined_state_hash(), before_combined);
    assert_eq!(session.order_log(), before_orders.as_slice());
    assert!(session.hud().timers.timers().is_empty());

    session
        .apply(SessionOrder::StartTimer {
            one_shot: true,
            period_ticks: 1,
        })
        .expect("valid timer after rejection");
    assert_eq!(
        session.hud().timers.timers()[0].id,
        0,
        "rejected session order must not consume the next timer id"
    );
}
