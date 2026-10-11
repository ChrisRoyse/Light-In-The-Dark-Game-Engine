use litd_hash::sha256_hex;
use std::fs;

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    play::{PlayCameraRig, PlaySession},
    play_session_capture::{
        PlaySessionCaptureRequest, PlaySessionCaptureRequestArgs, PlaySessionVariantInput,
        capture_play_session, parse_play_session_capture_ledger, read_play_session_capture_ledger,
        restart_session_again, toggle_ab_variant, write_play_session_capture_ledger,
    },
};
use litd_engine::{
    EngineError,
    math::{FixedVec3, Transform, Vec3},
    sim::SimBodyId,
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE1186_FSV_ROOT",
        "issue1186_session_capture",
        "manual",
    )?;
    let aster_dir = root.join("aster");
    let ledger_path = root.join("session-capture-ledger.json");
    let artifact_path = root.join("issue1186-session-capture-readback.json");

    let variants = fixture_variants()?;
    let request = request(&variants[0].document, "issue1186-manual-ledger", "a")?;
    let ledger = capture_play_session(variants, request)?;
    println!(
        "happy_path before_write active={} final_hash={:016X} restart_hash={:016X} variants={} checkpoint={}",
        ledger.active_variant_id,
        ledger.readback.full_replay_final_hash,
        ledger.readback.restart_final_hash,
        ledger.readback.variant_count,
        ledger.readback.checkpoint_tick
    );
    let toggled = toggle_ab_variant(&ledger, "b")?;
    let writeback = write_play_session_capture_ledger(&ledger_path, &aster_dir, &toggled)?;
    let aster = read_play_session_capture_ledger(&aster_dir, &toggled.ledger_id)?
        .ok_or("missing session-capture ledger in Aster")?;
    let parsed = parse_play_session_capture_ledger(&aster.bytes)?;
    let state_bytes = fs::read(&ledger_path)?;
    if state_bytes != aster.bytes {
        return Err(format!("ledger state/Aster mismatch at {}", ledger_path.display()).into());
    }
    println!(
        "happy_path after_aster session={} active={} seq={} graph_rows={} ledger_sha={} state_matches_aster={}",
        parsed.session_id,
        parsed.active_variant_id,
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        state_bytes == aster.bytes
    );

    let edge_restart = edge_restart_checkpoint(&ledger)?;
    let edge_corrupt = edge_corrupted_session(&ledger)?;
    let edge_isolation = edge_ab_isolation(&ledger)?;

    let artifact = json!({
        "issue": 1186,
        "source_of_truth": "Aster Graph CF play-session capture ledger rows plus JSON state-file readback and deterministic replay hashes",
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
            "session": {
                "session_id": parsed.session_id,
                "active_variant_id": parsed.active_variant_id,
                "variant_count": parsed.readback.variant_count,
                "variant_catalog_fingerprint": parsed.readback.variant_catalog_fingerprint,
                "a_b_isolated": parsed.readback.a_b_isolated,
                "lowering_boundary": parsed.readback.lowering_boundary,
                "lowered_order_count": parsed.readback.lowered_order_count,
                "human_order_count": parsed.readback.human_order_count
            },
            "replay": {
                "run_ticks": parsed.readback.run_ticks,
                "checkpoint_tick": parsed.readback.checkpoint_tick,
                "full_replay_final_hash": parsed.readback.full_replay_final_hash,
                "full_replay_final_hash_hex": format!("{:016X}", parsed.readback.full_replay_final_hash),
                "restart_final_hash": parsed.readback.restart_final_hash,
                "restart_final_hash_hex": format!("{:016X}", parsed.readback.restart_final_hash),
                "replay_final_hash_matches": parsed.readback.replay_final_hash_matches,
                "restart_matches_full_replay": parsed.readback.restart_matches_full_replay
            }
        },
        "edge_cases": [edge_restart, edge_corrupt, edge_isolation]
    });
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    fs::write(&artifact_path, &artifact_bytes)?;
    let artifact_readback = fs::read(&artifact_path)?;
    if artifact_readback != artifact_bytes {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "wrote issue1186 session-capture FSV artifact={} bytes={} artifact_sha={} ledger_sha={}",
        artifact_path.display(),
        artifact_readback.len(),
        report_readback.sha256,
        aster.sha256
    );
    Ok(())
}

fn edge_restart_checkpoint(
    ledger: &litd_editor::play_session_capture::PlaySessionCaptureLedger,
) -> Result<serde_json::Value, EngineError> {
    let restarted = restart_session_again(ledger)?;
    println!(
        "edge_restart before_events={} after_events={} checkpoint={} final_hash={:016X}",
        ledger.events.len(),
        restarted.events.len(),
        restarted.checkpoint.checkpoint_tick,
        restarted.restart.restart_final_hash
    );
    Ok(json!({
        "name": "mid_session_checkpoint_restart",
        "before_events": ledger.events.len(),
        "after_events": restarted.events.len(),
        "checkpoint_tick": restarted.checkpoint.checkpoint_tick,
        "restart_final_hash": restarted.restart.restart_final_hash,
        "restart_final_hash_hex": format!("{:016X}", restarted.restart.restart_final_hash),
        "full_replay_final_hash": ledger.replay.expected_final_hash,
        "matches_full_replay": restarted.restart.matches_full_replay
    }))
}

fn edge_corrupted_session(
    ledger: &litd_editor::play_session_capture::PlaySessionCaptureLedger,
) -> Result<serde_json::Value, EngineError> {
    let bytes =
        serde_json::to_vec_pretty(ledger).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize edge ledger: {error}"),
        })?;
    let mut corrupted = bytes.clone();
    corrupted.truncate(bytes.len() / 3);
    let error = parse_play_session_capture_ledger(&corrupted)
        .unwrap_err()
        .to_string();
    println!(
        "edge_corrupt before_bytes={} after_bytes={} error={}",
        bytes.len(),
        corrupted.len(),
        error
    );
    Ok(json!({
        "name": "corrupted_session_rejected",
        "before_bytes": bytes.len(),
        "after_bytes": corrupted.len(),
        "state_unchanged": bytes.len() != corrupted.len(),
        "error": error
    }))
}

fn edge_ab_isolation(
    ledger: &litd_editor::play_session_capture::PlaySessionCaptureLedger,
) -> Result<serde_json::Value, EngineError> {
    let before_a = ledger
        .variants
        .iter()
        .find(|variant| variant.variant_id == "a")
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.session_capture.fsv.variant".to_string(),
            reason: "missing variant a".to_string(),
        })?
        .document_fingerprint
        .clone();
    let before_catalog = ledger.readback.variant_catalog_fingerprint.clone();
    let toggled = toggle_ab_variant(ledger, "b")?;
    let after_a = toggled
        .variants
        .iter()
        .find(|variant| variant.variant_id == "a")
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.session_capture.fsv.variant".to_string(),
            reason: "missing variant a after toggle".to_string(),
        })?
        .document_fingerprint
        .clone();
    println!(
        "edge_ab_isolation before_active={} after_active={} a_unchanged={} catalog_unchanged={}",
        ledger.active_variant_id,
        toggled.active_variant_id,
        before_a == after_a,
        before_catalog == toggled.readback.variant_catalog_fingerprint
    );
    Ok(json!({
        "name": "ab_variants_stay_isolated",
        "before_active": ledger.active_variant_id,
        "after_active": toggled.active_variant_id,
        "a_fingerprint_before": before_a,
        "a_fingerprint_after": after_a,
        "a_unchanged": before_a == after_a,
        "catalog_unchanged": before_catalog == toggled.readback.variant_catalog_fingerprint,
        "a_b_isolated": toggled.readback.a_b_isolated
    }))
}

fn request(
    document: &EditorDocument,
    ledger_id: &str,
    active_variant_id: &str,
) -> Result<PlaySessionCaptureRequest, EngineError> {
    Ok(PlaySessionCaptureRequest::new(
        PlaySessionCaptureRequestArgs {
            ledger_id,
            session_id: "issue1186-manual-session",
            actor_id: "human.qa.ember",
            active_variant_id,
            camera_rig: PlayCameraRig::TopDown,
            run_ticks: 4,
            checkpoint_tick: 2,
            orders: orders(document)?,
        },
    ))
}

fn orders(document: &EditorDocument) -> Result<Vec<SimOrder>, EngineError> {
    let session = PlaySession::start(document, PlayCameraRig::TopDown)?;
    let body = SimBodyId::from_raw(session.build.spawned_bodies[0].body_id);
    Ok(vec![
        SimOrder::new(
            1,
            SimOrderSource::human(1),
            1,
            body,
            SimOrderKind::SetVelocity(FixedVec3::from_i32s(64, 0, 0)),
        ),
        SimOrder::new(
            3,
            SimOrderSource::lowered_agent(7),
            1,
            body,
            SimOrderKind::AddVelocity(FixedVec3::from_i32s(0, 64, 0)),
        ),
    ])
}

fn fixture_variants() -> Result<Vec<PlaySessionVariantInput>, EngineError> {
    Ok(vec![
        PlaySessionVariantInput {
            variant_id: "a".to_string(),
            label: "baseline combat route".to_string(),
            document: fixture_document("issue1186-rev-a", 3.0, "Dawnward")?,
        },
        PlaySessionVariantInput {
            variant_id: "b".to_string(),
            label: "variant ember route".to_string(),
            document: fixture_document("issue1186-rev-b", 8.0, "Vesper")?,
        },
    ])
}

fn fixture_document(
    revision_id: &str,
    x: f32,
    faction: &str,
) -> Result<EditorDocument, EngineError> {
    let mut document = EditorDocument::new("calyx://world/issue1186-session-capture", revision_id)?;
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
            Transform::from_translation(Vec3::new(x, 0.0, -6.0)),
        )
        .with_property("faction", faction),
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
            field: "editor.session_capture.fsv.asset".to_string(),
            reason: "missing real Quaternius Rock1 asset record".to_string(),
        })?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    Ok(document)
}
