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
        "LITD_ISSUE1180_FSV_ROOT",
        "issue1180_instant_play",
        "manual",
    )?;
    let aster_dir = root.join("aster");
    let ledger_path = root.join("instant-play-ledger.json");
    let artifact_path = root.join("issue1180-instant-play-readback.json");
    let document = fixture_document(64)?;
    let request = request("issue1180-manual-ledger", 5, Vec3::new(14.0, 0.0, -22.0));
    println!(
        "happy_path before_enter repeat={} cursor={:?} budget_ms={}",
        request.repeat_count, request.cursor_world, request.latency_budget_ms
    );
    let ledger = build_instant_play_ledger(&document, request)?;
    let writeback = write_instant_play_ledger(&ledger_path, &aster_dir, &ledger)?;
    let aster = read_instant_play_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing instant-play ledger in Aster")?;
    let parsed = parse_instant_play_ledger(&aster.bytes)?;
    let state_bytes = fs::read(&ledger_path)?;
    if state_bytes != aster.bytes {
        return Err(format!("ledger state/Aster mismatch at {}", ledger_path.display()).into());
    }
    println!(
        "happy_path after_aster seq={} graph_rows={} ledger_sha={} state_matches_aster={} runs={} max_ms={:.3} spawn={:?}",
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        state_bytes == aster.bytes,
        parsed.readback.run_count,
        parsed.readback.max_elapsed_ms,
        parsed.runs[0].player_transform
    );

    let edge_large = edge_large_world()?;
    let edge_invalid = edge_invalid_state(&ledger_path, &aster_dir, &parsed, &state_bytes)?;
    let edge_repeated = edge_repeated()?;
    let artifact = json!({
        "issue": 1180,
        "source_of_truth": "Aster Graph CF instant-play ledger row plus JSON state-file readback",
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
            "instant_play": {
                "runs": parsed.runs,
                "readback": parsed.readback
            }
        },
        "edge_cases": [edge_large, edge_invalid, edge_repeated]
    });
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    fs::write(&artifact_path, &artifact_bytes)?;
    let artifact_readback = fs::read(&artifact_path)?;
    if artifact_readback != artifact_bytes {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "wrote issue1180 instant play FSV artifact={} bytes={} artifact_sha={} ledger_sha={}",
        artifact_path.display(),
        artifact_readback.len(),
        report_readback.sha256,
        aster.sha256
    );
    Ok(())
}

fn edge_large_world() -> Result<serde_json::Value, EngineError> {
    let ledger = build_instant_play_ledger(
        &fixture_document(1_000)?,
        request("issue1180-manual-large", 3, Vec3::new(20.0, 0.0, -30.0)),
    )?;
    println!(
        "edge_large before_doodads=1000 before_count=0 after_count={} max_ms={:.3} stable={} visible={} draw_batches={}",
        ledger.readback.run_count,
        ledger.readback.max_elapsed_ms,
        ledger.readback.repeated_enter_exit_stable,
        ledger.readback.visible_instances,
        ledger.readback.draw_batches
    );
    Ok(json!({
        "name": "large_world_bounded",
        "before_doodads": 1000,
        "before_count": 0,
        "after_count": ledger.readback.run_count,
        "max_elapsed_ms": ledger.readback.max_elapsed_ms,
        "within_budget": ledger.readback.all_runs_within_budget,
        "stable": ledger.readback.repeated_enter_exit_stable,
        "visible_instances": ledger.readback.visible_instances,
        "draw_batches": ledger.readback.draw_batches
    }))
}

fn edge_invalid_state(
    ledger_path: &std::path::Path,
    aster_dir: &std::path::Path,
    baseline: &litd_editor::instant_play::InstantPlayLedger,
    before_state: &[u8],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let invalid = EditorDocument::new("calyx://world/issue1180-invalid", "rev-invalid")?;
    let error =
        build_instant_play_ledger(&invalid, request("issue1180-manual-invalid", 1, Vec3::ZERO))
            .expect_err("invalid edit state should fail")
            .to_string();
    let after_state = fs::read(ledger_path)?;
    let after_aster = read_instant_play_ledger(aster_dir, &baseline.ledger_id)?
        .ok_or("missing baseline after invalid state")?;
    println!(
        "edge_invalid before_bytes={} after_bytes={} after_error={} state_unchanged={}",
        before_state.len(),
        after_state.len(),
        error,
        after_state == before_state && after_aster.bytes == before_state
    );
    Ok(json!({
        "name": "invalid_mid_edit_blocks_play",
        "before_state_bytes": before_state.len(),
        "after_state_bytes": after_state.len(),
        "after_error": error,
        "state_unchanged": after_state == before_state && after_aster.bytes == before_state
    }))
}

fn edge_repeated() -> Result<serde_json::Value, EngineError> {
    let ledger = build_instant_play_ledger(
        &fixture_document(32)?,
        request("issue1180-manual-repeated", 12, Vec3::new(2.0, 0.0, -2.0)),
    )?;
    println!(
        "edge_repeated before_count=0 after_count={} max_ms={:.3} stable={}",
        ledger.readback.run_count,
        ledger.readback.max_elapsed_ms,
        ledger.readback.repeated_enter_exit_stable
    );
    Ok(json!({
        "name": "repeated_enter_exit_stable",
        "before_count": 0,
        "after_count": ledger.readback.run_count,
        "max_elapsed_ms": ledger.readback.max_elapsed_ms,
        "stable": ledger.readback.repeated_enter_exit_stable,
        "within_budget": ledger.readback.all_runs_within_budget
    }))
}

fn request(ledger_id: &str, repeat_count: usize, cursor_world: Vec3) -> InstantPlayRequest {
    InstantPlayRequest {
        ledger_id: ledger_id.to_string(),
        project_id: "litd.project.issue1180.manual".to_string(),
        actor_id: "human.qa.ember".to_string(),
        player_object_id: "unit_playtest_hero".to_string(),
        camera_rig: PlayCameraRig::FirstPerson,
        cursor_world,
        latency_budget_ms: DEFAULT_INSTANT_PLAY_BUDGET_MS,
        repeat_count,
    }
}

fn fixture_document(extra_doodads: usize) -> Result<EditorDocument, EngineError> {
    let mut document =
        EditorDocument::new("calyx://world/issue1180-instant-play", "issue1180-rev-1")?;
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
    for index in 0..extra_doodads {
        document.upsert_object(EditorObject::new(
            format!("doodad_{index:04}"),
            "Doodad",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(index as f32, 0.0, -(index as f32))),
        ))?;
    }
    let record = AssetVaultCatalog::load_default_cached()
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.instant_play.fixture.asset_vault".to_string(),
            reason: error.to_string(),
        })?
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.instant_play.fixture.asset".to_string(),
            reason: "missing rematerialized unit mesh asset-vault record (Stan.gltf)".to_string(),
        })?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    Ok(document)
}
