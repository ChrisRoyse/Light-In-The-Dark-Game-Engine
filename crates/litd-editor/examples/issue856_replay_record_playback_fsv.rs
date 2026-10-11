use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject, stable_fingerprint},
    play::{PlayCameraRig, PlaySession},
    play_session_capture::{
        PlaySessionCaptureRequest, PlaySessionCaptureRequestArgs, PlaySessionVariantInput,
        capture_play_session, parse_play_session_capture_ledger, read_play_session_capture_ledger,
        restart_session_again, write_play_session_capture_ledger,
    },
};
use litd_engine::{
    EngineError,
    math::{FixedVec3, Transform, Vec3},
    sim::SimBodyId,
    sim_order::{SimOrder, SimOrderKind, SimOrderSource},
    sim_replay::{SimReplayRecord, SimSnapshot},
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run_root = run_root()?;
    fs::create_dir_all(&run_root)?;

    let variants = fixture_variants()?;
    let request = request(&variants[0].document)?;
    let ledger = capture_play_session(variants, request)?;
    let restarted = restart_session_again(&ledger)?;

    let aster_dir = run_root.join("session-capture-aster");
    let legacy_json_path = run_root.join("must-not-exist-session-capture.json");
    let writeback = write_play_session_capture_ledger(&legacy_json_path, &aster_dir, &restarted)?;
    let readback = read_play_session_capture_ledger(&aster_dir, &restarted.ledger_id)?
        .ok_or_else(|| "missing session-capture Aster readback".to_string())?;
    let parsed = parse_play_session_capture_ledger(&readback.bytes)?;

    let replay_path = run_root.join("issue856-replay.ron");
    write_text(&replay_path, &parsed.replay.record.to_ron_string()?)?;
    let replay_bytes = fs::read(&replay_path)?;
    let replay_disk = SimReplayRecord::from_ron_str(&String::from_utf8(replay_bytes.clone())?)?;
    let replay_outcome = replay_disk.replay::<16, 32>()?;

    let final_snapshot_path = run_root.join("issue856-final-snapshot.ron");
    write_text(
        &final_snapshot_path,
        &replay_outcome.final_snapshot.to_ron_string()?,
    )?;
    let final_snapshot_bytes = fs::read(&final_snapshot_path)?;
    let final_snapshot_disk =
        SimSnapshot::from_ron_str(&String::from_utf8(final_snapshot_bytes.clone())?)?;
    let restored_final = final_snapshot_disk.restore::<16>()?;

    let edges = vec![
        edge(
            "tampered_expected_hash_rejected",
            "replay.expected_final_hash",
            tampered_hash_edge(&replay_disk),
            "LITD-REFLECT-FINGERPRINT-MISMATCH",
        ),
        edge(
            "malformed_replay_ron_rejected",
            "replay.ron",
            malformed_ron_edge(),
            "LITD-REFLECT-SERIALIZATION-FAILED",
        ),
        edge(
            "duplicate_order_fingerprint_rejected",
            "request.orders",
            duplicate_order_edge(&parsed),
            "LITD-RENDER-UI-INVALID-CONFIG",
        ),
        edge(
            "invalid_checkpoint_rejected",
            "request.checkpoint_tick",
            invalid_checkpoint_edge(&parsed),
            "LITD-RENDER-UI-INVALID-CONFIG",
        ),
    ];
    let matched_edges = edges
        .iter()
        .filter(|edge| edge["matched"].as_bool() == Some(true))
        .count();

    let report = json!({
        "schema": "litd.editor.issue856.replay_record_playback_fsv.v1",
        "issue": 856,
        "source_of_truth": {
            "session_capture_aster": aster_dir,
            "replay_record": replay_path,
            "final_snapshot": final_snapshot_path,
            "manual_gate": "Manual Synapse/browser readback must inspect this report plus the replay and Aster ledger hashes before close"
        },
        "research": {
            "sources": [
                "https://docs.coherence.io/manual/advanced-topics/competitive-games/determinism-prediction-rollback",
                "https://www.qemu.org/docs/master/system/replay.html",
                "https://cacm.acm.org/practice/deterministic-record-and-replay/"
            ],
            "pattern": "persist a deterministic initial state/checkpoint and nondeterministic input/order log, replay from that record, and compare final state hashes/checksums"
        },
        "acceptance": {
            "session_id": parsed.session_id,
            "active_variant_id": parsed.active_variant_id,
            "variant_count": parsed.readback.variant_count,
            "a_b_isolated": parsed.readback.a_b_isolated,
            "run_ticks": parsed.readback.run_ticks,
            "checkpoint_tick": parsed.readback.checkpoint_tick,
            "order_count": parsed.replay.record.orders.len(),
            "human_order_count": parsed.readback.human_order_count,
            "lowered_order_count": parsed.readback.lowered_order_count,
            "initial_hash": parsed.replay.initial_hash,
            "expected_final_hash": parsed.replay.expected_final_hash,
            "replayed_final_hash": replay_outcome.final_hash,
            "restart_final_hash": parsed.restart.restart_final_hash,
            "restored_final_hash": restored_final.state_hash(),
            "final_tick": replay_outcome.final_tick,
            "steps": replay_outcome.steps,
            "orders_applied": replay_outcome.orders_applied,
            "replay_final_hash_matches": parsed.readback.replay_final_hash_matches,
            "restart_matches_full_replay": parsed.readback.restart_matches_full_replay,
            "disk_replay_matches": replay_outcome.final_hash == parsed.replay.expected_final_hash,
            "disk_snapshot_matches": restored_final.state_hash() == replay_outcome.final_hash,
            "all_edges_matched": matched_edges == edges.len()
        },
        "artifacts": {
            "ledger_write": {
                "sha256": writeback.sha256,
                "bytes": writeback.bytes.len(),
                "seq": writeback.seq,
                "graph_rows": writeback.graph_rows,
                "cell_key": writeback.cell_key,
                "index_key": writeback.index_key
            },
            "ledger_read": {
                "sha256": readback.sha256,
                "bytes": readback.bytes.len(),
                "seq": readback.seq,
                "graph_rows": readback.graph_rows,
                "active_variant_id": readback.active_variant_id,
                "restart_matches_full_replay": readback.restart_matches_full_replay
            },
            "replay_record": file_readback(&replay_path)?,
            "final_snapshot": file_readback(&final_snapshot_path)?
        },
        "edges": edges,
        "sufficiency_argument": "The smallest #856 proof uses one playable document pair, one captured play session, one checkpoint restart, one persisted Aster ledger, one replay RON, and one final snapshot. The known order stream covers human and lowered-agent orders over multiple ticks; replay and restart both reproduce the same final hash. Edges cover tampered final hash, malformed record bytes, duplicate order identity, and invalid checkpoint bounds. Larger maps add volume but no distinct record/playback state transition."
    });

    let report_path = run_root.join("manual-fsv-report.json");
    let report_bytes = serde_json::to_vec_pretty(&report)?;
    fs::write(&report_path, &report_bytes)?;
    println!(
        "ISSUE856_REPLAY_FSV report={} sha256={} ledger_sha={} replay_sha={} final_hash={} edges={} replay_match={}",
        report_path.display(),
        sha256_hex(&report_bytes),
        readback.sha256,
        sha256_hex(&replay_bytes),
        replay_outcome.final_hash,
        matched_edges,
        replay_outcome.final_hash == parsed.replay.expected_final_hash
            && restored_final.state_hash() == replay_outcome.final_hash
    );
    Ok(())
}

fn request(document: &EditorDocument) -> Result<PlaySessionCaptureRequest, EngineError> {
    Ok(PlaySessionCaptureRequest::new(
        PlaySessionCaptureRequestArgs {
            ledger_id: "issue856-replay-ledger",
            session_id: "issue856-session",
            actor_id: "human.qa.replay",
            active_variant_id: "a",
            camera_rig: PlayCameraRig::TopDown,
            run_ticks: 5,
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
            2,
            SimOrderSource::lowered_agent(7),
            1,
            body,
            SimOrderKind::AddVelocity(FixedVec3::from_i32s(0, 64, 0)),
        ),
        SimOrder::new(
            4,
            SimOrderSource::human(1),
            2,
            body,
            SimOrderKind::SetAcceleration(FixedVec3::from_i32s(0, 0, -64)),
        ),
    ])
}

fn fixture_variants() -> Result<Vec<PlaySessionVariantInput>, EngineError> {
    Ok(vec![
        PlaySessionVariantInput {
            variant_id: "a".to_string(),
            label: "baseline replay route".to_string(),
            document: fixture_document("issue856-rev-a", 3.0, "Dawnward")?,
        },
        PlaySessionVariantInput {
            variant_id: "b".to_string(),
            label: "alternate replay route".to_string(),
            document: fixture_document("issue856-rev-b", 8.0, "Vesper")?,
        },
    ])
}

fn fixture_document(
    revision_id: &str,
    x: f32,
    faction: &str,
) -> Result<EditorDocument, EngineError> {
    let mut document = EditorDocument::new("calyx://world/issue856-replay", revision_id)?;
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
            field: "editor.issue856.asset_vault.record".to_string(),
            reason: "missing medieval dungeon Rock1.fbx vault record".to_string(),
        })?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    Ok(document)
}

fn tampered_hash_edge(record: &SimReplayRecord) -> Result<(), EngineError> {
    let mut bad = record.clone();
    bad.expected_final_hash = 0;
    bad.replay::<16, 32>().map(|_| ())
}

fn malformed_ron_edge() -> Result<(), EngineError> {
    SimReplayRecord::from_ron_str("not a replay record").map(|_| ())
}

fn duplicate_order_edge(
    parsed: &litd_editor::play_session_capture::PlaySessionCaptureLedger,
) -> Result<(), EngineError> {
    let mut request = PlaySessionCaptureRequest::new(PlaySessionCaptureRequestArgs {
        ledger_id: "issue856-duplicate-order",
        session_id: "issue856-session-duplicate",
        actor_id: "human.qa.replay",
        active_variant_id: "a",
        camera_rig: PlayCameraRig::TopDown,
        run_ticks: parsed.readback.run_ticks,
        checkpoint_tick: parsed.readback.checkpoint_tick,
        orders: parsed.replay.record.orders.clone(),
    });
    request.orders.push(request.orders[0]);
    capture_play_session(
        parsed
            .variants
            .iter()
            .map(|variant| PlaySessionVariantInput {
                variant_id: variant.variant_id.clone(),
                label: format!("{} edge", variant.variant_id),
                document: variant.document.clone(),
            })
            .collect(),
        request,
    )
    .map(|_| ())
}

fn invalid_checkpoint_edge(
    parsed: &litd_editor::play_session_capture::PlaySessionCaptureLedger,
) -> Result<(), EngineError> {
    let request = PlaySessionCaptureRequest::new(PlaySessionCaptureRequestArgs {
        ledger_id: "issue856-invalid-checkpoint",
        session_id: "issue856-session-invalid-checkpoint",
        actor_id: "human.qa.replay",
        active_variant_id: "a",
        camera_rig: PlayCameraRig::TopDown,
        run_ticks: parsed.readback.run_ticks,
        checkpoint_tick: parsed.readback.run_ticks,
        orders: parsed.replay.record.orders.clone(),
    });
    capture_play_session(
        parsed
            .variants
            .iter()
            .map(|variant| PlaySessionVariantInput {
                variant_id: variant.variant_id.clone(),
                label: format!("{} checkpoint edge", variant.variant_id),
                document: variant.document.clone(),
            })
            .collect(),
        request,
    )
    .map(|_| ())
}

fn edge(label: &str, field: &str, result: Result<(), EngineError>, expected_code: &str) -> Value {
    match result {
        Ok(()) => json!({
            "label": label,
            "field": field,
            "expected_code": expected_code,
            "actual_code": "ok",
            "matched": false
        }),
        Err(error) => json!({
            "label": label,
            "field": field,
            "expected_code": expected_code,
            "actual_code": error.code(),
            "matched": error.code() == expected_code,
            "reason": error.to_string()
        }),
    }
}

fn file_readback(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    Ok(json!({
        "path": path,
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes)
    }))
}

fn write_text(path: &Path, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    Ok(())
}

fn run_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    Ok(workspace_root()
        .join("target")
        .join("fsv")
        .join("issue856_replays")
        .join("runs")
        .join(format!("run-{stamp}-{}", std::process::id())))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("litd-editor is under workspace crates")
        .to_path_buf()
}

#[allow(dead_code)]
fn document_fingerprint(document: &EditorDocument) -> Result<String, EngineError> {
    stable_fingerprint(document)
}
