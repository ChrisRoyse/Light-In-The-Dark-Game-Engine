//! #804/#837 harvest-deposit atomicity at the serialized session boundary.

use crate::EngineError;
use crate::game_runtime::datasets::RuntimeDatasetsManifest;
use crate::game_runtime::economy::apply_upkeep_multiplier;
use crate::game_runtime::lowered::{
    EconomyRulesetAuthor, LoweredResourceKind, LoweredResourceNode, LoweredUpkeepTier,
};
use crate::game_runtime::{GameRuntimeError, MatchRuntimeSession, SessionOrder};

use super::super::session_order_test_support::manifest_with;

const WORKER: u32 = 77;
const NODE: &str = "overflow_mine";

fn harvest_manifest(
    starting_balance: u64,
    yield_per_trip: u64,
    income_numerator: u32,
    income_denominator: u32,
) -> RuntimeDatasetsManifest {
    let economy = EconomyRulesetAuthor::new(4, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".to_string(),
            food_min: 0,
            food_max: 0,
            income_numerator,
            income_denominator,
        })
        .node(LoweredResourceNode {
            node_id: NODE.to_string(),
            resource_key: "gold".to_string(),
            cell_x: 1,
            cell_z: 0,
            amount_total: yield_per_trip,
            yield_per_trip,
        })
        .lower()
        .expect("lower overflow harvest economy");
    let mut manifest = manifest_with(None);
    manifest.economy = economy;
    manifest.datasets_fingerprint = manifest.compute_fingerprint();
    manifest
}

#[test]
fn issue804_deposit_overflow_rejects_before_tick_worker_or_carried_mutation() {
    let manifest = harvest_manifest(u64::MAX, 10, 1, 1);
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    session
        .apply(SessionOrder::AddWorker {
            worker_id: WORKER,
            node_id: NODE.to_string(),
        })
        .expect("add worker");
    for _ in 0..2 {
        session
            .apply(SessionOrder::Tick)
            .expect("reach deposit countdown");
    }
    let before_worker = session.economy().worker(WORKER).expect("worker").clone();
    assert_eq!(before_worker.carried, 10);
    assert_eq!(before_worker.deposit_cooldown, 1);
    assert_eq!(session.economy().node_remaining(NODE), 90);
    assert_eq!(session.economy().accounts().balance("gold"), u64::MAX);
    let before_economy_tick = session.economy().tick_count();
    let before_applied_tick = session.applied_tick();
    let before_hash = session.combined_state_hash();
    let before_orders = session.order_log().len();

    let error = session
        .apply(SessionOrder::Tick)
        .expect_err("deposit credit must overflow");
    assert!(matches!(
        error,
        GameRuntimeError::Sim(EngineError::GamePackageInvalidConfig { field, .. })
            if field == "economy.grant.overflow"
    ));
    assert_eq!(
        session.economy().worker(WORKER).expect("worker"),
        &before_worker
    );
    assert_eq!(session.economy().node_remaining(NODE), 90);
    assert_eq!(session.economy().accounts().balance("gold"), u64::MAX);
    assert_eq!(session.economy().tick_count(), before_economy_tick);
    assert_eq!(session.applied_tick(), before_applied_tick);
    assert_eq!(session.combined_state_hash(), before_hash);
    assert_eq!(session.order_log().len(), before_orders);
}

#[test]
fn issue804_upkeep_ratio_uses_exact_wide_arithmetic_at_u64_max() {
    let manifest = harvest_manifest(0, u64::MAX, 2, 2);
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    session
        .apply(SessionOrder::AddWorker {
            worker_id: WORKER,
            node_id: NODE.to_string(),
        })
        .expect("add worker");
    for _ in 0..3 {
        session.apply(SessionOrder::Tick).expect("harvest tick");
    }
    assert_eq!(session.economy().accounts().balance("gold"), u64::MAX);
    assert_eq!(session.economy().node_remaining(NODE), 0);
}

#[test]
fn issue804_upkeep_result_overflow_rejects_before_deposit_tick_mutation() {
    let manifest = harvest_manifest(0, u64::MAX, 2, 1);
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    session
        .apply(SessionOrder::AddWorker {
            worker_id: WORKER,
            node_id: NODE.to_string(),
        })
        .expect("add worker");
    for _ in 0..2 {
        session
            .apply(SessionOrder::Tick)
            .expect("reach deposit countdown");
    }
    let before_worker = session.economy().worker(WORKER).expect("worker").clone();
    let before_tick = session.economy().tick_count();
    let before_applied = session.applied_tick();
    let before_hash = session.combined_state_hash();
    let before_orders = session.order_log().len();
    let error = session
        .apply(SessionOrder::Tick)
        .expect_err("scaled income must exceed u64");
    assert!(matches!(
        error,
        GameRuntimeError::Sim(EngineError::GamePackageInvalidConfig { field, .. })
            if field == "economy.upkeep.overflow"
    ));
    assert_eq!(session.economy().worker(WORKER).unwrap(), &before_worker);
    assert_eq!(session.economy().accounts().balance("gold"), 0);
    assert_eq!(session.economy().tick_count(), before_tick);
    assert_eq!(session.applied_tick(), before_applied);
    assert_eq!(session.combined_state_hash(), before_hash);
    assert_eq!(session.order_log().len(), before_orders);
}

#[test]
fn issue804_upkeep_zero_denominator_is_typed() {
    let tier = LoweredUpkeepTier {
        name: "invalid".to_string(),
        food_min: 0,
        food_max: 0,
        income_numerator: 1,
        income_denominator: 0,
    };
    let error = apply_upkeep_multiplier(1, &tier).expect_err("zero denominator");
    assert!(matches!(
        error,
        EngineError::GamePackageInvalidConfig { field, .. }
            if field == "economy.upkeep.denominator"
    ));
}
