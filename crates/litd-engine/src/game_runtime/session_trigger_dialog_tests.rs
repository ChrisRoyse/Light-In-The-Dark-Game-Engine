//! #804 trigger-authored dialogs must use the deterministic HUD dialog orders.

use crate::game_runtime::trigger_graph::{
    LoweredTriggerGraph, TriggerAction, TriggerEvent, lower_trigger_graph,
};
use crate::game_runtime::{MatchRuntimeSession, SessionOrder};

use super::session_trigger_tests::{manifest_with, on_trigger};

fn dialog_graph(actions: Vec<TriggerAction>) -> LoweredTriggerGraph {
    lower_trigger_graph(
        Vec::new(),
        vec![on_trigger(
            "trg_scripted_dialog",
            vec![TriggerEvent::MapInitialization],
            Vec::new(),
            actions,
        )],
    )
    .expect("lower dialog graph")
}

fn authored_dialog_actions() -> Vec<TriggerAction> {
    vec![
        TriggerAction::CreateDialog {
            title: "Parley?".to_string(),
        },
        TriggerAction::AddDialogButton {
            dialog: 0,
            text: "Yield".to_string(),
            hotkey: u32::from(b'Y'),
        },
        TriggerAction::AddDialogButton {
            dialog: 0,
            text: "Fight".to_string(),
            hotkey: u32::from(b'F'),
        },
        TriggerAction::ShowDialog {
            dialog: 0,
            players: 0b11,
        },
    ]
}

#[test]
fn issue804_map_init_builds_and_shows_dialog_deterministically() {
    let manifest = manifest_with(Some(dialog_graph(authored_dialog_actions())));
    let run = || {
        let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");

        session.apply(SessionOrder::Tick).expect("dialog tick");

        let dialogs = session.hud().dialogs.dialogs();
        assert_eq!(dialogs.len(), 1);
        let dialog = &dialogs[0];
        assert_eq!(dialog.id, 0);
        assert_eq!(dialog.title, "Parley?");
        assert_eq!(dialog.shown_to.bits(), 0b11);
        assert_eq!(dialog.buttons.len(), 2);
        assert_eq!(
            (
                dialog.buttons[0].id,
                dialog.buttons[0].text.as_str(),
                dialog.buttons[0].hotkey,
            ),
            (0, "Yield", u32::from(b'Y'))
        );
        assert_eq!(
            (
                dialog.buttons[1].id,
                dialog.buttons[1].text.as_str(),
                dialog.buttons[1].hotkey,
            ),
            (1, "Fight", u32::from(b'F'))
        );
        assert_eq!(
            session.triggers().fire_count("trg_scripted_dialog"),
            Some(1)
        );
        session.combined_state_hash()
    };
    assert_eq!(run(), run(), "identical dialog actions must hash equally");
}

#[test]
fn issue804_empty_dialog_title_rejected_before_play() {
    let manifest = manifest_with(Some(dialog_graph(vec![TriggerAction::CreateDialog {
        title: "  ".to_string(),
    }])));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("empty dialog title must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("create_dialog title must be non-empty"),
        "unexpected diagnostic: {diagnostic}"
    );
}

#[test]
fn issue804_empty_dialog_button_text_rejected_before_play() {
    let manifest = manifest_with(Some(dialog_graph(vec![
        TriggerAction::CreateDialog {
            title: "Parley?".to_string(),
        },
        TriggerAction::AddDialogButton {
            dialog: 0,
            text: String::new(),
            hotkey: 0,
        },
    ])));
    let error = MatchRuntimeSession::from_manifest(&manifest)
        .err()
        .expect("empty dialog button must reject package");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("add_dialog_button text for dialog 0 must be non-empty"),
        "unexpected diagnostic: {diagnostic}"
    );
}

#[test]
fn issue804_missing_dialog_rejected_when_trigger_fires() {
    let manifest = manifest_with(Some(dialog_graph(vec![TriggerAction::AddDialogButton {
        dialog: 9,
        text: "Yield".to_string(),
        hotkey: 0,
    }])));
    let mut session = MatchRuntimeSession::from_manifest(&manifest).expect("session");
    let error = session
        .apply(SessionOrder::Tick)
        .expect_err("missing runtime dialog must fail loud");
    let diagnostic = error.to_string();
    assert!(
        diagnostic.contains("dialog 9 not found"),
        "unexpected diagnostic: {diagnostic}"
    );
}
