//! Runtime package validators and lowered-document summarizer (#1466 split of
//! `game_package.rs`).
//!
//! Every fail-loud check the game runner applies to an ecosystem-locked `.litdgame`
//! envelope: runtime-lock enforcement, endpoint/asset/token/fingerprint validation and the
//! document summary. The public types and the load/inspect/build entry points stay in
//! `game_package.rs`; these helpers are `pub(super)` so those entry points call them
//! unchanged.

use std::path::Path;

use serde_json::Value;

use super::{
    LITD_LOCK_IN_POLICY, LITD_LOWERED_DOCUMENT_SCHEMA, LITD_QUICK_SHARE_SCHEMA, LITD_RUNTIME_ID,
    RuntimePackageAssetRef, RuntimePackageDocumentSummary, RuntimePackageEndpoint,
    RuntimePackageEnvelope, RuntimePackageHostMode, compute_runtime_manifest_fingerprint_bound,
};
use crate::EngineError;

pub(super) fn validate_runtime_package_envelope(
    package: &RuntimePackageEnvelope,
    expected_runtime_id: &str,
) -> Result<(), EngineError> {
    if package.runtime_id != expected_runtime_id || package.runtime_id != LITD_RUNTIME_ID {
        tracing::error!(
            package_id = %package.package_id,
            package_runtime = %package.runtime_id,
            expected_runtime_id,
            code = "LITD-GAME-PACKAGE-RUNTIME-LOCK",
            "game package rejected by runtime lock"
        );
        return invalid_game_package(
            "runtime_lock",
            format!(
                "package runtime {} cannot run on {expected_runtime_id}",
                package.runtime_id
            ),
        );
    }
    if package.schema != LITD_QUICK_SHARE_SCHEMA
        || package.lock_in_policy != LITD_LOCK_IN_POLICY
        || package.host_uri != package.endpoint.uri()
        || package.asset_count == 0
        || package.asset_count != package.assets.len()
    {
        return invalid_game_package("envelope", "required runtime package fields are invalid");
    }
    validate_token("share_id", &package.share_id)?;
    validate_token("package_id", &package.package_id)?;
    validate_token("created_by", &package.created_by)?;
    validate_token("document_fingerprint", &package.document_fingerprint)?;
    validate_token("play_build_fingerprint", &package.play_build_fingerprint)?;
    validate_fingerprint("manifest_fingerprint", &package.manifest_fingerprint)?;
    let expected_package_id = format!("litd.quick_share.{}", clean_segment(&package.share_id));
    if package.package_id != expected_package_id {
        return invalid_game_package("package_id", "package id does not match share id");
    }
    validate_endpoint(&package.endpoint)?;
    validate_asset_refs(&package.assets)?;
    let expected_manifest = compute_runtime_manifest_fingerprint_bound(
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
        return invalid_game_package(
            "manifest_fingerprint",
            "manifest fingerprint does not match runtime package envelope",
        );
    }
    Ok(())
}

pub(super) fn validate_endpoint(endpoint: &RuntimePackageEndpoint) -> Result<(), EngineError> {
    validate_token("endpoint.host", &endpoint.host)?;
    validate_token("endpoint.path", &endpoint.path)?;
    if endpoint.port == 0 {
        return invalid_game_package("endpoint.port", "port must be non-zero");
    }
    if !endpoint.path.starts_with("/share/") {
        return invalid_game_package(
            "endpoint.path",
            "runtime package path must start with /share/",
        );
    }
    if matches!(endpoint.mode, RuntimePackageHostMode::Loopback)
        && !matches!(endpoint.host.as_str(), "127.0.0.1" | "localhost" | "::1")
    {
        return invalid_game_package("endpoint.loopback", "loopback host must stay on loopback");
    }
    if matches!(
        endpoint.mode,
        RuntimePackageHostMode::Lan | RuntimePackageHostMode::Host
    ) && matches!(endpoint.host.as_str(), "0.0.0.0" | "::")
    {
        return invalid_game_package("endpoint.host", "wildcard hosts are not playable endpoints");
    }
    Ok(())
}

pub(super) fn validate_asset_refs(assets: &[RuntimePackageAssetRef]) -> Result<(), EngineError> {
    for asset in assets {
        validate_token("asset.product_id", &asset.product_id)?;
        validate_token("asset.source_relative_path", &asset.source_relative_path)?;
        validate_sha256_hex("asset.source_sha256", &asset.source_sha256)?;
        if asset.source_bytes == 0 {
            return invalid_game_package("asset.source_bytes", "asset bytes must be non-zero");
        }
    }
    Ok(())
}

pub(super) fn validate_runtime_document_payload(value: &Value) -> Result<(), EngineError> {
    let Some(document) = value.get("document").and_then(Value::as_object) else {
        return invalid_game_package(
            "document",
            "runtime package must carry lowered document data",
        );
    };
    if document.get("schema").and_then(Value::as_str) != Some(LITD_LOWERED_DOCUMENT_SCHEMA) {
        return invalid_game_package(
            "document.schema",
            format!("runtime package document schema must be {LITD_LOWERED_DOCUMENT_SCHEMA}"),
        );
    }
    Ok(())
}

pub(super) fn summarize_runtime_document(
    value: &Value,
) -> Result<RuntimePackageDocumentSummary, EngineError> {
    validate_runtime_document_payload(value)?;
    let document = value
        .get("document")
        .and_then(Value::as_object)
        .expect("validate_runtime_document_payload accepted only object documents");
    Ok(RuntimePackageDocumentSummary {
        top_level_key_count: document.len(),
        document_schema: string_field(document, "schema"),
        document_id: string_field(document, "calyx_document_id")
            .or_else(|| string_field(document, "document_id")),
        revision_id: string_field(document, "revision_id"),
        lowered_artifact_count: object_field_len(document, "lowered_artifacts"),
        object_count: object_field_len(document, "objects"),
        unit_count: unit_object_count(document),
        region_count: object_field_len(document, "regions"),
        terrain_cell_count: object_field_len(document, "terrain_cells"),
        asset_product_count: object_field_len(document, "asset_products"),
    })
}

pub(super) fn string_field(
    document: &serde_json::Map<String, Value>,
    field: &str,
) -> Option<String> {
    document
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn object_field_len(document: &serde_json::Map<String, Value>, field: &str) -> usize {
    document
        .get(field)
        .and_then(Value::as_object)
        .map_or(0, serde_json::Map::len)
}

pub(super) fn unit_object_count(document: &serde_json::Map<String, Value>) -> usize {
    document
        .get("objects")
        .and_then(Value::as_object)
        .map(|objects| {
            objects
                .values()
                .filter(|object| object.get("kind").and_then(Value::as_str) == Some("Unit"))
                .count()
        })
        .unwrap_or(0)
}

pub(super) fn validate_package_path(path: &Path) -> Result<(), EngineError> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| game_package_error("package_path", "path must include a UTF-8 file name"))?;
    if !(file_name.ends_with(".litdgame") || file_name.ends_with(".litdgame.json")) {
        return invalid_game_package(
            "package_path.extension",
            "package path must end in .litdgame or .litdgame.json",
        );
    }
    Ok(())
}

pub(super) fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_game_package(field, "value must be non-empty and single-line");
    }
    Ok(())
}

pub(super) fn validate_fingerprint(field: &str, value: &str) -> Result<(), EngineError> {
    validate_token(field, value)?;
    if value.len() != 16 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return invalid_game_package(field, "value must be a 16-char stable fingerprint");
    }
    Ok(())
}

pub(super) fn validate_sha256_hex(field: &str, value: &str) -> Result<(), EngineError> {
    validate_token(field, value)?;
    if !litd_hash::is_sha256_hex(value) {
        return invalid_game_package(field, "value must be 64 lowercase hex chars");
    }
    Ok(())
}

pub(super) fn clean_segment(value: &str) -> String {
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

pub(super) fn invalid_game_package<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(game_package_error(field, reason))
}

pub(super) fn game_package_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    let field = format!("litd_game.package.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-GAME-PACKAGE-VALIDATION",
        "game package operation rejected"
    );
    EngineError::GamePackageInvalidConfig { field, reason }
}
