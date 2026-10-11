//! #804 trigger-authored worker, rally, and training lifecycle over the real economy.

use crate::EngineError;
use crate::game_runtime::datasets::RuntimeDatasetsManifest;
use crate::game_runtime::lowered::{
    EconomyRulesetAuthor, LoweredResourceKind, LoweredResourceNode, LoweredStructureType,
    LoweredUnitEconomy, LoweredUpkeepTier,
};
use crate::game_runtime::structures::RallyPoint;
use crate::game_runtime::trigger_graph::{
    LoweredTriggerGraph, TriggerAction, TriggerEvent, lower_trigger_graph,
};
use crate::game_runtime::{ECONOMY_MAX_UNITS, GameRuntimeError, MatchRuntimeSession, SessionOrder};

use super::super::session_order_test_support::manifest_with;
use super::session_trigger_tests::on_trigger;

const WORKER: u32 = 42;
const NODE: &str = "gold_mine";
pub(super) const BARRACKS: &str = "barracks";
pub(super) const SOLDIER: &str = "soldier";

fn production_economy(gold: u64, lumber: u64) -> crate::game_runtime::LoweredEconomyRuleset {
    EconomyRulesetAuthor::new(4, 1, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: gold,
        })
        .resource(LoweredResourceKind {
            key: "lumber".to_string(),
            display_name: "Lumber".to_string(),
            starting_balance: lumber,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".to_string(),
            food_min: 0,
            food_max: 50,
            income_numerator: 1,
            income_denominator: 1,
        })
        .node(LoweredResourceNode {
            node_id: NODE.to_string(),
            resource_key: "gold".to_string(),
            cell_x: 12,
            cell_z: 12,
            amount_total: 10_000,
            yield_per_trip: 10,
        })
        .unit(LoweredUnitEconomy {
            unit_id: SOLDIER.to_string(),
            gold_cost: 100,
            lumber_cost: 10,
            food_cost: 2,
            build_time_secs: 1,
            food_provided: 0,
        })
        .structure(LoweredStructureType {
            struct_id: BARRACKS.to_string(),
            gold_cost: 200,
            lumber_cost: 50,
            build_time_secs: 1,
            footprint_w: 2,
            footprint_h: 2,
            food_provided: 10,
            trains: vec![SOLDIER.to_string()],
        })
        .lower()
        .expect("lower production economy")
}

fn production_manifest(
    graph: LoweredTriggerGraph,
    gold: u64,
    lumber: u64,
) -> RuntimeDatasetsManifest {
    let mut manifest = manifest_with(None);
    manifest.economy = production_economy(gold, lumber);
    manifest.triggers = Some(graph);
    manifest.datasets_fingerprint = manifest.compute_fingerprint();
    manifest
}

pub(super) fn production_manifest_without_triggers(
    gold: u64,
    lumber: u64,
) -> RuntimeDatasetsManifest {
    let mut manifest = manifest_with(None);
    manifest.economy = production_economy(gold, lumber);
    manifest.datasets_fingerprint = manifest.compute_fingerprint();
    manifest
}

pub(super) fn capacity_manifest(food_provided: u32, food_cost: u32) -> RuntimeDatasetsManifest {
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
            food_max: food_provided,
            income_numerator: 1,
            income_denominator: 1,
        })
        .unit(LoweredUnitEconomy {
            unit_id: SOLDIER.to_string(),
            gold_cost: 1,
            lumber_cost: 1,
            food_cost,
            build_time_secs: 1,
            food_provided: 0,
        })
        .structure(LoweredStructureType {
            struct_id: BARRACKS.to_string(),
            gold_cost: 2,
            lumber_cost: 2,
            build_time_secs: 1,
            footprint_w: 2,
            footprint_h: 2,
            food_provided,
            trains: vec![SOLDIER.to_string()],
        })
        .lower()
        .expect("lower capacity economy");
    let mut manifest = manifest_with(None);
    manifest.economy = economy;
    manifest.datasets_fingerprint = manifest.compute_fingerprint();
    manifest
}

fn production_graph(node: &str, unit: &str) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![
            on_trigger(
                "trg_worker_and_structure",
                vec![TriggerEvent::MapInitialization],
                Vec::new(),
                vec![
                    TriggerAction::AddWorker {
                        worker_id: WORKER,
                        node_id: node.to_string(),
                    },
                    TriggerAction::PlaceStructure {
                        structure: BARRACKS.to_string(),
                        cell_x: 2,
                        cell_z: 2,
                    },
                ],
            ),
            on_trigger(
                "trg_rally_and_train",
                vec![TriggerEvent::ElapsedGameTime { elapsed_ticks: 5 }],
                Vec::new(),
                vec![
                    TriggerAction::SetRally {
                        instance_id: 0,
                        rally: RallyPoint::Point { x: 7, z: 8 },
                    },
                    TriggerAction::TrainUnit {
                        instance_id: 0,
                        unit_id: unit.to_string(),
                    },
                ],
            ),
        ],
    )
    .expect("lower production graph")
}

#[test]
fn issue804_worker_rally_and_training_actions_are_deterministic() {
    let manifest = production_manifest(production_graph(NODE, SOLDIER), 1_000, 1_000);
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
        for _ in 0..5 {
            session.apply(SessionOrder::Tick).expect("lifecycle tick");
        }
        assert_eq!(session.economy().worker_count(), 1);
        let structure = session.economy().structure(0).expect("barracks");
        assert!(structure.complete);
        assert_eq!(structure.rally, RallyPoint::Point { x: 7, z: 8 });
        assert_eq!(structure.queue.len(), 1);
        assert_eq!(structure.queue[0].unit_id, SOLDIER);
        assert_eq!(session.economy().accounts().used_food(), 2);
        assert_eq!(
            session.triggers().fire_count("trg_worker_and_structure"),
            Some(1)
        );
        assert_eq!(
            session.triggers().fire_count("trg_rally_and_train"),
            Some(1)
        );
        for _ in 0..4 {
            session.apply(SessionOrder::Tick).expect("training tick");
        }
        assert_eq!(session.economy().unit_body_count(), 1);
        assert!(session.economy().structure(0).unwrap().queue.is_empty());
        session.combined_state_hash()
    };
    assert_eq!(
        run(),
        run(),
        "identical lifecycle actions must hash equally"
    );
}

#[test]
fn issue804_unknown_worker_node_and_training_unit_reject_before_play() {
    for (node, unit, expected) in [
        (
            "missing_mine",
            SOLDIER,
            "add_worker references unknown node",
        ),
        (NODE, "missing_unit", "train_unit references unknown unit"),
    ] {
        let manifest = production_manifest(production_graph(node, unit), 1_000, 1_000);
        let error = MatchRuntimeSession::from_manifest(&manifest)
            .err()
            .expect("unknown economy reference must reject package");
        assert!(
            error.to_string().contains(expected),
            "unexpected diagnostic: {error}"
        );
    }
}

#[test]
fn issue804_duplicate_worker_id_rejects_package_and_runtime_without_mutation() {
    let graph = lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_duplicate_workers",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            vec![
                TriggerAction::AddWorker {
                    worker_id: WORKER,
                    node_id: NODE.to_string(),
                },
                TriggerAction::AddWorker {
                    worker_id: WORKER,
                    node_id: NODE.to_string(),
                },
            ],
        )],
    )
    .expect("lower duplicate graph");
    let manifest = production_manifest(graph, 1_000, 1_000);
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("duplicate authored worker must reject package");
    assert!(
        error
            .to_string()
            .contains("duplicates authored worker id 42")
    );

    let valid = production_manifest_without_triggers(1_000, 1_000);
    let mut session = MatchRuntimeSession::from_manifest(&valid).expect("session");
    session
        .apply(SessionOrder::AddWorker {
            worker_id: WORKER,
            node_id: NODE.to_string(),
        })
        .expect("first worker");
    let hash = session.combined_state_hash();
    let log_len = session.order_log().len();
    let error = session
        .apply(SessionOrder::AddWorker {
            worker_id: WORKER,
            node_id: NODE.to_string(),
        })
        .expect_err("duplicate live worker must reject");
    assert!(error.to_string().contains("worker id 42 already exists"));
    assert_eq!(session.economy().worker_count(), 1);
    assert_eq!(session.order_log().len(), log_len);
    assert_eq!(session.combined_state_hash(), hash);
}

#[test]
fn issue804_training_rejection_is_transactional() {
    let manifest = production_manifest_without_triggers(1_000, 55);
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
    let gold = session.economy().accounts().balance("gold");
    let lumber = session.economy().accounts().balance("lumber");
    let food = session.economy().accounts().used_food();
    let hash = session.combined_state_hash();
    let error = session
        .apply(SessionOrder::TrainUnit {
            instance_id: 0,
            unit_id: SOLDIER.to_string(),
        })
        .expect_err("insufficient lumber must reject");
    assert!(error.to_string().contains("cannot spend 10 lumber"));
    assert_eq!(session.economy().accounts().balance("gold"), gold);
    assert_eq!(session.economy().accounts().balance("lumber"), lumber);
    assert_eq!(session.economy().accounts().used_food(), food);
    assert!(session.economy().structure(0).unwrap().queue.is_empty());
    assert_eq!(session.combined_state_hash(), hash);
}

#[test]
fn issue804_structure_lifecycle_rejections_are_transactional() {
    let manifest = production_manifest_without_triggers(1_000, 1_000);
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    let initial_hash = session.combined_state_hash();
    let initial_log = session.order_log().len();
    for order in [
        SessionOrder::SetRally {
            instance_id: 99,
            rally: RallyPoint::Point { x: 7, z: 8 },
        },
        SessionOrder::TrainUnit {
            instance_id: 99,
            unit_id: SOLDIER.to_string(),
        },
    ] {
        let error = session
            .apply(order)
            .expect_err("missing structure must reject");
        assert!(error.to_string().contains("no structure 99"));
        assert_eq!(session.combined_state_hash(), initial_hash);
        assert_eq!(session.order_log().len(), initial_log);
    }

    session
        .apply(SessionOrder::PlaceStructure {
            struct_id: BARRACKS.to_string(),
            x: 2,
            z: 2,
        })
        .expect("place barracks");
    let placed_hash = session.combined_state_hash();
    let placed_log = session.order_log().len();
    for order in [
        SessionOrder::SetRally {
            instance_id: 0,
            rally: RallyPoint::Point { x: 7, z: 8 },
        },
        SessionOrder::TrainUnit {
            instance_id: 0,
            unit_id: SOLDIER.to_string(),
        },
    ] {
        let error = session
            .apply(order)
            .expect_err("incomplete structure must reject");
        assert!(
            error.to_string().contains("not complete")
                || error.to_string().contains("cannot train")
        );
        assert_eq!(session.combined_state_hash(), placed_hash);
        assert_eq!(session.order_log().len(), placed_log);
    }
}

#[test]
fn issue804_production_capacity_rejection_preserves_paid_reserved_job_and_session() {
    let manifest = capacity_manifest(ECONOMY_MAX_UNITS as u32 + 1, 1);
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
    for _ in 0..=ECONOMY_MAX_UNITS {
        session
            .apply(SessionOrder::TrainUnit {
                instance_id: 0,
                unit_id: SOLDIER.to_string(),
            })
            .expect("queue paid reserved unit");
    }
    for _ in 0..ECONOMY_MAX_UNITS * 4 + 3 {
        session
            .apply(SessionOrder::Tick)
            .expect("fill production sim");
    }

    assert_eq!(session.economy().unit_body_count(), ECONOMY_MAX_UNITS);
    assert_eq!(session.economy().accounts().balance("gold"), 741);
    assert_eq!(session.economy().accounts().balance("lumber"), 741);
    assert_eq!(session.economy().accounts().used_food(), 257);
    assert_eq!(session.economy().accounts().supply_cap(), 257);
    let before_queue = session
        .economy()
        .structure(0)
        .expect("barracks")
        .queue
        .clone();
    assert_eq!(before_queue.len(), 1);
    assert_eq!(before_queue[0].ticks_remaining, 1);
    let before_gold = session.economy().accounts().balance("gold");
    let before_lumber = session.economy().accounts().balance("lumber");
    let before_food = session.economy().accounts().used_food();
    let before_cap = session.economy().accounts().supply_cap();
    let before_economy_tick = session.economy().tick_count();
    let before_applied_tick = session.applied_tick();
    let before_hash = session.combined_state_hash();
    let before_orders = session.order_log().len();

    let error = session
        .apply(SessionOrder::Tick)
        .expect_err("capacity must reject before popping the completed job");
    assert!(matches!(
        error,
        GameRuntimeError::Sim(EngineError::SimCapacityExceeded { capacity })
            if capacity == ECONOMY_MAX_UNITS
    ));
    assert_eq!(session.economy().unit_body_count(), ECONOMY_MAX_UNITS);
    assert_eq!(
        session.economy().structure(0).expect("barracks").queue,
        before_queue
    );
    assert_eq!(session.economy().accounts().balance("gold"), before_gold);
    assert_eq!(
        session.economy().accounts().balance("lumber"),
        before_lumber
    );
    assert_eq!(session.economy().accounts().used_food(), before_food);
    assert_eq!(session.economy().accounts().supply_cap(), before_cap);
    assert_eq!(session.economy().tick_count(), before_economy_tick);
    assert_eq!(session.applied_tick(), before_applied_tick);
    assert_eq!(session.combined_state_hash(), before_hash);
    assert_eq!(session.order_log().len(), before_orders);
}
