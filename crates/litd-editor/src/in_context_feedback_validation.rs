use std::collections::BTreeSet;

use litd_engine::EngineError;

use crate::play::PlaySessionState;
use crate::world_memory::validate_world_memory_index;

use super::in_context_feedback_builders::build_readback;
use super::in_context_feedback_semantics::feedback_index;
use super::*;

pub fn validate_context_feedback_ledger(ledger: &ContextFeedbackLedger) -> Result<(), EngineError> {
    validate_session_request(&ledger.session)?;
    ledger.source_document.validate()?;
    validate_session_snapshot(&ledger.session_snapshot)?;
    if ledger.schema != IN_CONTEXT_FEEDBACK_LEDGER_SCHEMA
        || ledger.ledger_id != ledger.session.ledger_id
        || ledger.gui_route != IN_CONTEXT_FEEDBACK_GUI_ROUTE
        || ledger.agent_route != IN_CONTEXT_FEEDBACK_AGENT_ROUTE
        || ledger.capture_tool_name != IN_CONTEXT_FEEDBACK_CAPTURE_TOOL
        || ledger.readback_tool_name != IN_CONTEXT_FEEDBACK_READBACK_TOOL
        || ledger.event_count != ledger.events.len()
        || ledger.events.is_empty()
    {
        return invalid_context_feedback("ledger", "required ledger fields are invalid");
    }
    validate_single_line("ledger.ledger_id", &ledger.ledger_id)?;
    let mut seen = BTreeSet::new();
    for (index, event) in ledger.events.iter().enumerate() {
        validate_event(event, ledger)?;
        if event.sequence != index + 1 || !seen.insert(event.event_id.clone()) {
            return invalid_context_feedback(
                "event.sequence",
                "events must be unique and contiguous",
            );
        }
    }
    if ledger.events[0].action != ContextFeedbackAction::SessionStarted {
        return invalid_context_feedback("event.start", "first event must start the session");
    }
    let expected = build_readback(
        &ledger.session,
        &ledger.source_document,
        &ledger.session_snapshot,
        &ledger.events,
    )?;
    if expected != ledger.readback {
        return invalid_context_feedback(
            "ledger.readback",
            "stored readback does not match replayed context-feedback state",
        );
    }
    Ok(())
}

fn validate_event(
    event: &ContextFeedbackEvent,
    ledger: &ContextFeedbackLedger,
) -> Result<(), EngineError> {
    validate_single_line("event.id", &event.event_id)?;
    validate_single_line("event.actor", &event.actor_id)?;
    validate_single_line("event.code", &event.code)?;
    validate_single_line("event.details", &event.details)?;
    if event.sequence == 0 || event.code != code_for_action(event.action) {
        return invalid_context_feedback("event", "event sequence and code must match action");
    }
    match event.action {
        ContextFeedbackAction::SessionStarted => {
            if event.feedback.is_some() || event.actor_id != ledger.session.actor_id {
                return invalid_context_feedback("event.start", "start event payload is invalid");
            }
        }
        ContextFeedbackAction::FeedbackCaptured => {
            let record = event.feedback.as_ref().ok_or_else(|| {
                context_feedback_error("event.feedback", "feedback event needs a record")
            })?;
            if event.actor_id != record.actor_id {
                return invalid_context_feedback("event.feedback", "event actor mismatch");
            }
            validate_feedback_record(record)?;
        }
    }
    Ok(())
}

pub(super) fn validate_feedback_record(record: &ContextFeedbackRecord) -> Result<(), EngineError> {
    validate_single_line("feedback.id", &record.feedback_id)?;
    validate_single_line("feedback.actor", &record.actor_id)?;
    validate_single_line("feedback.severity", &record.severity)?;
    validate_annotation(&record.annotation)?;
    validate_screenshot_evidence(&record.screenshot)?;
    if record.world_state.target_object_id != record.annotation.target.selector.marker_object_id
        || record.world_state.source_document_fingerprint
            != record.annotation.target.state.source_document_fingerprint
        || record.repro.play_session_fingerprint
            != record.annotation.target.state.play_session_fingerprint
        || record.editor_route != IN_CONTEXT_FEEDBACK_GUI_ROUTE
        || record.agent_route != IN_CONTEXT_FEEDBACK_AGENT_ROUTE
        || !record.sextant.self_found
        || record.sextant.self_constellation_id != record.constellation.constellation_id
        || record.sextant.self_hit_rank == 0
        || record.loom.source_feedback_id != record.feedback_id
    {
        return invalid_context_feedback(
            "feedback.record",
            "feedback record readbacks are inconsistent",
        );
    }
    validate_world_memory_index(&feedback_index(
        &record.constellation.world_id,
        vec![record.constellation.clone()],
    )?)?;
    Ok(())
}

fn validate_annotation(annotation: &ContextFeedbackAnnotation) -> Result<(), EngineError> {
    validate_single_line("annotation.id", &annotation.annotation_id)?;
    validate_single_line("annotation.motivation", &annotation.motivation)?;
    validate_single_line("annotation.body", &annotation.body.value)?;
    validate_single_line("annotation.target", &annotation.target.source)?;
    validate_single_line(
        "annotation.selector.target",
        &annotation.target.selector.marker_object_id,
    )?;
    validate_single_line(
        "annotation.selector.label",
        &annotation.target.selector.marker_label,
    )?;
    if annotation.motivation != "commenting"
        || annotation.body.purpose != "commenting"
        || annotation.body.format != "text/plain"
        || annotation.target.selector.coordinate_space != "world"
    {
        return invalid_context_feedback("annotation", "annotation body/target semantics invalid");
    }
    Ok(())
}

fn validate_screenshot_evidence(
    screenshot: &ContextFeedbackScreenshotEvidence,
) -> Result<(), EngineError> {
    validate_single_line("screenshot.path", &screenshot.source_path)?;
    validate_single_line("screenshot.media_type", &screenshot.media_type)?;
    validate_sha256("screenshot.sha256", &screenshot.sha256)?;
    if screenshot.media_type != "image/png" || !screenshot_blob_manifest_ok(screenshot) {
        return invalid_context_feedback(
            "screenshot.blob",
            "screenshot blob manifest must be chunked and match sha256",
        );
    }
    Ok(())
}

pub(super) fn validate_session_request(
    request: &ContextFeedbackSessionRequest,
) -> Result<(), EngineError> {
    validate_single_line("request.ledger_id", &request.ledger_id)?;
    validate_single_line("request.session_id", &request.session_id)?;
    validate_single_line("request.project_id", &request.project_id)?;
    validate_single_line("request.actor_id", &request.actor_id)?;
    validate_single_line("request.correlation_id", &request.correlation_id)?;
    validate_traceparent(&request.traceparent)?;
    if request.max_feedback_count == 0 {
        return invalid_context_feedback("request.max_feedback_count", "limit must be non-zero");
    }
    Ok(())
}

pub(super) fn validate_capture_request(
    request: &ContextFeedbackCaptureRequest,
) -> Result<(), EngineError> {
    validate_single_line("feedback.id", &request.feedback_id)?;
    validate_single_line("feedback.actor", &request.actor_id)?;
    validate_single_line("feedback.note", &request.note)?;
    validate_single_line("feedback.severity", &request.severity)?;
    validate_single_line("marker.target", &request.marker.target_object_id)?;
    validate_single_line("marker.label", &request.marker.label)?;
    validate_screenshot_input(&request.screenshot)
}

pub(super) fn validate_screenshot_input(
    input: &ContextFeedbackScreenshotInput,
) -> Result<(), EngineError> {
    validate_single_line("screenshot.path", &input.path)?;
    validate_single_line("screenshot.media_type", &input.media_type)?;
    validate_sha256("screenshot.expected_sha256", &input.expected_sha256)?;
    if input.media_type != "image/png" {
        return invalid_context_feedback("screenshot.media_type", "only image/png is accepted");
    }
    Ok(())
}

fn validate_session_snapshot(snapshot: &ContextFeedbackSessionSnapshot) -> Result<(), EngineError> {
    validate_single_line("snapshot.document_id", &snapshot.calyx_document_id)?;
    validate_single_line("snapshot.source_revision", &snapshot.source_revision_id)?;
    validate_single_line("snapshot.source_fingerprint", &snapshot.source_fingerprint)?;
    validate_single_line(
        "snapshot.session_fingerprint",
        &snapshot.play_session_fingerprint,
    )?;
    validate_single_line(
        "snapshot.build_fingerprint",
        &snapshot.play_build_fingerprint,
    )?;
    if snapshot.play_session_state != PlaySessionState::Running
        || snapshot.spawned_bodies == 0
        || snapshot.visible_instances == 0
        || snapshot.draw_batches == 0
    {
        return invalid_context_feedback(
            "snapshot",
            "context-feedback play snapshot must be running and renderable",
        );
    }
    Ok(())
}

pub(super) fn feedback_records(ledger: &ContextFeedbackLedger) -> Vec<&ContextFeedbackRecord> {
    ledger
        .events
        .iter()
        .filter_map(|event| event.feedback.as_ref())
        .collect()
}
