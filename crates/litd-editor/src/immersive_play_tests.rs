use std::{fs, path::PathBuf};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject, stable_fingerprint},
};
use litd_engine::math::{Transform, Vec3};

use super::*;

const FIRST_PERSON_SCREENSHOT_SHA256: &str =
    "728ee8db43128440e2c74bcd2bc70a257a001b06d8418c7f9ba309bb92394bce";
const FIRST_PERSON_SCREENSHOT_BYTES: usize = 1_161_735;

#[test]
fn issue1182_immersive_play_source_of_truth_fsv() {
    let document = fixture_document();
    let root = test_root("source-of-truth");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("immersive-play-ledger.json");
    let aster_dir = root.join("aster");
    let request = happy_request(&document, "issue1182-immersive-ledger");
    println!(
        "issue1182 happy before_enter entry={:?} cursor={:?} selected={:?}",
        request.entry.kind, request.entry.cursor_world, request.editor_before.selected_object_ids
    );

    let ledger = build_immersive_play_ledger(&document, request).unwrap();
    let writeback = write_immersive_play_ledger(&state_path, &aster_dir, &ledger).unwrap();
    let aster = read_immersive_play_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_immersive_play_ledger(&aster.bytes).unwrap();

    assert!(!state_path.exists());
    assert_eq!(writeback.bytes, aster.bytes);
    assert!(parsed.readback.render_active);
    assert!(parsed.readback.hud_active);
    assert!(parsed.readback.input_active);
    assert!(parsed.readback.screenshot_verified);
    assert!(parsed.readback.pause_verified);
    assert!(parsed.readback.fullscreen_toggle_stable);
    assert!(parsed.readback.editor_restored_exact);
    assert_eq!(parsed.readback.camera_rig, PlayCameraRig::FirstPerson);
    assert_eq!(parsed.readback.visible_instances, 1);
    assert_eq!(parsed.readback.draw_batches, 1);
    assert_eq!(parsed.readback.input_event_count, 4);
    assert_eq!(parsed.screenshot.byte_len, FIRST_PERSON_SCREENSHOT_BYTES);
    assert_eq!(parsed.screenshot.sha256, FIRST_PERSON_SCREENSHOT_SHA256);
    println!(
        "issue1182 happy after_aster seq={} graph_rows={} ledger_sha={} state_matches_aster={} render={} hud={} input={} pause={} fullscreen={} restored={} screenshot_sha={}",
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        writeback.bytes == aster.bytes,
        parsed.readback.render_active,
        parsed.readback.hud_active,
        parsed.readback.input_active,
        parsed.readback.pause_verified,
        parsed.readback.fullscreen_toggle_stable,
        parsed.readback.editor_restored_exact,
        parsed.readback.screenshot_sha256
    );
}

#[test]
fn issue1182_edge_cases_fail_closed_without_mutating_state() {
    let document = fixture_document();
    let root = test_root("edges");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("immersive-play-ledger.json");
    let aster_dir = root.join("aster");
    let ledger = build_immersive_play_ledger(
        &document,
        happy_request(&document, "issue1182-edge-baseline"),
    )
    .unwrap();
    write_immersive_play_ledger(&state_path, &aster_dir, &ledger).unwrap();
    let before_aster = read_immersive_play_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    assert!(!state_path.exists());

    let mut bad_exit = happy_request(&document, "issue1182-edge-bad-exit");
    bad_exit.editor_after_exit.active_panel = "object_inspector".to_string();
    let exit_error = build_immersive_play_ledger(&document, bad_exit)
        .unwrap_err()
        .to_string();
    assert!(exit_error.contains("editor.immersive_play.exit"));
    assert!(!state_path.exists());
    assert_eq!(
        read_immersive_play_ledger(&aster_dir, &ledger.ledger_id)
            .unwrap()
            .unwrap()
            .bytes,
        before_aster.bytes
    );
    println!(
        "issue1182 edge_exit before_bytes={} after_error={} state_unchanged=true",
        before_aster.bytes.len(),
        exit_error
    );

    let mut bad_pause = ledger.clone();
    bad_pause.pause.simulation_fingerprint_after_pause = "changed-while-paused".to_string();
    let pause_error = write_immersive_play_ledger(&state_path, &aster_dir, &bad_pause)
        .unwrap_err()
        .to_string();
    assert!(pause_error.contains("editor.immersive_play.pause"));
    assert!(!state_path.exists());
    assert_eq!(
        read_immersive_play_ledger(&aster_dir, &ledger.ledger_id)
            .unwrap()
            .unwrap()
            .bytes,
        before_aster.bytes
    );
    println!(
        "issue1182 edge_pause before_hash={} after_error={} state_unchanged=true",
        ledger.pause.simulation_fingerprint_before_pause, pause_error
    );

    let mut bad_display = happy_request(&document, "issue1182-edge-bad-display");
    bad_display.display_toggle.entered_fullscreen.monitor_index = 2;
    bad_display.display_toggle.entered_fullscreen.monitor_count = 1;
    let display_error = build_immersive_play_ledger(&document, bad_display)
        .unwrap_err()
        .to_string();
    assert!(display_error.contains("editor.immersive_play.display.entered_fullscreen"));
    assert!(!state_path.exists());
    assert_eq!(
        read_immersive_play_ledger(&aster_dir, &ledger.ledger_id)
            .unwrap()
            .unwrap()
            .bytes,
        before_aster.bytes
    );
    println!(
        "issue1182 edge_display before_monitor=0/1 after_error={} state_unchanged=true",
        display_error
    );
}

fn happy_request(document: &EditorDocument, ledger_id: &str) -> ImmersivePlayRequest {
    let fingerprint = stable_fingerprint(document).unwrap();
    let editor_snapshot = ImmersiveEditorStateSnapshot {
        document_id: document.calyx_document_id.clone(),
        revision_id: document.revision_id.clone(),
        document_fingerprint: fingerprint,
        selected_object_ids: vec!["unit_playtest_hero".to_string()],
        active_panel: "project_playtest".to_string(),
        viewport_mode: "editor_world_view".to_string(),
    };
    ImmersivePlayRequest {
        ledger_id: ledger_id.to_string(),
        project_id: "litd.project.issue1182.fsv".to_string(),
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
    }
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

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1182-immersive-play", "issue1182-rev-1").unwrap();
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
    crate::test_scratch::issue_scratch_dir("issue1182", label)
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("litd-editor is under workspace crates")
        .to_path_buf()
}
