use std::{fs, path::PathBuf};

use crate::{
    asset_products::{MODEL_SOURCE_SHA256_PROPERTY, assign_vault_model_to_unit},
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject, stable_fingerprint},
};
use litd_engine::math::{Transform, Vec3};

use super::*;

#[test]
fn issue1187_quick_share_source_of_truth_fsv() {
    let document = fixture_document();
    let root = test_root("source-of-truth");
    fs::create_dir_all(&root).unwrap();
    let package_path = root.join("ember-quick-share.litdgame.json");
    let ledger_path = root.join("quick-share-ledger.json");
    let aster_dir = root.join("aster");
    let request = request("issue1187-share", 128 * 1024);

    let package = build_quick_share_package(&document, &request).unwrap();
    println!(
        "issue1187 happy before_write package_id={} doc_hash={} play_hash={} assets={} host={}",
        package.package_id,
        package.document_fingerprint,
        package.play_build_fingerprint,
        package.asset_count,
        package.host_uri
    );
    let writeback =
        write_quick_share_package(&package_path, &package, request.max_package_bytes).unwrap();
    let client =
        load_quick_share_package(&package_path, LITD_RUNTIME_ID, "human.client.two").unwrap();
    let runtime =
        litd_engine::game_package::load_runtime_locked_package(&package_path, "litd-game-cli")
            .unwrap();
    let ledger = build_quick_share_ledger(
        "issue1187-quick-share-ledger",
        &package,
        &writeback,
        &client,
    )
    .unwrap();
    let ledger_writeback = write_quick_share_ledger(&ledger_path, &aster_dir, &ledger).unwrap();
    let ledger_readback = read_quick_share_ledger(&aster_dir, &ledger.ledger_id)
        .unwrap()
        .unwrap();
    let parsed = parse_quick_share_ledger(&ledger_readback.bytes).unwrap();
    let package_disk = fs::read(&package_path).unwrap();

    assert!(!ledger_path.exists());
    assert_eq!(ledger_readback.bytes, ledger_writeback.bytes);
    assert_eq!(package_disk.len(), writeback.bytes);
    assert_eq!(sha256_hex(&package_disk), writeback.sha256);
    assert!(parsed.world_hash_matches);
    assert!(parsed.runtime_lock_verified);
    assert_eq!(
        parsed.document_fingerprint,
        parsed.client_document_fingerprint
    );
    assert_eq!(
        parsed.play_build_fingerprint,
        parsed.client_play_build_fingerprint
    );
    assert_eq!(
        package.document["schema"],
        litd_engine::game_package::LITD_LOWERED_DOCUMENT_SCHEMA
    );
    assert_eq!(
        package.source_provenance.source_document_fingerprint,
        stable_fingerprint(&document).unwrap()
    );
    assert_eq!(
        runtime.document_summary.document_schema.as_deref(),
        Some(litd_engine::game_package::LITD_LOWERED_DOCUMENT_SCHEMA)
    );
    assert_eq!(runtime.document_fingerprint, package.document_fingerprint);
    assert!(runtime.runtime_lock_verified);
    assert!(runtime.manifest_integrity_verified);
    assert_eq!(client.host_resolution.resolved_host, "127.0.0.1");
    println!(
        "issue1187 happy after_aster share={} package_bytes={} package_sha={} seq={} graph_rows={} ledger_sha={} cell={} index={}",
        parsed.share_id,
        parsed.package_bytes,
        parsed.package_sha256,
        ledger_readback.seq,
        ledger_readback.graph_rows,
        ledger_readback.sha256,
        ledger_writeback.cell_key,
        ledger_writeback.index_key
    );
}

#[test]
fn issue1187_edges_fail_closed() {
    let document = fixture_document();
    let root = test_root("edges");
    fs::create_dir_all(&root).unwrap();

    let mut stale = document.clone();
    let fake_sha = "0".repeat(64);
    let first_product_id = stale.asset_products.keys().next().cloned().unwrap();
    stale
        .asset_products
        .get_mut(&first_product_id)
        .unwrap()
        .source_sha256 = fake_sha.clone();
    stale
        .objects
        .get_mut("unit_playtest_hero")
        .unwrap()
        .properties
        .insert(MODEL_SOURCE_SHA256_PROPERTY.to_string(), fake_sha);
    let stale_before_assets = stale.asset_products.len();
    let stale_error = build_quick_share_package(&stale, &request("issue1187-stale", 128 * 1024))
        .unwrap_err()
        .to_string();
    assert!(stale_error.contains("editor.quick_share.assets.stale"));
    println!(
        "issue1187 edge_stale before_assets={} after_package=none error={}",
        stale_before_assets, stale_error
    );

    let tiny_package_path = root.join("too-large.litdgame.json");
    let tiny_request = request("issue1187-large", 256);
    let existed_before = tiny_package_path.exists();
    let large_error = build_quick_share_package(&document, &tiny_request)
        .unwrap_err()
        .to_string();
    assert!(large_error.contains("editor.quick_share.package.size"));
    assert_eq!(tiny_package_path.exists(), existed_before);
    println!(
        "issue1187 edge_large before_file_exists={} after_file_exists={} limit={} error={}",
        existed_before,
        tiny_package_path.exists(),
        tiny_request.max_package_bytes,
        large_error
    );

    let package_path = root.join("runtime-lock.litdgame.json");
    let runtime_request = request("issue1187-runtime-lock", 128 * 1024);
    let package = build_quick_share_package(&document, &runtime_request).unwrap();
    let writeback =
        write_quick_share_package(&package_path, &package, runtime_request.max_package_bytes)
            .unwrap();
    let mismatch_error =
        load_quick_share_package(&package_path, "foreign.runtime.v1", "human.client.two")
            .unwrap_err()
            .to_string();
    assert!(mismatch_error.contains("editor.quick_share.runtime_lock"));
    assert!(package_path.exists());
    println!(
        "issue1187 edge_runtime package_sha={} file_exists={} error={}",
        writeback.sha256,
        package_path.exists(),
        mismatch_error
    );
}

fn request(share_id: &str, max_package_bytes: usize) -> QuickShareBuildRequest {
    QuickShareBuildRequest::new(
        share_id,
        "human.qa.ember",
        QuickShareEndpoint::loopback(share_id, 11870),
        max_package_bytes,
    )
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1187-quick-share", "issue1187-rev-1").unwrap();
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
    crate::test_scratch::issue_scratch_dir("issue1187", label)
}
