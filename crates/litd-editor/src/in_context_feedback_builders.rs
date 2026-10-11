use std::{fs, path::Path};

use litd_engine::EngineError;
use litd_hash::sha256_hex;

use crate::{
    history::{EditorObject, stable_fingerprint},
    play::PlaySession,
    semantic_authoring_ops::{
        SemanticAuthoringOperation, SemanticAuthoringRequest, run_semantic_authoring_action,
    },
    world_memory::{WorldMemoryLensKind, query_world_memory},
};

use super::in_context_feedback_semantics::{
    feedback_constellation, feedback_index, loom_association,
};
use super::in_context_feedback_validation::{
    validate_capture_request, validate_feedback_record, validate_screenshot_input,
};
use super::*;

pub(super) fn build_ledger(
    session: ContextFeedbackSessionRequest,
    source_document: EditorDocument,
    session_snapshot: ContextFeedbackSessionSnapshot,
    events: Vec<ContextFeedbackEvent>,
) -> Result<ContextFeedbackLedger, EngineError> {
    let readback = build_readback(&session, &source_document, &session_snapshot, &events)?;
    let ledger = ContextFeedbackLedger {
        schema: IN_CONTEXT_FEEDBACK_LEDGER_SCHEMA.to_string(),
        ledger_id: session.ledger_id.clone(),
        session,
        source_document,
        session_snapshot,
        gui_route: IN_CONTEXT_FEEDBACK_GUI_ROUTE.to_string(),
        agent_route: IN_CONTEXT_FEEDBACK_AGENT_ROUTE.to_string(),
        capture_tool_name: IN_CONTEXT_FEEDBACK_CAPTURE_TOOL.to_string(),
        readback_tool_name: IN_CONTEXT_FEEDBACK_READBACK_TOOL.to_string(),
        event_count: events.len(),
        events,
        readback,
    };
    validate_context_feedback_ledger(&ledger)?;
    Ok(ledger)
}

pub(super) fn build_feedback_record(
    ledger: &ContextFeedbackLedger,
    request: &ContextFeedbackCaptureRequest,
    existing: &[&ContextFeedbackRecord],
) -> Result<ContextFeedbackRecord, EngineError> {
    validate_capture_request(request)?;
    if request.actor_id != ledger.session.actor_id {
        return invalid_context_feedback("feedback.actor", "feedback actor must own the session");
    }
    let target = ledger
        .source_document
        .objects
        .get(&request.marker.target_object_id)
        .ok_or_else(|| {
            context_feedback_error(
                "marker.target",
                format!("missing marker target {}", request.marker.target_object_id),
            )
        })?;
    if target.transform.translation != request.marker.world_position {
        return invalid_context_feedback(
            "marker.position",
            "marker world position must match the target object transform",
        );
    }
    let screenshot = read_screenshot(&request.screenshot)?;
    let annotation = annotation(ledger, request)?;
    let world_state = world_state(ledger, target)?;
    let repro = repro(&ledger.session_snapshot);
    let source_fingerprint = stable_fingerprint(&(&annotation, &screenshot.sha256, &world_state))?;
    let constellation = feedback_constellation(
        &ledger.session.project_id,
        request,
        &annotation,
        &screenshot,
        &world_state,
        source_fingerprint,
    )?;
    let index = feedback_index(
        &ledger.session.project_id,
        existing
            .iter()
            .map(|record| record.constellation.clone())
            .chain([constellation.clone()])
            .collect(),
    )?;
    let query = format!(
        "{} {} {}",
        request.note, request.marker.target_object_id, request.severity
    );
    let hits = query_world_memory(&index, &query, 8)?;
    let self_rank = hits
        .iter()
        .position(|hit| hit.constellation_id == constellation.constellation_id)
        .map(|index| index + 1)
        .ok_or_else(|| {
            context_feedback_error(
                "sextant.self",
                "captured feedback constellation was not returned by semantic search",
            )
        })?;
    let definition = run_semantic_authoring_action(
        "ContextFeedbackLedger.feedback_constellations",
        &index,
        &SemanticAuthoringRequest {
            query_id: format!("{}.sextant.define", request.feedback_id),
            operation: SemanticAuthoringOperation::Define,
            subject_constellation_id: constellation.constellation_id.clone(),
            lens: Some(WorldMemoryLensKind::Text),
            slot_filter: Vec::new(),
            k: 1,
            requested_by: request.actor_id.clone(),
        },
    )?;
    let neighbors = run_semantic_authoring_action(
        "ContextFeedbackLedger.feedback_constellations",
        &index,
        &SemanticAuthoringRequest {
            query_id: format!("{}.sextant.neighbors", request.feedback_id),
            operation: SemanticAuthoringOperation::Neighbors,
            subject_constellation_id: constellation.constellation_id.clone(),
            lens: Some(WorldMemoryLensKind::Text),
            slot_filter: Vec::new(),
            k: 4,
            requested_by: request.actor_id.clone(),
        },
    )?;
    let loom = loom_association(request, existing)?;
    let record = ContextFeedbackRecord {
        feedback_id: request.feedback_id.clone(),
        actor_id: request.actor_id.clone(),
        severity: request.severity.clone(),
        annotation,
        screenshot,
        world_state,
        repro,
        constellation: constellation.clone(),
        sextant: ContextFeedbackSextantReadback {
            query,
            world_memory_hit_count: hits.len(),
            self_constellation_id: constellation.constellation_id,
            self_hit_rank: self_rank,
            self_found: true,
            definition,
            neighbors,
            hits,
        },
        loom,
        editor_route: IN_CONTEXT_FEEDBACK_GUI_ROUTE.to_string(),
        agent_route: IN_CONTEXT_FEEDBACK_AGENT_ROUTE.to_string(),
    };
    validate_feedback_record(&record)?;
    tracing::info!(
        feedback_id = %record.feedback_id,
        target_object_id = %record.world_state.target_object_id,
        screenshot_sha256 = %record.screenshot.sha256,
        code = code_for_action(ContextFeedbackAction::FeedbackCaptured),
        "context feedback captured"
    );
    Ok(record)
}

pub(super) fn build_readback(
    session: &ContextFeedbackSessionRequest,
    source_document: &EditorDocument,
    snapshot: &ContextFeedbackSessionSnapshot,
    events: &[ContextFeedbackEvent],
) -> Result<ContextFeedbackReadback, EngineError> {
    let records = events
        .iter()
        .filter_map(|event| event.feedback.as_ref())
        .collect::<Vec<_>>();
    let latest = records.last();
    Ok(ContextFeedbackReadback {
        schema: IN_CONTEXT_FEEDBACK_SCHEMA.to_string(),
        session_id: session.session_id.clone(),
        project_id: session.project_id.clone(),
        document_id: source_document.calyx_document_id.clone(),
        source_revision_id: source_document.revision_id.clone(),
        source_document_fingerprint: stable_fingerprint(source_document)?,
        play_session_fingerprint: snapshot.play_session_fingerprint.clone(),
        play_build_fingerprint: snapshot.play_build_fingerprint.clone(),
        feedback_count: records.len(),
        screenshot_count: records.len(),
        marker_count: records.len(),
        latest_feedback_id: latest.map(|record| record.feedback_id.clone()),
        latest_constellation_id: latest.map(|record| record.constellation.constellation_id.clone()),
        latest_sextant_self_found: latest
            .map(|record| record.sextant.self_found)
            .unwrap_or(false),
        latest_loom_duplicate_count: latest
            .map(|record| record.loom.duplicate_feedback_ids.len())
            .unwrap_or(0),
        world_state_attached: records.iter().all(|record| {
            record.world_state.source_document_fingerprint
                == stable_fingerprint(source_document).unwrap_or_default()
        }),
        repro_attached: records.iter().all(|record| {
            record.repro.play_session_fingerprint == snapshot.play_session_fingerprint
        }),
        screenshot_blob_retrievable: records
            .iter()
            .all(|record| screenshot_blob_manifest_ok(&record.screenshot)),
        gui_route: IN_CONTEXT_FEEDBACK_GUI_ROUTE.to_string(),
        agent_route: IN_CONTEXT_FEEDBACK_AGENT_ROUTE.to_string(),
        capture_tool_name: IN_CONTEXT_FEEDBACK_CAPTURE_TOOL.to_string(),
        readback_tool_name: IN_CONTEXT_FEEDBACK_READBACK_TOOL.to_string(),
    })
}

pub(super) fn session_snapshot(
    session: &PlaySession,
) -> Result<ContextFeedbackSessionSnapshot, EngineError> {
    Ok(ContextFeedbackSessionSnapshot {
        play_session_state: session.state,
        play_session_fingerprint: stable_fingerprint(session)?,
        play_build_fingerprint: stable_fingerprint(&session.build)?,
        calyx_document_id: session.build.calyx_document_id.clone(),
        source_revision_id: session.build.source_revision_id.clone(),
        source_fingerprint: session.build.source_fingerprint.clone(),
        generation: session.generation,
        camera_rig: session.build.camera_rig,
        sim_seed: session.build.sim_seed,
        spawned_bodies: session.build.spawned_bodies.len(),
        visible_instances: session.build.visible_instance_ids.len(),
        draw_batches: session.build.draw_batches,
    })
}

fn annotation(
    ledger: &ContextFeedbackLedger,
    request: &ContextFeedbackCaptureRequest,
) -> Result<ContextFeedbackAnnotation, EngineError> {
    Ok(ContextFeedbackAnnotation {
        annotation_id: format!("annotation://issue1184/{}", request.feedback_id),
        motivation: "commenting".to_string(),
        body: ContextFeedbackBody {
            purpose: "commenting".to_string(),
            value: request.note.clone(),
            format: "text/plain".to_string(),
            creator: request.actor_id.clone(),
        },
        target: ContextFeedbackTarget {
            source: ledger.source_document.calyx_document_id.clone(),
            selector: ContextFeedbackSelector {
                selector_type: "WorldMarkerSelector".to_string(),
                marker_object_id: request.marker.target_object_id.clone(),
                marker_label: request.marker.label.clone(),
                coordinate_space: "world".to_string(),
                world_position: request.marker.world_position,
            },
            state: ContextFeedbackTargetState {
                source_revision_id: ledger.source_document.revision_id.clone(),
                source_document_fingerprint: stable_fingerprint(&ledger.source_document)?,
                play_session_fingerprint: ledger.session_snapshot.play_session_fingerprint.clone(),
                play_build_fingerprint: ledger.session_snapshot.play_build_fingerprint.clone(),
            },
        },
    })
}

fn world_state(
    ledger: &ContextFeedbackLedger,
    target: &EditorObject,
) -> Result<ContextFeedbackWorldState, EngineError> {
    Ok(ContextFeedbackWorldState {
        document_id: ledger.source_document.calyx_document_id.clone(),
        source_revision_id: ledger.source_document.revision_id.clone(),
        source_document_fingerprint: stable_fingerprint(&ledger.source_document)?,
        target_object_id: target.id.clone(),
        target_object_kind: target.kind.clone(),
        target_transform: target.transform,
        target_object_fingerprint: stable_fingerprint(target)?,
    })
}

fn repro(snapshot: &ContextFeedbackSessionSnapshot) -> ContextFeedbackRepro {
    ContextFeedbackRepro {
        camera_rig: snapshot.camera_rig,
        sim_seed: snapshot.sim_seed,
        generation: snapshot.generation,
        play_session_state: snapshot.play_session_state,
        play_session_fingerprint: snapshot.play_session_fingerprint.clone(),
        play_build_fingerprint: snapshot.play_build_fingerprint.clone(),
        spawned_bodies: snapshot.spawned_bodies,
        visible_instances: snapshot.visible_instances,
        draw_batches: snapshot.draw_batches,
    }
}

fn read_screenshot(
    input: &ContextFeedbackScreenshotInput,
) -> Result<ContextFeedbackScreenshotEvidence, EngineError> {
    validate_screenshot_input(input)?;
    let bytes = fs::read(Path::new(&input.path)).map_err(|error| {
        context_feedback_error(
            "screenshot.read",
            format!("failed to read screenshot {}: {error}", input.path),
        )
    })?;
    if bytes.is_empty() {
        return invalid_context_feedback("screenshot.bytes", "screenshot file is empty");
    }
    let sha256 = sha256_hex(&bytes);
    if sha256 != input.expected_sha256 {
        return invalid_context_feedback(
            "screenshot.sha256",
            format!(
                "screenshot sha256 mismatch expected {} actual {}",
                input.expected_sha256, sha256
            ),
        );
    }
    Ok(ContextFeedbackScreenshotEvidence {
        source_path: input.path.clone(),
        media_type: input.media_type.clone(),
        size_bytes: bytes.len(),
        blob: screenshot_blob_ref(&sha256, bytes.len()),
        sha256,
    })
}
