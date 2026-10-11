//! #804/#837 residual: resource rewards must be reachable from editor-authored
//! ECA triggers, not only direct Rust session callers.

use crate::game_runtime::trigger_graph::{
    LoweredTriggerGraph, TriggerAction, TriggerEvent, lower_trigger_graph,
};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

use super::session_trigger_tests::{manifest_with, on_trigger};

fn reward_graph(key: &str, amount: u64) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_reward",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            vec![TriggerAction::GrantResource {
                key: key.to_string(),
                amount,
            }],
        )],
    )
    .expect("lower reward graph")
}

fn spend_graph(key: &str, amount: u64) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_spend",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            vec![TriggerAction::SpendResource {
                key: key.to_string(),
                amount,
            }],
        )],
    )
    .expect("lower spend graph")
}

#[test]
fn issue804_map_init_reward_grants_exact_resource_deterministically() {
    let manifest = manifest_with(Some(reward_graph("gold", 250)));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
        assert_eq!(session.economy().accounts().balance("gold"), 1_000);
        session.apply(SessionOrder::Tick).expect("reward tick");
        assert_eq!(session.economy().accounts().balance("gold"), 1_250);
        assert_eq!(session.triggers().fire_count("trg_reward"), Some(1));
        session.combined_state_hash()
    };
    assert_eq!(run(), run(), "identical authored rewards must hash equally");
}

#[test]
fn issue804_unknown_reward_resource_fails_before_play() {
    let manifest = manifest_with(Some(reward_graph("goold", 250)));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("unknown resource must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("grant_resource references unknown resource \"goold\""),
        "unexpected diagnostic: {diagnostic}"
    );
}

#[test]
fn issue804_zero_reward_fails_before_play() {
    let manifest = manifest_with(Some(reward_graph("gold", 0)));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("zero reward must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("grant_resource amount must be greater than zero"),
        "unexpected diagnostic: {diagnostic}"
    );
}

#[test]
fn issue804_reward_overflow_fails_without_mutating_balance() {
    let manifest = manifest_with(Some(reward_graph("gold", u64::MAX)));
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    let error = session
        .apply(SessionOrder::Tick)
        .expect_err("overflow must fail loud");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("economy.grant.overflow"),
        "unexpected diagnostic: {diagnostic}"
    );
    assert_eq!(
        session.economy().accounts().balance("gold"),
        1_000,
        "rejected reward must not clamp or mutate the balance"
    );
}

#[test]
fn issue804_authored_spend_is_exact_and_deterministic() {
    let manifest = manifest_with(Some(spend_graph("gold", 250)));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
        session.apply(SessionOrder::Tick).expect("spend tick");
        assert_eq!(session.economy().accounts().balance("gold"), 750);
        assert_eq!(session.triggers().fire_count("trg_spend"), Some(1));
        session.combined_state_hash()
    };
    assert_eq!(run(), run());
}

#[test]
fn issue804_rejected_spend_is_atomic_and_not_recorded() {
    let manifest = manifest_with(None);
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    let before_hash = session.combined_state_hash();
    let before_orders = session.order_log().len();

    for order in [
        SessionOrder::SpendResource {
            key: "gold".to_string(),
            amount: 1_001,
        },
        SessionOrder::SpendResource {
            key: "missing".to_string(),
            amount: 1,
        },
        SessionOrder::SpendResource {
            key: "gold".to_string(),
            amount: 0,
        },
    ] {
        session.apply(order).expect_err("spend must reject");
        assert_eq!(session.economy().accounts().balance("gold"), 1_000);
        assert_eq!(session.combined_state_hash(), before_hash);
        assert_eq!(session.order_log().len(), before_orders);
    }
}

#[test]
fn issue804_invalid_authored_spend_fails_before_play() {
    for (key, amount, expected) in [
        (
            "goold",
            25,
            "spend_resource references unknown resource \"goold\"",
        ),
        ("gold", 0, "spend_resource amount must be greater than zero"),
    ] {
        let manifest = manifest_with(Some(spend_graph(key, amount)));
        let error = MatchRuntimeSession::from_manifest(&manifest)
            .err()
            .expect("invalid spend must reject package");
        assert!(error.to_string().contains(expected), "{error}");
    }
}
