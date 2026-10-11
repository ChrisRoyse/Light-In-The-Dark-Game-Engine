use litd_hash::sha256_hex;
use std::{fs, path::PathBuf};

use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject, stable_fingerprint},
    immersive_play::{
        ImmersiveDisplayMode, ImmersiveDisplayState, ImmersiveDisplayToggleProof,
        ImmersiveEditorStateSnapshot, ImmersiveEntryKind, ImmersiveEntryRequest,
        ImmersiveInputAction, ImmersiveInputCommand, ImmersivePlayRequest,
        ImmersiveScreenshotInput, ImmersiveViewport, build_immersive_play_ledger,
        parse_immersive_play_ledger, read_immersive_play_ledger, validate_immersive_play_ledger,
        write_immersive_play_ledger,
    },
    play::PlayCameraRig,
};
use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use serde_json::json;

const FIRST_PERSON_SCREENSHOT_SHA256: &str =
    "728ee8db43128440e2c74bcd2bc70a257a001b06d8418c7f9ba309bb92394bce";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE1182_FSV_ROOT",
        "issue1182_immersive_play",
        "manual",
    )?;
    let aster_dir = root.join("aster");
    let ledger_path = root.join("immersive-play-ledger.json");
    let artifact_path = root.join("issue1182-immersive-play-readback.json");
    let document = fixture_document()?;

    let request = happy_request(&document, "issue1182-manual-ledger")?;
    println!(
        "happy_path before_enter entry={:?} cursor={:?} selected={:?}",
        request.entry.kind, request.entry.cursor_world, request.editor_before.selected_object_ids
    );
    let ledger = build_immersive_play_ledger(&document, request)?;
    let writeback = write_immersive_play_ledger(&ledger_path, &aster_dir, &ledger)?;
    let aster = read_immersive_play_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing immersive-play ledger in Aster")?;
    let parsed = parse_immersive_play_ledger(&aster.bytes)?;
    let state_bytes = fs::read(&ledger_path)?;
    if state_bytes != aster.bytes {
        return Err(format!("ledger state/Aster mismatch at {}", ledger_path.display()).into());
    }
    println!(
        "happy_path after_aster seq={} graph_rows={} ledger_sha={} state_matches_aster={} render={} hud={} input={} pause={} fullscreen={} restored={}",
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        state_bytes == aster.bytes,
        parsed.readback.render_active,
        parsed.readback.hud_active,
        parsed.readback.input_active,
        parsed.readback.pause_verified,
        parsed.readback.fullscreen_toggle_stable,
        parsed.readback.editor_restored_exact
    );

    let edge_exit = edge_exit_restore(&document, &ledger_path, &aster_dir, &ledger, &state_bytes)?;
    let edge_pause = edge_pause_freeze(&ledger_path, &aster_dir, &ledger, &state_bytes)?;
    let edge_display =
        edge_invalid_display(&document, &ledger_path, &aster_dir, &ledger, &state_bytes)?;
    let screenshot_bytes = fs::read(&parsed.screenshot.path)?;
    let artifact = json!({
        "issue": 1182,
        "source_of_truth": "Aster Graph CF immersive-play ledger row plus JSON state-file readback and physical docs/readme first-person screenshot bytes",
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
            "immersive_state": {
                "entry": parsed.entry,
                "render": parsed.render,
                "hud": parsed.hud,
                "input_log": parsed.input_log,
                "pause": parsed.pause,
                "display_toggle": parsed.display_toggle,
                "exit": parsed.exit,
                "readback": parsed.readback
            },
            "screenshot_file_readback": {
                "path": parsed.screenshot.path,
                "bytes": screenshot_bytes.len(),
                "sha256": sha256_hex(&screenshot_bytes),
                "matches_ledger": sha256_hex(&screenshot_bytes) == parsed.screenshot.sha256
            }
        },
        "edge_cases": [edge_exit, edge_pause, edge_display]
    });
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    fs::write(&artifact_path, &artifact_bytes)?;
    let artifact_readback = fs::read(&artifact_path)?;
    if artifact_readback != artifact_bytes {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "wrote issue1182 immersive FSV artifact={} bytes={} artifact_sha={} ledger_sha={} screenshot_sha={}",
        artifact_path.display(),
        artifact_readback.len(),
        report_readback.sha256,
        aster.sha256,
        parsed.screenshot.sha256
    );
    Ok(())
}

fn edge_exit_restore(
    document: &EditorDocument,
    ledger_path: &std::path::Path,
    aster_dir: &std::path::Path,
    baseline: &litd_editor::immersive_play::ImmersivePlayLedger,
    before_state: &[u8],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut request = happy_request(document, "issue1182-manual-bad-exit")?;
    request.editor_after_exit.active_panel = "object_inspector".to_string();
    let error = build_immersive_play_ledger(document, request)
        .expect_err("bad exit restore should fail")
        .to_string();
    let after_state = fs::read(ledger_path)?;
    let after_aster = read_immersive_play_ledger(aster_dir, &baseline.ledger_id)?
        .ok_or("missing baseline after bad exit")?;
    println!(
        "edge_exit before_bytes={} after_error={} state_unchanged={}",
        before_state.len(),
        error,
        after_state == before_state && after_aster.bytes == before_state
    );
    Ok(json!({
        "name": "exit_restores_exact_editor_state",
        "before_state_bytes": before_state.len(),
        "after_error": error,
        "state_unchanged": after_state == before_state && after_aster.bytes == before_state
    }))
}

fn edge_pause_freeze(
    ledger_path: &std::path::Path,
    aster_dir: &std::path::Path,
    baseline: &litd_editor::immersive_play::ImmersivePlayLedger,
    before_state: &[u8],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut bad = baseline.clone();
    bad.pause.simulation_fingerprint_after_pause = "changed-while-paused".to_string();
    let error = validate_immersive_play_ledger(&bad)
        .expect_err("pause drift should fail")
        .to_string();
    let after_state = fs::read(ledger_path)?;
    let after_aster = read_immersive_play_ledger(aster_dir, &baseline.ledger_id)?
        .ok_or("missing baseline after pause drift")?;
    println!(
        "edge_pause before_hash={} after_error={} state_unchanged={}",
        baseline.pause.simulation_fingerprint_before_pause,
        error,
        after_state == before_state && after_aster.bytes == before_state
    );
    Ok(json!({
        "name": "pause_freezes_simulation_snapshot",
        "before_pause_hash": baseline.pause.simulation_fingerprint_before_pause,
        "after_error": error,
        "state_unchanged": after_state == before_state && after_aster.bytes == before_state
    }))
}

fn edge_invalid_display(
    document: &EditorDocument,
    ledger_path: &std::path::Path,
    aster_dir: &std::path::Path,
    baseline: &litd_editor::immersive_play::ImmersivePlayLedger,
    before_state: &[u8],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut request = happy_request(document, "issue1182-manual-bad-display")?;
    request.display_toggle.entered_fullscreen.monitor_index = 2;
    request.display_toggle.entered_fullscreen.monitor_count = 1;
    let error = build_immersive_play_ledger(document, request)
        .expect_err("invalid monitor should fail")
        .to_string();
    let after_state = fs::read(ledger_path)?;
    let after_aster = read_immersive_play_ledger(aster_dir, &baseline.ledger_id)?
        .ok_or("missing baseline after invalid display")?;
    println!(
        "edge_display before_monitor=0/1 after_error={} state_unchanged={}",
        error,
        after_state == before_state && after_aster.bytes == before_state
    );
    Ok(json!({
        "name": "fullscreen_monitor_index_rejected",
        "before_monitor": "0/1",
        "after_error": error,
        "state_unchanged": after_state == before_state && after_aster.bytes == before_state
    }))
}

fn happy_request(
    document: &EditorDocument,
    ledger_id: &str,
) -> Result<ImmersivePlayRequest, EngineError> {
    let fingerprint = stable_fingerprint(document)?;
    let editor_snapshot = ImmersiveEditorStateSnapshot {
        document_id: document.calyx_document_id.clone(),
        revision_id: document.revision_id.clone(),
        document_fingerprint: fingerprint,
        selected_object_ids: vec!["unit_playtest_hero".to_string()],
        active_panel: "project_playtest".to_string(),
        viewport_mode: "editor_world_view".to_string(),
    };
    Ok(ImmersivePlayRequest {
        ledger_id: ledger_id.to_string(),
        project_id: "litd.project.issue1182.manual".to_string(),
        actor_id: "human.qa.ember".to_string(),
        player_object_id: "unit_playtest_hero".to_string(),
        camera_rig: PlayCameraRig::FirstPerson,
        entry: ImmersiveEntryRequest {
            kind: ImmersiveEntryKind::Cursor,
            spawn_object_id: None,
            cursor_world: Some(Vec3::new(12.0, 0.0, -18.0)),
        },
        viewport: ImmersiveViewport {
            width_px: 1920,
            height_px: 1080,
        },
        screenshot: ImmersiveScreenshotInput {
            path: workspace_root()
                .join("docs/readme/ED13-play-firstperson.png")
                .display()
                .to_string(),
            expected_sha256: FIRST_PERSON_SCREENSHOT_SHA256.to_string(),
        },
        input_commands: vec![
            input(
                "input-forward-down",
                ImmersiveInputAction::MoveForward,
                true,
                0.0,
            ),
            input(
                "input-forward-up",
                ImmersiveInputAction::MoveForward,
                false,
                16.0,
            ),
            input(
                "input-confirm-down",
                ImmersiveInputAction::Confirm,
                true,
                32.0,
            ),
            input(
                "input-confirm-up",
                ImmersiveInputAction::Confirm,
                false,
                48.0,
            ),
        ],
        editor_before: editor_snapshot.clone(),
        editor_after_exit: editor_snapshot,
        display_toggle: display_toggle(),
    })
}

fn input(
    command_id: &str,
    action: ImmersiveInputAction,
    pressed: bool,
    timestamp_ms: f64,
) -> ImmersiveInputCommand {
    ImmersiveInputCommand {
        command_id: command_id.to_string(),
        action,
        pressed,
        timestamp_ms,
    }
}

fn display_toggle() -> ImmersiveDisplayToggleProof {
    let before = ImmersiveDisplayState {
        mode: ImmersiveDisplayMode::Windowed,
        monitor_index: 0,
        monitor_count: 1,
        width_px: 1920,
        height_px: 1080,
        scale_factor_milli: 1000,
    };
    ImmersiveDisplayToggleProof {
        before: before.clone(),
        entered_fullscreen: ImmersiveDisplayState {
            mode: ImmersiveDisplayMode::Fullscreen,
            monitor_index: 0,
            monitor_count: 1,
            width_px: 1920,
            height_px: 1080,
            scale_factor_milli: 1000,
        },
        restored: before,
        stable: true,
    }
}

fn fixture_document() -> Result<EditorDocument, EngineError> {
    let mut document =
        EditorDocument::new("calyx://world/issue1182-immersive-play", "issue1182-rev-1")?;
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
    let record = AssetVaultCatalog::load_default_cached()
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.immersive_play.fixture.asset_vault".to_string(),
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
            field: "editor.immersive_play.fixture.asset".to_string(),
            reason: "missing rematerialized unit mesh asset-vault record (Stan.gltf)".to_string(),
        })?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    Ok(document)
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("litd-editor is under workspace crates")
        .to_path_buf()
}
