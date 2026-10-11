use litd_engine::EngineError;

use crate::{
    history::{EditorDocument, stable_fingerprint},
    play::{PlayCameraRig, PlaySession},
};

#[path = "human_loop_store.rs"]
mod human_loop_store;

pub use human_loop_store::{
    HumanLoopAsterReadback, parse_human_loop_ledger, read_human_loop_ledger,
    write_human_loop_ledger,
};

#[path = "human_loop_types.rs"]
mod human_loop_types;

pub use human_loop_types::{
    HumanLoopAction, HumanLoopActorKind, HumanLoopAgentVerdict, HumanLoopEntryPoint,
    HumanLoopEvent, HumanLoopFeedback, HumanLoopLaunchRequest, HumanLoopLedger,
    HumanLoopParityReport, HumanLoopPermission, HumanLoopPlaySnapshot, HumanLoopReadback,
    HumanLoopSessionConfig, HumanLoopSessionState,
};

#[path = "human_loop_validation.rs"]
mod human_loop_validation;

pub use human_loop_validation::validate_human_loop_ledger;

use human_loop_validation::{
    validate_agent_verdict, validate_config, validate_event, validate_feedback,
    validate_launch_request, validate_readback,
};

pub const HUMAN_LOOP_SCHEMA: &str = "litd.human_loop.playtest_ledger.v1";
pub const HUMAN_LOOP_GUI_ROUTE: &str = "egui://editor.project/human_playtest";
pub const HUMAN_LOOP_LAUNCH_TOOL: &str = "litd.editor.play.launch_human_playtest";
pub const HUMAN_LOOP_FEEDBACK_TOOL: &str = "litd.editor.play.capture_human_feedback";
pub const HUMAN_LOOP_READBACK_TOOL: &str = "litd.editor.play.read_human_loop";
const HUMAN_LOOP_ACCESS_DENIED: &str = "LITD-HUMAN-LOOP-ACCESS-DENIED";

pub fn launch_human_loop_playtest(
    document: &EditorDocument,
    camera_rig: PlayCameraRig,
    request: HumanLoopLaunchRequest,
) -> Result<HumanLoopLedger, EngineError> {
    validate_launch_request(&request)?;
    require_permission(
        &request.config,
        &request.actor_id,
        HumanLoopAction::Launched,
    )?;
    let session = PlaySession::start(document, camera_rig)?;
    let snapshot = play_snapshot(&session)?;
    let config_event = HumanLoopEvent {
        event_id: event_id(&request.config.session_id, 1, HumanLoopAction::Configured),
        sequence: 1,
        action: HumanLoopAction::Configured,
        entry_point: request.entry_point,
        actor_id: request.actor_id.clone(),
        code: code_for_action(HumanLoopAction::Configured).to_string(),
        details: "human playtest session configured through shared GUI/MCP capability".to_string(),
        play_snapshot: None,
        feedback: None,
        agent_verdict: None,
    };
    let launch_event = HumanLoopEvent {
        event_id: event_id(&request.config.session_id, 2, HumanLoopAction::Launched),
        sequence: 2,
        action: HumanLoopAction::Launched,
        entry_point: request.entry_point,
        actor_id: request.actor_id.clone(),
        code: code_for_action(HumanLoopAction::Launched).to_string(),
        details: format!(
            "play session launched from {:?} with {} spawned bodies",
            request.entry_point, snapshot.spawned_bodies
        ),
        play_snapshot: Some(snapshot),
        feedback: None,
        agent_verdict: None,
    };
    let ledger = build_ledger(
        request.ledger_id,
        request.config,
        vec![config_event, launch_event],
    )?;
    tracing::info!(
        session_id = %ledger.config.session_id,
        ledger_id = %ledger.ledger_id,
        actor_id = %request.actor_id,
        entry_point = ?request.entry_point,
        code = code_for_action(HumanLoopAction::Launched),
        "human-loop playtest launched"
    );
    Ok(ledger)
}

pub fn capture_human_feedback(
    ledger: &HumanLoopLedger,
    feedback: HumanLoopFeedback,
    entry_point: HumanLoopEntryPoint,
) -> Result<HumanLoopLedger, EngineError> {
    validate_human_loop_ledger(ledger)?;
    validate_feedback(&feedback)?;
    if feedback.session_id != ledger.config.session_id {
        return invalid_human_loop(
            "feedback.session_id",
            "feedback session id must match the active human-loop session",
        );
    }
    require_permission(
        &ledger.config,
        &feedback.actor_id,
        HumanLoopAction::FeedbackCaptured,
    )?;
    if ledger.readback.human_feedback_count >= ledger.config.max_human_feedback {
        tracing::error!(
            session_id = %ledger.config.session_id,
            actor_id = %feedback.actor_id,
            limit = ledger.config.max_human_feedback,
            code = "LITD-HUMAN-LOOP-FEEDBACK-LIMIT",
            "human-loop feedback rejected by configured limit"
        );
        return invalid_human_loop(
            "feedback.limit",
            format!(
                "human feedback count {} reached configured limit {}",
                ledger.readback.human_feedback_count, ledger.config.max_human_feedback
            ),
        );
    }
    let next_sequence = ledger.events.len() + 1;
    let event = HumanLoopEvent {
        event_id: event_id(
            &ledger.config.session_id,
            next_sequence,
            HumanLoopAction::FeedbackCaptured,
        ),
        sequence: next_sequence,
        action: HumanLoopAction::FeedbackCaptured,
        entry_point,
        actor_id: feedback.actor_id.clone(),
        code: code_for_action(HumanLoopAction::FeedbackCaptured).to_string(),
        details: format!("captured human feedback {}", feedback.feedback_id),
        play_snapshot: None,
        feedback: Some(feedback),
        agent_verdict: None,
    };
    let mut events = ledger.events.clone();
    events.push(event);
    let next = build_ledger(ledger.ledger_id.clone(), ledger.config.clone(), events)?;
    tracing::info!(
        session_id = %next.config.session_id,
        ledger_id = %next.ledger_id,
        feedback_count = next.readback.human_feedback_count,
        code = code_for_action(HumanLoopAction::FeedbackCaptured),
        "human-loop feedback captured"
    );
    Ok(next)
}

pub fn record_agent_verdict(
    ledger: &HumanLoopLedger,
    verdict: HumanLoopAgentVerdict,
    entry_point: HumanLoopEntryPoint,
) -> Result<HumanLoopLedger, EngineError> {
    validate_human_loop_ledger(ledger)?;
    validate_agent_verdict(&verdict)?;
    if verdict.session_id != ledger.config.session_id {
        return invalid_human_loop(
            "verdict.session_id",
            "agent verdict session id must match the active human-loop session",
        );
    }
    require_permission(
        &ledger.config,
        &verdict.actor_id,
        HumanLoopAction::AgentVerdictRecorded,
    )?;
    if !ledger.config.allow_concurrent_agent_verdicts && ledger.readback.human_feedback_count > 0 {
        tracing::error!(
            session_id = %ledger.config.session_id,
            actor_id = %verdict.actor_id,
            code = "LITD-HUMAN-LOOP-CONCURRENCY-DENIED",
            "human-loop agent verdict rejected by concurrency policy"
        );
        return invalid_human_loop(
            "verdict.concurrency",
            "agent verdicts are disabled after human feedback by this session policy",
        );
    }
    let next_sequence = ledger.events.len() + 1;
    let event = HumanLoopEvent {
        event_id: event_id(
            &ledger.config.session_id,
            next_sequence,
            HumanLoopAction::AgentVerdictRecorded,
        ),
        sequence: next_sequence,
        action: HumanLoopAction::AgentVerdictRecorded,
        entry_point,
        actor_id: verdict.actor_id.clone(),
        code: code_for_action(HumanLoopAction::AgentVerdictRecorded).to_string(),
        details: format!("recorded agent verdict {}", verdict.verdict_id),
        play_snapshot: None,
        feedback: None,
        agent_verdict: Some(verdict),
    };
    let mut events = ledger.events.clone();
    events.push(event);
    let next = build_ledger(ledger.ledger_id.clone(), ledger.config.clone(), events)?;
    tracing::info!(
        session_id = %next.config.session_id,
        ledger_id = %next.ledger_id,
        agent_verdict_count = next.readback.agent_verdict_count,
        code = code_for_action(HumanLoopAction::AgentVerdictRecorded),
        "human-loop agent verdict recorded"
    );
    Ok(next)
}

pub fn readback_human_loop_session(
    ledger: &HumanLoopLedger,
) -> Result<HumanLoopReadback, EngineError> {
    validate_human_loop_ledger(ledger)?;
    Ok(ledger.readback.clone())
}

pub fn compare_human_loop_gui_tool_parity(
    gui: &HumanLoopReadback,
    tool: &HumanLoopReadback,
) -> Result<HumanLoopParityReport, EngineError> {
    validate_readback(gui)?;
    validate_readback(tool)?;
    let checks = [
        ("schema", gui.schema == tool.schema),
        ("project_id", gui.project_id == tool.project_id),
        ("document_id", gui.document_id == tool.document_id),
        (
            "source_revision_id",
            gui.source_revision_id == tool.source_revision_id,
        ),
        (
            "source_fingerprint",
            gui.source_fingerprint == tool.source_fingerprint,
        ),
        (
            "play_build_fingerprint",
            gui.play_build_fingerprint == tool.play_build_fingerprint,
        ),
        ("camera_rig", gui.camera_rig == tool.camera_rig),
        ("spawned_bodies", gui.spawned_bodies == tool.spawned_bodies),
        (
            "visible_instances",
            gui.visible_instances == tool.visible_instances,
        ),
        ("draw_batches", gui.draw_batches == tool.draw_batches),
        (
            "permission_fingerprint",
            gui.permission_fingerprint == tool.permission_fingerprint,
        ),
        (
            "readback_tool_name",
            gui.readback_tool_name == tool.readback_tool_name,
        ),
    ];
    let mut matching_fields = Vec::new();
    let mut mismatched_fields = Vec::new();
    for (field, ok) in checks {
        if ok {
            matching_fields.push(field.to_string());
        } else {
            mismatched_fields.push(field.to_string());
        }
    }
    let report = HumanLoopParityReport {
        schema: HUMAN_LOOP_SCHEMA.to_string(),
        gui_session_id: gui.session_id.clone(),
        tool_session_id: tool.session_id.clone(),
        parity: mismatched_fields.is_empty(),
        matching_fields,
        mismatched_fields,
    };
    if !report.parity {
        tracing::error!(
            gui_session_id = %report.gui_session_id,
            tool_session_id = %report.tool_session_id,
            mismatched_fields = %report.mismatched_fields.join("|"),
            code = "LITD-HUMAN-LOOP-PARITY-MISMATCH",
            "human-loop GUI/MCP parity mismatch"
        );
        return invalid_human_loop(
            "parity",
            format!(
                "GUI and MCP human-loop readbacks diverged: {}",
                report.mismatched_fields.join(",")
            ),
        );
    }
    Ok(report)
}

fn build_ledger(
    ledger_id: impl Into<String>,
    config: HumanLoopSessionConfig,
    events: Vec<HumanLoopEvent>,
) -> Result<HumanLoopLedger, EngineError> {
    let readback = replay_readback(&config, &events)?;
    let ledger = HumanLoopLedger {
        schema: HUMAN_LOOP_SCHEMA.to_string(),
        ledger_id: ledger_id.into(),
        event_count: events.len(),
        config,
        events,
        readback,
    };
    validate_human_loop_ledger(&ledger)?;
    Ok(ledger)
}

fn replay_readback(
    config: &HumanLoopSessionConfig,
    events: &[HumanLoopEvent],
) -> Result<HumanLoopReadback, EngineError> {
    validate_config(config)?;
    let mut launch = None::<&HumanLoopPlaySnapshot>;
    let mut feedback_count = 0usize;
    let mut verdict_count = 0usize;
    let mut last_entry_point = HumanLoopEntryPoint::Gui;
    for event in events {
        validate_event(event, config)?;
        last_entry_point = event.entry_point;
        match event.action {
            HumanLoopAction::Configured => {}
            HumanLoopAction::Launched => {
                if launch
                    .replace(event.play_snapshot.as_ref().ok_or_else(|| {
                        human_loop_error("event.launch", "launch snapshot missing")
                    })?)
                    .is_some()
                {
                    return invalid_human_loop(
                        "event.launch",
                        "ledger cannot contain multiple launches",
                    );
                }
            }
            HumanLoopAction::FeedbackCaptured => {
                feedback_count = feedback_count.saturating_add(1);
            }
            HumanLoopAction::AgentVerdictRecorded => {
                verdict_count = verdict_count.saturating_add(1);
            }
        }
    }
    if feedback_count > config.max_human_feedback {
        return invalid_human_loop(
            "readback.feedback_count",
            "human feedback count exceeds configured limit",
        );
    }
    let launch =
        launch.ok_or_else(|| human_loop_error("readback.launch", "missing launch event"))?;
    Ok(HumanLoopReadback {
        schema: HUMAN_LOOP_SCHEMA.to_string(),
        session_id: config.session_id.clone(),
        project_id: config.project_id.clone(),
        owner_actor_id: config.owner_actor_id.clone(),
        state: HumanLoopSessionState::Running,
        correlation_id: config.correlation_id.clone(),
        traceparent: config.traceparent.clone(),
        document_id: launch.calyx_document_id.clone(),
        source_revision_id: launch.source_revision_id.clone(),
        source_fingerprint: launch.source_fingerprint.clone(),
        play_session_fingerprint: launch.play_session_fingerprint.clone(),
        play_build_fingerprint: launch.play_build_fingerprint.clone(),
        camera_rig: launch.camera_rig,
        spawned_bodies: launch.spawned_bodies,
        visible_instances: launch.visible_instances,
        draw_batches: launch.draw_batches,
        human_feedback_count: feedback_count,
        agent_verdict_count: verdict_count,
        participant_count: config.permissions.len(),
        permission_fingerprint: permission_fingerprint(config)?,
        last_entry_point,
        readback_tool_name: config.readback_tool_name.clone(),
    })
}

fn play_snapshot(session: &PlaySession) -> Result<HumanLoopPlaySnapshot, EngineError> {
    Ok(HumanLoopPlaySnapshot {
        play_session_state: session.state,
        play_session_fingerprint: stable_fingerprint(session)?,
        play_build_fingerprint: stable_fingerprint(&session.build)?,
        calyx_document_id: session.build.calyx_document_id.clone(),
        source_revision_id: session.build.source_revision_id.clone(),
        source_fingerprint: session.build.source_fingerprint.clone(),
        generation: session.generation,
        camera_rig: session.build.camera_rig,
        camera_player_id: session.build.camera_pose.player_id.clone(),
        spawned_bodies: session.build.spawned_bodies.len(),
        visible_instances: session.build.visible_instance_ids.len(),
        draw_batches: session.build.draw_batches,
        sim_seed: session.build.sim_seed,
    })
}

fn require_permission(
    config: &HumanLoopSessionConfig,
    actor_id: &str,
    action: HumanLoopAction,
) -> Result<(), EngineError> {
    let permission = require_known_actor(config, actor_id)?;
    let allowed = match action {
        HumanLoopAction::Configured => permission.can_launch,
        HumanLoopAction::Launched => permission.can_launch,
        HumanLoopAction::FeedbackCaptured => {
            permission.actor_kind == HumanLoopActorKind::Human && permission.can_submit_feedback
        }
        HumanLoopAction::AgentVerdictRecorded => {
            permission.actor_kind == HumanLoopActorKind::Agent
                && permission.can_record_agent_verdict
        }
    };
    if !allowed {
        tracing::error!(
            session_id = %config.session_id,
            actor_id,
            action = ?action,
            code = HUMAN_LOOP_ACCESS_DENIED,
            "human-loop action denied by permission policy"
        );
        return invalid_human_loop(
            "permission.denied",
            format!("{actor_id} is not authorized for {action:?}"),
        );
    }
    Ok(())
}

fn require_known_actor<'a>(
    config: &'a HumanLoopSessionConfig,
    actor_id: &str,
) -> Result<&'a HumanLoopPermission, EngineError> {
    config
        .permissions
        .iter()
        .find(|permission| permission.actor_id == actor_id)
        .ok_or_else(|| {
            tracing::error!(
                session_id = %config.session_id,
                actor_id,
                code = HUMAN_LOOP_ACCESS_DENIED,
                "human-loop action denied for unknown actor"
            );
            human_loop_error("permission.actor", format!("unknown actor {actor_id}"))
        })
}

fn permission_fingerprint(config: &HumanLoopSessionConfig) -> Result<String, EngineError> {
    stable_fingerprint(&config.permissions)
}

fn code_for_action(action: HumanLoopAction) -> &'static str {
    match action {
        HumanLoopAction::Configured => "LITD-HUMAN-LOOP-CONFIGURED",
        HumanLoopAction::Launched => "LITD-HUMAN-LOOP-LAUNCHED",
        HumanLoopAction::FeedbackCaptured => "LITD-HUMAN-LOOP-FEEDBACK-CAPTURED",
        HumanLoopAction::AgentVerdictRecorded => "LITD-HUMAN-LOOP-AGENT-VERDICT-RECORDED",
    }
}

fn event_id(session_id: &str, sequence: usize, action: HumanLoopAction) -> String {
    format!(
        "{session_id}.event.{sequence:04}.{}",
        code_for_action(action).to_ascii_lowercase()
    )
}

fn invalid_human_loop<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(human_loop_error(field, reason))
}

fn human_loop_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.human_loop.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "human_loop_tests.rs"]
mod tests;
