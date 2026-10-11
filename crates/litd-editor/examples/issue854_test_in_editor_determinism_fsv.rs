use litd_hash::sha256_hex;
use std::fs;

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    instant_play::{
        DEFAULT_INSTANT_PLAY_BUDGET_MS, InstantPlayRequest, build_instant_play_ledger,
        parse_instant_play_ledger, read_instant_play_ledger, write_instant_play_ledger,
    },
    play::{PlayCameraRig, PlaySession},
};
use litd_engine::{
    EngineError,
    math::{FixedVec3, Transform, Vec3},
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_order::{SimOrder, SimOrderKind, SimOrderQueue, SimOrderSource},
    sim_replay::{SIM_REPLAY_SCHEMA_VERSION, SimReplayRecord},
};
use serde_json::json;

const HERO_ID: &str = "unit_playtest_hero";
const TARGET_ID: &str = "unit_training_dummy";
const ROCK_MODEL: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";
const FIXED_PLAY_SEED: u64 = 0x1141;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE854_FSV_ROOT",
        "issue854_test_in_editor_determinism",
        "manual",
    )?;
    let aster_dir = root.join("aster");
    let ledger_path = root.join("instant-play-ledger.json");
    let replay_path = root.join("fixed-seed-replay.ron");
    let artifact_path = root.join("issue854-test-in-editor-determinism-readback.json");

    let document = fixture_document()?;
    let request = InstantPlayRequest {
        ledger_id: "issue854-instant-play-ledger".to_string(),
        project_id: "litd.project.issue854".to_string(),
        actor_id: "human.qa.ember".to_string(),
        player_object_id: HERO_ID.to_string(),
        camera_rig: PlayCameraRig::TopDown,
        cursor_world: Vec3::new(12.0, 0.0, -18.0),
        latency_budget_ms: DEFAULT_INSTANT_PLAY_BUDGET_MS,
        repeat_count: 4,
    };
    println!(
        "instant_play before_enter repeat={} cursor={:?} budget_ms={}",
        request.repeat_count, request.cursor_world, request.latency_budget_ms
    );
    let ledger = build_instant_play_ledger(&document, request)?;
    let writeback = write_instant_play_ledger(&ledger_path, &aster_dir, &ledger)?;
    let aster = read_instant_play_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing issue854 instant-play ledger in Aster")?;
    let parsed = parse_instant_play_ledger(&aster.bytes)?;
    if writeback.bytes != aster.bytes {
        return Err("instant-play Aster write/read bytes differed".into());
    }
    let legacy_state_path_exists = ledger_path.exists();

    let first_session = PlaySession::start(&document, PlayCameraRig::TopDown)?;
    let second_session = PlaySession::start(&document, PlayCameraRig::TopDown)?;
    if first_session.build.sim_seed != FIXED_PLAY_SEED
        || second_session.build.sim_seed != FIXED_PLAY_SEED
    {
        return Err("PlaySession did not use the expected fixed seed".into());
    }
    if first_session.build.snapshot.state_hash != second_session.build.snapshot.state_hash {
        return Err("same editor document produced different fixed-seed play snapshots".into());
    }

    let replay = build_editor_replay(&first_session)?;
    let replay_text = replay.to_ron_string()?;
    fs::write(&replay_path, replay_text.as_bytes())?;
    let replay_readback = fs::read_to_string(&replay_path)?;
    if replay_readback != replay_text {
        return Err(format!("replay readback mismatch at {}", replay_path.display()).into());
    }
    let parsed_replay = SimReplayRecord::from_ron_str(&replay_readback)?;
    let replay_one = parsed_replay.replay::<16, 8>()?;
    let replay_two = parsed_replay.replay::<16, 8>()?;
    if replay_one.final_hash != replay_two.final_hash
        || replay_one.orders_applied != replay_two.orders_applied
        || replay_one.final_tick != replay_two.final_tick
    {
        return Err("fixed-seed replay was not deterministic across repeated replays".into());
    }

    let seed_probe = fixed_seed_probe()?;
    let edge_invalid =
        edge_invalid_editor_state(&ledger_path, &aster_dir, &parsed.ledger_id, &aster.bytes)?;
    let edge_tampered = edge_tampered_replay_hash(&parsed_replay)?;
    let edge_pending = edge_future_order_pending(&first_session)?;

    let artifact = json!({
        "issue": 854,
        "source_of_truth": "Aster instant-play ledger, PlaySession fixed seed snapshots, persisted SimReplay RON, JSON report readback, and manual Synapse capture",
        "reference_image_reviewed": "docs/readme/ED09-test-in-editor.png",
        "artifact_path": artifact_path,
        "instant_play": {
            "legacy_state_path": ledger_path,
            "legacy_state_path_exists": legacy_state_path_exists,
            "aster_write_readback_matches": writeback.bytes == aster.bytes,
            "aster_sha256": aster.sha256,
            "aster_seq": aster.seq,
            "aster_graph_rows": aster.graph_rows,
            "aster_bytes": aster.bytes.len(),
            "cell_key": writeback.cell_key,
            "index_key": writeback.index_key,
            "run_count": parsed.readback.run_count,
            "max_elapsed_ms": parsed.readback.max_elapsed_ms,
            "all_runs_within_budget": parsed.readback.all_runs_within_budget,
            "spawn_transform_matches_cursor": parsed.readback.spawn_transform_matches_cursor,
            "repeated_enter_exit_stable": parsed.readback.repeated_enter_exit_stable,
            "visible_instances": parsed.readback.visible_instances,
            "draw_batches": parsed.readback.draw_batches
        },
        "test_in_editor_fixed_seed": {
            "camera_rig": first_session.build.camera_rig,
            "sim_seed": first_session.build.sim_seed,
            "repeat_sim_seed": second_session.build.sim_seed,
            "initial_hash": first_session.build.snapshot.state_hash,
            "initial_hash_hex": format!("{:016X}", first_session.build.snapshot.state_hash),
            "repeat_initial_hash": second_session.build.snapshot.state_hash,
            "repeat_initial_hash_hex": format!("{:016X}", second_session.build.snapshot.state_hash),
            "body_bindings": first_session.build.spawned_bodies,
            "visible_instance_ids": first_session.build.visible_instance_ids,
            "draw_batches": first_session.build.draw_batches
        },
        "fixed_seed_replay": {
            "replay_path": replay_path,
            "replay_sha256": sha256_hex(replay_readback.as_bytes()),
            "schema_version": parsed_replay.schema_version,
            "run_ticks": parsed_replay.run_ticks,
            "order_count": parsed_replay.orders.len(),
            "initial_hash": replay_one.initial_hash,
            "initial_hash_hex": format!("{:016X}", replay_one.initial_hash),
            "final_hash": replay_one.final_hash,
            "final_hash_hex": format!("{:016X}", replay_one.final_hash),
            "repeat_final_hash": replay_two.final_hash,
            "repeat_final_hash_hex": format!("{:016X}", replay_two.final_hash),
            "final_tick": replay_one.final_tick,
            "steps": replay_one.steps,
            "orders_applied": replay_one.orders_applied,
            "attack_event_count": replay_one.final_snapshot.attack_events.len()
        },
        "seed_probe": seed_probe,
        "edge_cases": [edge_invalid, edge_tampered, edge_pending]
    });
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    fs::write(&artifact_path, &artifact_bytes)?;
    let artifact_readback = fs::read(&artifact_path)?;
    if artifact_readback != artifact_bytes {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "wrote issue854 test-in-editor determinism FSV artifact={} bytes={} artifact_sha={} replay_sha={}",
        artifact_path.display(),
        artifact_readback.len(),
        report_readback.sha256,
        sha256_hex(replay_readback.as_bytes())
    );
    Ok(())
}

fn build_editor_replay(session: &PlaySession) -> Result<SimReplayRecord, EngineError> {
    let hero = body_id(session, HERO_ID)?;
    let target = body_id(session, TARGET_ID)?;
    let orders = vec![
        SimOrder::new(
            1,
            SimOrderSource::human(1),
            0,
            hero,
            SimOrderKind::SetVelocity(FixedVec3::from_i32s(64, 0, 0)),
        ),
        SimOrder::new(
            2,
            SimOrderSource::human(1),
            1,
            hero,
            SimOrderKind::AddVelocity(FixedVec3::from_i32s(0, 0, -32)),
        ),
        SimOrder::new(
            3,
            SimOrderSource::human(1),
            2,
            target,
            SimOrderKind::ApplyAttack {
                attacker: hero,
                damage_fp: 23,
            },
        ),
    ];
    let mut sim = session.build.snapshot.restore::<16>()?;
    let mut queue = SimOrderQueue::<8>::default();
    for order in &orders {
        queue.enqueue(*order)?;
    }
    let run = queue.run_ticks(&mut sim, 6)?;
    if queue.pending_orders() != 0 {
        return Err(EngineError::SerializationFailed {
            format: "sim-replay",
            reason: format!("{} orders remained pending", queue.pending_orders()),
        });
    }
    Ok(SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot: session.build.snapshot.clone(),
        orders,
        run_ticks: 6,
        expected_final_hash: run.state_hash,
    })
}

fn edge_invalid_editor_state(
    legacy_state_path: &std::path::Path,
    aster_dir: &std::path::Path,
    baseline_ledger_id: &str,
    before_aster: &[u8],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let invalid = EditorDocument::new("calyx://world/issue854-invalid", "issue854-invalid")?;
    let instant_error = build_instant_play_ledger(
        &invalid,
        InstantPlayRequest {
            ledger_id: "issue854-invalid-ledger".to_string(),
            project_id: "litd.project.issue854".to_string(),
            actor_id: "human.qa.ember".to_string(),
            player_object_id: HERO_ID.to_string(),
            camera_rig: PlayCameraRig::TopDown,
            cursor_world: Vec3::ZERO,
            latency_budget_ms: DEFAULT_INSTANT_PLAY_BUDGET_MS,
            repeat_count: 1,
        },
    )
    .expect_err("invalid editor state should block instant play")
    .to_string();
    let play_error = PlaySession::start(&invalid, PlayCameraRig::TopDown)
        .expect_err("invalid editor state should block PlaySession")
        .to_string();
    let after_aster = read_instant_play_ledger(aster_dir, baseline_ledger_id)?
        .ok_or("missing baseline instant-play ledger after invalid state")?;
    let unchanged = after_aster.bytes == before_aster && !legacy_state_path.exists();
    println!(
        "edge_invalid_editor_state before_aster_bytes={} after_aster_bytes={} unchanged={} instant_error={} play_error={}",
        before_aster.len(),
        after_aster.bytes.len(),
        unchanged,
        instant_error,
        play_error
    );
    Ok(json!({
        "name": "invalid_editor_state_blocks_play",
        "failed_closed": true,
        "before_aster_bytes": before_aster.len(),
        "after_aster_bytes": after_aster.bytes.len(),
        "legacy_state_path_exists_after": legacy_state_path.exists(),
        "aster_unchanged": unchanged,
        "instant_play_error": instant_error,
        "play_session_error": play_error
    }))
}

fn edge_tampered_replay_hash(
    replay: &SimReplayRecord,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut tampered = replay.clone();
    tampered.expected_final_hash ^= 1;
    let error = tampered
        .replay::<16, 8>()
        .expect_err("tampered replay hash should fail closed")
        .to_string();
    let original = replay.replay::<16, 8>()?;
    println!(
        "edge_tampered_replay_hash expected={:016X} tampered={:016X} error={}",
        replay.expected_final_hash, tampered.expected_final_hash, error
    );
    Ok(json!({
        "name": "tampered_replay_hash_rejected",
        "failed_closed": true,
        "original_final_hash": original.final_hash,
        "original_final_hash_hex": format!("{:016X}", original.final_hash),
        "tampered_expected_hash": tampered.expected_final_hash,
        "tampered_expected_hash_hex": format!("{:016X}", tampered.expected_final_hash),
        "error": error
    }))
}

fn edge_future_order_pending(
    session: &PlaySession,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let hero = body_id(session, HERO_ID)?;
    let replay = SimReplayRecord {
        schema_version: SIM_REPLAY_SCHEMA_VERSION,
        initial_snapshot: session.build.snapshot.clone(),
        orders: vec![SimOrder::new(
            99,
            SimOrderSource::human(1),
            99,
            hero,
            SimOrderKind::SetVelocity(FixedVec3::from_i32s(128, 0, 0)),
        )],
        run_ticks: 4,
        expected_final_hash: 0,
    };
    let error = replay
        .replay::<16, 8>()
        .expect_err("future order should remain pending and fail closed")
        .to_string();
    println!("edge_future_order_pending pending_tick=99 run_ticks=4 error={error}");
    Ok(json!({
        "name": "future_order_pending_rejected",
        "failed_closed": true,
        "pending_execute_tick": 99,
        "run_ticks": replay.run_ticks,
        "error": error
    }))
}

fn fixed_seed_probe() -> Result<serde_json::Value, EngineError> {
    let same_a = fixed_seed_hash(FIXED_PLAY_SEED)?;
    let same_b = fixed_seed_hash(FIXED_PLAY_SEED)?;
    let different = fixed_seed_hash(FIXED_PLAY_SEED + 1)?;
    if same_a != same_b {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: format!("{same_a:016X}"),
            actual: format!("{same_b:016X}"),
        });
    }
    if same_a == different {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: "different seed must change the state hash".to_string(),
            actual: format!("{different:016X}"),
        });
    }
    Ok(json!({
        "same_seed": FIXED_PLAY_SEED,
        "same_seed_hash_a": same_a,
        "same_seed_hash_a_hex": format!("{:016X}", same_a),
        "same_seed_hash_b": same_b,
        "same_seed_hash_b_hex": format!("{:016X}", same_b),
        "different_seed": FIXED_PLAY_SEED + 1,
        "different_seed_hash": different,
        "different_seed_hash_hex": format!("{:016X}", different),
        "same_seed_stable": same_a == same_b,
        "different_seed_changes_hash": same_a != different
    }))
}

fn fixed_seed_hash(seed: u64) -> Result<u64, EngineError> {
    let mut sim = FixedTickSim::<16>::new(SimConfig::new(64)?, seed)?;
    sim.spawn_body(
        FixedVec3::from_i32s(3, 0, -6),
        FixedVec3::ZERO,
        FixedVec3::ZERO,
    )?;
    sim.spawn_body(
        FixedVec3::from_i32s(7, 0, -6),
        FixedVec3::ZERO,
        FixedVec3::ZERO,
    )?;
    Ok(sim.state_hash())
}

fn body_id(session: &PlaySession, object_id: &str) -> Result<SimBodyId, EngineError> {
    session
        .build
        .spawned_bodies
        .iter()
        .find(|binding| binding.object_id == object_id)
        .map(|binding| SimBodyId::from_raw(binding.body_id))
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.issue854.body".to_string(),
            reason: format!("missing spawned body for {object_id}"),
        })
}

fn fixture_document() -> Result<EditorDocument, EngineError> {
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| record.relative_path == ROCK_MODEL)
        .cloned()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.issue854.asset".to_string(),
            reason: format!("missing real asset {ROCK_MODEL}"),
        })?;
    let mut document =
        EditorDocument::new("calyx://world/issue854-test-in-editor", "issue854-rev-base")?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    document.upsert_object(
        EditorObject::new(
            HERO_ID,
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(3.0, 0.0, -6.0)),
        )
        .with_property("faction", "Dawnward")
        .with_property("stat.damage", "23"),
    )?;
    document.upsert_object(
        EditorObject::new(
            TARGET_ID,
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(7.0, 0.0, -6.0)),
        )
        .with_property("faction", "Ironbound"),
    )?;
    assign_vault_model_to_unit(&mut document, HERO_ID, &record)?;
    assign_vault_model_to_unit(&mut document, TARGET_ID, &record)?;
    Ok(document)
}
