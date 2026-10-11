use litd_hash::sha256_hex;
use std::fs;

use litd_editor::{
    asset_products::{MODEL_SOURCE_SHA256_PROPERTY, assign_vault_model_to_unit},
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    quick_share::{
        LITD_RUNTIME_ID, QuickShareBuildRequest, QuickShareEndpoint, build_quick_share_ledger,
        build_quick_share_package, load_quick_share_package, parse_quick_share_ledger,
        read_quick_share_ledger, write_quick_share_ledger, write_quick_share_package,
    },
};
use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE1187_FSV_ROOT",
        "issue1187_quick_share",
        "manual",
    )?;
    let aster_dir = root.join("aster");
    let package_path = root.join("ember-quick-share.litdgame.json");
    let ledger_path = root.join("quick-share-ledger.json");
    let artifact_path = root.join("issue1187-quick-share-readback.json");
    let document = fixture_document()?;
    let request = request("issue1187-manual-share", 128 * 1024);

    let package = build_quick_share_package(&document, &request)?;
    println!(
        "happy_path before_write package_id={} doc_hash={} play_hash={} assets={} host={}",
        package.package_id,
        package.document_fingerprint,
        package.play_build_fingerprint,
        package.asset_count,
        package.host_uri
    );
    let writeback = write_quick_share_package(&package_path, &package, request.max_package_bytes)?;
    let client = load_quick_share_package(&package_path, LITD_RUNTIME_ID, "human.client.two")?;
    let ledger =
        build_quick_share_ledger("issue1187-manual-ledger", &package, &writeback, &client)?;
    let ledger_writeback = write_quick_share_ledger(&ledger_path, &aster_dir, &ledger)?;
    let ledger_readback = read_quick_share_ledger(&aster_dir, &ledger.ledger_id)?
        .ok_or("missing quick-share ledger in Aster")?;
    let parsed = parse_quick_share_ledger(&ledger_readback.bytes)?;
    let package_disk = fs::read(&package_path)?;
    let ledger_disk = fs::read(&ledger_path)?;
    if package_disk.len() != writeback.bytes || sha256_hex(&package_disk) != writeback.sha256 {
        return Err(format!("package readback mismatch at {}", package_path.display()).into());
    }
    if ledger_disk != ledger_readback.bytes {
        return Err(format!("ledger state/Aster mismatch at {}", ledger_path.display()).into());
    }
    println!(
        "happy_path after_aster share={} package_bytes={} package_sha={} seq={} graph_rows={} ledger_sha={} host_resolved={}",
        parsed.share_id,
        parsed.package_bytes,
        parsed.package_sha256,
        ledger_readback.seq,
        ledger_readback.graph_rows,
        ledger_readback.sha256,
        parsed.resolved_host
    );

    let edge_stale = edge_stale_asset(&root)?;
    let edge_limit = edge_package_limit(&document, &root)?;
    let edge_runtime = edge_runtime_lock(&document, &root)?;

    let artifact = json!({
        "issue": 1187,
        "source_of_truth": "Package file bytes plus Aster Graph CF quick-share ledger rows and second-client load readback",
        "artifact_path": artifact_path,
        "happy_path": {
            "package": {
                "path": package_path,
                "package_id": package.package_id,
                "share_id": parsed.share_id,
                "bytes": writeback.bytes,
                "sha256": writeback.sha256,
                "file_sha256": sha256_hex(&package_disk),
                "file_matches_writeback": package_disk.len() == writeback.bytes && sha256_hex(&package_disk) == writeback.sha256,
                "runtime_id": package.runtime_id,
                "lock_in_policy": package.lock_in_policy,
                "asset_count": package.asset_count
            },
            "second_client": {
                "loaded_by": client.loaded_by,
                "runtime_id": client.runtime_id,
                "package_sha256": client.package_sha256,
                "document_fingerprint": client.document_fingerprint,
                "play_build_fingerprint": client.play_build_fingerprint,
                "world_hash_matches": client.world_hash_matches,
                "runtime_lock_verified": client.runtime_lock_verified,
                "host_resolution": client.host_resolution
            },
            "ledger": {
                "path": ledger_path,
                "state_file_matches_aster": ledger_disk == ledger_readback.bytes,
                "state_sha256": sha256_hex(&ledger_disk),
                "aster_sha256": ledger_readback.sha256,
                "aster_seq": ledger_readback.seq,
                "aster_graph_rows": ledger_readback.graph_rows,
                "cell_key": ledger_writeback.cell_key,
                "index_key": ledger_writeback.index_key,
                "document_fingerprint": parsed.document_fingerprint,
                "client_document_fingerprint": parsed.client_document_fingerprint,
                "play_build_fingerprint": parsed.play_build_fingerprint,
                "client_play_build_fingerprint": parsed.client_play_build_fingerprint,
                "world_hash_matches": parsed.world_hash_matches,
                "runtime_lock_verified": parsed.runtime_lock_verified
            }
        },
        "edge_cases": [edge_stale, edge_limit, edge_runtime]
    });
    let artifact_bytes = serde_json::to_vec_pretty(&artifact)?;
    fs::write(&artifact_path, &artifact_bytes)?;
    let artifact_readback = fs::read(&artifact_path)?;
    if artifact_readback != artifact_bytes {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    println!(
        "wrote issue1187 quick-share FSV artifact={} bytes={} artifact_sha={} package_sha={} ledger_sha={}",
        artifact_path.display(),
        artifact_readback.len(),
        report_readback.sha256,
        writeback.sha256,
        ledger_readback.sha256
    );
    Ok(())
}

fn edge_stale_asset(root: &std::path::Path) -> Result<serde_json::Value, EngineError> {
    let mut document = fixture_document()?;
    let package_path = root.join("edge-stale.litdgame.json");
    let fake_sha = "0".repeat(64);
    let first_product_id = document
        .asset_products
        .keys()
        .next()
        .cloned()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.quick_share.fsv.asset".to_string(),
            reason: "missing asset product for stale edge".to_string(),
        })?;
    let before_asset_count = document.asset_products.len();
    document
        .asset_products
        .get_mut(&first_product_id)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.quick_share.fsv.asset".to_string(),
            reason: format!("missing product {first_product_id}"),
        })?
        .source_sha256 = fake_sha.clone();
    document
        .objects
        .get_mut("unit_playtest_hero")
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.quick_share.fsv.object".to_string(),
            reason: "missing unit_playtest_hero".to_string(),
        })?
        .properties
        .insert(MODEL_SOURCE_SHA256_PROPERTY.to_string(), fake_sha);
    let before_file_exists = package_path.exists();
    let error = build_quick_share_package(&document, &request("issue1187-stale", 128 * 1024))
        .unwrap_err()
        .to_string();
    println!(
        "edge_stale before_assets={} before_file_exists={} after_file_exists={} error={}",
        before_asset_count,
        before_file_exists,
        package_path.exists(),
        error
    );
    Ok(json!({
        "name": "stale_asset_rejected",
        "before_asset_count": before_asset_count,
        "before_file_exists": before_file_exists,
        "after_file_exists": package_path.exists(),
        "state_unchanged": package_path.exists() == before_file_exists,
        "error": error
    }))
}

fn edge_package_limit(
    document: &EditorDocument,
    root: &std::path::Path,
) -> Result<serde_json::Value, EngineError> {
    let package_path = root.join("edge-large.litdgame.json");
    let before_file_exists = package_path.exists();
    let limit = 256;
    let error = build_quick_share_package(document, &request("issue1187-large", limit))
        .unwrap_err()
        .to_string();
    println!(
        "edge_limit before_file_exists={} after_file_exists={} limit={} error={}",
        before_file_exists,
        package_path.exists(),
        limit,
        error
    );
    Ok(json!({
        "name": "package_limit_rejected",
        "before_file_exists": before_file_exists,
        "after_file_exists": package_path.exists(),
        "state_unchanged": package_path.exists() == before_file_exists,
        "max_package_bytes": limit,
        "error": error
    }))
}

fn edge_runtime_lock(
    document: &EditorDocument,
    root: &std::path::Path,
) -> Result<serde_json::Value, EngineError> {
    let package_path = root.join("edge-runtime-lock.litdgame.json");
    let request = request("issue1187-runtime-lock", 128 * 1024);
    let package = build_quick_share_package(document, &request)?;
    let before_file_exists = package_path.exists();
    let writeback = write_quick_share_package(&package_path, &package, request.max_package_bytes)?;
    let error = load_quick_share_package(&package_path, "foreign.runtime.v1", "human.client.two")
        .unwrap_err()
        .to_string();
    println!(
        "edge_runtime before_file_exists={} after_file_exists={} package_sha={} error={}",
        before_file_exists,
        package_path.exists(),
        writeback.sha256,
        error
    );
    Ok(json!({
        "name": "wrong_runtime_rejected",
        "before_file_exists": before_file_exists,
        "after_file_exists": package_path.exists(),
        "package_sha256": writeback.sha256,
        "package_preserved": package_path.exists(),
        "error": error
    }))
}

fn request(share_id: &str, max_package_bytes: usize) -> QuickShareBuildRequest {
    QuickShareBuildRequest::new(
        share_id,
        "human.qa.ember",
        QuickShareEndpoint::loopback(share_id, 11870),
        max_package_bytes,
    )
}

fn fixture_document() -> Result<EditorDocument, EngineError> {
    let mut document =
        EditorDocument::new("calyx://world/issue1187-quick-share", "issue1187-rev-1")?;
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
    let record = AssetVaultCatalog::load_default_cached()?
        .records()
        .iter()
        .find(|record| {
            record.relative_path
                == "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf"
        })
        .cloned()
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.quick_share.fsv.asset".to_string(),
            reason: "missing real Quaternius Rock1 asset record".to_string(),
        })?;
    assign_vault_model_to_unit(&mut document, "unit_playtest_hero", &record)?;
    Ok(document)
}
