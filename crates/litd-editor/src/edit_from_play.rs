use litd_engine::{
    EngineError,
    math::{Aabb, Vec3},
};
use serde::{Deserialize, Serialize};

use crate::{
    history::{EditorDocument, stable_fingerprint},
    play::{PlayCameraRig, PlaySession, PlaySessionState},
    shell::{
        WorldViewCameraState, WorldViewFrameSelection, ed11_bird_eye_default_camera,
        frame_world_view_camera_on_selection,
    },
};

#[path = "edit_from_play_store.rs"]
mod edit_from_play_store;

#[path = "edit_from_play_types.rs"]
mod edit_from_play_types;

#[path = "edit_from_play_validation.rs"]
mod edit_from_play_validation;

pub use edit_from_play_store::{
    EditFromPlayAsterReadback, parse_edit_from_play_ledger, read_edit_from_play_ledger,
    write_edit_from_play_ledger,
};
pub use edit_from_play_types::{
    EditFromPlayAction, EditFromPlayEvent, EditFromPlayFocusReadback, EditFromPlayLedger,
    EditFromPlayReadback, EditFromPlayRequest, EditFromPlayResumeReadback,
    EditFromPlayResumeRequest, EditFromPlayState, EditFromPlayTargetReadback,
};
use edit_from_play_validation::{
    validate_events, validate_focus, validate_resume, validate_target,
};

pub const EDIT_FROM_PLAY_SCHEMA: &str = "litd.edit_from_play.bridge.v1";
pub const EDIT_FROM_PLAY_LEDGER_SCHEMA: &str = "litd.edit_from_play.bridge_ledger.v1";
pub const EDIT_FROM_PLAY_GUI_ROUTE: &str = "egui://editor.project/edit_from_play";
pub const EDIT_FROM_PLAY_PAUSE_TOOL: &str = "litd.editor.play.pause_jump_to_editor";
pub const EDIT_FROM_PLAY_RESUME_TOOL: &str = "litd.editor.play.resume_from_editor_edit";
pub const EDIT_FROM_PLAY_READBACK_TOOL: &str = "litd.editor.play.read_edit_from_play";
pub const MAX_EDIT_FROM_PLAY_ELAPSED_TICKS: u64 = 86_400;

impl EditFromPlayRequest {
    pub fn new(
        ledger_id: impl Into<String>,
        session_id: impl Into<String>,
        actor_id: impl Into<String>,
        target_object_id: impl Into<String>,
        camera_rig: PlayCameraRig,
    ) -> Self {
        Self {
            ledger_id: ledger_id.into(),
            session_id: session_id.into(),
            actor_id: actor_id.into(),
            target_object_id: target_object_id.into(),
            camera_rig,
        }
    }
}

pub fn pause_play_and_jump_to_editor(
    document: &EditorDocument,
    request: EditFromPlayRequest,
) -> Result<EditFromPlayLedger, EngineError> {
    validate_request(&request)?;
    document.validate()?;
    let session = PlaySession::start(document, request.camera_rig)?;
    let target = target_readback(document, &session, &request.target_object_id)?;
    let focus = focus_readback(document, &target)?;
    let events = vec![event(
        &request.session_id,
        1,
        EditFromPlayAction::PausedAndFocused,
        &request.actor_id,
        &request.target_object_id,
        format!(
            "paused play and focused editor selection on {}",
            request.target_object_id
        ),
    )];
    build_ledger(BuildLedgerArgs {
        ledger_id: request.ledger_id,
        session_id: request.session_id,
        actor_id: request.actor_id,
        state: EditFromPlayState::Paused,
        target,
        focus,
        source_document: document.clone(),
        current_document: document.clone(),
        resume: None,
        events,
    })
}

pub fn resume_from_editor_edit(
    ledger: &EditFromPlayLedger,
    request: EditFromPlayResumeRequest,
) -> Result<EditFromPlayLedger, EngineError> {
    validate_edit_from_play_ledger(ledger)?;
    validate_resume_request(&request)?;
    if request.actor_id != ledger.actor_id {
        return invalid_edit_from_play("resume.actor", "resume actor must match paused actor");
    }
    if request.target_object_id != ledger.target.target_object_id {
        return invalid_edit_from_play("resume.target", "resume target must match paused target");
    }
    request.edited_document.validate()?;
    let Some(after_target) = request
        .edited_document
        .objects
        .get(&request.target_object_id)
    else {
        return invalid_edit_from_play(
            "resume.target_deleted",
            format!(
                "target {} was deleted while paused",
                request.target_object_id
            ),
        );
    };
    let before_target = ledger
        .source_document
        .objects
        .get(&request.target_object_id)
        .ok_or_else(|| {
            edit_from_play_error("source.target", "paused target missing from source")
        })?;
    let before_fingerprint = stable_fingerprint(&ledger.source_document)?;
    let after_fingerprint = stable_fingerprint(&request.edited_document)?;
    if before_fingerprint == after_fingerprint {
        return invalid_edit_from_play(
            "resume.edit",
            "edited document must differ from paused source",
        );
    }
    let resumed_session = PlaySession::start(&request.edited_document, ledger.target.camera_rig)?;
    let resumed_build_fingerprint = stable_fingerprint(&resumed_session.build)?;
    let target_changed = before_target.transform.translation != after_target.transform.translation;
    let resume = EditFromPlayResumeReadback {
        edit_id: request.edit_id.clone(),
        edit_elapsed_ticks: request.edit_elapsed_ticks,
        before_document_fingerprint: before_fingerprint,
        after_document_fingerprint: after_fingerprint,
        resumed_play_build_fingerprint: resumed_build_fingerprint,
        target_before_translation: before_target.transform.translation,
        target_after_translation: after_target.transform.translation,
        target_changed,
        live_change_visible: target_changed
            && resumed_session.build.source_fingerprint
                == stable_fingerprint(&request.edited_document)?,
        preserved_paused_snapshot_hash: ledger.target.paused_snapshot_hash,
    };
    if !resume.live_change_visible {
        return invalid_edit_from_play(
            "resume.live_change",
            "resumed play build did not expose the editor target change",
        );
    }
    let mut events = ledger.events.clone();
    let sequence = events.len() + 1;
    events.push(event(
        &ledger.session_id,
        sequence,
        EditFromPlayAction::ResumedAfterEdit,
        &ledger.actor_id,
        &ledger.target.target_object_id,
        format!(
            "resumed play after edit {} with live target change",
            request.edit_id
        ),
    ));
    build_ledger(BuildLedgerArgs {
        ledger_id: ledger.ledger_id.clone(),
        session_id: ledger.session_id.clone(),
        actor_id: ledger.actor_id.clone(),
        state: EditFromPlayState::Resumed,
        target: ledger.target.clone(),
        focus: ledger.focus.clone(),
        source_document: ledger.source_document.clone(),
        current_document: request.edited_document,
        resume: Some(resume),
        events,
    })
}

pub fn validate_edit_from_play_ledger(ledger: &EditFromPlayLedger) -> Result<(), EngineError> {
    if ledger.schema != EDIT_FROM_PLAY_LEDGER_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.session_id.trim().is_empty()
        || ledger.actor_id.trim().is_empty()
        || ledger.gui_route != EDIT_FROM_PLAY_GUI_ROUTE
        || ledger.pause_tool_name != EDIT_FROM_PLAY_PAUSE_TOOL
        || ledger.resume_tool_name != EDIT_FROM_PLAY_RESUME_TOOL
        || ledger.readback_tool_name != EDIT_FROM_PLAY_READBACK_TOOL
        || ledger.event_count != ledger.events.len()
        || ledger.events.is_empty()
    {
        return invalid_edit_from_play("ledger", "required ledger fields are invalid");
    }
    validate_single_line("ledger.ledger_id", &ledger.ledger_id)?;
    validate_single_line("ledger.session_id", &ledger.session_id)?;
    validate_single_line("ledger.actor_id", &ledger.actor_id)?;
    ledger.source_document.validate()?;
    ledger.current_document.validate()?;
    validate_target(&ledger.source_document, &ledger.target)?;
    validate_focus(&ledger.source_document, &ledger.target, &ledger.focus)?;
    validate_events(&ledger.events, ledger)?;
    match ledger.state {
        EditFromPlayState::Paused => {
            if ledger.resume.is_some()
                || stable_fingerprint(&ledger.source_document)?
                    != stable_fingerprint(&ledger.current_document)?
            {
                return invalid_edit_from_play(
                    "ledger.paused",
                    "paused ledger must not carry an edit",
                );
            }
        }
        EditFromPlayState::Resumed => {
            let resume = ledger
                .resume
                .as_ref()
                .ok_or_else(|| edit_from_play_error("ledger.resume", "missing resume readback"))?;
            validate_resume(ledger, resume)?;
        }
    }
    let expected = build_readback(
        &ledger.session_id,
        ledger.state,
        &ledger.target,
        &ledger.focus,
        &ledger.source_document,
        &ledger.current_document,
        ledger.resume.as_ref(),
    )?;
    if expected != ledger.readback {
        return invalid_edit_from_play(
            "ledger.readback",
            "stored readback does not match replayed edit-from-play state",
        );
    }
    Ok(())
}

struct BuildLedgerArgs {
    ledger_id: String,
    session_id: String,
    actor_id: String,
    state: EditFromPlayState,
    target: EditFromPlayTargetReadback,
    focus: EditFromPlayFocusReadback,
    source_document: EditorDocument,
    current_document: EditorDocument,
    resume: Option<EditFromPlayResumeReadback>,
    events: Vec<EditFromPlayEvent>,
}

fn build_ledger(args: BuildLedgerArgs) -> Result<EditFromPlayLedger, EngineError> {
    let BuildLedgerArgs {
        ledger_id,
        session_id,
        actor_id,
        state,
        target,
        focus,
        source_document,
        current_document,
        resume,
        events,
    } = args;
    let readback = build_readback(
        &session_id,
        state,
        &target,
        &focus,
        &source_document,
        &current_document,
        resume.as_ref(),
    )?;
    let ledger = EditFromPlayLedger {
        schema: EDIT_FROM_PLAY_LEDGER_SCHEMA.to_string(),
        ledger_id,
        session_id,
        actor_id,
        gui_route: EDIT_FROM_PLAY_GUI_ROUTE.to_string(),
        pause_tool_name: EDIT_FROM_PLAY_PAUSE_TOOL.to_string(),
        resume_tool_name: EDIT_FROM_PLAY_RESUME_TOOL.to_string(),
        readback_tool_name: EDIT_FROM_PLAY_READBACK_TOOL.to_string(),
        state,
        target,
        focus,
        source_document,
        current_document,
        resume,
        event_count: events.len(),
        events,
        readback,
    };
    validate_edit_from_play_ledger(&ledger)?;
    Ok(ledger)
}

fn target_readback(
    document: &EditorDocument,
    session: &PlaySession,
    target_object_id: &str,
) -> Result<EditFromPlayTargetReadback, EngineError> {
    let object = document.objects.get(target_object_id).ok_or_else(|| {
        edit_from_play_error("target", format!("missing target {target_object_id}"))
    })?;
    let binding = session
        .build
        .spawned_bodies
        .iter()
        .find(|binding| binding.object_id == target_object_id)
        .ok_or_else(|| {
            edit_from_play_error(
                "target.body",
                format!("target {target_object_id} is not a spawned play body"),
            )
        })?;
    Ok(EditFromPlayTargetReadback {
        target_object_id: target_object_id.to_string(),
        body_id: binding.body_id,
        object_kind: object.kind.clone(),
        camera_rig: session.build.camera_rig,
        play_state: session.state,
        paused_translation: object.transform.translation,
        paused_snapshot_hash: session.build.snapshot.state_hash,
        source_fingerprint: session.build.source_fingerprint.clone(),
        play_build_fingerprint: stable_fingerprint(&session.build)?,
    })
}

fn focus_readback(
    document: &EditorDocument,
    target: &EditFromPlayTargetReadback,
) -> Result<EditFromPlayFocusReadback, EngineError> {
    let object = document
        .objects
        .get(&target.target_object_id)
        .ok_or_else(|| {
            edit_from_play_error("focus.target", "focused target missing from document")
        })?;
    let bounds = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(128.0))?;
    let camera = frame_world_view_camera_on_selection(
        ed11_bird_eye_default_camera(),
        bounds,
        WorldViewFrameSelection {
            selected_entities: 1,
            pivot_world: object.transform.translation,
            radius: 8.0,
        },
    )?;
    Ok(EditFromPlayFocusReadback {
        focused_object_id: target.target_object_id.clone(),
        selected_object_ids: vec![target.target_object_id.clone()],
        camera,
        camera_target_matches_object: camera.target_world == object.transform.translation,
        source_document_fingerprint: stable_fingerprint(document)?,
    })
}

fn build_readback(
    session_id: &str,
    state: EditFromPlayState,
    target: &EditFromPlayTargetReadback,
    focus: &EditFromPlayFocusReadback,
    source_document: &EditorDocument,
    current_document: &EditorDocument,
    resume: Option<&EditFromPlayResumeReadback>,
) -> Result<EditFromPlayReadback, EngineError> {
    let source_document_fingerprint = stable_fingerprint(source_document)?;
    let current_document_fingerprint = stable_fingerprint(current_document)?;
    let live_change_visible = resume
        .map(|value| value.live_change_visible)
        .unwrap_or(false);
    // No resume readback means the play->editor->play round trip has not been
    // exercised, so preservation is UNVERIFIED, not proven. Defaulting to true made
    // the flag vacuously green and let assertions on it pass without a real round trip
    // (#1489, #800 §4). Report false until a resume actually confirms the hash.
    let round_trip_preserves_play_state = resume
        .map(|value| value.preserved_paused_snapshot_hash == target.paused_snapshot_hash)
        .unwrap_or(false)
        && focus.selected_object_ids == [target.target_object_id.clone()];
    Ok(EditFromPlayReadback {
        schema: EDIT_FROM_PLAY_SCHEMA.to_string(),
        session_id: session_id.to_string(),
        state,
        target_object_id: target.target_object_id.clone(),
        editor_focus_matches_play_target: focus.focused_object_id == target.target_object_id
            && focus.camera_target_matches_object
            && focus.selected_object_ids == [target.target_object_id.clone()],
        selected_object_ids: focus.selected_object_ids.clone(),
        source_document_fingerprint,
        current_document_fingerprint,
        paused_snapshot_hash: target.paused_snapshot_hash,
        resumed_play_build_fingerprint: resume
            .map(|value| value.resumed_play_build_fingerprint.clone()),
        live_change_visible,
        round_trip_preserves_play_state,
        edit_elapsed_ticks: resume.map(|value| value.edit_elapsed_ticks),
        readback_tool_name: EDIT_FROM_PLAY_READBACK_TOOL.to_string(),
    })
}

fn validate_request(request: &EditFromPlayRequest) -> Result<(), EngineError> {
    validate_single_line("request.ledger_id", &request.ledger_id)?;
    validate_single_line("request.session_id", &request.session_id)?;
    validate_single_line("request.actor_id", &request.actor_id)?;
    validate_single_line("request.target_object_id", &request.target_object_id)
}

fn validate_resume_request(request: &EditFromPlayResumeRequest) -> Result<(), EngineError> {
    validate_single_line("resume.actor_id", &request.actor_id)?;
    validate_single_line("resume.edit_id", &request.edit_id)?;
    validate_single_line("resume.target_object_id", &request.target_object_id)?;
    if request.edit_elapsed_ticks > MAX_EDIT_FROM_PLAY_ELAPSED_TICKS {
        return invalid_edit_from_play(
            "resume.elapsed",
            format!("edit elapsed ticks exceed {MAX_EDIT_FROM_PLAY_ELAPSED_TICKS}"),
        );
    }
    Ok(())
}

fn event(
    session_id: &str,
    sequence: usize,
    action: EditFromPlayAction,
    actor_id: &str,
    target_object_id: &str,
    details: String,
) -> EditFromPlayEvent {
    EditFromPlayEvent {
        event_id: format!(
            "{session_id}.event.{sequence:04}.{}",
            code_for_action(action).to_ascii_lowercase()
        ),
        sequence,
        action,
        actor_id: actor_id.to_string(),
        target_object_id: target_object_id.to_string(),
        code: code_for_action(action).to_string(),
        details,
    }
}

fn code_for_action(action: EditFromPlayAction) -> &'static str {
    match action {
        EditFromPlayAction::PausedAndFocused => "LITD-EDIT-FROM-PLAY-PAUSED-FOCUSED",
        EditFromPlayAction::ResumedAfterEdit => "LITD-EDIT-FROM-PLAY-RESUMED",
    }
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_edit_from_play(field, "value must be non-empty and single-line");
    }
    Ok(())
}

fn invalid_edit_from_play<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(edit_from_play_error(field, reason))
}

fn edit_from_play_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let field = format!("editor.edit_from_play.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-EDIT-FROM-PLAY-VALIDATION",
        "edit-from-play bridge rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

#[cfg(test)]
#[path = "edit_from_play_tests.rs"]
mod tests;
