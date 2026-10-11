use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
};
use litd_engine::math::{Transform, Vec3};
use sha2::{Digest, Sha256};

use super::*;

#[test]
fn issue1184_context_feedback_source_of_truth_fsv() {
    let document = fixture_document();
    let root = test_root("source-of-truth");
    fs::create_dir_all(&root).unwrap();
    let ledger_path = root.join("context-feedback-ledger.json");
    let aster_dir = root.join("aster");
    let screenshot_path = screenshot_fixture();
    let screenshot_sha = sha256_file(&screenshot_path);
    let mut ledger = start_context_feedback_session(
        &document,
        ContextFeedbackSessionRequest::new(
            "issue1184-context-ledger",
            "issue1184-context-session",
            "litd.project.issue1184.fsv",
            "human.qa.ember",
            PlayCameraRig::TopDown,
        ),
    )
    .unwrap();
    println!(
        "issue1184 happy before_feedback session={} feedback={} doc_hash={} play_hash={}",
        ledger.readback.session_id,
        ledger.readback.feedback_count,
        ledger.readback.source_document_fingerprint,
        ledger.readback.play_session_fingerprint
    );

    ledger = capture_context_feedback(
        &ledger,
        request(
            "feedback-ember-gate-collision",
            "Ember gate pathing collision clips the hero near the arch",
            &screenshot_path,
            &screenshot_sha,
        ),
    )
    .unwrap();
    ledger = capture_context_feedback(
        &ledger,
        request(
            "feedback-ember-gate-collision-duplicate",
            "Ember gate pathing clips the hero near the arch",
            &screenshot_path,
            &screenshot_sha,
        ),
    )
    .unwrap();
    let readback = readback_context_feedback_session(&ledger).unwrap();
    assert_eq!(readback.feedback_count, 2);
    assert_eq!(readback.screenshot_count, 2);
    assert!(readback.latest_sextant_self_found);
    assert_eq!(readback.latest_loom_duplicate_count, 1);
    assert!(readback.world_state_attached);
    assert!(readback.repro_attached);
    assert!(readback.screenshot_blob_retrievable);
    println!(
        "issue1184 happy before_write feedback={} screenshots={} sextant={} loom_duplicates={}",
        readback.feedback_count,
        readback.screenshot_count,
        readback.latest_sextant_self_found,
        readback.latest_loom_duplicate_count
    );

    let writeback = write_context_feedback_ledger(&ledger_path, &aster_dir, &ledger).unwrap();
    let aster = read_context_feedback_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_context_feedback_ledger(&aster.bytes).unwrap();
    assert!(!ledger_path.exists());
    assert_eq!(writeback.bytes, aster.bytes);
    assert_eq!(parsed.readback.feedback_count, 2);
    assert_eq!(parsed.readback.screenshot_count, 2);
    assert!(parsed.readback.latest_sextant_self_found);
    assert_eq!(parsed.readback.latest_loom_duplicate_count, 1);
    assert_eq!(aster.screenshot_blobs.len(), 1);
    assert_eq!(aster.screenshot_blobs[0].sha256, screenshot_sha);
    assert_eq!(
        aster.screenshot_blobs[0].bytes,
        fs::read(&screenshot_path).unwrap()
    );
    let latest = parsed.events.last().unwrap().feedback.as_ref().unwrap();
    assert_eq!(latest.screenshot.sha256, screenshot_sha);
    assert_eq!(
        latest.screenshot.size_bytes,
        fs::read(&screenshot_path).unwrap().len()
    );
    assert!(
        latest
            .sextant
            .hits
            .iter()
            .any(|hit| { hit.constellation_id == latest.constellation.constellation_id })
    );
    assert_eq!(
        latest.loom.duplicate_feedback_ids,
        vec!["feedback-ember-gate-collision".to_string()]
    );
    println!(
        "issue1184 happy after_aster session={} seq={} graph_rows={} ledger_sha={} state_matches_aster={} cell={} index={} screenshot_sha={}",
        parsed.readback.session_id,
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        writeback.bytes == aster.bytes,
        writeback.cell_key,
        writeback.index_key,
        latest.screenshot.sha256
    );
}

#[test]
fn issue1184_edges_fail_closed_without_state_mutation() {
    let document = fixture_document();
    let screenshot_path = screenshot_fixture();
    let screenshot_sha = sha256_file(&screenshot_path);
    let ledger = start_context_feedback_session(
        &document,
        ContextFeedbackSessionRequest::new(
            "issue1184-edge-ledger",
            "issue1184-edge-session",
            "litd.project.issue1184.fsv",
            "human.qa.ember",
            PlayCameraRig::TopDown,
        ),
    )
    .unwrap();

    let before_empty = readback_context_feedback_session(&ledger).unwrap();
    let empty_error = capture_context_feedback(
        &ledger,
        request("feedback-empty", "", &screenshot_path, &screenshot_sha),
    )
    .unwrap_err()
    .to_string();
    let after_empty = readback_context_feedback_session(&ledger).unwrap();
    assert_eq!(before_empty, after_empty);
    assert!(empty_error.contains("editor.context_feedback.feedback.note"));
    println!(
        "issue1184 edge_empty before_feedback={} after_feedback={} error={}",
        before_empty.feedback_count, after_empty.feedback_count, empty_error
    );

    let before_missing = readback_context_feedback_session(&ledger).unwrap();
    let mut missing = request(
        "feedback-missing-marker",
        "Ember gate collision note",
        &screenshot_path,
        &screenshot_sha,
    );
    missing.marker.target_object_id = "missing_unit".to_string();
    let missing_error = capture_context_feedback(&ledger, missing)
        .unwrap_err()
        .to_string();
    let after_missing = readback_context_feedback_session(&ledger).unwrap();
    assert_eq!(before_missing, after_missing);
    assert!(missing_error.contains("editor.context_feedback.marker.target"));
    println!(
        "issue1184 edge_missing_marker before_feedback={} after_feedback={} error={}",
        before_missing.feedback_count, after_missing.feedback_count, missing_error
    );

    let before_bad_sha = readback_context_feedback_session(&ledger).unwrap();
    let bad_sha_error = capture_context_feedback(
        &ledger,
        request(
            "feedback-bad-sha",
            "Ember gate screenshot mismatch",
            &screenshot_path,
            &"0".repeat(64),
        ),
    )
    .unwrap_err()
    .to_string();
    let after_bad_sha = readback_context_feedback_session(&ledger).unwrap();
    assert_eq!(before_bad_sha, after_bad_sha);
    assert!(bad_sha_error.contains("editor.context_feedback.screenshot.sha256"));
    println!(
        "issue1184 edge_bad_sha before_feedback={} after_feedback={} error={}",
        before_bad_sha.feedback_count, after_bad_sha.feedback_count, bad_sha_error
    );
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

fn fixture_document() -> EditorDocument {
    let mut document = EditorDocument::new(
        "calyx://world/issue1184-context-feedback",
        "issue1184-rev-1",
    )
    .unwrap();
    document
        .upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    document
        .upsert_object(
            EditorObject::new(
                "unit_playtest_hero",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(3.0, 0.0, -6.0)),
            )
            .with_property("faction", "Dawnward"),
        )
        .unwrap();
    let record = AssetVaultCatalog::load_default_cached()
        .unwrap()
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .unwrap();
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record).unwrap();
    document
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

fn test_root(label: &str) -> PathBuf {
    crate::test_scratch::issue_scratch_dir("issue1184", label)
}

fn sha256_file(path: &PathBuf) -> String {
    let bytes = fs::read(path).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
