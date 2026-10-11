//! Structural re-validation of edit-from-play ledger sub-records (#1466: extracted
//! from `edit_from_play.rs` to keep the parent module under the 500-line doctrine).
//!
//! Each validator replays the deterministic derivation and compares it against the
//! stored readback, so a stale or tampered ledger is rejected rather than trusted.

use super::*;

pub(super) fn validate_target(
    source_document: &EditorDocument,
    target: &EditFromPlayTargetReadback,
) -> Result<(), EngineError> {
    validate_single_line("target.object_id", &target.target_object_id)?;
    validate_single_line("target.object_kind", &target.object_kind)?;
    validate_single_line("target.source_fingerprint", &target.source_fingerprint)?;
    validate_single_line(
        "target.play_build_fingerprint",
        &target.play_build_fingerprint,
    )?;
    let session = PlaySession::start(source_document, target.camera_rig)?;
    let object = source_document
        .objects
        .get(&target.target_object_id)
        .ok_or_else(|| {
            edit_from_play_error("target.object", "target missing from source document")
        })?;
    let binding = session
        .build
        .spawned_bodies
        .iter()
        .find(|binding| binding.object_id == target.target_object_id)
        .ok_or_else(|| {
            edit_from_play_error("target.body", "target body missing from play build")
        })?;
    if target.play_state != PlaySessionState::Running
        || target.body_id != binding.body_id
        || target.paused_translation != object.transform.translation
        || target.paused_snapshot_hash != session.build.snapshot.state_hash
        || target.source_fingerprint != session.build.source_fingerprint
        || target.play_build_fingerprint != stable_fingerprint(&session.build)?
    {
        return invalid_edit_from_play("target.readback", "paused play target readback is stale");
    }
    Ok(())
}

pub(super) fn validate_focus(
    source_document: &EditorDocument,
    target: &EditFromPlayTargetReadback,
    focus: &EditFromPlayFocusReadback,
) -> Result<(), EngineError> {
    validate_single_line("focus.object_id", &focus.focused_object_id)?;
    focus.camera.validate()?;
    let expected = focus_readback(source_document, target)?;
    if &expected != focus {
        return invalid_edit_from_play("focus.readback", "editor focus does not match play target");
    }
    Ok(())
}

pub(super) fn validate_resume(
    ledger: &EditFromPlayLedger,
    resume: &EditFromPlayResumeReadback,
) -> Result<(), EngineError> {
    validate_single_line("resume.edit_id", &resume.edit_id)?;
    if resume.edit_elapsed_ticks > MAX_EDIT_FROM_PLAY_ELAPSED_TICKS {
        return invalid_edit_from_play(
            "resume.elapsed",
            format!("edit elapsed ticks exceed {MAX_EDIT_FROM_PLAY_ELAPSED_TICKS}"),
        );
    }
    let Some(before_target) = ledger
        .source_document
        .objects
        .get(&ledger.target.target_object_id)
    else {
        return invalid_edit_from_play("resume.before_target", "missing source target");
    };
    let Some(after_target) = ledger
        .current_document
        .objects
        .get(&ledger.target.target_object_id)
    else {
        return invalid_edit_from_play(
            "resume.target_deleted",
            "target was deleted from current document",
        );
    };
    let session = PlaySession::start(&ledger.current_document, ledger.target.camera_rig)?;
    if resume.before_document_fingerprint != stable_fingerprint(&ledger.source_document)?
        || resume.after_document_fingerprint != stable_fingerprint(&ledger.current_document)?
        || resume.resumed_play_build_fingerprint != stable_fingerprint(&session.build)?
        || resume.target_before_translation != before_target.transform.translation
        || resume.target_after_translation != after_target.transform.translation
        || !resume.target_changed
        || !resume.live_change_visible
        || resume.preserved_paused_snapshot_hash != ledger.target.paused_snapshot_hash
    {
        return invalid_edit_from_play("resume.readback", "resume readback is stale");
    }
    Ok(())
}

pub(super) fn validate_events(
    events: &[EditFromPlayEvent],
    ledger: &EditFromPlayLedger,
) -> Result<(), EngineError> {
    for (index, event) in events.iter().enumerate() {
        validate_single_line("event.id", &event.event_id)?;
        validate_single_line("event.actor", &event.actor_id)?;
        validate_single_line("event.target", &event.target_object_id)?;
        validate_single_line("event.code", &event.code)?;
        validate_single_line("event.details", &event.details)?;
        if event.sequence != index + 1
            || event.actor_id != ledger.actor_id
            || event.target_object_id != ledger.target.target_object_id
            || event.code != code_for_action(event.action)
        {
            return invalid_edit_from_play(
                "event.sequence",
                "event identity or sequence is invalid",
            );
        }
    }
    if events[0].action != EditFromPlayAction::PausedAndFocused {
        return invalid_edit_from_play("event.start", "first event must pause and focus");
    }
    if ledger.state == EditFromPlayState::Resumed
        && !events
            .iter()
            .any(|event| event.action == EditFromPlayAction::ResumedAfterEdit)
    {
        return invalid_edit_from_play("event.resume", "resumed ledger needs a resume event");
    }
    Ok(())
}
