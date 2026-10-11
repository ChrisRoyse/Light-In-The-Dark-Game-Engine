//! #804 scripted selection must use the deterministic match selection model.

use crate::game_runtime::trigger_graph::{
    LoweredTriggerGraph, TriggerAction, TriggerEvent, lower_trigger_graph,
};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

use super::session_trigger_tests::{manifest_with, on_trigger};

fn selection_graph(actions: Vec<TriggerAction>) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_scripted_selection",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            actions,
        )],
    )
    .expect("lower selection graph")
}

#[test]
fn issue804_map_init_selection_is_sorted_deduped_and_deterministic() {
    let manifest = manifest_with(Some(selection_graph(vec![TriggerAction::SelectUnits {
        player: 0,
        units: vec![11, 10, 11],
    }])));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");

        session.apply(SessionOrder::Tick).expect("selection tick");

        assert_eq!(
            session
                .match_state()
                .selection()
                .selection(0)
                .expect("player 0 selection")
                .members(),
            &[10, 11],
            "selection keeps the runtime's cross-owner sorted/deduplicated contract"
        );
        assert!(
            session
                .match_state()
                .selection()
                .selection(1)
                .expect("player 1 selection")
                .is_empty(),
            "scripted selection must not affect a peer player"
        );
        assert_eq!(
            session.triggers().fire_count("trg_scripted_selection"),
            Some(1)
        );
        session.combined_state_hash()
    };
    assert_eq!(run(), run(), "identical selections must hash equally");
}

#[test]
fn issue804_empty_selection_replaces_existing_selection() {
    let manifest = manifest_with(Some(selection_graph(vec![
        TriggerAction::SelectUnits {
            player: 0,
            units: vec![10],
        },
        TriggerAction::SelectUnits {
            player: 0,
            units: Vec::new(),
        },
    ])));
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");

    session.apply(SessionOrder::Tick).expect("selection tick");

    assert!(
        session
            .match_state()
            .selection()
            .selection(0)
            .expect("player 0 selection")
            .is_empty(),
        "an empty authored selection must explicitly clear the prior selection"
    );
}

#[test]
fn issue804_missing_selection_player_rejected_before_play() {
    let manifest = manifest_with(Some(selection_graph(vec![TriggerAction::SelectUnits {
        player: 7,
        units: vec![10],
    }])));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("missing selection player must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("select_units player 7 is absent from the match roster"),
        "unexpected diagnostic: {diagnostic}"
    );
}

#[test]
fn issue804_missing_selection_unit_rejected_before_play() {
    let manifest = manifest_with(Some(selection_graph(vec![TriggerAction::SelectUnits {
        player: 0,
        units: vec![999],
    }])));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("missing selection unit must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("select_units action references unit 999 absent"),
        "unexpected diagnostic: {diagnostic}"
    );
}
