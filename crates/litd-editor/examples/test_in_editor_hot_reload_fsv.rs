use litd_hash::sha256_hex;
use std::fs;

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject, stable_fingerprint},
    live_hot_reload::{
        LiveHotReloadPatch, LiveHotReloadRequest, build_live_hot_reload_ledger,
        parse_live_hot_reload_ledger, read_live_hot_reload_ledger, write_live_hot_reload_ledger,
    },
    play::{PlayCameraRig, PlaySession},
    world_validation::{WorldValidationUseCase, require_editor_world_valid, validate_editor_world},
};
use litd_engine::{
    EngineError,
    math::{FixedVec3, Transform, Vec3},
    sim::SimBodyId,
    sim_order::{SimOrder, SimOrderKind, SimOrderQueue, SimOrderSource},
    sim_replay::SimSnapshot,
};
use serde_json::json;

const HERO_ID: &str = "unit_playtest_hero";
const TARGET_ID: &str = "unit_training_dummy";
const ROCK_MODEL: &str = "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE738_FSV_ROOT",
        "issue738_test_in_editor_hot_reload",
        "manual",
    )?;
    let aster_dir = root.join("live-hot-reload-aster");
    let state_path = root.join("live-hot-reload-ledger.json");
    let artifact_path = root.join("issue738-test-in-editor-readback.json");

    let base = fixture_document()?;
    let first_doc = edit_damage_value(&base, "17", "issue738-rev-damage-17")?;
    let first_play = play_attack_readback(&first_doc)?;
    let first_repeat = play_attack_readback(&first_doc)?;
    require_same_run("first damage run", &first_play, &first_repeat)?;

    let second_doc = edit_damage_value(&first_doc, "31", "issue738-rev-damage-31")?;
    let second_play = play_attack_readback(&second_doc)?;
    let second_repeat = play_attack_readback(&second_doc)?;
    require_same_run("second damage run", &second_play, &second_repeat)?;
    if first_play["damage_fp"] == second_play["damage_fp"] {
        return Err("damage edit did not change in-game attack damage".into());
    }

    let hot_reload = build_live_hot_reload_ledger(
        &first_doc,
        LiveHotReloadRequest {
            ledger_id: "issue738-live-hot-reload-ledger".to_string(),
            session_id: "issue738-live-session".to_string(),
            project_id: "litd.project.issue738".to_string(),
            actor_id: "human.qa.ember".to_string(),
            player_object_id: HERO_ID.to_string(),
            camera_rig: PlayCameraRig::TopDown,
            pre_reload_ticks: 64,
            runtime_velocity: FixedVec3::from_i32s(64, 0, 0),
            patches: vec![LiveHotReloadPatch::UnitStat {
                patch_id: "patch-damage-31".to_string(),
                object_id: HERO_ID.to_string(),
                stat_key: "damage".to_string(),
                value: "31".to_string(),
            }],
        },
    )?;
    let hot_write = write_live_hot_reload_ledger(&state_path, &aster_dir, &hot_reload)?;
    let hot_aster = read_live_hot_reload_ledger(&aster_dir, &hot_reload.ledger_id)?
        .ok_or("missing live hot-reload Aster ledger")?;
    let hot_parsed = parse_live_hot_reload_ledger(&hot_aster.bytes)?;
    if hot_write.bytes != hot_aster.bytes {
        return Err("hot-reload write/read bytes differed".into());
    }
    let hot_step = hot_parsed
        .steps
        .first()
        .ok_or("hot-reload ledger had no step")?;
    if hot_step.runtime_before.player_position != hot_step.runtime_after.player_position {
        return Err("hot reload did not preserve the actual live player position".into());
    }
    if hot_step.runtime_before.tick != 64 || hot_step.generation_after <= hot_step.generation_before
    {
        return Err("hot reload did not run against an advanced live session".into());
    }

    let invalid_doc = edit_damage_value(&base, "0", "issue738-rev-invalid-damage")?;
    let invalid_report = validate_editor_world(&invalid_doc, WorldValidationUseCase::Play);
    let invalid_error = require_editor_world_valid(&invalid_doc, WorldValidationUseCase::Play)
        .unwrap_err()
        .to_string();
    if !invalid_report.blocking || !invalid_error.contains("stat.damage") {
        return Err("invalid damage edit did not block Play with stat.damage reason".into());
    }

    let save_edge = save_mid_tick_edge(&first_doc)?;
    let artifact = json!({
        "issue": 738,
        "source_of_truth": "Aster live-hot-reload ledger, rebuilt PlaySession sim snapshots, world-validation report, and JSON report readback",
        "first_play_damage": first_play,
        "first_play_repeat": first_repeat,
        "second_play_damage_after_stop_edit_play": second_play,
        "second_play_repeat": second_repeat,
        "hot_reload": {
            "ledger_id": hot_parsed.ledger_id,
            "state_path_exists": state_path.exists(),
            "state_sha256": sha256_hex(&hot_write.bytes),
            "aster_sha256": hot_aster.sha256,
            "aster_seq": hot_aster.seq,
            "aster_graph_rows": hot_aster.graph_rows,
            "applied_live_count": hot_parsed.readback.applied_live_count,
            "final_generation": hot_parsed.readback.final_generation,
            "runtime_tick_before": hot_step.runtime_before.tick,
            "runtime_tick_after": hot_step.runtime_after.tick,
            "runtime_position_before": hot_step.runtime_before.player_position,
            "runtime_position_after": hot_step.runtime_after.player_position,
            "position_preserved": hot_step.player_position_preserved,
            "live_world_reflects_patch": hot_step.live_world_reflects_patch
        },
        "invalid_edit_blocks_play": {
            "blocking": invalid_report.blocking,
            "error_count": invalid_report.error_count,
            "summary": invalid_report.summary,
            "error": invalid_error
        },
        "save_mid_tick": save_edge
    });
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    fs::write(&artifact_path, &artifact_bytes)?;
    let artifact_readback = fs::read(&artifact_path)?;
    if artifact_readback != artifact_bytes {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "wrote issue738 test-in-editor FSV artifact={} bytes={} artifact_sha={} hot_ledger_sha={}",
        artifact_path.display(),
        artifact_readback.len(),
        report_readback.sha256,
        hot_aster.sha256
    );
    Ok(())
}

fn play_attack_readback(document: &EditorDocument) -> Result<serde_json::Value, EngineError> {
    require_editor_world_valid(document, WorldValidationUseCase::Play)?;
    let session = PlaySession::start(document, PlayCameraRig::TopDown)?;
    let attacker = body_id(&session, HERO_ID)?;
    let target = body_id(&session, TARGET_ID)?;
    let damage = unit_damage(document, HERO_ID)?;
    let mut sim = session.build.snapshot.restore::<16>()?;
    let mut queue = SimOrderQueue::<4>::default();
    queue.enqueue(SimOrder::new(
        1,
        SimOrderSource::human(1),
        0,
        target,
        SimOrderKind::ApplyAttack {
            attacker,
            damage_fp: damage,
        },
    ))?;
    let run = queue.run_ticks(&mut sim, 1)?;
    let snapshot = SimSnapshot::capture(&sim);
    let event = snapshot
        .attack_events
        .iter()
        .find(|event| event.attacker == attacker && event.target == target)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.issue738.attack.event".to_string(),
            reason: "attack event was not recorded in the play-session sim".to_string(),
        })?;
    if event.damage_fp != damage {
        return Err(EngineError::UiRenderInvalidConfig {
            field: "editor.issue738.attack.damage".to_string(),
            reason: format!("expected {damage}, got {}", event.damage_fp),
        });
    }
    Ok(json!({
        "document_fingerprint": stable_fingerprint(document)?,
        "damage_fp": event.damage_fp,
        "attacker_body_id": attacker.index(),
        "target_body_id": target.index(),
        "event_tick": event.tick,
        "final_tick": run.final_tick,
        "final_state_hash": snapshot.state_hash,
        "final_state_hash_hex": format!("{:016X}", snapshot.state_hash),
        "attack_event_count": snapshot.attack_events.len()
    }))
}

fn save_mid_tick_edge(document: &EditorDocument) -> Result<serde_json::Value, EngineError> {
    let mut session = PlaySession::start(document, PlayCameraRig::TopDown)?;
    let player = body_id(&session, HERO_ID)?;
    session.run_runtime_ticks_with_body_velocity(player, FixedVec3::from_i32s(64, 0, 0), 4)?;
    let before_hash = session.build.snapshot.state_hash;
    let error = session
        .require_save_allowed_during_tick(true)
        .unwrap_err()
        .to_string();
    let allowed_between_ticks = session.require_save_allowed_during_tick(false).is_ok();
    Ok(json!({
        "runtime_tick": session.build.snapshot.tick,
        "runtime_state_hash": before_hash,
        "mid_tick_error": error,
        "allowed_between_ticks": allowed_between_ticks
    }))
}

fn require_same_run(
    label: &str,
    left: &serde_json::Value,
    right: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if left["damage_fp"] != right["damage_fp"]
        || left["final_state_hash_hex"] != right["final_state_hash_hex"]
        || left["attack_event_count"] != right["attack_event_count"]
    {
        return Err(format!("{label} was not deterministic").into());
    }
    Ok(())
}

fn body_id(session: &PlaySession, object_id: &str) -> Result<SimBodyId, EngineError> {
    session
        .build
        .spawned_bodies
        .iter()
        .find(|binding| binding.object_id == object_id)
        .map(|binding| SimBodyId::from_raw(binding.body_id))
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.issue738.body".to_string(),
            reason: format!("missing spawned body for {object_id}"),
        })
}

fn unit_damage(document: &EditorDocument, object_id: &str) -> Result<i64, EngineError> {
    let object =
        document
            .objects
            .get(object_id)
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.issue738.damage.object".to_string(),
                reason: format!("missing unit {object_id}"),
            })?;
    let value =
        object
            .properties
            .get("stat.damage")
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.issue738.damage".to_string(),
                reason: format!("unit {object_id} is missing stat.damage"),
            })?;
    let damage = value
        .parse::<i64>()
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.issue738.damage".to_string(),
            reason: format!("unit {object_id} stat.damage is not an integer: {error}"),
        })?;
    if damage <= 0 {
        return Err(EngineError::UiRenderInvalidConfig {
            field: "editor.issue738.damage".to_string(),
            reason: format!("unit {object_id} stat.damage must be positive"),
        });
    }
    Ok(damage)
}

fn edit_damage_value(
    document: &EditorDocument,
    value: &str,
    revision_id: &str,
) -> Result<EditorDocument, EngineError> {
    let mut next = document.clone();
    next.revision_id = revision_id.to_string();
    let object =
        next.objects
            .get_mut(HERO_ID)
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.issue738.edit.hero".to_string(),
                reason: "hero unit is missing".to_string(),
            })?;
    object
        .properties
        .insert("stat.damage".to_string(), value.to_string());
    next.validate()?;
    Ok(next)
}

fn fixture_document() -> Result<EditorDocument, EngineError> {
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| record.relative_path == ROCK_MODEL)
        .cloned()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.issue738.asset".to_string(),
            reason: format!("missing real asset {ROCK_MODEL}"),
        })?;
    let mut document =
        EditorDocument::new("calyx://world/issue738-test-in-editor", "issue738-rev-base")?;
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
        .with_property("stat.damage", "11"),
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
