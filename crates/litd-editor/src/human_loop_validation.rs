use std::collections::BTreeSet;

use litd_engine::EngineError;

use crate::play::PlaySessionState;

use super::*;

pub fn validate_human_loop_ledger(ledger: &HumanLoopLedger) -> Result<(), EngineError> {
    validate_config(&ledger.config)?;
    if ledger.schema != HUMAN_LOOP_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.event_count != ledger.events.len()
        || ledger.events.len() < 2
    {
        return invalid_human_loop("ledger", "schema, id, and event count must be valid");
    }
    validate_single_line("ledger.ledger_id", &ledger.ledger_id)?;
    let mut seen = BTreeSet::new();
    for (index, event) in ledger.events.iter().enumerate() {
        validate_event(event, &ledger.config)?;
        if !seen.insert(event.event_id.clone()) {
            return invalid_human_loop("ledger.events", "duplicate event id");
        }
        if event.sequence != index + 1 {
            return invalid_human_loop(
                "ledger.events.sequence",
                "event sequence must be contiguous",
            );
        }
    }
    let replayed = replay_readback(&ledger.config, &ledger.events)?;
    if replayed != ledger.readback {
        return invalid_human_loop(
            "ledger.readback",
            "stored readback must exactly match replayed event state",
        );
    }
    Ok(())
}

pub(super) fn validate_launch_request(request: &HumanLoopLaunchRequest) -> Result<(), EngineError> {
    validate_single_line("request.ledger_id", &request.ledger_id)?;
    validate_single_line("request.actor_id", &request.actor_id)?;
    validate_config(&request.config)
}

pub(super) fn validate_config(config: &HumanLoopSessionConfig) -> Result<(), EngineError> {
    validate_single_line("config.session_id", &config.session_id)?;
    validate_single_line("config.project_id", &config.project_id)?;
    validate_single_line("config.owner_actor_id", &config.owner_actor_id)?;
    validate_single_line("config.correlation_id", &config.correlation_id)?;
    validate_traceparent(&config.traceparent)?;
    if config.max_human_feedback == 0 {
        return invalid_human_loop("config.max_human_feedback", "limit must be non-zero");
    }
    if config.gui_route != HUMAN_LOOP_GUI_ROUTE
        || config.mcp_launch_tool_name != HUMAN_LOOP_LAUNCH_TOOL
        || config.mcp_feedback_tool_name != HUMAN_LOOP_FEEDBACK_TOOL
        || config.readback_tool_name != HUMAN_LOOP_READBACK_TOOL
    {
        return invalid_human_loop(
            "config.routes",
            "human-loop GUI route and MCP tool names must match the shared capability surface",
        );
    }
    if config.permissions.is_empty() {
        return invalid_human_loop("config.permissions", "at least one permission is required");
    }
    let mut seen = BTreeSet::new();
    let mut owner_seen = false;
    let mut inspect_seen = false;
    for permission in &config.permissions {
        validate_permission(permission)?;
        if !seen.insert(permission.actor_id.clone()) {
            return invalid_human_loop("config.permissions", "duplicate actor permission");
        }
        owner_seen |= permission.actor_id == config.owner_actor_id;
        inspect_seen |= permission.can_inspect;
    }
    if !owner_seen {
        return invalid_human_loop(
            "config.owner_actor_id",
            "owner actor must have an explicit permission row",
        );
    }
    if !inspect_seen {
        return invalid_human_loop(
            "config.permissions.inspect",
            "at least one actor must be allowed to inspect readback",
        );
    }
    Ok(())
}

fn validate_permission(permission: &HumanLoopPermission) -> Result<(), EngineError> {
    validate_single_line("permission.actor_id", &permission.actor_id)?;
    if permission.actor_kind == HumanLoopActorKind::Human && permission.can_record_agent_verdict {
        return invalid_human_loop(
            "permission.actor_kind",
            "human actors cannot record agent verdicts",
        );
    }
    if permission.actor_kind == HumanLoopActorKind::Agent && permission.can_submit_feedback {
        return invalid_human_loop(
            "permission.actor_kind",
            "agent actors cannot submit human feedback",
        );
    }
    Ok(())
}

pub(super) fn validate_event(
    event: &HumanLoopEvent,
    config: &HumanLoopSessionConfig,
) -> Result<(), EngineError> {
    validate_single_line("event.event_id", &event.event_id)?;
    validate_single_line("event.actor_id", &event.actor_id)?;
    validate_single_line("event.code", &event.code)?;
    validate_single_line("event.details", &event.details)?;
    if event.sequence == 0 {
        return invalid_human_loop("event.sequence", "sequence must be non-zero");
    }
    if event.code != code_for_action(event.action) {
        return invalid_human_loop("event.code", "event code must match action");
    }
    require_known_actor(config, &event.actor_id)?;
    match event.action {
        HumanLoopAction::Configured => {
            if event.play_snapshot.is_some()
                || event.feedback.is_some()
                || event.agent_verdict.is_some()
            {
                return invalid_human_loop(
                    "event.configured",
                    "configured events cannot carry payloads",
                );
            }
        }
        HumanLoopAction::Launched => {
            let snapshot = event.play_snapshot.as_ref().ok_or_else(|| {
                human_loop_error("event.launched", "launch event requires snapshot")
            })?;
            validate_play_snapshot(snapshot)?;
            if event.feedback.is_some() || event.agent_verdict.is_some() {
                return invalid_human_loop("event.launched", "launch event payload is invalid");
            }
        }
        HumanLoopAction::FeedbackCaptured => {
            let feedback = event.feedback.as_ref().ok_or_else(|| {
                human_loop_error("event.feedback", "feedback event requires feedback payload")
            })?;
            validate_feedback(feedback)?;
            if feedback.actor_id != event.actor_id || feedback.session_id != config.session_id {
                return invalid_human_loop("event.feedback", "feedback actor/session mismatch");
            }
            if event.play_snapshot.is_some() || event.agent_verdict.is_some() {
                return invalid_human_loop("event.feedback", "feedback event payload is invalid");
            }
        }
        HumanLoopAction::AgentVerdictRecorded => {
            let verdict = event.agent_verdict.as_ref().ok_or_else(|| {
                human_loop_error(
                    "event.agent_verdict",
                    "verdict event requires verdict payload",
                )
            })?;
            validate_agent_verdict(verdict)?;
            if verdict.actor_id != event.actor_id || verdict.session_id != config.session_id {
                return invalid_human_loop("event.agent_verdict", "verdict actor/session mismatch");
            }
            if event.play_snapshot.is_some() || event.feedback.is_some() {
                return invalid_human_loop(
                    "event.agent_verdict",
                    "verdict event payload is invalid",
                );
            }
        }
    }
    Ok(())
}

fn validate_play_snapshot(snapshot: &HumanLoopPlaySnapshot) -> Result<(), EngineError> {
    validate_single_line("play.calyx_document_id", &snapshot.calyx_document_id)?;
    validate_single_line("play.source_revision_id", &snapshot.source_revision_id)?;
    validate_single_line("play.source_fingerprint", &snapshot.source_fingerprint)?;
    validate_single_line(
        "play.session_fingerprint",
        &snapshot.play_session_fingerprint,
    )?;
    validate_single_line("play.build_fingerprint", &snapshot.play_build_fingerprint)?;
    validate_single_line("play.camera_player_id", &snapshot.camera_player_id)?;
    if snapshot.play_session_state != PlaySessionState::Running
        || snapshot.spawned_bodies == 0
        || snapshot.visible_instances == 0
        || snapshot.draw_batches == 0
    {
        return invalid_human_loop(
            "play.snapshot",
            "launched human-loop play snapshot must be running and renderable",
        );
    }
    Ok(())
}

pub(super) fn validate_feedback(feedback: &HumanLoopFeedback) -> Result<(), EngineError> {
    validate_single_line("feedback.feedback_id", &feedback.feedback_id)?;
    validate_single_line("feedback.session_id", &feedback.session_id)?;
    validate_single_line("feedback.actor_id", &feedback.actor_id)?;
    validate_single_line("feedback.summary", &feedback.summary)?;
    validate_single_line("feedback.severity", &feedback.severity)?;
    validate_string_list(
        "feedback.referenced_object_ids",
        &feedback.referenced_object_ids,
    )?;
    Ok(())
}

pub(super) fn validate_agent_verdict(verdict: &HumanLoopAgentVerdict) -> Result<(), EngineError> {
    validate_single_line("verdict.verdict_id", &verdict.verdict_id)?;
    validate_single_line("verdict.session_id", &verdict.session_id)?;
    validate_single_line("verdict.actor_id", &verdict.actor_id)?;
    validate_single_line("verdict.verdict", &verdict.verdict)?;
    validate_single_line("verdict.confidence_basis", &verdict.confidence_basis)?;
    validate_string_list("verdict.artifact_refs", &verdict.artifact_refs)?;
    if verdict.artifact_refs.is_empty() {
        return invalid_human_loop(
            "verdict.artifact_refs",
            "agent verdict requires evidence refs",
        );
    }
    Ok(())
}

pub(super) fn validate_readback(readback: &HumanLoopReadback) -> Result<(), EngineError> {
    if readback.schema != HUMAN_LOOP_SCHEMA
        || readback.session_id.trim().is_empty()
        || readback.project_id.trim().is_empty()
        || readback.owner_actor_id.trim().is_empty()
        || readback.readback_tool_name != HUMAN_LOOP_READBACK_TOOL
    {
        return invalid_human_loop("readback", "readback identity fields are invalid");
    }
    validate_traceparent(&readback.traceparent)?;
    Ok(())
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_human_loop(field, "value must be non-empty and single-line");
    }
    Ok(())
}

fn validate_string_list(field: &str, values: &[String]) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for value in values {
        validate_single_line(field, value)?;
        if !seen.insert(value.clone()) {
            return invalid_human_loop(field, "list values must be unique");
        }
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
        return invalid_human_loop(
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
