use std::{fs, path::PathBuf};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject, SelectionSet, stable_fingerprint},
    play::PlayCameraRig,
    shell::SnapSettings,
    viewport_transform::{ViewportTransformOperation, apply_viewport_operation},
};
use litd_engine::math::{Transform, Vec3};

use super::*;

#[test]
fn issue1185_edit_from_play_source_of_truth_fsv() {
    let document = fixture_document();
    let root = test_root("source-of-truth");
    fs::create_dir_all(&root).unwrap();
    let ledger_path = root.join("edit-from-play-ledger.json");
    let aster_dir = root.join("aster");

    let paused = pause_play_and_jump_to_editor(
        &document,
        EditFromPlayRequest::new(
            "issue1185-edit-bridge-ledger",
            "issue1185-edit-bridge-session",
            "human.qa.ember",
            "unit_playtest_hero",
            PlayCameraRig::TopDown,
        ),
    )
    .unwrap();
    println!(
        "issue1185 happy before_edit target={} selected={} focus_match={} paused_hash={:016X}",
        paused.target.target_object_id,
        paused.readback.selected_object_ids.join("|"),
        paused.readback.editor_focus_matches_play_target,
        paused.readback.paused_snapshot_hash
    );

    let edited = moved_document(
        &paused.source_document,
        Vec3::new(2.0, 0.0, 1.0),
        "issue1185-rev-edit",
    );
    let resumed = resume_from_editor_edit(
        &paused,
        EditFromPlayResumeRequest {
            actor_id: "human.qa.ember".to_string(),
            edit_id: "issue1185-move-target".to_string(),
            target_object_id: "unit_playtest_hero".to_string(),
            edited_document: edited,
            edit_elapsed_ticks: 12,
        },
    )
    .unwrap();
    let writeback = write_edit_from_play_ledger(&ledger_path, &aster_dir, &resumed).unwrap();
    let aster = read_edit_from_play_ledger(&aster_dir, &resumed.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_edit_from_play_ledger(&aster.bytes).unwrap();

    assert!(!ledger_path.exists());
    assert_eq!(writeback.bytes, aster.bytes);
    assert!(parsed.readback.editor_focus_matches_play_target);
    assert!(parsed.readback.live_change_visible);
    assert!(parsed.readback.round_trip_preserves_play_state);
    assert_eq!(parsed.readback.target_object_id, "unit_playtest_hero");
    assert_eq!(
        parsed.current_document.objects["unit_playtest_hero"]
            .transform
            .translation,
        Vec3::new(5.0, 0.0, -5.0)
    );
    println!(
        "issue1185 happy after_aster session={} target={} seq={} graph_rows={} ledger_sha={} state_matches_aster={} live={} round_trip={}",
        parsed.session_id,
        parsed.target.target_object_id,
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        writeback.bytes == aster.bytes,
        parsed.readback.live_change_visible,
        parsed.readback.round_trip_preserves_play_state
    );
}

#[test]
fn issue1185_edges_fail_closed_and_preserve_state() {
    let document = fixture_document();
    let paused = pause_play_and_jump_to_editor(
        &document,
        EditFromPlayRequest::new(
            "issue1185-edge-ledger",
            "issue1185-edge-session",
            "human.qa.ember",
            "unit_playtest_hero",
            PlayCameraRig::TopDown,
        ),
    )
    .unwrap();

    let before_delete_fingerprint = stable_fingerprint(&paused.current_document).unwrap();
    let mut deleted = paused.source_document.clone();
    deleted.remove_object("unit_playtest_hero").unwrap();
    let deleted_error = resume_from_editor_edit(
        &paused,
        EditFromPlayResumeRequest {
            actor_id: "human.qa.ember".to_string(),
            edit_id: "issue1185-delete-target".to_string(),
            target_object_id: "unit_playtest_hero".to_string(),
            edited_document: deleted,
            edit_elapsed_ticks: 3,
        },
    )
    .unwrap_err()
    .to_string();
    assert!(deleted_error.contains("editor.edit_from_play.resume.target_deleted"));
    assert_eq!(
        stable_fingerprint(&paused.current_document).unwrap(),
        before_delete_fingerprint
    );
    println!(
        "issue1185 edge_deleted before_fingerprint={} after_fingerprint={} error={}",
        before_delete_fingerprint,
        stable_fingerprint(&paused.current_document).unwrap(),
        deleted_error
    );

    let long_edit = moved_document(
        &paused.source_document,
        Vec3::new(1.0, 0.0, 0.0),
        "issue1185-rev-long-edit",
    );
    let long_resumed = resume_from_editor_edit(
        &paused,
        EditFromPlayResumeRequest {
            actor_id: "human.qa.ember".to_string(),
            edit_id: "issue1185-long-edit".to_string(),
            target_object_id: "unit_playtest_hero".to_string(),
            edited_document: long_edit,
            edit_elapsed_ticks: MAX_EDIT_FROM_PLAY_ELAPSED_TICKS,
        },
    )
    .unwrap();
    assert!(long_resumed.readback.live_change_visible);
    assert_eq!(
        long_resumed.readback.edit_elapsed_ticks,
        Some(MAX_EDIT_FROM_PLAY_ELAPSED_TICKS)
    );
    println!(
        "issue1185 edge_long_edit elapsed={} live={} current_fingerprint={}",
        MAX_EDIT_FROM_PLAY_ELAPSED_TICKS,
        long_resumed.readback.live_change_visible,
        long_resumed.readback.current_document_fingerprint
    );

    let round_trip = moved_document(
        &paused.source_document,
        Vec3::new(0.0, 0.0, 2.0),
        "issue1185-rev-round-trip",
    );
    let round_resumed = resume_from_editor_edit(
        &paused,
        EditFromPlayResumeRequest {
            actor_id: "human.qa.ember".to_string(),
            edit_id: "issue1185-round-trip".to_string(),
            target_object_id: "unit_playtest_hero".to_string(),
            edited_document: round_trip,
            edit_elapsed_ticks: 9,
        },
    )
    .unwrap();
    assert_eq!(
        round_resumed.target.paused_snapshot_hash,
        paused.target.paused_snapshot_hash
    );
    assert!(round_resumed.readback.round_trip_preserves_play_state);
    println!(
        "issue1185 edge_round_trip before_hash={:016X} after_hash={:016X} selected={} preserved={}",
        paused.target.paused_snapshot_hash,
        round_resumed.target.paused_snapshot_hash,
        round_resumed.readback.selected_object_ids.join("|"),
        round_resumed.readback.round_trip_preserves_play_state
    );
}

fn moved_document(document: &EditorDocument, delta: Vec3, revision_id: &str) -> EditorDocument {
    let mut next = document.clone();
    next.revision_id = revision_id.to_string();
    let mut selection = SelectionSet::default();
    selection.select_many(["unit_playtest_hero"]);
    apply_viewport_operation(
        &mut next,
        &selection,
        ViewportTransformOperation::Move {
            delta,
            snap: SnapSettings {
                grid_enabled: false,
                grid_size: 1.0,
                rotation_enabled: false,
                rotation_degrees: 15.0,
                scale_enabled: false,
                scale_step: 0.1,
            },
        },
        None,
    )
    .unwrap();
    next
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1185-edit-from-play", "issue1185-rev-1").unwrap();
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

fn test_root(label: &str) -> PathBuf {
    crate::test_scratch::issue_scratch_dir("issue1185", label)
}
