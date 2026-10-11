//! #804 scripted damage must use the deterministic HP/death/victory order.

use crate::game_runtime::trigger_graph::{
    LoweredTriggerGraph, TriggerAction, TriggerEvent, lower_trigger_graph,
};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

use super::session_trigger_tests::{manifest_with, on_trigger};

fn damage_graph(unit: u32, amount_milli: u32) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_scripted_damage",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            vec![TriggerAction::DamageUnit { unit, amount_milli }],
        )],
    )
    .expect("lower damage graph")
}

#[test]
fn issue804_map_init_damage_reduces_exact_hp_deterministically() {
    let manifest = manifest_with(Some(damage_graph(10, 250)));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
        let target_before = session.match_state().hp_milli(10).expect("target HP");
        let peer_before = session.match_state().hp_milli(11).expect("peer HP");

        session.apply(SessionOrder::Tick).expect("damage tick");

        assert_eq!(
            session.match_state().hp_milli(10).expect("target HP"),
            target_before - 250
        );
        assert_eq!(
            session.match_state().hp_milli(11).expect("peer HP"),
            peer_before,
            "scripted damage must not spill onto another unit"
        );
        assert!(session.match_state().is_alive(10).expect("target alive"));
        assert_eq!(
            session.triggers().fire_count("trg_scripted_damage"),
            Some(1)
        );
        session.combined_state_hash()
    };
    assert_eq!(run(), run(), "identical damage actions must hash equally");
}

#[test]
fn issue804_zero_damage_rejected_before_play() {
    let manifest = manifest_with(Some(damage_graph(10, 0)));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("zero damage must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("unit 10 must have amount_milli > 0"),
        "unexpected diagnostic: {diagnostic}"
    );
}

#[test]
fn issue804_missing_damage_target_rejected_before_play() {
    let manifest = manifest_with(Some(damage_graph(999, 250)));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("missing damage target must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("damage_unit action references unit 999 absent"),
        "unexpected diagnostic: {diagnostic}"
    );
}
