//! #804 authored worker reassignment across trigger -> session -> economy.

use crate::game_runtime::{
    MatchRuntimeSession, SessionOrder,
    datasets::RuntimeDatasetsManifest,
    lowered::{EconomyRulesetAuthor, LoweredResourceKind, LoweredResourceNode, LoweredUpkeepTier},
    trigger_graph::{LoweredTriggerGraph, TriggerAction, TriggerEvent, lower_trigger_graph},
};

use super::super::session_order_test_support::manifest_with;
use super::session_trigger_tests::on_trigger;

const WORKER: u32 = 804;
const GOLD_NORTH: &str = "gold_north";
const GOLD_SOUTH: &str = "gold_south";

#[test]
fn authored_assignment_retargets_exact_worker_deterministically() {
    let graph = worker_graph(WORKER, GOLD_SOUTH);
    let manifest = worker_manifest(Some(graph));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
        session.apply(SessionOrder::Tick).expect("worker tick");
        let worker = session.economy().worker(WORKER).expect("worker");
        assert_eq!(worker.node_id, GOLD_SOUTH);
        assert_eq!(worker.resource_key, "gold");
        assert_eq!(session.triggers().fire_count("trg_worker"), Some(1));
        session.combined_state_hash()
    };
    assert_eq!(run(), run());
}

#[test]
fn missing_worker_and_node_fail_without_mutation_or_order_record() {
    let manifest = worker_manifest(None);
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    session
        .apply(SessionOrder::AddWorker {
            worker_id: WORKER,
            node_id: GOLD_NORTH.to_string(),
        })
        .expect("add worker");
    let before_worker = session.economy().worker(WORKER).expect("worker").clone();
    let before_hash = session.combined_state_hash();
    let before_orders = session.order_log().len();

    for order in [
        SessionOrder::AssignWorker {
            worker_id: WORKER + 1,
            node_id: GOLD_SOUTH.to_string(),
        },
        SessionOrder::AssignWorker {
            worker_id: WORKER,
            node_id: "missing_node".to_string(),
        },
    ] {
        session.apply(order).expect_err("assignment must reject");
        assert_eq!(session.economy().worker(WORKER), Some(&before_worker));
        assert_eq!(session.combined_state_hash(), before_hash);
        assert_eq!(session.order_log().len(), before_orders);
    }
}

#[test]
fn invalid_authored_assignment_fails_before_play() {
    for (worker_id, node_id, expected) in [
        (
            WORKER,
            "missing_node",
            "assign_worker references unknown node \"missing_node\"",
        ),
        (
            WORKER + 1,
            GOLD_SOUTH,
            "assign_worker references unauthored worker id 805",
        ),
    ] {
        let manifest = worker_manifest(Some(worker_graph(worker_id, node_id)));
        let error = MatchRuntimeSession::from_manifest(&manifest)
            .err()
            .expect("invalid assignment must reject package");
        assert!(error.to_string().contains(expected), "{error}");
    }
}

fn worker_graph(worker_id: u32, assigned_node: &str) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_worker",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            vec![
                TriggerAction::AddWorker {
                    worker_id: WORKER,
                    node_id: GOLD_NORTH.to_string(),
                },
                TriggerAction::AssignWorker {
                    worker_id,
                    node_id: assigned_node.to_string(),
                },
            ],
        )],
    )
    .expect("lower worker graph")
}

fn worker_manifest(triggers: Option<LoweredTriggerGraph>) -> RuntimeDatasetsManifest {
    let economy = EconomyRulesetAuthor::new(4, 8, 1)
        .resource(LoweredResourceKind {
            key: "gold".to_string(),
            display_name: "Gold".to_string(),
            starting_balance: 1_000,
        })
        .upkeep_tier(LoweredUpkeepTier {
            name: "standard".to_string(),
            food_min: 0,
            food_max: 50,
            income_numerator: 1,
            income_denominator: 1,
        })
        .node(node(GOLD_NORTH, 2, 2))
        .node(node(GOLD_SOUTH, 10, 10))
        .lower()
        .expect("lower worker economy");
    let mut manifest = manifest_with(None);
    manifest.economy = economy;
    manifest.triggers = triggers;
    manifest.datasets_fingerprint = manifest.compute_fingerprint();
    manifest
}

fn node(node_id: &str, cell_x: i32, cell_z: i32) -> LoweredResourceNode {
    LoweredResourceNode {
        node_id: node_id.to_string(),
        resource_key: "gold".to_string(),
        cell_x,
        cell_z,
        amount_total: 10_000,
        yield_per_trip: 10,
    }
}
