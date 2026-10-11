use litd_hash::sha256_hex;
use std::fs;

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    cinematic_authoring::EditorCinematicTimeline,
    history::{EditorDocument, EditorNamedCamera, EditorObject},
    live_hot_reload::{
        LiveHotReloadLedger, LiveHotReloadPatch, LiveHotReloadRequest,
        build_live_hot_reload_ledger, parse_live_hot_reload_ledger, read_live_hot_reload_ledger,
        write_live_hot_reload_ledger,
    },
    play::PlayCameraRig,
};
use litd_engine::{
    EngineError,
    cinematic_timeline::CinematicSubtitle,
    math::{FixedVec3, Transform, Vec3},
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE1181_FSV_ROOT",
        "issue1181_live_hot_reload",
        "manual",
    )?;
    let aster_dir = root.join("aster");
    let ledger_path = root.join("live-hot-reload-ledger.json");
    let artifact_path = root.join("issue1181-live-hot-reload-readback.json");
    let document = fixture_document()?;

    let request = request("issue1181-manual-ledger", safe_patches());
    println!(
        "happy_path before patches={} ticks={} velocity={:?}",
        request.patches.len(),
        request.pre_reload_ticks,
        request.runtime_velocity
    );
    let ledger = build_live_hot_reload_ledger(&document, request)?;
    // Aster vault is the sole source of truth for live-hot-reload ledgers (#1181).
    // state_path is accepted for API stability but is intentionally not written.
    let writeback = write_live_hot_reload_ledger(&ledger_path, &aster_dir, &ledger)?;
    let aster = read_live_hot_reload_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing live-hot-reload ledger in Aster")?;
    let parsed = parse_live_hot_reload_ledger(&aster.bytes)?;
    if writeback.bytes != aster.bytes {
        return Err("writeback bytes must match independent Aster readback".into());
    }
    if ledger_path.exists() {
        return Err(format!(
            "state path must remain unwritten (Aster-only SoT); found {}",
            ledger_path.display()
        )
        .into());
    }
    // #902: unit + material + abilities/triggers + dialogue + cinematic.
    let kinds: Vec<String> = parsed.steps.iter().map(|s| s.patch_id.clone()).collect();
    let has_material = kinds.iter().any(|k| k.contains("material"));
    let has_trigger = kinds.iter().any(|k| k.contains("trigger"));
    let has_unit_stat = kinds
        .iter()
        .any(|k| k.contains("hp") && !k.contains("rapid"));
    let has_dialogue = kinds.iter().any(|k| k.contains("dialogue"));
    let has_cinematic = kinds.iter().any(|k| k.contains("cinematic"));
    if !has_material || !has_trigger || !has_unit_stat || !has_dialogue || !has_cinematic {
        return Err(format!(
            "safe_patches must cover unit+material+trigger+dialogue+cinematic; got {kinds:?}"
        )
        .into());
    }
    println!(
        "happy_path after_aster seq={} graph_rows={} ledger_sha={} aster_only=true applied={} generation={} runtime_hash={} position={:?} material={} trigger={} dialogue={} cinematic={}",
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        parsed.readback.applied_live_count,
        parsed.readback.final_generation,
        parsed.readback.final_runtime_state_hash,
        parsed.final_runtime.player_position,
        has_material,
        has_trigger,
        has_dialogue,
        has_cinematic
    );

    let edge_structural = edge_structural(&document)?;
    let edge_rapid = edge_rapid(&document)?;
    let edge_position = edge_position_guard(&ledger_path, &aster_dir, &parsed, &aster.bytes)?;
    let artifact = json!({
        "issue": 1181,
        "parent_issue": 902,
        "source_of_truth": "Aster Graph CF live-hot-reload ledger row (no side-store JSON state file)",
        "artifact_path": artifact_path,
        "happy_path": {
            "ledger": {
                "state_path_unwritten": !ledger_path.exists(),
                "aster_sha256": aster.sha256,
                "writeback_sha256": sha256_hex(&writeback.bytes),
                "aster_seq": aster.seq,
                "aster_graph_rows": aster.graph_rows,
                "cell_key": writeback.cell_key,
                "index_key": writeback.index_key,
                "bytes": writeback.bytes.len()
            },
            "live_reload": {
                "initial_runtime": parsed.initial_runtime,
                "final_runtime": parsed.final_runtime,
                "steps": parsed.steps,
                "readback": parsed.readback
            }
        },
        "edge_cases": [edge_structural, edge_rapid, edge_position]
    });
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    fs::write(&artifact_path, &artifact_bytes)?;
    let artifact_readback = fs::read(&artifact_path)?;
    if artifact_readback != artifact_bytes {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "wrote issue1181 live hot reload FSV artifact={} bytes={} artifact_sha={} ledger_sha={}",
        artifact_path.display(),
        artifact_readback.len(),
        report_readback.sha256,
        aster.sha256
    );
    Ok(())
}

fn edge_structural(document: &EditorDocument) -> Result<serde_json::Value, EngineError> {
    let ledger = build_live_hot_reload_ledger(
        document,
        request("issue1181-manual-structural", vec![structural_patch()]),
    )?;
    println!(
        "edge_structural before_generation=0 after_generation={} notice={}",
        ledger.readback.final_generation,
        ledger.steps[0].notice_code.as_deref().unwrap_or("")
    );
    Ok(json!({
        "name": "structural_change_deferred_with_notice",
        "before_generation": 0,
        "after_generation": ledger.readback.final_generation,
        "notice_code": ledger.steps[0].notice_code,
        "deferred_structural_count": ledger.readback.deferred_structural_count,
        "player_position_preserved_all": ledger.readback.player_position_preserved_all
    }))
}

fn edge_rapid(document: &EditorDocument) -> Result<serde_json::Value, EngineError> {
    let ledger =
        build_live_hot_reload_ledger(document, request("issue1181-manual-rapid", rapid_patches()))?;
    println!(
        "edge_rapid before_count=0 after_count={} generation={} runtime_hash={}",
        ledger.readback.rapid_reload_count,
        ledger.readback.final_generation,
        ledger.readback.final_runtime_state_hash
    );
    Ok(json!({
        "name": "rapid_repeated_reloads_stable",
        "before_count": 0,
        "after_count": ledger.readback.rapid_reload_count,
        "generation": ledger.readback.final_generation,
        "player_position_preserved_all": ledger.readback.player_position_preserved_all,
        "safe_patches_reflected_all": ledger.readback.safe_patches_reflected_all,
        "runtime_hash": ledger.readback.final_runtime_state_hash
    }))
}

fn edge_position_guard(
    ledger_path: &std::path::Path,
    aster_dir: &std::path::Path,
    baseline: &LiveHotReloadLedger,
    before_aster_bytes: &[u8],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut bad = baseline.clone();
    bad.steps[0].runtime_after.player_position = FixedVec3::from_i32s(99, 0, 99);
    bad.steps[0].player_position_preserved = false;
    let error = write_live_hot_reload_ledger(ledger_path, aster_dir, &bad)
        .expect_err("position drift must fail")
        .to_string();
    let after_aster = read_live_hot_reload_ledger(aster_dir, &baseline.ledger_id)?
        .ok_or("missing baseline after position drift")?;
    let aster_unchanged = after_aster.bytes == before_aster_bytes;
    println!(
        "edge_position before_preserved=true after_error={} aster_unchanged={aster_unchanged}",
        error
    );
    Ok(json!({
        "name": "position_drift_rejected",
        "before_preserved": true,
        "after_error": error,
        "aster_unchanged": aster_unchanged
    }))
}

fn request(ledger_id: &str, patches: Vec<LiveHotReloadPatch>) -> LiveHotReloadRequest {
    LiveHotReloadRequest {
        ledger_id: ledger_id.to_string(),
        session_id: "issue1181-session".to_string(),
        project_id: "litd.project.issue1181.manual".to_string(),
        actor_id: "human.qa.ember".to_string(),
        player_object_id: "unit_playtest_hero".to_string(),
        camera_rig: PlayCameraRig::FirstPerson,
        pre_reload_ticks: 64,
        runtime_velocity: FixedVec3::from_i32s(64, 0, 0),
        patches,
    }
}

fn safe_patches() -> Vec<LiveHotReloadPatch> {
    vec![
        LiveHotReloadPatch::UnitStat {
            patch_id: "patch-hp".to_string(),
            object_id: "unit_playtest_hero".to_string(),
            stat_key: "hp".to_string(),
            value: "420".to_string(),
        },
        LiveHotReloadPatch::TextureMaterial {
            patch_id: "patch-material".to_string(),
            object_id: "unit_playtest_hero".to_string(),
            material_id: "Dawnward_Reloaded".to_string(),
        },
        LiveHotReloadPatch::TriggerFlag {
            patch_id: "patch-trigger".to_string(),
            object_id: "world".to_string(),
            trigger_id: "ember_gate_open".to_string(),
            enabled: true,
        },
        LiveHotReloadPatch::DialogueLine {
            patch_id: "patch-dialogue".to_string(),
            object_id: "dialogue.intro".to_string(),
            line: "Hold the ridge — live-preview.".to_string(),
        },
        LiveHotReloadPatch::CinematicSubtitle {
            patch_id: "patch-cinematic".to_string(),
            timeline_id: "cine.intro".to_string(),
            text: "Ashen dawn — live.".to_string(),
        },
    ]
}

fn rapid_patches() -> Vec<LiveHotReloadPatch> {
    (0..8)
        .map(|index| LiveHotReloadPatch::UnitStat {
            patch_id: format!("patch-rapid-{index}"),
            object_id: "unit_playtest_hero".to_string(),
            stat_key: format!("hp_{index}"),
            value: (400 + index).to_string(),
        })
        .collect()
}

fn structural_patch() -> LiveHotReloadPatch {
    LiveHotReloadPatch::StructuralAddUnit {
        patch_id: "patch-add-unit".to_string(),
        object_id: "unit_new_structural".to_string(),
        parent_id: Some("world".to_string()),
        position: FixedVec3::from_i32s(8, 0, -4),
    }
}

fn fixture_document() -> Result<EditorDocument, EngineError> {
    let mut document =
        EditorDocument::new("calyx://world/issue1181-live-hot-reload", "issue1181-rev-1")?;
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
    document.upsert_object(
        EditorObject::new(
            "dialogue.intro",
            "Dialogue",
            Some("world".to_string()),
            Transform::IDENTITY,
        )
        .with_property("dialogue.line", "Hold the ridge."),
    )?;
    document.upsert_named_camera(EditorNamedCamera::top_down(
        "camera.establishing",
        "Establishing",
        Vec3::new(2.0, 0.0, 2.0),
    ))?;
    document.upsert_cinematic_timeline(
        EditorCinematicTimeline::new("cine.intro", "Intro Cutscene", 8.0)
            .with_camera_key(0.0, "camera.establishing")
            .with_camera_key(8.0, "camera.establishing")
            .with_subtitle(CinematicSubtitle {
                start_seconds: 1.0,
                duration_seconds: 3.0,
                text: "Ashen dawn.".to_string(),
            }),
    )?;
    let record = AssetVaultCatalog::load_default_cached()
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.live_hot_reload.fixture.asset_vault".to_string(),
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
            field: "editor.live_hot_reload.fixture.asset".to_string(),
            reason: "missing rematerialized unit mesh asset-vault record (Stan.gltf)".to_string(),
        })?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    Ok(document)
}
