use litd_engine::EngineError;
use serde_json::{Value, json};

use crate::{
    asset_products::AssetProduct, asset_vault::AssetVaultCatalog, history::EditorDocument,
    play::PlaySession,
};

use super::quick_share_types::*;
use super::quick_share_validation::{
    invalid_quick_share, lowered_asset_products, validate_asset_refs,
    validate_lowered_runtime_document, validate_source_provenance,
};

pub(crate) fn build_source_provenance(
    document: &EditorDocument,
    source_document_fingerprint: String,
) -> Result<QuickShareSourceProvenance, EngineError> {
    let provenance = QuickShareSourceProvenance {
        calyx_document_id: document.calyx_document_id.clone(),
        revision_id: document.revision_id.clone(),
        source_document_fingerprint,
    };
    validate_source_provenance(&provenance)?;
    Ok(provenance)
}

pub(crate) fn lower_document_for_runtime(
    document: &EditorDocument,
    session: &PlaySession,
    assets: &[QuickShareAssetRef],
    play_build_fingerprint: &str,
) -> Result<Value, EngineError> {
    let mut objects = serde_json::Map::new();
    for binding in &session.build.spawned_bodies {
        let Some(source_object) = document.objects.get(&binding.object_id) else {
            return invalid_quick_share(
                "lowered_document.objects",
                format!("play build references missing object {}", binding.object_id),
            );
        };
        objects.insert(
            source_object.id.clone(),
            json!({
                "id": source_object.id,
                "kind": source_object.kind,
                "body_id": binding.body_id,
                "transform": source_object.transform,
                "properties": source_object.properties,
            }),
        );
    }
    let source_provenance =
        build_source_provenance(document, session.build.source_fingerprint.clone())?;
    let lowered_assets = lowered_asset_products(assets)?;
    let lowered = json!({
        "schema": LITD_LOWERED_DOCUMENT_SCHEMA,
        "calyx_document_id": document.calyx_document_id,
        "revision_id": document.revision_id,
        "source_provenance": source_provenance,
        "play_build_fingerprint": play_build_fingerprint,
        "lowered_artifacts": {
            "camera_rig": session.build.camera_rig,
            "camera_config": session.build.camera_config,
            "camera_pose": session.build.camera_pose,
            "sim_seed": session.build.sim_seed,
            "spawned_bodies": session.build.spawned_bodies,
            "snapshot": session.build.snapshot,
            "presentation": session.build.presentation,
            "visible_instance_ids": session.build.visible_instance_ids,
            "draw_batches": session.build.draw_batches,
        },
        "objects": objects,
        "regions": document.regions,
        "terrain_cells": document.terrain_cells,
        "asset_products": lowered_assets,
    });
    validate_lowered_runtime_document(&lowered, play_build_fingerprint, assets)?;
    Ok(lowered)
}

pub(crate) fn collect_verified_assets(
    document: &EditorDocument,
) -> Result<Vec<QuickShareAssetRef>, EngineError> {
    if document.asset_products.is_empty() {
        return invalid_quick_share(
            "assets",
            "quick-share package requires at least one asset product",
        );
    }
    let catalog = AssetVaultCatalog::load_default_cached()?;
    let mut refs = Vec::new();
    for product in document.asset_products.values() {
        product.validate()?;
        validate_product_against_vault(product, &catalog)?;
        refs.push(QuickShareAssetRef {
            product_id: product.product_id.clone(),
            source_relative_path: product.source_relative_path.clone(),
            source_sha256: product.source_sha256.clone(),
            source_bytes: product.source_bytes,
        });
    }
    refs.sort_by(|left, right| left.product_id.cmp(&right.product_id));
    validate_asset_refs(&refs)?;
    Ok(refs)
}

fn validate_product_against_vault(
    product: &AssetProduct,
    catalog: &AssetVaultCatalog,
) -> Result<(), EngineError> {
    let Some(record) = catalog
        .records()
        .iter()
        .find(|record| record.relative_path == product.source_relative_path)
    else {
        tracing::error!(
            product_id = %product.product_id,
            source_relative_path = %product.source_relative_path,
            code = "LITD-QUICK-SHARE-STALE-ASSET",
            "quick-share product rejected because asset vault record is missing"
        );
        return invalid_quick_share(
            "assets.vault",
            format!(
                "missing asset vault record {}",
                product.source_relative_path
            ),
        );
    };
    if record.sha256 != product.source_sha256 || record.bytes_on_disk != product.source_bytes {
        tracing::error!(
            product_id = %product.product_id,
            source_relative_path = %product.source_relative_path,
            expected_sha256 = %record.sha256,
            product_sha256 = %product.source_sha256,
            code = "LITD-QUICK-SHARE-STALE-ASSET",
            "quick-share product rejected because asset provenance is stale"
        );
        return invalid_quick_share(
            "assets.stale",
            format!(
                "asset product {} does not match vault manifest",
                product.product_id
            ),
        );
    }
    Ok(())
}
