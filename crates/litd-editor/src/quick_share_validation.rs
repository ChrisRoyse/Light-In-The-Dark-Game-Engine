use litd_engine::game_runtime::validate_runtime_datasets;
use litd_engine::{EngineError, game_package::compute_runtime_manifest_fingerprint_bound};
use serde_json::Value;
use std::collections::BTreeSet;

use crate::history::stable_fingerprint;

use super::quick_share_types::*;

pub(crate) fn validate_request(request: &QuickShareBuildRequest) -> Result<(), EngineError> {
    validate_token("request.share_id", &request.share_id)?;
    validate_token("request.created_by", &request.created_by)?;
    validate_token("request.runtime_id", &request.runtime_id)?;
    validate_endpoint(&request.endpoint)?;
    if request.runtime_id != LITD_RUNTIME_ID {
        return invalid_quick_share(
            "request.runtime_id",
            "quick-share builds must target LitD runtime",
        );
    }
    if request.max_package_bytes == 0 {
        return invalid_quick_share(
            "request.max_package_bytes",
            "max package bytes must be non-zero",
        );
    }
    Ok(())
}

pub(crate) fn resolve_validated_endpoint(
    endpoint: &QuickShareEndpoint,
) -> Result<QuickShareHostResolution, EngineError> {
    validate_endpoint(endpoint)?;
    Ok(QuickShareHostResolution {
        host_uri: endpoint.uri(),
        mode: endpoint.mode,
        resolved_host: match endpoint.mode {
            QuickShareHostMode::Loopback => "127.0.0.1".to_string(),
            QuickShareHostMode::Lan | QuickShareHostMode::Host => endpoint.host.clone(),
        },
        port: endpoint.port,
        path: endpoint.path.clone(),
    })
}

fn validate_endpoint(endpoint: &QuickShareEndpoint) -> Result<(), EngineError> {
    validate_token("endpoint.host", &endpoint.host)?;
    validate_token("endpoint.path", &endpoint.path)?;
    if endpoint.port == 0 {
        return invalid_quick_share("endpoint.port", "port must be non-zero");
    }
    if !endpoint.path.starts_with("/share/") {
        return invalid_quick_share("endpoint.path", "quick-share path must start with /share/");
    }
    match endpoint.mode {
        QuickShareHostMode::Loopback => {
            if !matches!(endpoint.host.as_str(), "127.0.0.1" | "localhost" | "::1") {
                return invalid_quick_share(
                    "endpoint.loopback",
                    "loopback quick-share host must stay on loopback",
                );
            }
        }
        QuickShareHostMode::Lan | QuickShareHostMode::Host => {
            if endpoint.host == "0.0.0.0" || endpoint.host == "::" {
                return invalid_quick_share(
                    "endpoint.host",
                    "wildcard share hosts are not accepted as resolvable client endpoints",
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_package(package: &QuickSharePackage) -> Result<(), EngineError> {
    if package.schema != QUICK_SHARE_SCHEMA
        || package.share_id.trim().is_empty()
        || package.package_id.trim().is_empty()
        || package.created_by.trim().is_empty()
        || package.runtime_id != LITD_RUNTIME_ID
        || package.lock_in_policy != LITD_LOCK_IN_POLICY
        || package.gui_route != QUICK_SHARE_GUI_ROUTE
        || package.tool_name != QUICK_SHARE_TOOL_NAME
        || package.readback_tool_name != QUICK_SHARE_READBACK_TOOL
        || package.host_uri != package.endpoint.uri()
        || package.asset_count != package.assets.len()
        || package.asset_count == 0
        || package.manifest_fingerprint.trim().is_empty()
    {
        return invalid_quick_share("package", "required package fields are invalid");
    }
    let expected_package_id = format!("litd.quick_share.{}", clean_segment(&package.share_id));
    if package.package_id != expected_package_id {
        return invalid_quick_share("package.package_id", "package id does not match share id");
    }
    validate_endpoint(&package.endpoint)?;
    validate_source_provenance(&package.source_provenance)?;
    validate_lowered_runtime_document(
        &package.document,
        &package.play_build_fingerprint,
        &package.assets,
    )?;
    if lowered_source_provenance(&package.document)? != package.source_provenance {
        return invalid_quick_share(
            "package.source_provenance",
            "package source provenance does not match lowered document",
        );
    }
    validate_stable_fingerprint(
        "package.document_fingerprint",
        &package.document_fingerprint,
    )?;
    validate_stable_fingerprint(
        "package.play_build_fingerprint",
        &package.play_build_fingerprint,
    )?;
    validate_stable_fingerprint(
        "package.manifest_fingerprint",
        &package.manifest_fingerprint,
    )?;
    if stable_fingerprint(&package.document)? != package.document_fingerprint {
        return invalid_quick_share(
            "package.document_fingerprint",
            "document fingerprint drifted",
        );
    }
    if lowered_play_build_fingerprint(&package.document)? != package.play_build_fingerprint {
        return invalid_quick_share(
            "package.play_build_fingerprint",
            "play build fingerprint drifted",
        );
    }
    let expected_manifest = compute_manifest_fingerprint(
        &package.share_id,
        &package.created_by,
        &package.runtime_id,
        &package.endpoint,
        &package.document_fingerprint,
        &package.play_build_fingerprint,
        &package.assets,
        package
            .runtime
            .as_ref()
            .map(|runtime| runtime.datasets_fingerprint.as_str()),
    )?;
    if package.manifest_fingerprint != expected_manifest {
        return invalid_quick_share(
            "package.manifest_fingerprint",
            "manifest fingerprint does not match package content",
        );
    }
    validate_asset_refs(&package.assets)?;
    // #1843: a runtime-carrying package re-derives every runtime sub-fingerprint here, the
    // same fail-loud check litd-game runs on load, so a corrupt/sabotaged economy ruleset,
    // match blueprint, or runtime constant never reaches disk.
    if let Some(runtime) = &package.runtime {
        validate_runtime_datasets(runtime)?;
    }
    Ok(())
}

pub fn validate_quick_share_ledger(ledger: &QuickShareLedger) -> Result<(), EngineError> {
    if ledger.schema != QUICK_SHARE_LEDGER_SCHEMA
        || !crate::row_safe_ledger_id::is_row_safe_ledger_id(&ledger.ledger_id)
        || ledger.share_id.trim().is_empty()
        || ledger.package_id.trim().is_empty()
        || ledger.package_path.trim().is_empty()
        || ledger.package_sha256.trim().is_empty()
        || ledger.host_uri.trim().is_empty()
        || ledger.resolved_host.trim().is_empty()
        || ledger.asset_count == 0
        || ledger.package_bytes == 0
    {
        return invalid_quick_share("ledger", "required ledger fields are missing");
    }
    validate_sha256_hex("ledger.package_sha256", &ledger.package_sha256)?;
    if ledger.document_fingerprint != ledger.client_document_fingerprint
        || ledger.play_build_fingerprint != ledger.client_play_build_fingerprint
        || !ledger.world_hash_matches
        || !ledger.runtime_lock_verified
    {
        return invalid_quick_share(
            "ledger.readback",
            "client readback must match package world hashes and runtime lock",
        );
    }
    Ok(())
}

pub(crate) fn validate_source_provenance(
    provenance: &QuickShareSourceProvenance,
) -> Result<(), EngineError> {
    validate_token(
        "source_provenance.calyx_document_id",
        &provenance.calyx_document_id,
    )?;
    validate_token("source_provenance.revision_id", &provenance.revision_id)?;
    validate_stable_fingerprint(
        "source_provenance.source_document_fingerprint",
        &provenance.source_document_fingerprint,
    )
}

pub(crate) fn validate_lowered_runtime_document(
    document: &Value,
    expected_play_build_fingerprint: &str,
    assets: &[QuickShareAssetRef],
) -> Result<(), EngineError> {
    let Some(document_object) = document.as_object() else {
        return invalid_quick_share(
            "document",
            "quick-share package document must be a lowered runtime object",
        );
    };
    if document_object.get("schema").and_then(Value::as_str) != Some(LITD_LOWERED_DOCUMENT_SCHEMA) {
        return invalid_quick_share(
            "document.schema",
            format!("quick-share package document must use {LITD_LOWERED_DOCUMENT_SCHEMA}"),
        );
    }
    if lowered_play_build_fingerprint(document)? != expected_play_build_fingerprint {
        return invalid_quick_share(
            "document.play_build_fingerprint",
            "lowered document play build fingerprint does not match package",
        );
    }
    let Some(provenance) = document_object
        .get("source_provenance")
        .and_then(Value::as_object)
    else {
        return invalid_quick_share(
            "document.source_provenance",
            "lowered document must preserve source provenance",
        );
    };
    validate_token(
        "document.source_provenance.calyx_document_id",
        provenance
            .get("calyx_document_id")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?;
    validate_token(
        "document.source_provenance.revision_id",
        provenance
            .get("revision_id")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?;
    validate_stable_fingerprint(
        "document.source_provenance.source_document_fingerprint",
        provenance
            .get("source_document_fingerprint")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?;
    if document_object
        .get("lowered_artifacts")
        .and_then(Value::as_object)
        .is_none_or(serde_json::Map::is_empty)
    {
        return invalid_quick_share(
            "document.lowered_artifacts",
            "lowered document must carry runtime artifacts",
        );
    }
    if document_object
        .get("objects")
        .and_then(Value::as_object)
        .is_none_or(serde_json::Map::is_empty)
    {
        return invalid_quick_share(
            "document.objects",
            "lowered document must carry runtime objects",
        );
    }
    let expected_assets = lowered_asset_products(assets)?;
    if document_object.get("asset_products") != Some(&expected_assets) {
        return invalid_quick_share(
            "document.asset_products",
            "lowered document asset refs do not match package asset manifest",
        );
    }
    Ok(())
}

pub(crate) fn lowered_asset_products(assets: &[QuickShareAssetRef]) -> Result<Value, EngineError> {
    let mut map = serde_json::Map::new();
    for asset in assets {
        if map.contains_key(&asset.product_id) {
            return invalid_quick_share("document.asset_products", "duplicate asset product id");
        }
        map.insert(
            asset.product_id.clone(),
            serde_json::to_value(asset).map_err(|error| {
                quick_share_error(
                    "document.asset_products",
                    format!("serialize asset ref: {error}"),
                )
            })?,
        );
    }
    Ok(Value::Object(map))
}

pub(crate) fn lowered_play_build_fingerprint(document: &Value) -> Result<String, EngineError> {
    let Some(value) = document
        .get("play_build_fingerprint")
        .and_then(Value::as_str)
    else {
        return invalid_quick_share(
            "document.play_build_fingerprint",
            "lowered document is missing play build fingerprint",
        );
    };
    validate_stable_fingerprint("document.play_build_fingerprint", value)?;
    Ok(value.to_string())
}

fn lowered_source_provenance(document: &Value) -> Result<QuickShareSourceProvenance, EngineError> {
    let value = document.get("source_provenance").cloned().ok_or_else(|| {
        quick_share_error(
            "document.source_provenance",
            "lowered document is missing source provenance",
        )
    })?;
    let provenance: QuickShareSourceProvenance =
        serde_json::from_value(value).map_err(|error| {
            quick_share_error(
                "document.source_provenance",
                format!("parse source provenance: {error}"),
            )
        })?;
    validate_source_provenance(&provenance)?;
    Ok(provenance)
}

pub(crate) fn validate_writeback(
    writeback: &QuickSharePackageWriteback,
    package: &QuickSharePackage,
) -> Result<(), EngineError> {
    validate_token("writeback.package_path", &writeback.package_path)?;
    validate_sha256_hex("writeback.sha256", &writeback.sha256)?;
    if writeback.bytes == 0 || writeback.host_uri != package.host_uri {
        return invalid_quick_share("writeback", "writeback bytes and host uri must be valid");
    }
    Ok(())
}

pub(crate) fn validate_client_readback(
    client: &QuickShareClientReadback,
    package: &QuickSharePackage,
) -> Result<(), EngineError> {
    validate_token("client.loaded_by", &client.loaded_by)?;
    validate_sha256_hex("client.package_sha256", &client.package_sha256)?;
    if client.package_id != package.package_id
        || client.runtime_id != LITD_RUNTIME_ID
        || client.document_fingerprint != package.document_fingerprint
        || client.play_build_fingerprint != package.play_build_fingerprint
        || !client.world_hash_matches
        || !client.runtime_lock_verified
    {
        return invalid_quick_share("client", "client readback does not match package");
    }
    Ok(())
}

pub(crate) fn validate_asset_refs(assets: &[QuickShareAssetRef]) -> Result<(), EngineError> {
    if assets.is_empty() {
        return invalid_quick_share("assets", "asset manifest must not be empty");
    }
    let mut seen = BTreeSet::new();
    for asset in assets {
        validate_token("asset.product_id", &asset.product_id)?;
        validate_token("asset.source_relative_path", &asset.source_relative_path)?;
        validate_token("asset.source_sha256", &asset.source_sha256)?;
        if asset.source_bytes == 0 {
            return invalid_quick_share("asset.source_bytes", "asset bytes must be non-zero");
        }
        validate_sha256_hex("asset.source_sha256", &asset.source_sha256)?;
        if !seen.insert(asset.product_id.clone()) {
            return invalid_quick_share("assets.duplicate", "duplicate asset product id");
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_manifest_fingerprint(
    share_id: &str,
    created_by: &str,
    runtime_id: &str,
    endpoint: &QuickShareEndpoint,
    document_fingerprint: &str,
    play_build_fingerprint: &str,
    assets: &[QuickShareAssetRef],
    runtime_datasets_fingerprint: Option<&str>,
) -> Result<String, EngineError> {
    compute_runtime_manifest_fingerprint_bound(
        share_id,
        created_by,
        runtime_id,
        endpoint,
        document_fingerprint,
        play_build_fingerprint,
        assets,
        runtime_datasets_fingerprint,
    )
}

pub(crate) fn package_bytes(package: &QuickSharePackage) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(package).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize quick-share package: {error}"),
    })
}

pub(crate) fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_quick_share(field, "value must be non-empty and single-line");
    }
    Ok(())
}

fn validate_sha256_hex(field: &str, value: &str) -> Result<(), EngineError> {
    validate_token(field, value)?;
    if !litd_hash::is_sha256_hex(value) {
        return invalid_quick_share(field, "value must be 64 lowercase hex chars");
    }
    Ok(())
}

fn validate_stable_fingerprint(field: &str, value: &str) -> Result<(), EngineError> {
    validate_token(field, value)?;
    if value.len() != 16 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return invalid_quick_share(field, "value must be a 16-char stable fingerprint");
    }
    Ok(())
}

pub(crate) fn clean_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

pub(crate) fn invalid_quick_share<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(quick_share_error(field, reason))
}

pub(crate) fn quick_share_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    let field = format!("editor.quick_share.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-QUICK-SHARE-VALIDATION",
        "quick-share operation rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}
