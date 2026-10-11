use litd_engine::EngineError;

use super::*;

pub fn capture_play_session(
    variants: Vec<PlaySessionVariantInput>,
    request: PlaySessionCaptureRequest,
) -> Result<PlaySessionCaptureLedger, EngineError> {
    validate_request(&request)?;
    let variant_snapshots = build_variant_snapshots(variants, request.camera_rig)?;
    require_variant(&variant_snapshots, &request.active_variant_id)?;
    let active = require_variant(&variant_snapshots, &request.active_variant_id)?;
    let replay = build_replay_proof(&active.document, request.camera_rig, &request)?;
    let checkpoint =
        build_checkpoint(&request.session_id, &replay.record, request.checkpoint_tick)?;
    let restart = restart_from_checkpoint(&checkpoint, replay.expected_final_hash)?;
    let events = vec![
        event(
            &request.session_id,
            1,
            PlaySessionCaptureAction::Captured,
            &request.actor_id,
            &request.active_variant_id,
            format!(
                "captured {} ticks and {} orders through {}",
                request.run_ticks,
                request.orders.len(),
                PLAY_SESSION_LOWERING_BOUNDARY
            ),
        ),
        event(
            &request.session_id,
            2,
            PlaySessionCaptureAction::RestartedAgain,
            &request.actor_id,
            &request.active_variant_id,
            format!(
                "restart command {} resumed from tick {}",
                PLAY_SESSION_RESTART_COMMAND, request.checkpoint_tick
            ),
        ),
    ];
    build_ledger(BuildLedgerArgs {
        ledger_id: request.ledger_id,
        session_id: request.session_id,
        actor_id: request.actor_id,
        active_variant_id: request.active_variant_id,
        variants: variant_snapshots,
        replay,
        checkpoint,
        restart,
        events,
    })
}

pub fn restart_session_again(
    ledger: &PlaySessionCaptureLedger,
) -> Result<PlaySessionCaptureLedger, EngineError> {
    validate_play_session_capture_ledger(ledger)?;
    let restart = restart_from_checkpoint(&ledger.checkpoint, ledger.replay.expected_final_hash)?;
    let mut events = ledger.events.clone();
    let sequence = events.len() + 1;
    events.push(event(
        &ledger.session_id,
        sequence,
        PlaySessionCaptureAction::RestartedAgain,
        &ledger.actor_id,
        &ledger.active_variant_id,
        format!(
            "restart command {} replayed checkpoint {}",
            PLAY_SESSION_RESTART_COMMAND, ledger.checkpoint.checkpoint_id
        ),
    ));
    build_ledger(BuildLedgerArgs {
        ledger_id: ledger.ledger_id.clone(),
        session_id: ledger.session_id.clone(),
        actor_id: ledger.actor_id.clone(),
        active_variant_id: ledger.active_variant_id.clone(),
        variants: ledger.variants.clone(),
        replay: ledger.replay.clone(),
        checkpoint: ledger.checkpoint.clone(),
        restart,
        events,
    })
}

pub fn toggle_ab_variant(
    ledger: &PlaySessionCaptureLedger,
    target_variant_id: &str,
) -> Result<PlaySessionCaptureLedger, EngineError> {
    validate_play_session_capture_ledger(ledger)?;
    validate_variant_id(target_variant_id)?;
    if target_variant_id == ledger.active_variant_id {
        return invalid_session_capture("variant.target", "target variant is already active");
    }
    require_variant(&ledger.variants, target_variant_id)?;
    let mut events = ledger.events.clone();
    let sequence = events.len() + 1;
    events.push(event(
        &ledger.session_id,
        sequence,
        PlaySessionCaptureAction::VariantToggled,
        &ledger.actor_id,
        target_variant_id,
        format!(
            "A/B toggle changed active variant from {} to {}",
            ledger.active_variant_id, target_variant_id
        ),
    ));
    build_ledger(BuildLedgerArgs {
        ledger_id: ledger.ledger_id.clone(),
        session_id: ledger.session_id.clone(),
        actor_id: ledger.actor_id.clone(),
        active_variant_id: target_variant_id.to_string(),
        variants: ledger.variants.clone(),
        replay: ledger.replay.clone(),
        checkpoint: ledger.checkpoint.clone(),
        restart: ledger.restart.clone(),
        events,
    })
}

pub fn validate_play_session_capture_ledger(
    ledger: &PlaySessionCaptureLedger,
) -> Result<(), EngineError> {
    if ledger.schema != PLAY_SESSION_CAPTURE_LEDGER_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.session_id.trim().is_empty()
        || ledger.actor_id.trim().is_empty()
        || ledger.gui_route != PLAY_SESSION_CAPTURE_GUI_ROUTE
        || ledger.capture_tool_name != PLAY_SESSION_CAPTURE_TOOL
        || ledger.restart_tool_name != PLAY_SESSION_RESTART_TOOL
        || ledger.variant_tool_name != PLAY_SESSION_VARIANT_TOOL
        || ledger.readback_tool_name != PLAY_SESSION_CAPTURE_READBACK_TOOL
        || ledger.event_count != ledger.events.len()
        || ledger.events.len() < 2
    {
        return invalid_session_capture("ledger", "required ledger fields are invalid");
    }
    validate_single_line("ledger.ledger_id", &ledger.ledger_id)?;
    validate_single_line("ledger.session_id", &ledger.session_id)?;
    validate_single_line("ledger.actor_id", &ledger.actor_id)?;
    validate_variant_snapshots(&ledger.variants)?;
    require_variant(&ledger.variants, &ledger.active_variant_id)?;
    validate_replay_proof(&ledger.replay)?;
    validate_checkpoint(&ledger.checkpoint, &ledger.replay)?;
    validate_restart(&ledger.restart, &ledger.checkpoint, &ledger.replay)?;
    validate_events(&ledger.events, ledger)?;
    let expected = build_readback(
        &ledger.session_id,
        &ledger.active_variant_id,
        &ledger.variants,
        &ledger.replay,
        &ledger.checkpoint,
        &ledger.restart,
    )?;
    if expected != ledger.readback {
        return invalid_session_capture(
            "ledger.readback",
            "stored readback does not match replayed session-capture state",
        );
    }
    Ok(())
}

struct BuildLedgerArgs {
    ledger_id: String,
    session_id: String,
    actor_id: String,
    active_variant_id: String,
    variants: Vec<PlaySessionVariantSnapshot>,
    replay: PlaySessionReplayProof,
    checkpoint: PlaySessionCheckpoint,
    restart: PlaySessionRestartReadback,
    events: Vec<PlaySessionCaptureEvent>,
}

fn build_ledger(args: BuildLedgerArgs) -> Result<PlaySessionCaptureLedger, EngineError> {
    let BuildLedgerArgs {
        ledger_id,
        session_id,
        actor_id,
        active_variant_id,
        variants,
        replay,
        checkpoint,
        restart,
        events,
    } = args;
    let readback = build_readback(
        &session_id,
        &active_variant_id,
        &variants,
        &replay,
        &checkpoint,
        &restart,
    )?;
    let ledger = PlaySessionCaptureLedger {
        schema: PLAY_SESSION_CAPTURE_LEDGER_SCHEMA.to_string(),
        ledger_id,
        session_id,
        actor_id,
        gui_route: PLAY_SESSION_CAPTURE_GUI_ROUTE.to_string(),
        capture_tool_name: PLAY_SESSION_CAPTURE_TOOL.to_string(),
        restart_tool_name: PLAY_SESSION_RESTART_TOOL.to_string(),
        variant_tool_name: PLAY_SESSION_VARIANT_TOOL.to_string(),
        readback_tool_name: PLAY_SESSION_CAPTURE_READBACK_TOOL.to_string(),
        active_variant_id,
        variants,
        replay,
        checkpoint,
        restart,
        event_count: events.len(),
        events,
        readback,
    };
    validate_play_session_capture_ledger(&ledger)?;
    Ok(ledger)
}
