//! #804 runtime duration defense after package validation is deliberately bypassed.

use crate::EngineError;
use crate::game_runtime::datasets::RuntimeDatasetsManifest;
use crate::game_runtime::lowered::{
    EconomyRulesetAuthor, LoweredResourceKind, LoweredStructureType, LoweredUnitEconomy,
    LoweredUpkeepTier,
};
use crate::game_runtime::{GameRuntimeError, MatchRuntimeSession, SessionOrder};

use super::session_order_test_support::manifest_with;

const BARRACKS: &str = "duration_barracks";
const SOLDIER: &str = "duration_soldier";

fn duration_manifest() -> RuntimeDatasetsManifest {
    let economy = EconomyRulesetAuthor::new(4, 1, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: 1_000,
        })
        .resource(LoweredResourceKind {
            key: "lumber".to_string(),
            display_name: "Lumber".to_string(),
            starting_balance: 1_000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".to_string(),
            food_min: 0,
            food_max: 10,
            income_numerator: 1,
            income_denominator: 1,
        })
        .unit(LoweredUnitEconomy {
            unit_id: SOLDIER.to_string(),
            gold_cost: 5,
            lumber_cost: 3,
            food_cost: 1,
            build_time_secs: 1,
            food_provided: 0,
        })
        .structure(LoweredStructureType {
            struct_id: BARRACKS.to_string(),
            gold_cost: 20,
            lumber_cost: 10,
            build_time_secs: 1,
            footprint_w: 2,
            footprint_h: 2,
            food_provided: 10,
            trains: vec![SOLDIER.to_string()],
        })
        .lower()
        .expect("lower valid duration economy");
    let mut manifest = manifest_with(None);
    manifest.economy = economy;
    manifest.datasets_fingerprint = manifest.compute_fingerprint();
    manifest
}

fn assert_duration_error(error: GameRuntimeError, field: &str) {
    assert!(matches!(
        error,
        GameRuntimeError::Sim(EngineError::GamePackageInvalidConfig {
            field: actual,
            ..
        }) if actual == field
    ));
}

#[test]
fn issue804_forged_structure_duration_rejects_before_session_mutation() {
    let manifest = duration_manifest();
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    session
        .economy
        .forge_structure_duration_for_test(BARRACKS, u32::MAX);
    let before_hash = session.combined_state_hash();
    let before_orders = session.order_log().len();
    let error = session
        .apply(SessionOrder::PlaceStructure {
            struct_id: BARRACKS.to_string(),
            x: 2,
            z: 2,
        })
        .expect_err("forged structure duration must reject");
    assert_duration_error(error, "economy.structure.build_time_secs");
    assert_eq!(session.economy().accounts().balance("gold"), 1_000);
    assert_eq!(session.economy().accounts().balance("lumber"), 1_000);
    assert!(session.economy().structure(0).is_none());
    assert_eq!(session.combined_state_hash(), before_hash);
    assert_eq!(session.order_log().len(), before_orders);
}

#[test]
fn issue804_forged_unit_duration_rejects_before_training_mutation() {
    let manifest = duration_manifest();
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
        .economy
        .forge_unit_duration_for_test(SOLDIER, u32::MAX);
    let before_gold = session.economy().accounts().balance("gold");
    let before_lumber = session.economy().accounts().balance("lumber");
    let before_food = session.economy().accounts().used_food();
    let before_hash = session.combined_state_hash();
    let before_orders = session.order_log().len();
    let error = session
        .apply(SessionOrder::TrainUnit {
            instance_id: 0,
            unit_id: SOLDIER.to_string(),
        })
        .expect_err("forged unit duration must reject");
    assert_duration_error(error, "economy.unit.build_time_secs");
    assert_eq!(session.economy().accounts().balance("gold"), before_gold);
    assert_eq!(
        session.economy().accounts().balance("lumber"),
        before_lumber
    );
    assert_eq!(session.economy().accounts().used_food(), before_food);
    assert!(session.economy().structure(0).unwrap().queue.is_empty());
    assert_eq!(session.combined_state_hash(), before_hash);
    assert_eq!(session.order_log().len(), before_orders);
}
