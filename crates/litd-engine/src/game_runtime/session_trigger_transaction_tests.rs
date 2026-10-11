//! #804 rejected Tick rollback across base systems and sequential ECA actions.

use crate::game_runtime::trigger_graph::{TriggerAction, TriggerEvent, lower_trigger_graph};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

use super::session_trigger_tests::{manifest_with, on_trigger};

#[test]
fn issue804_repeated_trigger_plan_failure_restores_every_committed_boundary() {
    let graph = lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_partial_economy",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            vec![
                TriggerAction::GrantResource {
                    key: "gold".to_string(),
                    amount: 1,
                },
                TriggerAction::SpendResource {
                    key: "gold".to_string(),
                    amount: 2_000,
                },
            ],
        )],
    )
    .expect("lower graph");
    let manifest = manifest_with(Some(graph));
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    let initial_hash = session.combined_state_hash();
    let initial_time = session.time_of_day().time_permille();

    for _ in 0..2 {
        let error = session
            .apply(SessionOrder::Tick)
            .expect_err("second trigger action must reject");
        assert!(error.to_string().contains("cannot spend 2000 gold"));
        assert_eq!(session.economy().accounts().balance("gold"), 1_000);
        assert_eq!(session.economy().tick_count(), 0);
        assert_eq!(session.match_state().tick(), 0);
        assert_eq!(session.hud().tick(), 0);
        assert_eq!(session.applied_tick(), 0);
        assert_eq!(session.time_of_day().time_permille(), initial_time);
        assert_eq!(
            session.triggers().fire_count("trg_partial_economy"),
            Some(0)
        );
        assert!(session.triggers().timeline().is_empty());
        assert_eq!(session.combined_state_hash(), initial_hash);
        assert!(session.order_log().is_empty());
    }
}
