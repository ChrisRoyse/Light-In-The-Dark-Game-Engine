use std::{fs, path::PathBuf};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
};
use litd_engine::math::{Transform, Vec3};

use super::*;

#[test]
fn issue1180_instant_play_source_of_truth_fsv() {
    let document = fixture_document(64);
    let root = test_root("source-of-truth");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("instant-play-ledger.json");
    let aster_dir = root.join("aster");
    let request = request("issue1180-instant-ledger", 5, Vec3::new(14.0, 0.0, -22.0));
    println!(
        "issue1180 happy before_enter repeat={} cursor={:?} budget_ms={}",
        request.repeat_count, request.cursor_world, request.latency_budget_ms
    );

    let ledger = build_instant_play_ledger(&document, request).unwrap();
    let writeback = write_instant_play_ledger(&state_path, &aster_dir, &ledger).unwrap();
    let aster = read_instant_play_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_instant_play_ledger(&aster.bytes).unwrap();

    assert!(!state_path.exists());
    assert_eq!(writeback.bytes, aster.bytes);
    assert_eq!(parsed.readback.run_count, 5);
    assert!(parsed.readback.max_elapsed_ms < DEFAULT_INSTANT_PLAY_BUDGET_MS);
    assert!(parsed.readback.all_runs_within_budget);
    assert!(parsed.readback.spawn_transform_matches_cursor);
    assert!(parsed.readback.repeated_enter_exit_stable);
    assert_eq!(parsed.runs[0].player_transform, Vec3::new(14.0, 0.0, -22.0));
    println!(
        "issue1180 happy after_aster seq={} graph_rows={} ledger_sha={} state_matches_aster={} runs={} max_ms={:.3} spawn={:?} stable={}",
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        writeback.bytes == aster.bytes,
        parsed.readback.run_count,
        parsed.readback.max_elapsed_ms,
        parsed.runs[0].player_transform,
        parsed.readback.repeated_enter_exit_stable
    );
}

#[test]
fn issue1180_edges_large_invalid_and_repeated_stability() {
    let root = test_root("edges");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("instant-play-ledger.json");
    let aster_dir = root.join("aster");
    let large = fixture_document(1_000);
    let large_ledger = build_instant_play_ledger(
        &large,
        request("issue1180-large-world", 3, Vec3::new(20.0, 0.0, -30.0)),
    )
    .unwrap();
    let large_writeback =
        write_instant_play_ledger(&state_path, &aster_dir, &large_ledger).unwrap();
    let large_readback = read_instant_play_ledger(&aster_dir, &large_ledger.ledger_id)
        .unwrap()
        .unwrap();
    assert_eq!(large_readback.bytes, large_writeback.bytes);
    assert!(!state_path.exists());
    assert!(large_ledger.readback.max_elapsed_ms < DEFAULT_INSTANT_PLAY_BUDGET_MS);
    assert!(large_ledger.readback.spawn_transform_matches_cursor);
    println!(
        "issue1180 edge_large before_doodads=1000 before_count=0 after_count={} max_ms={:.3} state_bytes={}",
        large_ledger.readback.run_count,
        large_ledger.readback.max_elapsed_ms,
        large_writeback.bytes.len()
    );

    let invalid = EditorDocument::new("calyx://world/issue1180-invalid", "rev-invalid").unwrap();
    let error = build_instant_play_ledger(
        &invalid,
        request("issue1180-invalid", 1, Vec3::new(0.0, 0.0, 0.0)),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("document.units") || error.contains("player_object_id"));
    assert!(!state_path.exists());
    assert_eq!(
        read_instant_play_ledger(&aster_dir, &large_ledger.ledger_id)
            .unwrap()
            .unwrap()
            .bytes,
        large_writeback.bytes
    );
    println!(
        "issue1180 edge_invalid before_bytes={} after_bytes={} after_error={} state_unchanged=true",
        large_writeback.bytes.len(),
        large_writeback.bytes.len(),
        error
    );

    let repeated = build_instant_play_ledger(
        &fixture_document(32),
        request("issue1180-repeated", 12, Vec3::new(2.0, 0.0, -2.0)),
    )
    .unwrap();
    assert_eq!(repeated.readback.run_count, 12);
    assert!(repeated.readback.repeated_enter_exit_stable);
    assert!(repeated.readback.all_runs_within_budget);
    println!(
        "issue1180 edge_repeated before_count=0 after_count={} max_ms={:.3} stable={}",
        repeated.readback.run_count,
        repeated.readback.max_elapsed_ms,
        repeated.readback.repeated_enter_exit_stable
    );
}

fn request(ledger_id: &str, repeat_count: usize, cursor_world: Vec3) -> InstantPlayRequest {
    InstantPlayRequest {
        ledger_id: ledger_id.to_string(),
        project_id: "litd.project.issue1180.fsv".to_string(),
        actor_id: "human.qa.ember".to_string(),
        player_object_id: "unit_playtest_hero".to_string(),
        camera_rig: PlayCameraRig::FirstPerson,
        cursor_world,
        latency_budget_ms: DEFAULT_INSTANT_PLAY_BUDGET_MS,
        repeat_count,
    }
}

fn fixture_document(extra_doodads: usize) -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1180-instant-play", "issue1180-rev-1").unwrap();
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
    for index in 0..extra_doodads {
        document
            .upsert_object(EditorObject::new(
                format!("doodad_{index:04}"),
                "Doodad",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(index as f32, 0.0, -(index as f32))),
            ))
            .unwrap();
    }
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
    crate::test_scratch::issue_scratch_dir("issue1180", label)
}
