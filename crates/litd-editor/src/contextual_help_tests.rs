use crate::{
    contextual_help::{CONTEXTUAL_HELP_SCHEMA, ContextualHelpRequest, resolve_contextual_help},
    editor_commands::{COMMAND_MODE_CINEMATICS, COMMAND_MODE_OBJECTS, COMMAND_TOGGLE_PLAYTEST},
    history::stable_fingerprint,
    shell::EditorMode,
    template_gallery::{
        MELEE_SKIRMISH_TEMPLATE, STORY_CINEMATIC_TEMPLATE, build_template_document,
    },
};

#[test]
fn issue898_one_help_path_grounds_story_answer_in_real_project_and_selection() {
    let document = build_template_document(STORY_CINEMATIC_TEMPLATE).unwrap();
    let answer = resolve_contextual_help(
        &document,
        ContextualHelpRequest {
            query: "How do I change the next story beat?".to_string(),
            active_mode: EditorMode::Objects,
            selected_id: Some("cine.intro".to_string()),
        },
    )
    .unwrap();

    assert_eq!(answer.schema, CONTEXTUAL_HELP_SCHEMA);
    assert_eq!(answer.source_issue, 900);
    assert_eq!(
        answer.context.document_fingerprint,
        stable_fingerprint(&document).unwrap()
    );
    assert_eq!(answer.recommendation.command_id, COMMAND_MODE_CINEMATICS);
    assert_eq!(answer.recommendation.target_id, "cine.intro");
    assert_eq!(answer.recommendation.target_kind, "cinematic");
    assert!(
        answer
            .recommendation
            .gui_route
            .starts_with("egui://editor.commands/")
    );
}

#[test]
fn issue898_one_help_path_routes_live_preview_to_the_real_play_command() {
    let document = build_template_document(MELEE_SKIRMISH_TEMPLATE).unwrap();
    let answer = resolve_contextual_help(
        &document,
        ContextualHelpRequest {
            query: "How do I see it live?".to_string(),
            active_mode: EditorMode::Terrain,
            selected_id: None,
        },
    )
    .unwrap();

    assert_eq!(answer.recommendation.command_id, COMMAND_TOGGLE_PLAYTEST);
    assert_eq!(answer.recommendation.target_kind, "project");
    assert_eq!(answer.recommendation.target_id, document.calyx_document_id);
}

#[test]
fn issue898_one_help_path_uses_current_selection_for_an_ambiguous_question() {
    let document = build_template_document(MELEE_SKIRMISH_TEMPLATE).unwrap();
    let answer = resolve_contextual_help(
        &document,
        ContextualHelpRequest {
            query: "How do I improve this?".to_string(),
            active_mode: EditorMode::Terrain,
            selected_id: Some("start.player_1".to_string()),
        },
    )
    .unwrap();

    assert_eq!(answer.recommendation.command_id, COMMAND_MODE_OBJECTS);
    assert_eq!(answer.recommendation.target_id, "start.player_1");
    assert_eq!(answer.recommendation.target_kind, "object");
}

#[test]
fn issue898_one_help_path_fails_loud_for_empty_query_or_unknown_selection() {
    let document = build_template_document(MELEE_SKIRMISH_TEMPLATE).unwrap();
    let empty_error = resolve_contextual_help(
        &document,
        ContextualHelpRequest {
            query: " ".to_string(),
            active_mode: EditorMode::Terrain,
            selected_id: None,
        },
    )
    .unwrap_err();
    assert!(
        empty_error
            .to_string()
            .contains("editor.contextual_help.query")
    );

    let selection_error = resolve_contextual_help(
        &document,
        ContextualHelpRequest {
            query: "How do I edit this?".to_string(),
            active_mode: EditorMode::Objects,
            selected_id: Some("missing.selection".to_string()),
        },
    )
    .unwrap_err();
    assert!(
        selection_error
            .to_string()
            .contains("editor.contextual_help.selected_id")
    );
}
