use litd_hash::sha256_hex;
use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    in_context_feedback::{
        ContextFeedbackCaptureRequest, ContextFeedbackMarker, ContextFeedbackScreenshotInput,
        ContextFeedbackSessionRequest, capture_context_feedback, parse_context_feedback_ledger,
        read_context_feedback_ledger, readback_context_feedback_session,
        start_context_feedback_session, write_context_feedback_ledger,
    },
    play::PlayCameraRig,
};
use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE1184_FSV_ROOT",
        "issue1184_context_feedback",
        "manual",
    )?;
    let aster_dir = root.join("aster");
    let ledger_path = root.join("context-feedback-ledger.json");
    let artifact_path = root.join("issue1184-context-feedback-readback.json");
    let screenshot_path = screenshot_fixture();
    let screenshot_bytes = fs::read(&screenshot_path)?;
    let screenshot_sha = sha256_hex(&screenshot_bytes);

    let document = fixture_document()?;
    let mut ledger = start_context_feedback_session(
        &document,
        ContextFeedbackSessionRequest::new(
            "issue1184-manual-ledger",
            "issue1184-manual-session",
            "litd.project.issue1184.manual",
            "human.qa.ember",
            PlayCameraRig::TopDown,
        ),
    )?;
    let before = readback_context_feedback_session(&ledger)?;
    println!(
        "happy_path before_feedback session={} feedback={} doc_hash={} play_hash={}",
        before.session_id,
        before.feedback_count,
        before.source_document_fingerprint,
        before.play_session_fingerprint
    );

    ledger = capture_context_feedback(
        &ledger,
        request(
            "manual-ember-gate-collision",
            "Ember gate pathing collision clips the hero near the arch",
            &screenshot_path,
            &screenshot_sha,
        ),
    )?;
    ledger = capture_context_feedback(
        &ledger,
        request(
            "manual-ember-gate-collision-duplicate",
            "Ember gate pathing clips the hero near the arch",
            &screenshot_path,
            &screenshot_sha,
        ),
    )?;
    let writeback = write_context_feedback_ledger(&ledger_path, &aster_dir, &ledger)?;
    let aster = read_context_feedback_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing context-feedback ledger in Aster")?;
    let parsed = parse_context_feedback_ledger(&aster.bytes)?;
    let state_bytes = fs::read(&ledger_path)?;
    if state_bytes != aster.bytes {
        return Err(format!("ledger state/Aster mismatch at {}", ledger_path.display()).into());
    }
    let latest = parsed
        .events
        .last()
        .and_then(|event| event.feedback.as_ref())
        .ok_or("missing latest feedback record")?;
    println!(
        "happy_path after_aster session={} feedback={} screenshots={} seq={} graph_rows={} ledger_sha={} state_matches_aster={} screenshot_sha={} sextant_self={} loom_duplicates={}",
        parsed.readback.session_id,
        parsed.readback.feedback_count,
        parsed.readback.screenshot_count,
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        state_bytes == aster.bytes,
        latest.screenshot.sha256,
        parsed.readback.latest_sextant_self_found,
        parsed.readback.latest_loom_duplicate_count
    );

    let edge_empty = edge_empty_note(&ledger, &screenshot_path, &screenshot_sha)?;
    let edge_missing = edge_missing_marker(&ledger, &screenshot_path, &screenshot_sha)?;
    let edge_bad_sha = edge_bad_screenshot_sha(&ledger, &screenshot_path)?;

    let stored_blob = aster
        .screenshot_blobs
        .iter()
        .find(|blob| blob.sha256 == latest.screenshot.sha256)
        .ok_or("missing reconstructed screenshot blob in Aster readback")?;
    if stored_blob.bytes != screenshot_bytes {
        return Err("stored screenshot blob did not match real screenshot bytes".into());
    }
    let artifact = json!({
        "issue": 1184,
        "source_of_truth": "Aster Graph CF context-feedback ledger rows plus JSON state-file readback and decoded screenshot blob bytes",
        "artifact_path": artifact_path,
        "happy_path": {
            "ledger": {
                "path": ledger_path,
                "state_file_matches_aster": state_bytes == aster.bytes,
                "state_sha256": sha256_hex(&state_bytes),
                "aster_sha256": aster.sha256,
                "aster_seq": aster.seq,
                "aster_graph_rows": aster.graph_rows,
                "cell_key": writeback.cell_key,
                "index_key": writeback.index_key,
                "bytes": state_bytes.len()
            },
            "feedback": {
                "feedback_count": parsed.readback.feedback_count,
                "latest_feedback_id": parsed.readback.latest_feedback_id,
                "latest_constellation_id": parsed.readback.latest_constellation_id,
                "world_state_attached": parsed.readback.world_state_attached,
                "repro_attached": parsed.readback.repro_attached,
                "marker_target": latest.world_state.target_object_id,
                "marker_position": latest.annotation.target.selector.world_position
            },
            "screenshot": {
                "source_path": screenshot_path,
                "source_sha256": screenshot_sha,
                "source_bytes": screenshot_bytes.len(),
                "stored_sha256": latest.screenshot.sha256,
                "stored_bytes": latest.screenshot.size_bytes,
                "stored_chunk_count": stored_blob.chunk_count,
                "reconstructed_blob_matches_source": stored_blob.bytes == screenshot_bytes
            },
            "sextant": {
                "self_found": latest.sextant.self_found,
                "self_hit_rank": latest.sextant.self_hit_rank,
                "world_memory_hit_count": latest.sextant.world_memory_hit_count,
                "definition_slots": latest.sextant.definition.definition.as_ref().map(|definition| definition.slots.len()).unwrap_or_default(),
                "neighbor_hits": latest.sextant.neighbors.hits.len()
            },
            "loom": {
                "duplicate_feedback_ids": latest.loom.duplicate_feedback_ids,
                "highest_overlap": latest.loom.highest_overlap,
                "method": latest.loom.method
            }
        },
        "edge_cases": [edge_empty, edge_missing, edge_bad_sha]
    });
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    fs::write(&artifact_path, &artifact_bytes)?;
    let artifact_readback = fs::read(&artifact_path)?;
    if artifact_readback != artifact_bytes {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "wrote issue1184 context-feedback FSV artifact={} bytes={} artifact_sha={} ledger_sha={}",
        artifact_path.display(),
        artifact_readback.len(),
        report_readback.sha256,
        aster.sha256
    );
    Ok(())
}

fn edge_empty_note(
    ledger: &litd_editor::in_context_feedback::ContextFeedbackLedger,
    screenshot_path: &Path,
    screenshot_sha: &str,
) -> Result<serde_json::Value, EngineError> {
    let before = readback_context_feedback_session(ledger)?;
    let error = capture_context_feedback(
        ledger,
        request("manual-empty-note", "", screenshot_path, screenshot_sha),
    )
    .unwrap_err()
    .to_string();
    let after = readback_context_feedback_session(ledger)?;
    println!(
        "edge_empty before_feedback={} after_feedback={} error={}",
        before.feedback_count, after.feedback_count, error
    );
    Ok(json!({
        "name": "empty_note_rejected",
        "before_feedback": before.feedback_count,
        "after_feedback": after.feedback_count,
        "state_unchanged": before == after,
        "error": error
    }))
}

fn edge_missing_marker(
    ledger: &litd_editor::in_context_feedback::ContextFeedbackLedger,
    screenshot_path: &Path,
    screenshot_sha: &str,
) -> Result<serde_json::Value, EngineError> {
    let before = readback_context_feedback_session(ledger)?;
    let mut missing = request(
        "manual-missing-marker",
        "Ember gate collision marker missing",
        screenshot_path,
        screenshot_sha,
    );
    missing.marker.target_object_id = "missing_unit".to_string();
    let error = capture_context_feedback(ledger, missing)
        .unwrap_err()
        .to_string();
    let after = readback_context_feedback_session(ledger)?;
    println!(
        "edge_missing_marker before_feedback={} after_feedback={} error={}",
        before.feedback_count, after.feedback_count, error
    );
    Ok(json!({
        "name": "missing_marker_rejected",
        "before_feedback": before.feedback_count,
        "after_feedback": after.feedback_count,
        "state_unchanged": before == after,
        "error": error
    }))
}

fn edge_bad_screenshot_sha(
    ledger: &litd_editor::in_context_feedback::ContextFeedbackLedger,
    screenshot_path: &Path,
) -> Result<serde_json::Value, EngineError> {
    let before = readback_context_feedback_session(ledger)?;
    let error = capture_context_feedback(
        ledger,
        request(
            "manual-bad-screenshot-sha",
            "Ember gate screenshot hash mismatch",
            screenshot_path,
            &"0".repeat(64),
        ),
    )
    .unwrap_err()
    .to_string();
    let after = readback_context_feedback_session(ledger)?;
    println!(
        "edge_bad_sha before_feedback={} after_feedback={} error={}",
        before.feedback_count, after.feedback_count, error
    );
    Ok(json!({
        "name": "bad_screenshot_sha_rejected",
        "before_feedback": before.feedback_count,
        "after_feedback": after.feedback_count,
        "state_unchanged": before == after,
        "error": error
    }))
}

fn request(
    feedback_id: &str,
    note: &str,
    screenshot_path: &Path,
    screenshot_sha: &str,
) -> ContextFeedbackCaptureRequest {
    ContextFeedbackCaptureRequest {
        feedback_id: feedback_id.to_string(),
        actor_id: "human.qa.ember".to_string(),
        note: note.to_string(),
        severity: "bug".to_string(),
        marker: ContextFeedbackMarker {
            target_object_id: "unit_playtest_hero".to_string(),
            label: "ember gate hero marker".to_string(),
            world_position: Vec3::new(3.0, 0.0, -6.0),
        },
        screenshot: ContextFeedbackScreenshotInput {
            path: screenshot_path.display().to_string(),
            expected_sha256: screenshot_sha.to_string(),
            media_type: "image/png".to_string(),
        },
    }
}

fn fixture_document() -> Result<EditorDocument, EngineError> {
    let mut document = EditorDocument::new(
        "calyx://world/issue1184-context-feedback",
        "issue1184-rev-1",
    )?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    document.upsert_object(
        EditorObject::new(
            "unit_playtest_hero",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(3.0, 0.0, -6.0)),
        )
        .with_property("faction", "Dawnward"),
    )?;
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.context_feedback.fsv.asset".to_string(),
            reason: "missing real Quaternius Rock1 asset record".to_string(),
        })?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    Ok(document)
}

fn screenshot_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("litd-editor is under workspace crates")
        .join("docs")
        .join("readme")
        .join("ED12-play-topdown.png")
}
