use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_store::{ContentAddressedAssetStore, MeshAsset},
    asset_vault::{AssetVaultCatalog, AssetVaultRecord},
    history::{EditorDocument, EditorObject},
    mcp_project_tools::{
        DELIVERY_LITDGAME_TOOL, NEW_OPEN_SAVE_TOOL, PUBLISH_LITDGAME_TOOL, ProjectLifecycleAction,
        call_mcp_project_tool, read_delivery_store_manifest, read_litdgame_install_manifest,
        read_publish_catalog,
    },
};
use litd_engine::math::{Transform, Vec3};
use litd_hash::sha256_hex;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

const GLB_RECORD: &str = "extracted/kenney/game-kit/mini-dungeon/Models/GLB_format/gate.glb";
const PNG_1X1_RGBA: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 15, 4, 0, 9, 251, 3,
    253, 167, 197, 129, 218, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    litd_editor::fsv_support::ensure_project_root_env();
    let (run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE776_FSV_ROOT",
        "issue776_local_content_store",
        "manual",
    )?;
    let paths = prepare_fixture(&root, "issue776")?;
    let glb = verify_real_glb_asset()?;
    let content_address = published_content_address(&paths.catalog_path)?;
    let package_sha256 = content_sha256(&content_address);
    let before = delivery_summary(&paths, &package_sha256);

    let delivered = call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(&paths, &content_address, None),
    )?;
    let delivery = read_delivery_store_manifest(&paths.delivery_manifest_path)?;
    let install = read_litdgame_install_manifest(&paths.install_manifest_path)?;
    let delivered_record = delivery
        .manifest
        .entries
        .first()
        .ok_or("missing delivery record")?;
    let installed_record = install
        .manifest
        .records
        .first()
        .ok_or("missing install record")?;
    let package_bytes = fs::read(&paths.package_path)?;
    let blob_bytes = fs::read(&delivered_record.stored_blob_path)?;
    let installed_bytes = fs::read(&installed_record.installed_package_path)?;
    if package_bytes != blob_bytes || blob_bytes != installed_bytes {
        return Err("package/blob/install bytes diverged".into());
    }
    if sha256_hex(&blob_bytes) != package_sha256 {
        return Err("blob sha256 did not match content address".into());
    }
    let duplicate = call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(&paths, &content_address, None),
    )?;
    let after = delivery_summary(&paths, &package_sha256);

    let edges = json!({
        "remote_source_rejected": edge_remote_source(&root)?,
        "missing_content_address_rejected": edge_missing_address(&root)?,
        "partial_existing_blob_rejected": edge_partial_blob(&root)?,
        "hash_mismatch_existing_blob_rejected": edge_hash_mismatch_blob(&root)?,
    });
    let report_path = root.join("issue776-local-content-store-fsv.json");
    let report = json!({
        "issue": 776,
        "source_of_truth": {
            "research_sources": [
                "Git content-addressed object store: https://git-scm.com/book/en/v2/Git-Internals-Git-Objects",
                "Nix content-addressed store reference: https://nix.dev/manual/nix/2.19/command-ref/new-cli/nix3-store-make-content-addressed",
                "LLVM CAS immutability model: https://llvm.org/docs/ContentAddressableStorage.html"
            ],
            "delivery_manifest_aster_path": paths.delivery_manifest_path,
            "install_manifest_aster_path": paths.install_manifest_path,
            "blob_root_path": paths.blob_root_path,
            "package_path": paths.package_path,
            "published_catalog_path": paths.catalog_path,
        },
        "real_glb_asset_readback": glb,
        "before": before,
        "after": after,
        "happy_path": {
            "content_address": content_address,
            "package_sha256": package_sha256,
            "package_bytes": package_bytes.len(),
            "blob_path": delivered_record.stored_blob_path,
            "blob_relative_path": delivered_record.store_relative_path,
            "blob_sha256": sha256_hex(&blob_bytes),
            "installed_path": installed_record.installed_package_path,
            "installed_sha256": sha256_hex(&installed_bytes),
            "bytes_identical": package_bytes == blob_bytes && blob_bytes == installed_bytes,
            "world_hash_matches": installed_record.world_hash_matches,
            "runtime_lock_verified": installed_record.runtime_lock_verified,
            "delivery_manifest_sha256": delivery.sha256,
            "delivery_manifest_bytes": delivery.bytes,
            "install_manifest_sha256": install.sha256,
            "install_manifest_bytes": install.bytes,
            "state_packages": delivered.state_readback.package_count,
            "state_published": delivered.state_readback.published_count,
            "duplicate_delivery_state_packages": duplicate.state_readback.package_count,
            "duplicate_delivery_state_published": duplicate.state_readback.published_count
        },
        "edges": edges
    });
    let report_bytes = serde_json::to_vec_pretty(&report)?;
    fs::write(&report_path, &report_bytes)?;
    if fs::read(&report_path)? != report_bytes {
        return Err("report readback mismatch".into());
    }
    let report_readback = run.record_json_file(&report_path)?;
    println!(
        "ISSUE776_LOCAL_CONTENT_STORE_FSV report={} sha256={} blob={} blob_sha256={} package_bytes={} edges={}",
        report_path.display(),
        report_readback.sha256,
        delivered_record.stored_blob_path,
        sha256_hex(&blob_bytes),
        package_bytes.len(),
        report["edges"]
            .as_object()
            .map(|edges| edges.len())
            .unwrap_or(0)
    );
    Ok(())
}

fn verify_real_glb_asset() -> Result<Value, Box<dyn std::error::Error>> {
    let store = ContentAddressedAssetStore::load_default_cached()?;
    let record = real_record(store.catalog(), GLB_RECORD)?;
    let handle = store.handle_for_record::<MeshAsset>(&record)?;
    let receipt = store.load(handle.clone())?;
    if receipt.verified_sha256 != record.sha256 || receipt.bytes_read != record.bytes_on_disk {
        return Err("GLB asset store readback did not match manifest".into());
    }
    Ok(json!({
        "relative_path": record.relative_path,
        "absolute_path": record.absolute_path,
        "handle_id": handle.handle_id(),
        "manifest_sha256": record.sha256,
        "verified_sha256": receipt.verified_sha256,
        "manifest_bytes": record.bytes_on_disk,
        "bytes_read": receipt.bytes_read,
        "source_absolute_path": receipt.source_absolute_path,
        "loaded_on_background_thread": receipt.loaded_on_background_thread
    }))
}

fn prepare_fixture(root: &Path, label: &str) -> Result<FixturePaths, Box<dyn std::error::Error>> {
    let fixture_root = root.join(label);
    fs::create_dir_all(&fixture_root)?;
    let paths = FixturePaths {
        root: fixture_root.clone(),
        state_path: fixture_root.join("project-state-aster"),
        project_path: fixture_root.join("ashen-reach.litdworld"),
        package_path: fixture_root.join("ashen-reach.litdgame.json"),
        catalog_path: fixture_root.join("publish-catalog-aster"),
        thumbnail_path: fixture_root.join("thumbnail.png"),
        delivery_manifest_path: fixture_root.join("delivery-manifest-aster"),
        blob_root_path: fixture_root.join("content-store"),
        install_manifest_path: fixture_root.join("install-manifest-aster"),
        install_root_path: fixture_root.join("installed"),
    };
    fs::write(&paths.thumbnail_path, PNG_1X1_RGBA)?;
    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        save_args(
            &paths.state_path,
            &paths.root,
            &paths.project_path,
            fixture_document(label)?,
        ),
    )?;
    call_mcp_project_tool(PUBLISH_LITDGAME_TOOL, publish_args(&paths, label))?;
    Ok(paths)
}

fn fixture_document(label: &str) -> Result<EditorDocument, Box<dyn std::error::Error>> {
    let mut document = EditorDocument::new(
        format!("calyx://world/issue776-{label}"),
        format!("issue776-{label}-rev-1"),
    )?;
    document.upsert_object(EditorObject::new(
        "world",
        "World",
        None,
        Transform::IDENTITY,
    ))?;
    document.upsert_object(
        EditorObject::new(
            "unit_content_store_gate",
            "Unit",
            Some("world".to_string()),
            Transform::from_translation(Vec3::new(3.0, 0.0, -5.0)),
        )
        .with_property("role", "local-content-store-fixture"),
    )?;
    let catalog = AssetVaultCatalog::load_default_cached()?;
    let glb = real_record(&catalog, GLB_RECORD)?;
    assign_vault_model_to_unit(&mut document, "unit_content_store_gate", &glb)?;
    Ok(document)
}

fn edge_remote_source(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let paths = prepare_fixture(root, "edge-remote")?;
    let content_address = published_content_address(&paths.catalog_path)?;
    let sha = content_sha256(&content_address);
    let before = delivery_summary(&paths, &sha);
    let error = call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(
            &paths,
            &content_address,
            Some("https://cdn.example.invalid/game.litdgame"),
        ),
    )
    .unwrap_err()
    .to_string();
    let after = delivery_summary(&paths, &sha);
    Ok(edge_json(before, after, error))
}

fn edge_missing_address(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let paths = prepare_fixture(root, "edge-missing-address")?;
    let missing = format!("litdgame.sha256.{}", "0".repeat(64));
    let before = delivery_summary(&paths, &content_sha256(&missing));
    let error = call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(&paths, &missing, None),
    )
    .unwrap_err()
    .to_string();
    let after = delivery_summary(&paths, &content_sha256(&missing));
    Ok(edge_json(before, after, error))
}

fn edge_partial_blob(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let paths = prepare_fixture(root, "edge-partial-blob")?;
    let content_address = published_content_address(&paths.catalog_path)?;
    let sha = content_sha256(&content_address);
    let blob_path = blob_path(&paths.blob_root_path, &sha);
    fs::create_dir_all(blob_path.parent().ok_or("blob path missing parent")?)?;
    fs::write(&blob_path, b"partial")?;
    let before = delivery_summary(&paths, &sha);
    let error = call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(&paths, &content_address, None),
    )
    .unwrap_err()
    .to_string();
    let after = delivery_summary(&paths, &sha);
    Ok(edge_json(before, after, error))
}

fn edge_hash_mismatch_blob(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let paths = prepare_fixture(root, "edge-hash-mismatch")?;
    let content_address = published_content_address(&paths.catalog_path)?;
    let sha = content_sha256(&content_address);
    let blob_path = blob_path(&paths.blob_root_path, &sha);
    fs::create_dir_all(blob_path.parent().ok_or("blob path missing parent")?)?;
    fs::write(&blob_path, b"same path but wrong object bytes")?;
    let before = delivery_summary(&paths, &sha);
    let error = call_mcp_project_tool(
        DELIVERY_LITDGAME_TOOL,
        delivery_args(&paths, &content_address, None),
    )
    .unwrap_err()
    .to_string();
    let after = delivery_summary(&paths, &sha);
    Ok(edge_json(before, after, error))
}

fn save_args(
    state_path: &Path,
    root: &Path,
    project_path: &Path,
    document: EditorDocument,
) -> Value {
    json!({
        "request_id": "issue776-save",
        "requested_by": "litd-editor-issue776-fsv",
        "state_path": state_path,
        "state_id": "issue776-project-state",
        "ledger_path": root.join("project-ledger-aster"),
        "ledger_id": "issue776-project-ledger",
        "action": ProjectLifecycleAction::SaveSnapshot,
        "project_id": "project.ashen_reach_issue776",
        "title": "Ashen Reach Issue 776",
        "version": "1.0.0",
        "tags": ["local_first", "content_store", "fsv"],
        "project_path": project_path,
        "snapshot_label": "content-store-ready",
        "document": document
    })
}

fn publish_args(paths: &FixturePaths, label: &str) -> Value {
    json!({
        "request_id": format!("issue776-publish-{label}"),
        "requested_by": "litd-editor-issue776-fsv",
        "state_path": paths.state_path,
        "state_id": "issue776-project-state",
        "ledger_path": paths.root.join("publish-ledger-aster"),
        "ledger_id": "issue776-publish-ledger",
        "project_path": paths.project_path,
        "package_path": paths.package_path,
        "catalog_path": paths.catalog_path,
        "share_id": format!("issue776-{label}"),
        "created_by": "agent.local-content-store",
        "host_port": 19776,
        "max_package_bytes": 524_288,
        "title": "Ashen Reach Local Content Store",
        "version": "1.0.0",
        "summary": "Known deterministic local content-store fixture",
        "tags": ["local_first", "content_store"],
        "thumbnail_path": paths.thumbnail_path,
        "min_spec_tags": ["local", "windows", "rtx5090"]
    })
}

fn delivery_args(paths: &FixturePaths, content_address: &str, source_uri: Option<&str>) -> Value {
    let mut value = json!({
        "request_id": format!("issue776-deliver-{content_address}"),
        "requested_by": "litd-editor-issue776-fsv",
        "state_path": paths.state_path,
        "state_id": "issue776-project-state",
        "ledger_path": paths.root.join("delivery-ledger-aster"),
        "ledger_id": "issue776-delivery-ledger",
        "catalog_path": paths.catalog_path,
        "delivery_manifest_path": paths.delivery_manifest_path,
        "delivery_store_id": "issue776-local-store",
        "blob_root_path": paths.blob_root_path,
        "install_manifest_path": paths.install_manifest_path,
        "install_id": "issue776-install",
        "install_root_path": paths.install_root_path,
        "content_address": content_address,
        "delivered_by": "agent.delivery.store",
        "installed_by": "agent.delivery.install"
    });
    if let Some(source_uri) = source_uri {
        value["source_uri"] = json!(source_uri);
    }
    value
}

fn delivery_summary(paths: &FixturePaths, sha256: &str) -> Value {
    let delivery = read_delivery_store_manifest(&paths.delivery_manifest_path).ok();
    let install = read_litdgame_install_manifest(&paths.install_manifest_path).ok();
    let blob_path = blob_path(&paths.blob_root_path, sha256);
    let installed_path = install
        .as_ref()
        .and_then(|readback| readback.manifest.records.first())
        .map(|record| PathBuf::from(&record.installed_package_path))
        .unwrap_or_else(|| paths.install_root_path.join("missing"));
    json!({
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

fn edge_json(before: Value, after: Value, error: String) -> Value {
    json!({
        "before": before,
        "after": after,
        "error": error,
        "failed_closed": before == after
    })
}

fn published_content_address(catalog_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(read_publish_catalog(catalog_path)?.catalog.entries[0]
        .content_address
        .clone())
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

fn real_record(
    catalog: &AssetVaultCatalog,
    relative_path: &str,
) -> Result<AssetVaultRecord, Box<dyn std::error::Error>> {
    catalog
        .records()
        .iter()
        .find(|record| record.relative_path == relative_path)
        .cloned()
        .ok_or_else(|| format!("missing real asset vault record {relative_path}").into())
}

struct FixturePaths {
    root: PathBuf,
    state_path: PathBuf,
    project_path: PathBuf,
    package_path: PathBuf,
    catalog_path: PathBuf,
    thumbnail_path: PathBuf,
    delivery_manifest_path: PathBuf,
    blob_root_path: PathBuf,
    install_manifest_path: PathBuf,
    install_root_path: PathBuf,
}
