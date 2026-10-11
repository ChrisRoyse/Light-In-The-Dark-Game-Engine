use litd_hash::sha256_hex;
use std::fs;

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    edit_from_play::{
        EditFromPlayLedger, EditFromPlayRequest, EditFromPlayResumeRequest,
        MAX_EDIT_FROM_PLAY_ELAPSED_TICKS, parse_edit_from_play_ledger,
        pause_play_and_jump_to_editor, read_edit_from_play_ledger, resume_from_editor_edit,
        write_edit_from_play_ledger,
    },
    history::{EditorDocument, EditorObject, SelectionSet, stable_fingerprint},
    play::PlayCameraRig,
    shell::SnapSettings,
    viewport_transform::{ViewportTransformOperation, apply_viewport_operation},
};
use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE1185_FSV_ROOT",
        "issue1185_edit_from_play",
        "manual",
    )?;
    let aster_dir = root.join("aster");
    let ledger_path = root.join("edit-from-play-ledger.json");
    let artifact_path = root.join("issue1185-edit-from-play-readback.json");

    let document = fixture_document()?;
    let paused = pause_play_and_jump_to_editor(
        &document,
        EditFromPlayRequest::new(
            "issue1185-manual-ledger",
            "issue1185-manual-session",
            "human.qa.ember",
            "unit_playtest_hero",
            PlayCameraRig::TopDown,
        ),
    )?;
    println!(
        "happy_path before_edit target={} selected={} focus_match={} paused_hash={:016X}",
        paused.target.target_object_id,
        paused.readback.selected_object_ids.join("|"),
        paused.readback.editor_focus_matches_play_target,
        paused.readback.paused_snapshot_hash
    );
    let edited = moved_document(
        &paused.source_document,
        Vec3::new(2.0, 0.0, 1.0),
        "issue1185-manual-rev-edit",
    )?;
    let resumed = resume_from_editor_edit(
        &paused,
        EditFromPlayResumeRequest {
            actor_id: "human.qa.ember".to_string(),
            edit_id: "issue1185-manual-move-target".to_string(),
            target_object_id: "unit_playtest_hero".to_string(),
            edited_document: edited,
            edit_elapsed_ticks: 12,
        },
    )?;
    let writeback = write_edit_from_play_ledger(&ledger_path, &aster_dir, &resumed)?;
    let aster = read_edit_from_play_ledger(&aster_dir, &resumed.ledger_id)?
        .ok_or("missing edit-from-play ledger in Aster")?;
    let parsed = parse_edit_from_play_ledger(&aster.bytes)?;
    let state_bytes = fs::read(&ledger_path)?;
    if state_bytes != aster.bytes {
        return Err(format!("ledger state/Aster mismatch at {}", ledger_path.display()).into());
    }
    println!(
        "happy_path after_aster session={} target={} seq={} graph_rows={} ledger_sha={} state_matches_aster={} live={} round_trip={}",
        parsed.session_id,
        parsed.target.target_object_id,
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        state_bytes == aster.bytes,
        parsed.readback.live_change_visible,
        parsed.readback.round_trip_preserves_play_state
    );

    let edge_deleted = edge_deleted_while_paused(&paused)?;
    let edge_long = edge_long_edit(&paused)?;
    let edge_round_trip = edge_round_trip_preserves_play_state(&paused)?;

    let artifact = json!({
        "issue": 1185,
        "source_of_truth": "Aster Graph CF edit-from-play ledger rows plus JSON state-file readback and rebuilt play-session hashes",
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
            "pause": {
                "target_object_id": parsed.target.target_object_id,
                "selected_object_ids": parsed.readback.selected_object_ids,
                "editor_focus_matches_play_target": parsed.readback.editor_focus_matches_play_target,
                "paused_snapshot_hash": parsed.readback.paused_snapshot_hash,
                "paused_snapshot_hash_hex": format!("{:016X}", parsed.readback.paused_snapshot_hash)
            },
            "resume": {
                "state": format!("{:?}", parsed.readback.state),
                "source_document_fingerprint": parsed.readback.source_document_fingerprint,
                "current_document_fingerprint": parsed.readback.current_document_fingerprint,
                "resumed_play_build_fingerprint": parsed.readback.resumed_play_build_fingerprint,
                "live_change_visible": parsed.readback.live_change_visible,
                "round_trip_preserves_play_state": parsed.readback.round_trip_preserves_play_state,
                "target_after_translation": parsed.resume.as_ref().map(|resume| resume.target_after_translation)
            }
        },
        "edge_cases": [edge_deleted, edge_long, edge_round_trip]
    });
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    fs::write(&artifact_path, &artifact_bytes)?;
    let artifact_readback = fs::read(&artifact_path)?;
    if artifact_readback != artifact_bytes {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "wrote issue1185 edit-from-play FSV artifact={} bytes={} artifact_sha={} ledger_sha={}",
        artifact_path.display(),
        artifact_readback.len(),
        report_readback.sha256,
        aster.sha256
    );
    Ok(())
}

fn edge_deleted_while_paused(
    ledger: &EditFromPlayLedger,
) -> Result<serde_json::Value, EngineError> {
    let before = stable_fingerprint(&ledger.current_document)?;
    let mut deleted = ledger.source_document.clone();
    deleted.remove_object("unit_playtest_hero")?;
    let error = resume_from_editor_edit(
        ledger,
        EditFromPlayResumeRequest {
            actor_id: "human.qa.ember".to_string(),
            edit_id: "issue1185-manual-delete-target".to_string(),
            target_object_id: "unit_playtest_hero".to_string(),
            edited_document: deleted,
            edit_elapsed_ticks: 3,
        },
    )
    .unwrap_err()
    .to_string();
    let after = stable_fingerprint(&ledger.current_document)?;
    println!("edge_deleted before_fingerprint={before} after_fingerprint={after} error={error}");
    Ok(json!({
        "name": "deleted_while_paused_rejected",
        "before_fingerprint": before,
        "after_fingerprint": after,
        "state_unchanged": before == after,
        "error": error
    }))
}

fn edge_long_edit(ledger: &EditFromPlayLedger) -> Result<serde_json::Value, EngineError> {
    let edited = moved_document(
        &ledger.source_document,
        Vec3::new(1.0, 0.0, 0.0),
        "issue1185-manual-rev-long-edit",
    )?;
    let resumed = resume_from_editor_edit(
        ledger,
        EditFromPlayResumeRequest {
            actor_id: "human.qa.ember".to_string(),
            edit_id: "issue1185-manual-long-edit".to_string(),
            target_object_id: "unit_playtest_hero".to_string(),
            edited_document: edited,
            edit_elapsed_ticks: MAX_EDIT_FROM_PLAY_ELAPSED_TICKS,
        },
    )?;
    println!(
        "edge_long_edit elapsed={} live={} current_fingerprint={}",
        MAX_EDIT_FROM_PLAY_ELAPSED_TICKS,
        resumed.readback.live_change_visible,
        resumed.readback.current_document_fingerprint
    );
    Ok(json!({
        "name": "long_edit_resume_stable",
        "elapsed_ticks": MAX_EDIT_FROM_PLAY_ELAPSED_TICKS,
        "live_change_visible": resumed.readback.live_change_visible,
        "current_document_fingerprint": resumed.readback.current_document_fingerprint
    }))
}

fn edge_round_trip_preserves_play_state(
    ledger: &EditFromPlayLedger,
) -> Result<serde_json::Value, EngineError> {
    let edited = moved_document(
        &ledger.source_document,
        Vec3::new(0.0, 0.0, 2.0),
        "issue1185-manual-rev-round-trip",
    )?;
    let resumed = resume_from_editor_edit(
        ledger,
        EditFromPlayResumeRequest {
            actor_id: "human.qa.ember".to_string(),
            edit_id: "issue1185-manual-round-trip".to_string(),
            target_object_id: "unit_playtest_hero".to_string(),
            edited_document: edited,
            edit_elapsed_ticks: 9,
        },
    )?;
    println!(
        "edge_round_trip before_hash={:016X} after_hash={:016X} selected={} preserved={}",
        ledger.target.paused_snapshot_hash,
        resumed.target.paused_snapshot_hash,
        resumed.readback.selected_object_ids.join("|"),
        resumed.readback.round_trip_preserves_play_state
    );
    Ok(json!({
        "name": "round_trip_preserves_play_state",
        "before_paused_snapshot_hash": ledger.target.paused_snapshot_hash,
        "after_paused_snapshot_hash": resumed.target.paused_snapshot_hash,
        "selected_object_ids": resumed.readback.selected_object_ids,
        "preserved": resumed.readback.round_trip_preserves_play_state
    }))
}

fn moved_document(
    document: &EditorDocument,
    delta: Vec3,
    revision_id: &str,
) -> Result<EditorDocument, EngineError> {
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
    )?;
    Ok(next)
}

fn fixture_document() -> Result<EditorDocument, EngineError> {
    let mut document =
        EditorDocument::new("calyx://world/issue1185-edit-from-play", "issue1185-rev-1")?;
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
            field: "editor.edit_from_play.fsv.asset".to_string(),
            reason: "missing real Quaternius Rock1 asset record".to_string(),
        })?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    Ok(document)
}
