use std::collections::BTreeSet;

use litd_engine::{EngineError, sim_order::SimOrder, sim_replay::SIM_REPLAY_SCHEMA_VERSION};

use crate::{history::stable_fingerprint, play::PlaySession};

use super::*;

pub(crate) fn validate_request(request: &PlaySessionCaptureRequest) -> Result<(), EngineError> {
    validate_single_line("request.ledger_id", &request.ledger_id)?;
    validate_single_line("request.session_id", &request.session_id)?;
    validate_single_line("request.actor_id", &request.actor_id)?;
    validate_variant_id(&request.active_variant_id)?;
    if request.restart_command != PLAY_SESSION_RESTART_COMMAND {
        return invalid_session_capture("request.restart_command", "restart command must be again");
    }
    if request.run_ticks == 0 || request.run_ticks > MAX_CAPTURE_TICKS {
        return invalid_session_capture(
            "request.run_ticks",
            format!("run ticks must be within 1..={MAX_CAPTURE_TICKS}"),
        );
    }
    if request.checkpoint_tick == 0 || request.checkpoint_tick >= request.run_ticks {
        return invalid_session_capture(
            "request.checkpoint_tick",
            "checkpoint tick must be inside the captured run",
        );
    }
    if request.orders.is_empty() || request.orders.len() > MAX_CAPTURE_ORDERS {
        return invalid_session_capture(
            "request.orders",
            format!("order count must be within 1..={MAX_CAPTURE_ORDERS}"),
        );
    }
    validate_orders(&request.orders, request.run_ticks)
}

pub(crate) fn validate_orders(orders: &[SimOrder], run_ticks: u64) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for order in orders {
        if order.execute_tick == 0 || order.execute_tick > run_ticks {
            return invalid_session_capture(
                "orders.execute_tick",
                "order execute tick must fall inside the captured run",
            );
        }
        if !seen.insert(order.fingerprint()) {
            return invalid_session_capture("orders.duplicate", "duplicate order fingerprint");
        }
    }
    Ok(())
}

pub(crate) fn validate_variant_snapshots(
    variants: &[PlaySessionVariantSnapshot],
) -> Result<(), EngineError> {
    if variants.len() != 2 {
        return invalid_session_capture(
            "variants.count",
            "ledger must contain exactly two variants",
        );
    }
    let mut ids = BTreeSet::new();
    let mut fingerprints = BTreeSet::new();
    for variant in variants {
        validate_variant_id(&variant.variant_id)?;
        validate_single_line("variant.label", &variant.label)?;
        validate_single_line(
            "variant.document_fingerprint",
            &variant.document_fingerprint,
        )?;
        validate_single_line(
            "variant.play_build_fingerprint",
            &variant.play_build_fingerprint,
        )?;
        validate_single_line("variant.source_revision_id", &variant.source_revision_id)?;
        if variant.spawned_bodies == 0
            || variant.visible_instances == 0
            || variant.draw_batches == 0
        {
            return invalid_session_capture(
                "variant.play_build",
                "variant must be playable and renderable",
            );
        }
        let rebuilt = PlaySession::start_from_document(&variant.document)?;
        if stable_fingerprint(&rebuilt.source_document)? != variant.document_fingerprint
            || stable_fingerprint(&rebuilt.build)? != variant.play_build_fingerprint
            || rebuilt.build.snapshot.state_hash != variant.initial_state_hash
        {
            return invalid_session_capture(
                "variant.readback",
                "variant snapshot does not match rebuilt play session",
            );
        }
        if !ids.insert(variant.variant_id.as_str()) {
            return invalid_session_capture("variants.duplicate", "duplicate variant id");
        }
        if !fingerprints.insert(variant.document_fingerprint.as_str()) {
            return invalid_session_capture(
                "variants.isolation",
                "A and B variants must have distinct document fingerprints",
            );
        }
    }
    if !ids.contains("a") || !ids.contains("b") {
        return invalid_session_capture("variants.ids", "variant ids must be a and b");
    }
    Ok(())
}

pub(crate) fn validate_replay_proof(replay: &PlaySessionReplayProof) -> Result<(), EngineError> {
    if replay.replay_schema_version != SIM_REPLAY_SCHEMA_VERSION
        || replay.lowering_boundary != PLAY_SESSION_LOWERING_BOUNDARY
        || replay.record.schema_version != SIM_REPLAY_SCHEMA_VERSION
        || replay.record.run_ticks == 0
        || replay.record.run_ticks > MAX_CAPTURE_TICKS
        || replay.record.orders.is_empty()
        || replay.record.orders.len() > MAX_CAPTURE_ORDERS
    {
        return invalid_session_capture("replay", "replay proof fields are invalid");
    }
    validate_orders(&replay.record.orders, replay.record.run_ticks)?;
    let outcome = replay_record(&replay.record)?;
    if stable_fingerprint(&replay.record)? != replay.record_fingerprint
        || replay.initial_hash != replay.record.initial_snapshot.state_hash
        || replay.expected_final_hash != replay.record.expected_final_hash
        || replay.replayed_final_hash != outcome.final_hash
        || replay.final_tick != outcome.final_tick
        || replay.steps != outcome.steps
        || replay.orders_applied != outcome.orders_applied
        || !replay.deterministic_match
        || replay.expected_final_hash != outcome.final_hash
    {
        return invalid_session_capture(
            "replay.readback",
            "replay proof does not reproduce the expected final hash",
        );
    }
    Ok(())
}

pub(crate) fn validate_checkpoint(
    checkpoint: &PlaySessionCheckpoint,
    replay: &PlaySessionReplayProof,
) -> Result<(), EngineError> {
    validate_checkpoint_record(checkpoint)?;
    if checkpoint.checkpoint_tick == 0
        || checkpoint.checkpoint_tick >= replay.record.run_ticks
        || checkpoint.remaining_ticks != replay.record.run_ticks - checkpoint.checkpoint_tick
        || checkpoint.restart_record.expected_final_hash != replay.expected_final_hash
    {
        return invalid_session_capture("checkpoint", "checkpoint does not match replay record");
    }
    Ok(())
}

pub(crate) fn validate_checkpoint_record(
    checkpoint: &PlaySessionCheckpoint,
) -> Result<(), EngineError> {
    validate_single_line("checkpoint.id", &checkpoint.checkpoint_id)?;
    if checkpoint.checkpoint_tick == 0
        || checkpoint.remaining_ticks == 0
        || checkpoint.remaining_order_count != checkpoint.restart_record.orders.len()
        || checkpoint.restart_record.initial_snapshot.state_hash != checkpoint.checkpoint_hash
        || stable_fingerprint(&checkpoint.restart_record)? != checkpoint.restart_record_fingerprint
    {
        return invalid_session_capture(
            "checkpoint.record",
            "checkpoint restart record is invalid",
        );
    }
    replay_record(&checkpoint.restart_record)?;
    Ok(())
}

pub(crate) fn validate_restart(
    restart: &PlaySessionRestartReadback,
    checkpoint: &PlaySessionCheckpoint,
    replay: &PlaySessionReplayProof,
) -> Result<(), EngineError> {
    if restart.command != PLAY_SESSION_RESTART_COMMAND
        || restart.checkpoint_id != checkpoint.checkpoint_id
        || restart.checkpoint_tick != checkpoint.checkpoint_tick
        || restart.full_replay_final_hash != replay.expected_final_hash
        || restart.restart_final_hash != replay.expected_final_hash
        || !restart.matches_full_replay
    {
        return invalid_session_capture("restart", "restart readback does not match replay");
    }
    Ok(())
}

pub(crate) fn validate_events(
    events: &[PlaySessionCaptureEvent],
    ledger: &PlaySessionCaptureLedger,
) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for (index, event) in events.iter().enumerate() {
        validate_single_line("event.event_id", &event.event_id)?;
        validate_single_line("event.actor_id", &event.actor_id)?;
        validate_variant_id(&event.variant_id)?;
        validate_single_line("event.code", &event.code)?;
        validate_single_line("event.details", &event.details)?;
        if event.sequence != index + 1 || event.code != code_for_action(event.action) {
            return invalid_session_capture("event.sequence", "event sequence/code is invalid");
        }
        if event.actor_id != ledger.actor_id || !seen.insert(event.event_id.as_str()) {
            return invalid_session_capture("event.identity", "event identity is invalid");
        }
        require_variant(&ledger.variants, &event.variant_id)?;
    }
    if !events
        .iter()
        .any(|event| event.action == PlaySessionCaptureAction::RestartedAgain)
        || !events
            .iter()
            .any(|event| event.action == PlaySessionCaptureAction::Captured)
    {
        return invalid_session_capture(
            "event.required",
            "session capture ledger requires captured and restart events",
        );
    }
    Ok(())
}

pub(crate) fn require_variant<'a>(
    variants: &'a [PlaySessionVariantSnapshot],
    variant_id: &str,
) -> Result<&'a PlaySessionVariantSnapshot, EngineError> {
    variants
        .iter()
        .find(|variant| variant.variant_id == variant_id)
        .ok_or_else(|| {
            session_capture_error("variant.missing", format!("missing variant {variant_id}"))
        })
}

pub(crate) fn variants_are_isolated(variants: &[PlaySessionVariantSnapshot]) -> bool {
    variants.len() == 2
        && variants
            .iter()
            .map(|variant| variant.document_fingerprint.as_str())
            .collect::<BTreeSet<_>>()
            .len()
            == variants.len()
}

pub(crate) fn event(
    session_id: &str,
    sequence: usize,
    action: PlaySessionCaptureAction,
    actor_id: &str,
    variant_id: &str,
    details: String,
) -> PlaySessionCaptureEvent {
    PlaySessionCaptureEvent {
        event_id: format!(
            "{session_id}.event.{sequence:04}.{}",
            code_for_action(action).to_ascii_lowercase()
        ),
        sequence,
        action,
        actor_id: actor_id.to_string(),
        variant_id: variant_id.to_string(),
        code: code_for_action(action).to_string(),
        details,
    }
}

pub(crate) fn code_for_action(action: PlaySessionCaptureAction) -> &'static str {
    match action {
        PlaySessionCaptureAction::Captured => "LITD-SESSION-CAPTURED",
        PlaySessionCaptureAction::RestartedAgain => "LITD-SESSION-RESTARTED-AGAIN",
        PlaySessionCaptureAction::VariantToggled => "LITD-SESSION-VARIANT-TOGGLED",
    }
}

pub(crate) fn validate_variant_id(value: &str) -> Result<(), EngineError> {
    validate_single_line("variant.id", value)?;
    if !matches!(value, "a" | "b") {
        return invalid_session_capture("variant.id", "variant id must be a or b");
    }
    Ok(())
}

pub(crate) fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_session_capture(field, "value must be non-empty and single-line");
    }
    Ok(())
}

pub(crate) fn invalid_session_capture<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(session_capture_error(field, reason))
}

pub(crate) fn session_capture_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    let field = format!("editor.session_capture.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-SESSION-CAPTURE-VALIDATION",
        "play-session capture rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}
