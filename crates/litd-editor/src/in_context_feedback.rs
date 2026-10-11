use litd_engine::EngineError;

use crate::history::EditorDocument;
#[cfg(test)]
use crate::play::PlayCameraRig;
use crate::play::PlaySession;

#[path = "in_context_feedback_builders.rs"]
mod in_context_feedback_builders;
#[path = "in_context_feedback_semantics.rs"]
mod in_context_feedback_semantics;
#[path = "in_context_feedback_store.rs"]
mod in_context_feedback_store;
#[path = "in_context_feedback_types.rs"]
mod in_context_feedback_types;
#[path = "in_context_feedback_validation.rs"]
mod in_context_feedback_validation;

pub use in_context_feedback_store::{
    ContextFeedbackAsterReadback, parse_context_feedback_ledger, read_context_feedback_ledger,
    write_context_feedback_ledger,
};
pub use in_context_feedback_types::*;
pub use in_context_feedback_validation::validate_context_feedback_ledger;

use in_context_feedback_builders::{build_feedback_record, build_ledger, session_snapshot};
use in_context_feedback_validation::{feedback_records, validate_session_request};

pub const IN_CONTEXT_FEEDBACK_SCHEMA: &str = "litd.in_context_feedback.readback.v1";
pub const IN_CONTEXT_FEEDBACK_LEDGER_SCHEMA: &str = "litd.in_context_feedback.ledger.v1";
pub const IN_CONTEXT_FEEDBACK_GUI_ROUTE: &str = "egui://editor.project/context_feedback";
pub const IN_CONTEXT_FEEDBACK_AGENT_ROUTE: &str = "mcp://litd.editor.agents/context_feedback";
pub const IN_CONTEXT_FEEDBACK_CAPTURE_TOOL: &str = "litd.editor.play.capture_context_feedback";
pub const IN_CONTEXT_FEEDBACK_READBACK_TOOL: &str = "litd.editor.play.read_context_feedback";

pub fn start_context_feedback_session(
    document: &EditorDocument,
    request: ContextFeedbackSessionRequest,
) -> Result<ContextFeedbackLedger, EngineError> {
    validate_session_request(&request)?;
    document.validate()?;
    let session = PlaySession::start(document, request.camera_rig)?;
    let snapshot = session_snapshot(&session)?;
    let event = ContextFeedbackEvent {
        event_id: event_id(
            &request.session_id,
            1,
            ContextFeedbackAction::SessionStarted,
        ),
        sequence: 1,
        action: ContextFeedbackAction::SessionStarted,
        actor_id: request.actor_id.clone(),
        code: code_for_action(ContextFeedbackAction::SessionStarted).to_string(),
        details: "in-context feedback session started from live play state".to_string(),
        feedback: None,
    };
    build_ledger(request, document.clone(), snapshot, vec![event])
}

pub fn capture_context_feedback(
    ledger: &ContextFeedbackLedger,
    request: ContextFeedbackCaptureRequest,
) -> Result<ContextFeedbackLedger, EngineError> {
    validate_context_feedback_ledger(ledger)?;
    if ledger.readback.feedback_count >= ledger.session.max_feedback_count {
        return invalid_context_feedback(
            "feedback.limit",
            format!(
                "feedback count {} reached configured limit {}",
                ledger.readback.feedback_count, ledger.session.max_feedback_count
            ),
        );
    }
    let existing = feedback_records(ledger);
    let feedback = build_feedback_record(ledger, &request, &existing)?;
    let mut events = ledger.events.clone();
    let sequence = events.len() + 1;
    events.push(ContextFeedbackEvent {
        event_id: event_id(
            &ledger.session.session_id,
            sequence,
            ContextFeedbackAction::FeedbackCaptured,
        ),
        sequence,
        action: ContextFeedbackAction::FeedbackCaptured,
        actor_id: request.actor_id,
        code: code_for_action(ContextFeedbackAction::FeedbackCaptured).to_string(),
        details: format!("captured in-context feedback {}", request.feedback_id),
        feedback: Some(feedback),
    });
    build_ledger(
        ledger.session.clone(),
        ledger.source_document.clone(),
        ledger.session_snapshot.clone(),
        events,
    )
}

pub fn readback_context_feedback_session(
    ledger: &ContextFeedbackLedger,
) -> Result<ContextFeedbackReadback, EngineError> {
    validate_context_feedback_ledger(ledger)?;
    Ok(ledger.readback.clone())
}

fn screenshot_blob_manifest_ok(screenshot: &ContextFeedbackScreenshotEvidence) -> bool {
    screenshot.blob.sha256 == screenshot.sha256
        && screenshot.blob.total_bytes == screenshot.size_bytes
        && screenshot.blob.chunk_size_bytes > 0
        && screenshot.blob.chunk_count > 0
        && screenshot
            .blob
            .chunk_row_prefix
            .starts_with("play/context_feedback/screenshot/")
}

pub(crate) fn screenshot_blob_ref(
    sha256: &str,
    total_bytes: usize,
) -> ContextFeedbackScreenshotBlobRef {
    const CHUNK_SIZE_BYTES: usize = 256 * 1024;
    ContextFeedbackScreenshotBlobRef {
        blob_id: format!("screenshot://sha256/{sha256}"),
        chunk_row_prefix: format!("play/context_feedback/screenshot/{sha256}/chunk/"),
        chunk_count: total_bytes.div_ceil(CHUNK_SIZE_BYTES),
        chunk_size_bytes: CHUNK_SIZE_BYTES,
        total_bytes,
        sha256: sha256.to_string(),
    }
}

fn validate_sha256(field: &str, value: &str) -> Result<(), EngineError> {
    if !litd_hash::is_sha256_hex(value) {
        return invalid_context_feedback(field, "sha256 must be 64 lowercase hex characters");
    }
    Ok(())
}

fn validate_traceparent(value: &str) -> Result<(), EngineError> {
    let parts = value.split('-').collect::<Vec<_>>();
    if parts.len() != 4
        || parts[0] != "00"
        || !is_lower_hex(parts[1], 32)
        || !is_lower_hex(parts[2], 16)
        || !is_lower_hex(parts[3], 2)
        || parts[1].chars().all(|ch| ch == '0')
        || parts[2].chars().all(|ch| ch == '0')
    {
        return invalid_context_feedback(
            "traceparent",
            "traceparent must be W3C version 00 with non-zero trace and span ids",
        );
    }
    Ok(())
}

fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn event_id(session_id: &str, sequence: usize, action: ContextFeedbackAction) -> String {
    format!(
        "{session_id}.event.{sequence:04}.{}",
        code_for_action(action).to_ascii_lowercase()
    )
}

fn code_for_action(action: ContextFeedbackAction) -> &'static str {
    match action {
        ContextFeedbackAction::SessionStarted => "LITD-CONTEXT-FEEDBACK-SESSION-STARTED",
        ContextFeedbackAction::FeedbackCaptured => "LITD-CONTEXT-FEEDBACK-CAPTURED",
    }
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_context_feedback(field, "value must be non-empty and single-line");
    }
    Ok(())
}

fn invalid_context_feedback<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(context_feedback_error(field, reason))
}

fn context_feedback_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let field = format!("editor.context_feedback.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-CONTEXT-FEEDBACK-VALIDATION",
        "context feedback rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

#[cfg(test)]
#[path = "in_context_feedback_tests.rs"]
mod tests;
