use litd_engine::{
    EngineError,
    sim_order::SimOrder,
    sim_replay::{SIM_REPLAY_SCHEMA_VERSION, SimReplayOutcome, SimReplayRecord},
};

use crate::{
    history::{EditorDocument, stable_fingerprint},
    play::{PlayCameraRig, PlaySession},
};

use super::*;

pub(crate) fn build_variant_snapshots(
    inputs: Vec<PlaySessionVariantInput>,
    camera_rig: PlayCameraRig,
) -> Result<Vec<PlaySessionVariantSnapshot>, EngineError> {
    if inputs.len() != 2 {
        return invalid_session_capture(
            "variants.count",
            "A/B capture requires exactly two variants",
        );
    }
    let mut snapshots = Vec::with_capacity(inputs.len());
    for input in inputs {
        validate_variant_id(&input.variant_id)?;
        validate_single_line("variant.label", &input.label)?;
        let session = PlaySession::start(&input.document, camera_rig)?;
        snapshots.push(PlaySessionVariantSnapshot {
            variant_id: input.variant_id.to_ascii_lowercase(),
            label: input.label,
            document_fingerprint: stable_fingerprint(&session.source_document)?,
            play_build_fingerprint: stable_fingerprint(&session.build)?,
            initial_state_hash: session.build.snapshot.state_hash,
            source_revision_id: session.build.source_revision_id,
            spawned_bodies: session.build.spawned_bodies.len(),
            visible_instances: session.build.visible_instance_ids.len(),
            draw_batches: session.build.draw_batches,
            document: session.source_document,
        });
    }
    snapshots.sort_by(|left, right| left.variant_id.cmp(&right.variant_id));
    validate_variant_snapshots(&snapshots)?;
    Ok(snapshots)
}

pub(crate) fn build_replay_proof(
    document: &EditorDocument,
    camera_rig: PlayCameraRig,
    request: &PlaySessionCaptureRequest,
) -> Result<PlaySessionReplayProof, EngineError> {
    let session = PlaySession::start(document, camera_rig)?;
    let expected_final_hash = replay_expected_hash(
        session.build.snapshot.clone(),
        &request.orders,
        request.run_ticks,
    )?;
    let record = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot: session.build.snapshot,
        orders: request.orders.clone(),
        run_ticks: request.run_ticks,
        expected_final_hash,
    };
    let outcome = replay_record(&record)?;
    let proof = PlaySessionReplayProof {
        replay_schema_version: record.schema_version,
        lowering_boundary: PLAY_SESSION_LOWERING_BOUNDARY.to_string(),
        record_fingerprint: stable_fingerprint(&record)?,
        initial_hash: record.initial_snapshot.state_hash,
        expected_final_hash,
        replayed_final_hash: outcome.final_hash,
        final_tick: outcome.final_tick,
        steps: outcome.steps,
        orders_applied: outcome.orders_applied,
        deterministic_match: expected_final_hash == outcome.final_hash,
        record,
    };
    validate_replay_proof(&proof)?;
    Ok(proof)
}

pub(crate) fn build_checkpoint(
    session_id: &str,
    record: &SimReplayRecord,
    checkpoint_tick: u64,
) -> Result<PlaySessionCheckpoint, EngineError> {
    if checkpoint_tick == 0 || checkpoint_tick >= record.run_ticks {
        return invalid_session_capture(
            "checkpoint.tick",
            "checkpoint tick must be inside the captured run",
        );
    }
    let mut sim = record.initial_snapshot.restore::<MAX_CAPTURE_BODIES>()?;
    let prefix_orders = record
        .orders
        .iter()
        .copied()
        .filter(|order| order.execute_tick <= checkpoint_tick)
        .collect::<Vec<_>>();
    run_orders(&mut sim, &prefix_orders, checkpoint_tick)?;
    let checkpoint_snapshot = litd_engine::sim_replay::SimSnapshot::capture(&sim);
    let remaining_orders = record
        .orders
        .iter()
        .copied()
        .filter(|order| order.execute_tick > checkpoint_tick)
        .collect::<Vec<_>>();
    let restart_record = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot: checkpoint_snapshot.clone(),
        orders: remaining_orders,
        run_ticks: record.run_ticks - checkpoint_tick,
        expected_final_hash: record.expected_final_hash,
    };
    let checkpoint = PlaySessionCheckpoint {
        checkpoint_id: format!("{session_id}.checkpoint.{checkpoint_tick}"),
        checkpoint_tick,
        checkpoint_hash: checkpoint_snapshot.state_hash,
        remaining_ticks: restart_record.run_ticks,
        remaining_order_count: restart_record.orders.len(),
        restart_record_fingerprint: stable_fingerprint(&restart_record)?,
        restart_record,
    };
    validate_checkpoint_record(&checkpoint)?;
    Ok(checkpoint)
}

pub(crate) fn restart_from_checkpoint(
    checkpoint: &PlaySessionCheckpoint,
    full_replay_final_hash: u64,
) -> Result<PlaySessionRestartReadback, EngineError> {
    validate_checkpoint_record(checkpoint)?;
    let outcome = replay_record(&checkpoint.restart_record)?;
    let readback = PlaySessionRestartReadback {
        command: PLAY_SESSION_RESTART_COMMAND.to_string(),
        checkpoint_id: checkpoint.checkpoint_id.clone(),
        checkpoint_tick: checkpoint.checkpoint_tick,
        restart_final_hash: outcome.final_hash,
        full_replay_final_hash,
        final_tick: outcome.final_tick,
        orders_applied: outcome.orders_applied,
        matches_full_replay: outcome.final_hash == full_replay_final_hash,
    };
    if !readback.matches_full_replay {
        return invalid_session_capture(
            "restart.hash",
            "checkpoint restart did not reproduce the full replay final hash",
        );
    }
    Ok(readback)
}

pub(crate) fn build_readback(
    session_id: &str,
    active_variant_id: &str,
    variants: &[PlaySessionVariantSnapshot],
    replay: &PlaySessionReplayProof,
    checkpoint: &PlaySessionCheckpoint,
    restart: &PlaySessionRestartReadback,
) -> Result<PlaySessionCaptureReadback, EngineError> {
    let active = require_variant(variants, active_variant_id)?;
    let lowered_order_count = replay
        .record
        .orders
        .iter()
        .filter(|order| {
            matches!(
                order.source.kind,
                litd_engine::sim_order::SimOrderSourceKind::LoweredAgent
            )
        })
        .count();
    let human_order_count = replay
        .record
        .orders
        .len()
        .saturating_sub(lowered_order_count);
    Ok(PlaySessionCaptureReadback {
        schema: PLAY_SESSION_CAPTURE_SCHEMA.to_string(),
        session_id: session_id.to_string(),
        active_variant_id: active_variant_id.to_string(),
        active_document_fingerprint: active.document_fingerprint.clone(),
        variant_count: variants.len(),
        variant_catalog_fingerprint: stable_fingerprint(&variants)?,
        a_b_isolated: variants_are_isolated(variants),
        run_ticks: replay.record.run_ticks,
        checkpoint_tick: checkpoint.checkpoint_tick,
        full_replay_final_hash: replay.expected_final_hash,
        replay_final_hash_matches: replay.deterministic_match,
        restart_final_hash: restart.restart_final_hash,
        restart_matches_full_replay: restart.matches_full_replay,
        lowering_boundary: replay.lowering_boundary.clone(),
        lowered_order_count,
        human_order_count,
        readback_tool_name: PLAY_SESSION_CAPTURE_READBACK_TOOL.to_string(),
    })
}

pub(crate) fn replay_expected_hash(
    initial_snapshot: litd_engine::sim_replay::SimSnapshot,
    orders: &[SimOrder],
    run_ticks: u64,
) -> Result<u64, EngineError> {
    let mut sim = initial_snapshot.restore::<MAX_CAPTURE_BODIES>()?;
    Ok(run_orders(&mut sim, orders, run_ticks)?.state_hash)
}

pub(crate) fn replay_record(record: &SimReplayRecord) -> Result<SimReplayOutcome, EngineError> {
    record.replay::<MAX_CAPTURE_BODIES, MAX_CAPTURE_ORDERS>()
}

pub(crate) fn run_orders(
    sim: &mut litd_engine::sim::FixedTickSim<MAX_CAPTURE_BODIES>,
    orders: &[SimOrder],
    run_ticks: u64,
) -> Result<litd_engine::sim::SimRunReport, EngineError> {
    let mut queue = litd_engine::sim_order::SimOrderQueue::<MAX_CAPTURE_ORDERS>::default();
    for order in orders {
        queue.enqueue(*order)?;
    }
    let report = queue.run_ticks(sim, run_ticks)?;
    if queue.pending_orders() != 0 {
        return invalid_session_capture(
            "orders.pending",
            format!(
                "{} orders remained pending after replay",
                queue.pending_orders()
            ),
        );
    }
    Ok(report)
}
