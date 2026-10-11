//! #804/#836 residual: scripted diplomacy must be reachable from editor-authored
//! ECA triggers, not only direct Rust session callers.

use crate::game_runtime::players::AllianceState;
use crate::game_runtime::trigger_graph::{
    LoweredTriggerGraph, TriggerAction, TriggerEvent, lower_trigger_graph,
};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

use super::session_trigger_tests::{manifest_with, on_trigger};

fn alliance_graph(from: u8, to: u8) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_scripted_alliance",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            vec![TriggerAction::SetAlliance {
                from,
                to,
                ally: true,
                shared_vision: true,
                shared_control: true,
            }],
        )],
    )
    .expect("lower alliance graph")
}

#[test]
fn issue804_map_init_alliance_sets_exact_directional_flags_deterministically() {
    let manifest = manifest_with(Some(alliance_graph(0, 1)));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
        let before = session.match_state().roster().alliance(0, 1).unwrap();
        assert_eq!(before.state, AllianceState::Enemy);

        session.apply(SessionOrder::Tick).expect("alliance tick");
        let forward = session.match_state().roster().alliance(0, 1).unwrap();
        assert_eq!(forward.state, AllianceState::Ally);
        assert!(forward.shared_vision);
        assert!(forward.shared_control);
        let reverse = session.match_state().roster().alliance(1, 0).unwrap();
        assert_eq!(
            reverse.state,
            AllianceState::Enemy,
            "directed authored alliance must not silently rewrite the reverse cell"
        );
        assert_eq!(
            session.triggers().fire_count("trg_scripted_alliance"),
            Some(1)
        );
        session.combined_state_hash()
    };
    assert_eq!(
        run(),
        run(),
        "identical authored alliance changes must hash equally"
    );
}

#[test]
fn issue804_self_alliance_fails_before_play() {
    let manifest = manifest_with(Some(alliance_graph(0, 0)));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("self alliance must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("set_alliance cannot target player 0's fixed self-alliance"),
        "unexpected diagnostic: {diagnostic}"
    );
}

#[test]
fn issue804_unknown_alliance_player_fails_before_play() {
    let manifest = manifest_with(Some(alliance_graph(0, 7)));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("unknown alliance player must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("set_alliance to player 7 is absent from the match roster"),
        "unexpected diagnostic: {diagnostic}"
    );
}
