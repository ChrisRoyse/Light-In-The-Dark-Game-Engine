//! #804 trigger-authored structure cancellation and transactional refund coverage.

use crate::game_runtime::trigger_graph::{
    LoweredTriggerGraph, TriggerAction, TriggerEvent, lower_trigger_graph,
};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

use super::session_trigger_production_tests::{BARRACKS, production_manifest_without_triggers};
use super::session_trigger_tests::on_trigger;

fn cancel_graph(instance_id: u32) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![
            on_trigger(
                "trg_place_for_cancel",
                vec![TriggerEvent::MapInitialization],
                Vec::new(),
                vec![TriggerAction::PlaceStructure {
                    structure: BARRACKS.to_string(),
                    cell_x: 2,
                    cell_z: 2,
                }],
            ),
            on_trigger(
                "trg_cancel_structure",
                vec![TriggerEvent::ElapsedGameTime { elapsed_ticks: 2 }],
                Vec::new(),
                vec![TriggerAction::CancelStructure { instance_id }],
            ),
        ],
    )
    .expect("lower cancel graph")
}

#[test]
fn issue804_trigger_cancel_refunds_and_preserves_monotonic_instance_ids() {
    let mut manifest = production_manifest_without_triggers(1_000, 1_000);
    manifest.triggers = Some(cancel_graph(0));
    manifest.datasets_fingerprint = manifest.compute_fingerprint();
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
        session.apply(SessionOrder::Tick).expect("place tick");
        assert!(session.economy().structure(0).is_some());
        assert_eq!(session.economy().accounts().balance("gold"), 800);
        assert_eq!(session.economy().accounts().balance("lumber"), 950);

        session.apply(SessionOrder::Tick).expect("cancel tick");
        assert!(session.economy().structure(0).is_none());
        assert_eq!(session.economy().accounts().balance("gold"), 1_000);
        assert_eq!(session.economy().accounts().balance("lumber"), 1_000);
        assert_eq!(
            session.triggers().fire_count("trg_cancel_structure"),
            Some(1)
        );

        session
            .apply(SessionOrder::PlaceStructure {
                struct_id: BARRACKS.to_string(),
                x: 2,
                z: 2,
            })
            .expect("replacement structure");
        assert!(session.economy().structure(0).is_none());
        assert_eq!(
            session
                .economy()
                .structure(1)
                .expect("monotonic id")
                .instance_id,
            1
        );
        session.combined_state_hash()
    };
    assert_eq!(run(), run(), "cancel lifecycle must hash equally");
}

#[test]
fn issue804_missing_and_completed_cancel_rejections_retain_state() {
    let manifest = production_manifest_without_triggers(1_000, 1_000);
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    let initial_hash = session.combined_state_hash();
    let error = session
        .apply(SessionOrder::CancelStructure { instance_id: 99 })
        .expect_err("missing structure");
    assert!(error.to_string().contains("no structure 99"));
    assert_eq!(session.combined_state_hash(), initial_hash);
    assert!(session.order_log().is_empty());

    session
        .apply(SessionOrder::PlaceStructure {
            struct_id: BARRACKS.to_string(),
            x: 2,
            z: 2,
        })
        .expect("place");
    for _ in 0..4 {
        session.apply(SessionOrder::Tick).expect("complete");
    }
    let hash = session.combined_state_hash();
    let log_len = session.order_log().len();
    let gold = session.economy().accounts().balance("gold");
    let lumber = session.economy().accounts().balance("lumber");
    let error = session
        .apply(SessionOrder::CancelStructure { instance_id: 0 })
        .expect_err("completed structure");
    assert!(
        error
            .to_string()
            .contains("cannot cancel a completed structure")
    );
    assert!(session.economy().structure(0).unwrap().complete);
    assert_eq!(session.economy().accounts().balance("gold"), gold);
    assert_eq!(session.economy().accounts().balance("lumber"), lumber);
    assert_eq!(session.combined_state_hash(), hash);
    assert_eq!(session.order_log().len(), log_len);
}

#[test]
fn issue804_refund_overflow_rejection_is_atomic() {
    let manifest = production_manifest_without_triggers(1_000, u64::MAX);
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    session
        .apply(SessionOrder::PlaceStructure {
            struct_id: BARRACKS.to_string(),
            x: 2,
            z: 2,
        })
        .expect("place");
    session
        .apply(SessionOrder::GrantResource {
            key: "lumber".to_string(),
            amount: 50,
        })
        .expect("restore lumber to max");
    let hash = session.combined_state_hash();
    let log_len = session.order_log().len();
    let error = session
        .apply(SessionOrder::CancelStructure { instance_id: 0 })
        .expect_err("overflowing refund must reject");
    assert!(error.to_string().contains("would overflow"));
    assert!(session.economy().structure(0).is_some());
    assert_eq!(session.economy().accounts().balance("gold"), 800);
    assert_eq!(session.economy().accounts().balance("lumber"), u64::MAX);
    assert_eq!(session.combined_state_hash(), hash);
    assert_eq!(session.order_log().len(), log_len);
}
