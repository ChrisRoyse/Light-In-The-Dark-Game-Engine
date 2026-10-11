//! #804 scripted defeat must use the deterministic victory order and static roster ids.

use crate::game_runtime::trigger_graph::{
    LoweredTriggerGraph, TriggerAction, TriggerEvent, lower_trigger_graph,
};
use crate::game_runtime::victory::{GameOverEvent, PlayerResult};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

use super::session_trigger_tests::{manifest_with, on_trigger};

fn defeat_graph(player: u8) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_scripted_defeat",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            vec![TriggerAction::DefeatPlayer { player }],
        )],
    )
    .expect("lower defeat graph")
}

#[test]
fn issue804_map_init_defeat_cascades_victory_deterministically() {
    let manifest = manifest_with(Some(defeat_graph(1)));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
        assert_eq!(
            session.match_state().result(1).expect("player 1"),
            PlayerResult::Playing
        );

        session.apply(SessionOrder::Tick).expect("defeat tick");

        assert_eq!(
            session.match_state().result(1).expect("player 1"),
            PlayerResult::Defeat
        );
        assert_eq!(
            session.match_state().result(0).expect("player 0"),
            PlayerResult::Victory
        );
        assert_eq!(
            session.match_state().game_over_log(),
            &[
                GameOverEvent {
                    tick: 1,
                    player: 1,
                    result: PlayerResult::Defeat,
                },
                GameOverEvent {
                    tick: 1,
                    player: 0,
                    result: PlayerResult::Victory,
                },
            ]
        );
        assert_eq!(
            session.triggers().fire_count("trg_scripted_defeat"),
            Some(1)
        );
        session.combined_state_hash()
    };
    assert_eq!(run(), run(), "identical defeat actions must hash equally");
}

#[test]
fn issue804_unknown_defeat_player_rejected_before_play() {
    let manifest = manifest_with(Some(defeat_graph(7)));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("unknown defeat player must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("defeat_player player 7 is absent from the match roster"),
        "unexpected diagnostic: {diagnostic}"
    );
}
