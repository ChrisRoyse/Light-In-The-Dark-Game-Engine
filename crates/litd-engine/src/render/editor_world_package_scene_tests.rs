//! #1891 unit coverage: package map scene build is fail-closed and map-derived.

use super::editor_world_package_scene::build_package_world_scene;
use crate::game_package::{
    LITD_LOCK_IN_POLICY, LITD_QUICK_SHARE_SCHEMA, LITD_RUNTIME_ID, RuntimePackageAssetRef,
    RuntimePackageEndpoint, RuntimePackageHostMode, compute_runtime_manifest_fingerprint,
    inspect_runtime_locked_package_bytes,
};
use litd_hash::sha256_hex;
use serde_json::json;
use std::path::Path;

fn package_bytes(share_id: &str, document: serde_json::Value) -> Vec<u8> {
    let endpoint = RuntimePackageEndpoint {
        mode: RuntimePackageHostMode::Loopback,
        host: "127.0.0.1".to_string(),
        port: 8490,
        path: format!("/share/{share_id}"),
    };
    let document_fingerprint = "1891ad0c1891ad0c";
    let play_build_fingerprint = "1891beef1891beef";
    // Envelope validation requires asset_count > 0 (runtime lock-in).
    let placeholder = b"issue1891-package-scene-fixture";
    let assets = vec![RuntimePackageAssetRef {
        product_id: "litd.asset.issue1891.fixture".to_string(),
        source_relative_path: "assets/issue1891.bin".to_string(),
        source_sha256: sha256_hex(placeholder),
        source_bytes: placeholder.len() as u64,
    }];
    let manifest_fingerprint = compute_runtime_manifest_fingerprint(
        share_id,
        "agent.issue1891",
        LITD_RUNTIME_ID,
        &endpoint,
        document_fingerprint,
        play_build_fingerprint,
        &assets,
    )
    .expect("manifest fingerprint");
    let envelope = json!({
        "schema": LITD_QUICK_SHARE_SCHEMA,
        "share_id": share_id,
        "package_id": format!("litd.quick_share.{share_id}"),
        "created_by": "agent.issue1891",
        "runtime_id": LITD_RUNTIME_ID,
        "lock_in_policy": LITD_LOCK_IN_POLICY,
        "endpoint": endpoint,
        "host_uri": endpoint.uri(),
        "document": document,
        "document_fingerprint": document_fingerprint,
        "play_build_fingerprint": play_build_fingerprint,
        "asset_count": assets.len(),
        "assets": assets,
        "manifest_fingerprint": manifest_fingerprint
    });
    serde_json::to_vec_pretty(&envelope).expect("serialize package")
}

fn load_package(
    share_id: &str,
    document: serde_json::Value,
) -> crate::game_package::RuntimePackageReadback {
    let bytes = package_bytes(share_id, document);
    inspect_runtime_locked_package_bytes(
        Path::new(&format!("{share_id}.litdgame.json")),
        &bytes,
        LITD_RUNTIME_ID,
        "package_scene_tests",
    )
    .expect("inspect package")
}

#[test]
fn package_scene_builds_from_terrain_and_start_markers() {
    let package = load_package(
        "issue1891happy",
        json!({
            "schema": "litd.lowered_document.v1",
            "calyx_document_id": "calyx://world/issue1891-test-map",
            "terrain_cells": {
                "0_0": { "tile_x": 0, "tile_z": 0, "height": 0.4, "cliff_level": 0 },
                "1_0": { "tile_x": 1, "tile_z": 0, "height": 0.4, "cliff_level": 0 },
                "0_1": { "tile_x": 0, "tile_z": 1, "height": 1.2, "cliff_level": 1 },
                "1_1": { "tile_x": 1, "tile_z": 1, "height": 0.4, "cliff_level": 0 }
            },
            "objects": {
                "start_player_one": {
                    "kind": "StartLocation",
                    "transform": { "translation": [0.5, 0.0, 0.5] }
                },
                "gold_mine_a": {
                    "kind": "ResourceNode",
                    "transform": { "translation": [1.5, 0.0, 0.5] }
                }
            }
        }),
    );
    let built = build_package_world_scene(&package).expect("build package scene");
    assert_eq!(built.terrain_cell_count, 4);
    assert_eq!(built.marker_count, 2);
    assert_eq!(built.document_id, "calyx://world/issue1891-test-map");
    let scene_name = built.scene.scene_name();
    assert!(
        scene_name.starts_with("package_map:"),
        "scene_name must identify package map, got {scene_name}"
    );
    assert!(!built.scene.vertices().is_empty());
    assert!(built.map_half_extent >= 1.0);
}

#[test]
fn package_scene_fails_closed_without_terrain_cells() {
    let package = load_package(
        "issue1891empty",
        json!({
            "schema": "litd.lowered_document.v1",
            "document_id": "calyx://world/issue1891-no-terrain",
            "objects": {}
        }),
    );
    match build_package_world_scene(&package) {
        Ok(_) => panic!("must refuse ED11 fallback when terrain_cells missing"),
        Err(err) => {
            let text = err.to_string();
            assert!(
                text.contains("terrain_cells") || text.contains("package_world_scene"),
                "error must name the missing source of truth, got {text}"
            );
        }
    }
}

#[test]
fn package_scene_fails_closed_on_empty_terrain_object() {
    let package = load_package(
        "issue1891emptycells",
        json!({
            "schema": "litd.lowered_document.v1",
            "document_id": "calyx://world/issue1891-empty-cells",
            "terrain_cells": {}
        }),
    );
    match build_package_world_scene(&package) {
        Ok(_) => panic!("empty terrain must fail"),
        Err(err) => assert!(err.to_string().contains("terrain_cells")),
    }
}
