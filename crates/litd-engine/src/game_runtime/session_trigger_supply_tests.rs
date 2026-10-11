//! #804/#837 exact supply arithmetic at the serialized session boundary.

use crate::EngineError;
use crate::game_runtime::{GameRuntimeError, MatchRuntimeSession, SessionOrder};

use super::session_trigger_production_tests::{BARRACKS, SOLDIER, capacity_manifest};

#[test]
fn issue804_training_supply_overflow_rejects_before_resource_debits() {
    let manifest = capacity_manifest(u32::MAX, u32::MAX);
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    session
        .apply(SessionOrder::PlaceStructure {
            struct_id: BARRACKS.to_string(),
            x: 2,
            z: 2,
        })
        .expect("place barracks");
    for _ in 0..4 {
        session
            .apply(SessionOrder::Tick)
            .expect("complete barracks");
    }
    session
        .apply(SessionOrder::TrainUnit {
            instance_id: 0,
            unit_id: SOLDIER.to_string(),
        })
        .expect("first maximum reservation");
    assert_eq!(session.economy().accounts().used_food(), u32::MAX);
    assert_eq!(session.economy().accounts().supply_cap(), u32::MAX);
    let before_balances = (
        session.economy().accounts().balance("gold"),
        session.economy().accounts().balance("lumber"),
    );
    let before_queue = session.economy().structure(0).unwrap().queue.clone();
    let before_hash = session.combined_state_hash();
    let before_orders = session.order_log().len();

    let error = session
        .apply(SessionOrder::TrainUnit {
            instance_id: 0,
            unit_id: SOLDIER.to_string(),
        })
        .expect_err("second maximum reservation must overflow");
    assert!(matches!(
        error,
        GameRuntimeError::Sim(EngineError::GamePackageInvalidConfig { field, .. })
            if field == "economy.supply.overflow"
    ));
    assert_eq!(
        session.economy().accounts().balance("gold"),
        before_balances.0
    );
    assert_eq!(
        session.economy().accounts().balance("lumber"),
        before_balances.1
    );
    assert_eq!(session.economy().accounts().used_food(), u32::MAX);
    assert_eq!(session.economy().structure(0).unwrap().queue, before_queue);
    assert_eq!(session.combined_state_hash(), before_hash);
    assert_eq!(session.order_log().len(), before_orders);
}

#[test]
fn issue804_same_tick_supply_cap_overflow_rejects_before_build_completion() {
    let manifest = capacity_manifest(u32::MAX, 0);
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    for x in [2, 5] {
        session
            .apply(SessionOrder::PlaceStructure {
                struct_id: BARRACKS.to_string(),
                x,
                z: 2,
            })
            .expect("place barracks");
    }
    for _ in 0..3 {
        session
            .apply(SessionOrder::Tick)
            .expect("advance construction");
    }
    let before_structures = [
        session.economy().structure(0).expect("first").clone(),
        session.economy().structure(1).expect("second").clone(),
    ];
    assert_eq!(before_structures[0].build_ticks_remaining, 1);
    assert_eq!(before_structures[1].build_ticks_remaining, 1);
    assert_eq!(session.economy().accounts().balance("gold"), 996);
    assert_eq!(session.economy().accounts().balance("lumber"), 996);
    assert_eq!(session.economy().accounts().supply_cap(), 0);
    let before_balances = (
        session.economy().accounts().balance("gold"),
        session.economy().accounts().balance("lumber"),
    );
    let before_economy_tick = session.economy().tick_count();
    let before_applied_tick = session.applied_tick();
    let before_hash = session.combined_state_hash();
    let before_orders = session.order_log().len();

    let error = session
        .apply(SessionOrder::Tick)
        .expect_err("same-tick cap overflow must reject before completion");
    assert!(matches!(
        error,
        GameRuntimeError::Sim(EngineError::GamePackageInvalidConfig { field, .. })
            if field == "economy.supply_cap.overflow"
    ));
    assert_eq!(
        session.economy().structure(0).expect("first"),
        &before_structures[0]
    );
    assert_eq!(
        session.economy().structure(1).expect("second"),
        &before_structures[1]
    );
    assert_eq!(
        session.economy().accounts().balance("gold"),
        before_balances.0
    );
    assert_eq!(
        session.economy().accounts().balance("lumber"),
        before_balances.1
    );
    assert_eq!(session.economy().accounts().supply_cap(), 0);
    assert_eq!(session.economy().tick_count(), before_economy_tick);
    assert_eq!(session.applied_tick(), before_applied_tick);
    assert_eq!(session.combined_state_hash(), before_hash);
    assert_eq!(session.order_log().len(), before_orders);
}
