use std::{fs, path::PathBuf};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
};
use litd_engine::math::{FixedVec3, Transform, Vec3};

use super::*;

#[test]
fn issue1181_live_hot_reload_source_of_truth_fsv() {
    let document = fixture_document();
    let root = test_root("source-of-truth");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("live-hot-reload-ledger.json");
    let aster_dir = root.join("aster");
    let request = happy_request("issue1181-live-reload-ledger", safe_patches());
    println!(
        "issue1181 happy before patches={} ticks={} velocity={:?}",
        request.patches.len(),
        request.pre_reload_ticks,
        request.runtime_velocity
    );

    let ledger = build_live_hot_reload_ledger(&document, request).unwrap();
    let writeback = write_live_hot_reload_ledger(&state_path, &aster_dir, &ledger).unwrap();
    let aster = read_live_hot_reload_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_live_hot_reload_ledger(&aster.bytes).unwrap();

    assert!(!state_path.exists());
    assert_eq!(writeback.bytes, aster.bytes);
    assert_eq!(parsed.readback.patch_count, 3);
    assert_eq!(parsed.readback.applied_live_count, 3);
    assert_eq!(parsed.readback.deferred_structural_count, 0);
    assert_eq!(parsed.readback.final_generation, 3);
    assert!(parsed.readback.player_position_preserved_all);
    assert!(parsed.readback.safe_patches_reflected_all);
    assert_ne!(
        parsed.initial_runtime.player_position,
        FixedVec3::from_i32s(3, 0, -6)
    );
    assert_eq!(
        parsed.initial_runtime.player_position,
        parsed.final_runtime.player_position
    );
    println!(
        "issue1181 happy after_aster seq={} graph_rows={} ledger_sha={} state_matches_aster={} applied={} generation={} runtime_hash={} position={:?}",
        aster.seq,
        aster.graph_rows,
        aster.sha256,
        writeback.bytes == aster.bytes,
        parsed.readback.applied_live_count,
        parsed.readback.final_generation,
        parsed.readback.final_runtime_state_hash,
        parsed.final_runtime.player_position
    );
}

#[test]
fn issue1181_edges_defer_structural_and_keep_rapid_reloads_stable() {
    let document = fixture_document();
    let root = test_root("edges");
    fs::create_dir_all(&root).unwrap();
    let state_path = root.join("live-hot-reload-ledger.json");
    let aster_dir = root.join("aster");

    let structural = build_live_hot_reload_ledger(
        &document,
        happy_request("issue1181-structural-edge", vec![structural_patch()]),
    )
    .unwrap();
    let structural_writeback =
        write_live_hot_reload_ledger(&state_path, &aster_dir, &structural).unwrap();
    let structural_aster = read_live_hot_reload_ledger(&aster_dir, &structural.ledger_id)
        .unwrap()
        .unwrap();
    assert_eq!(structural_writeback.bytes, structural_aster.bytes);
    assert!(!state_path.exists());
    assert_eq!(structural.readback.applied_live_count, 0);
    assert_eq!(structural.readback.deferred_structural_count, 1);
    assert_eq!(structural.readback.final_generation, 0);
    assert!(structural.readback.player_position_preserved_all);
    assert_eq!(
        structural.steps[0].notice_code.as_deref(),
        Some(LIVE_HOT_RELOAD_NOTICE_STRUCTURAL)
    );
    println!(
        "issue1181 edge_structural before_generation=0 after_generation={} notice={} state_bytes={}",
        structural.readback.final_generation,
        structural.steps[0].notice_code.as_deref().unwrap(),
        structural_writeback.bytes.len()
    );

    let rapid = build_live_hot_reload_ledger(
        &document,
        happy_request("issue1181-rapid-edge", rapid_patches()),
    )
    .unwrap();
    let rapid_writeback = write_live_hot_reload_ledger(&state_path, &aster_dir, &rapid).unwrap();
    let rapid_aster = read_live_hot_reload_ledger(&aster_dir, &rapid.ledger_id)
        .unwrap()
        .unwrap();
    assert_eq!(rapid.readback.patch_count, 8);
    assert_eq!(rapid.readback.applied_live_count, 8);
    assert_eq!(rapid.readback.final_generation, 8);
    assert!(rapid.readback.player_position_preserved_all);
    assert!(rapid.readback.safe_patches_reflected_all);
    assert_eq!(rapid_writeback.bytes, rapid_aster.bytes);
    println!(
        "issue1181 edge_rapid before_count=0 after_count={} generation={} state_matches_aster={} runtime_hash={}",
        rapid.readback.rapid_reload_count,
        rapid.readback.final_generation,
        rapid_writeback.bytes == rapid_aster.bytes,
        rapid.readback.final_runtime_state_hash
    );

    let mut bad = rapid.clone();
    bad.steps[3].runtime_after.player_position = FixedVec3::from_i32s(99, 0, 99);
    bad.steps[3].player_position_preserved = false;
    let error = write_live_hot_reload_ledger(&state_path, &aster_dir, &bad)
        .unwrap_err()
        .to_string();
    assert!(!state_path.exists());
    assert_eq!(
        read_live_hot_reload_ledger(&aster_dir, &rapid.ledger_id)
            .unwrap()
            .unwrap()
            .bytes,
        rapid_writeback.bytes
    );
    assert!(error.contains("editor.live_hot_reload.step.safe"));
    println!(
        "issue1181 edge_position before_preserved=true after_error={} state_unchanged=true",
        error
    );
}

fn happy_request(ledger_id: &str, patches: Vec<LiveHotReloadPatch>) -> LiveHotReloadRequest {
    LiveHotReloadRequest {
        ledger_id: ledger_id.to_string(),
        session_id: "issue1181-session".to_string(),
        project_id: "litd.project.issue1181.fsv".to_string(),
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

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1181-live-hot-reload", "issue1181-rev-1").unwrap();
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
    crate::test_scratch::issue_scratch_dir("issue1181", label)
}
