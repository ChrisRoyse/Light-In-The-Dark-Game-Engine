use litd_hash::sha256_hex;
use std::{fs, path::Path, path::PathBuf};

use serde_json::{Value, json};

use crate::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    mcp_project_tools::{
        DELIVERY_LITDGAME_TOOL, NEW_OPEN_SAVE_TOOL, PUBLISH_LITDGAME_TOOL, ProjectLifecycleAction,
        call_mcp_project_tool, read_delivery_store_manifest, read_litdgame_install_manifest,
        read_mcp_project_state, read_publish_catalog,
    },
};
use litd_engine::math::{Transform, Vec3};

#[path = "mcp_project_delivery_test_fixture.rs"]
mod fixture;
use fixture::FixturePaths;
#[path = "mcp_project_delivery_test_setup.rs"]
mod setup;
use setup::{delivery_args, prepare_published_fixture};

const PNG_1X1_RGBA: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 15, 4, 0, 9, 251, 3,
    253, 167, 197, 129, 218, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

#[test]
fn issue1158_deliver_litdgame_source_of_truth_fsv() {
    let paths = prepare_published_fixture("unit");
    let content_address = published_content_address(&paths.catalog_path);
    let package_sha256 = content_sha256(&content_address);

    let before = delivery_summary(&paths, &package_sha256);
    println!(
        "issue1158 delivery before {}",
        serde_json::to_string(&before).unwrap()
    );

    let delivered = call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(&paths, &content_address, None),
    )
    .unwrap();
    assert_eq!(delivered.state_readback.package_count, 1);
    assert_eq!(delivered.state_readback.published_count, 1);

    let delivery = read_delivery_store_manifest(&paths.delivery_manifest_path).unwrap();
    let install = read_litdgame_install_manifest(&paths.install_manifest_path).unwrap();
    let delivered_record = &delivery.manifest.entries[0];
    let installed_record = &install.manifest.records[0];
    let package_bytes = fs::read(&paths.package_path).unwrap();
    let blob_bytes = fs::read(&delivered_record.stored_blob_path).unwrap();
    let installed_bytes = fs::read(&installed_record.installed_package_path).unwrap();

    assert_eq!(delivered_record.content_address, content_address);
    assert_eq!(delivered_record.package_sha256, sha256_hex(&package_bytes));
    assert_eq!(delivered_record.stored_blob_sha256, sha256_hex(&blob_bytes));
    assert_eq!(
        installed_record.installed_package_sha256,
        sha256_hex(&installed_bytes)
    );
    assert_eq!(package_bytes, blob_bytes);
    assert_eq!(blob_bytes, installed_bytes);
    assert!(installed_record.runtime_lock_verified);
    assert!(installed_record.world_hash_matches);

    // #1513 regression guard: the delivery fixture assigns a real asset-vault FBX to the
    // unit, so the published package declares assets. Prove the self-contained pipeline
    // actually bundled and materialized those bytes rather than silently shipping a
    // manifest-only package (which would still pass every assertion above).
    let package: Value = serde_json::from_slice(&package_bytes).unwrap();
    let declared_assets = package["assets"].as_array().cloned().unwrap_or_default();
    assert!(
        !declared_assets.is_empty(),
        "fixture must declare at least one asset to exercise #1513 bundling"
    );
    let declared_bytes: u64 = declared_assets
        .iter()
        .map(|asset| asset["source_bytes"].as_u64().unwrap())
        .sum();
    assert_eq!(
        delivered_record.bundled_asset_count,
        declared_assets.len(),
        "every declared asset must be bundled into the delivery store"
    );
    assert_eq!(delivered_record.bundled_asset_bytes as u64, declared_bytes);
    assert_eq!(
        installed_record.materialized_asset_count,
        declared_assets.len(),
        "every declared asset must be materialized co-located with the install"
    );
    assert!(
        installed_record.asset_bytes_verified,
        "#1513: installed package must be byte-verified self-contained"
    );

    // Full state verification against reality: each declared asset's bytes physically
    // exist co-located under the install dir and hash to the declared source_sha256.
    let install_dir = Path::new(&installed_record.installed_package_path)
        .parent()
        .unwrap();
    for asset in &declared_assets {
        let rel = asset["source_relative_path"].as_str().unwrap();
        let declared_sha = asset["source_sha256"].as_str().unwrap();
        let declared_len = asset["source_bytes"].as_u64().unwrap();
        let materialized = install_dir.join(rel);
        let bytes = fs::read(&materialized).unwrap_or_else(|error| {
            panic!(
                "#1513: declared asset {} not materialized co-located at {}: {error}",
                rel,
                materialized.display()
            )
        });
        assert_eq!(
            bytes.len() as u64,
            declared_len,
            "materialized {rel} byte count"
        );
        assert_eq!(
            sha256_hex(&bytes),
            declared_sha,
            "materialized {rel} sha256"
        );
        println!(
            "issue1513 co-located asset {} bytes={} sha256={} VERIFIED",
            rel,
            bytes.len(),
            declared_sha
        );
    }

    let after = delivery_summary(&paths, &package_sha256);
    println!(
        "issue1158 delivery after {}",
        serde_json::to_string(&after).unwrap()
    );
}

#[test]
fn issue1158_delivery_edges_fail_closed_without_state_drift() {
    let paths = prepare_published_fixture("edges");
    let content_address = published_content_address(&paths.catalog_path);
    let package_sha256 = content_sha256(&content_address);

    let remote_before = delivery_summary(&paths, &package_sha256);
    let remote_error = call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(
            &paths,
            &content_address,
            Some("https://cdn.example.invalid/game.litdgame"),
        ),
    )
    .unwrap_err()
    .to_string();
    let remote_after = delivery_summary(&paths, &package_sha256);
    assert_eq!(remote_before, remote_after);
    println!(
        "issue1158 edge remote_source before={} after={} error={}",
        remote_before, remote_after, remote_error
    );

    let wrong_address = format!("litdgame.sha256.{}", "0".repeat(64));
    let wrong_before = delivery_summary(&paths, &content_sha256(&wrong_address));
    let wrong_error = call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(&paths, &wrong_address, None),
    )
    .unwrap_err()
    .to_string();
    let wrong_after = delivery_summary(&paths, &content_sha256(&wrong_address));
    assert_eq!(wrong_before, wrong_after);
    println!(
        "issue1158 edge wrong_content_address before={} after={} error={}",
        wrong_before, wrong_after, wrong_error
    );

    call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(&paths, &content_address, None),
    )
    .unwrap();
    let blob_path = blob_path(&paths.blob_root_path, &package_sha256);
    fs::write(&blob_path, b"tampered-litdgame-bytes").unwrap();
    let tampered_before = delivery_summary(&paths, &package_sha256);
    let tampered_error = call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(&paths, &content_address, None),
    )
    .unwrap_err()
    .to_string();
    let tampered_after = delivery_summary(&paths, &package_sha256);
    assert_eq!(tampered_before, tampered_after);
    println!(
        "issue1158 edge tampered_blob before={} after={} error={}",
        tampered_before, tampered_after, tampered_error
    );
}

#[test]
fn issue1487_delivery_paths_confined_to_project_root() {
    // FSV close gate for #1487: a poisoned install_root_path outside the project
    // root (both absolute-escape and ..-traversal) is rejected with a structured
    // error and leaves the filesystem untouched, while legitimate in-root delivery
    // still succeeds and its outputs verify against the source of truth on disk.
    let paths = prepare_published_fixture("issue1487_confinement");
    let content_address = published_content_address(&paths.catalog_path);
    let package_sha256 = content_sha256(&content_address);

    // --- EDGE 1: absolute install_root_path OUTSIDE the workspace root ---
    let _temp_root = litd_engine::test_temp::TestTempDir::new("issue1487-escape");
    let escape_root = _temp_root.path().to_path_buf();
    if escape_root.exists() {
        fs::remove_dir_all(&escape_root).unwrap();
    }
    // Source of truth BEFORE: escape target absent, state not yet delivered.
    assert!(
        !escape_root.exists(),
        "precondition: escape root must not exist"
    );
    let before_state = read_mcp_project_state(&paths.state_path)
        .map(|readback| readback.state.update_count)
        .unwrap_or(0);

    let mut escape_args = delivery_args(&paths, &content_address, None);
    escape_args["install_root_path"] = json!(escape_root.join("installed"));
    let escape_error = call_mcp_project_tool(DELIVERY_LITDGAME_TOOL, escape_args)
        .unwrap_err()
        .to_string();
    println!("issue1487 absolute-escape rejected: {escape_error}");
    assert!(
        escape_error.contains("outside the project root"),
        "expected outside-root rejection, got: {escape_error}"
    );
    // Source of truth AFTER: NOTHING was written outside the root.
    assert!(
        !escape_root.exists(),
        "confinement breach: delivery created {} outside the project root",
        escape_root.display()
    );

    // --- EDGE 2: ..-traversal install_root_path. Use a literal RELATIVE string so the
    // `..` components actually reach confine_path: joining `..` onto the canonicalized
    // verbatim `paths.root` would silently collapse them at the PathBuf layer (#1487),
    // which is a test-construction artifact, not real attacker input. ---
    let mut traversal_args = delivery_args(&paths, &content_address, None);
    traversal_args["install_root_path"] = json!("installed/../../litd-issue1487-traversal");
    let traversal_error = call_mcp_project_tool(DELIVERY_LITDGAME_TOOL, traversal_args)
        .unwrap_err()
        .to_string();
    println!("issue1487 traversal rejected: {traversal_error}");
    assert!(
        traversal_error.contains("parent-directory"),
        "expected traversal rejection, got: {traversal_error}"
    );
    // Rejected before any fs effect: the location the traversal would have reached
    // (resolved from the working directory) was never created.
    assert!(
        !PathBuf::from("../litd-issue1487-traversal").exists(),
        "confinement breach: traversal created a directory outside the workspace"
    );

    // Both rejections must have left project state completely undrifted.
    let after_state = read_mcp_project_state(&paths.state_path)
        .map(|readback| readback.state.update_count)
        .unwrap_or(0);
    assert_eq!(
        before_state, after_state,
        "rejected deliveries must not drift project state"
    );

    // --- HAPPY PATH: legitimate in-root delivery still succeeds and persists ---
    call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(&paths, &content_address, None),
    )
    .unwrap();
    let install = read_litdgame_install_manifest(&paths.install_manifest_path).unwrap();
    let installed_record = &install.manifest.records[0];
    let installed_bytes = fs::read(&installed_record.installed_package_path).unwrap();
    assert_eq!(
        installed_record.installed_package_sha256,
        sha256_hex(&installed_bytes)
    );
    assert_eq!(installed_record.installed_package_sha256, package_sha256);
    assert!(
        PathBuf::from(&installed_record.installed_package_path)
            .starts_with(&paths.install_root_path),
        "in-root install must land under install_root_path"
    );
}

fn publish_args(paths: &FixturePaths, version: &str, max_package_bytes: usize) -> Value {
    json!({
        "request_id": format!("issue1158-publish-{version}-{max_package_bytes}"),
        "requested_by": "litd-editor-issue1158-test",
        "state_path": paths.state_path,
        "state_id": "issue1158-project-state",
        "ledger_path": paths.root.join("publish-ledger.json"),
        "ledger_id": "issue1158-publish-ledger",
        "project_path": paths.project_path,
        "package_path": paths.package_path,
        "catalog_path": paths.catalog_path,
        "share_id": "issue1158-share",
        "created_by": "agent.project.publisher",
        "host_port": 19580,
        "max_package_bytes": max_package_bytes,
        "title": "Ashen Reach Delivery",
        "version": version,
        "summary": "Known deterministic delivery fixture",
        "tags": ["dark_fantasy", "singleplayer", "local_first"],
        "thumbnail_path": paths.thumbnail_path,
        "min_spec_tags": ["ryzen5", "gtx1660", "ram16gb"]
    })
}

fn fixture_document() -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/issue1158-project", "issue1158-rev-1").unwrap();
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
                "unit_delivery_hero",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(6.0, 0.0, -8.0)),
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
    assign_vault_model_to_unit(&mut document, "unit_delivery_hero", &record).unwrap();
    document
}

fn delivery_summary(paths: &FixturePaths, sha256: &str) -> Value {
    let state = read_mcp_project_state(&paths.state_path).ok();
    let delivery = read_delivery_store_manifest(&paths.delivery_manifest_path).ok();
    let install = read_litdgame_install_manifest(&paths.install_manifest_path).ok();
    let blob_path = blob_path(&paths.blob_root_path, sha256);
    let installed_path = install
        .as_ref()
        .and_then(|readback| readback.manifest.records.first())
        .map(|record| PathBuf::from(&record.installed_package_path))
        .unwrap_or_else(|| paths.install_root_path.join("missing"));
    json!({
        "state_exists": paths.state_path.exists(),
        "state_update_count": state.as_ref().map(|readback| readback.state.update_count).unwrap_or(0),
        "delivery_manifest_exists": paths.delivery_manifest_path.exists(),
        "delivery_entry_count": delivery.as_ref().map(|readback| readback.manifest.entry_count).unwrap_or(0),
        "blob_exists": blob_path.exists(),
        "blob_bytes": blob_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "blob_sha256": fs::read(&blob_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default(),
        "install_manifest_exists": paths.install_manifest_path.exists(),
        "install_record_count": install.as_ref().map(|readback| readback.manifest.record_count).unwrap_or(0),
        "installed_exists": installed_path.exists(),
        "installed_bytes": installed_path.metadata().map(|metadata| metadata.len()).unwrap_or(0),
        "installed_sha256": fs::read(&installed_path).ok().map(|bytes| sha256_hex(&bytes)).unwrap_or_default()
    })
}

fn published_content_address(catalog_path: &Path) -> String {
    read_publish_catalog(catalog_path).unwrap().catalog.entries[0]
        .content_address
        .clone()
}

fn content_sha256(content_address: &str) -> String {
    content_address
        .strip_prefix("litdgame.sha256.")
        .unwrap()
        .to_string()
}

fn blob_path(root: &Path, sha256: &str) -> PathBuf {
    root.join("blobs").join("sha256").join(sha256)
}

fn test_root(label: &str) -> PathBuf {
    crate::test_scratch::issue_scratch_dir("issue1158_delivery_litdgame", label)
}

fn clean_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap();
    }
    fs::create_dir_all(path).unwrap();
}
