//! #804 scripted groups must use the deterministic persistent player-group order.

use crate::game_runtime::trigger_graph::{
    LoweredTriggerGraph, TriggerAction, TriggerEvent, lower_trigger_graph,
};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

use super::session_trigger_tests::{manifest_with, on_trigger};

fn group_graph(player: u8, units: &[u32]) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_scripted_group",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            units
                .iter()
                .map(|unit| TriggerAction::AddUnitToGroup {
                    player,
                    unit: *unit,
                })
                .collect(),
        )],
    )
    .expect("lower group graph")
}

#[test]
fn issue804_map_init_group_is_sorted_deduped_and_deterministic() {
    let manifest = manifest_with(Some(group_graph(0, &[10, 1, 10])));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");

        session.apply(SessionOrder::Tick).expect("group tick");

        assert_eq!(
            session
                .match_state()
                .player_groups()
                .group(0)
                .expect("player 0 group")
                .members(),
            &[1, 10],
            "scripted membership must use the sorted, deduplicated runtime group"
        );
        assert!(
            session
                .match_state()
                .player_groups()
                .group(1)
                .expect("player 1 group")
                .is_empty(),
            "scripted membership must not affect a peer group"
        );
        assert_eq!(session.triggers().fire_count("trg_scripted_group"), Some(1));
        session.combined_state_hash()
    };
    assert_eq!(run(), run(), "identical group actions must hash equally");
}

#[test]
fn issue804_missing_group_player_rejected_before_play() {
    let manifest = manifest_with(Some(group_graph(7, &[10])));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("missing group player must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("add_unit_to_group player 7 is absent from the match roster"),
        "unexpected diagnostic: {diagnostic}"
    );
}

#[test]
fn issue804_missing_group_unit_rejected_before_play() {
    let manifest = manifest_with(Some(group_graph(0, &[999])));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("missing group unit must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("add_unit_to_group action references unit 999 absent"),
        "unexpected diagnostic: {diagnostic}"
    );
}
