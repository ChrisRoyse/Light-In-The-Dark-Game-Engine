use std::path::{Path, PathBuf};

use litd_engine::EngineError;
use litd_hash::sha256_hex;

use crate::quick_share::{LITD_LOCK_IN_POLICY, LITD_RUNTIME_ID};

use super::super::mcp_project_tools_validation::{
    confine_path, validate_path, validate_request_identity, validate_single_line,
};
use super::mcp_project_delivery_types::{
    DELIVERY_STORE_SCHEMA, DeliverLitdGameArgs, DeliveredGameRecord, DeliveryStoreManifest,
    InstalledGameRecord, LITDGAME_INSTALL_MANIFEST_SCHEMA, LITDGAME_PACKAGE_MEDIA_TYPE,
    LitdGameInstallManifest,
};

pub fn validate_delivery_args(args: &DeliverLitdGameArgs) -> Result<(), EngineError> {
    validate_request_identity(&args.request_id, &args.requested_by)?;
    validate_path("state_path", Path::new(&args.state_path))?;
    validate_path("ledger_path", Path::new(&args.ledger_path))?;
    validate_path("catalog_path", Path::new(&args.catalog_path))?;
    validate_path(
        "delivery_manifest_path",
        Path::new(&args.delivery_manifest_path),
    )?;
    validate_path("blob_root_path", Path::new(&args.blob_root_path))?;
    validate_path(
        "install_manifest_path",
        Path::new(&args.install_manifest_path),
    )?;
    validate_path("install_root_path", Path::new(&args.install_root_path))?;
    validate_single_line("delivery_store_id", &args.delivery_store_id)?;
    validate_single_line("install_id", &args.install_id)?;
    validate_single_line("content_address", &args.content_address)?;
    validate_single_line("delivered_by", &args.delivered_by)?;
    validate_single_line("installed_by", &args.installed_by)?;
    if let Some(source_uri) = args
        .source_uri
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return Err(delivery_error(
            "source_uri",
            format!(
                "remote/CDN source_uri {source_uri} is rejected until provider verification exists"
            ),
        ));
    }
    content_address_sha256(&args.content_address)?;
    Ok(())
}

/// Confine every filesystem path the delivery tool will read, write, create, or
/// remove within the trusted project `root` (#1487). `deliver_litdgame` performs
/// `create_dir_all` + `write` + `remove_file` at `blob_root_path`/`install_root_path`
/// and reads at `catalog_path`/`state_path`/manifests — all taken straight from tool
/// arguments. Without this gate a poisoned catalog turns the delivery tool into an
/// arbitrary local write/clobber/read primitive outside the project directory. Runs
/// BEFORE any filesystem effect, so a rejected path leaves the disk untouched.
pub fn confine_delivery_paths(
    args: &DeliverLitdGameArgs,
    root: &std::path::Path,
) -> Result<(), EngineError> {
    confine_path("state_path", Path::new(&args.state_path), root)?;
    confine_path("ledger_path", Path::new(&args.ledger_path), root)?;
    confine_path("catalog_path", Path::new(&args.catalog_path), root)?;
    confine_path(
        "delivery_manifest_path",
        Path::new(&args.delivery_manifest_path),
        root,
    )?;
    confine_path("blob_root_path", Path::new(&args.blob_root_path), root)?;
    confine_path(
        "install_manifest_path",
        Path::new(&args.install_manifest_path),
        root,
    )?;
    confine_path(
        "install_root_path",
        Path::new(&args.install_root_path),
        root,
    )?;
    Ok(())
}

pub fn validate_delivery_store_manifest(
    manifest: &DeliveryStoreManifest,
) -> Result<(), EngineError> {
    if manifest.schema != DELIVERY_STORE_SCHEMA
        || manifest.issue != 1158
        || manifest.store_id.trim().is_empty()
        || manifest.source_of_truth
            != "Calyx Aster delivery manifest + content-addressed .litdgame blob store"
        || manifest.blob_root_path.trim().is_empty()
        || manifest.entry_count != manifest.entries.len()
    {
        return Err(delivery_error(
            "delivery_manifest",
            "schema, identity, source, and count are invalid",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for entry in &manifest.entries {
        validate_delivery_record(entry)?;
        if !seen.insert(entry.content_address.as_str()) {
            return Err(delivery_error(
                "delivery_manifest.entries",
                "duplicate content_address",
            ));
        }
    }
    Ok(())
}

pub fn validate_install_manifest(manifest: &LitdGameInstallManifest) -> Result<(), EngineError> {
    if manifest.schema != LITDGAME_INSTALL_MANIFEST_SCHEMA
        || manifest.issue != 1158
        || manifest.install_id.trim().is_empty()
        || manifest.source_of_truth != "Calyx Aster installed .litdgame package manifest"
        || manifest.install_root_path.trim().is_empty()
        || manifest.record_count != manifest.records.len()
    {
        return Err(delivery_error(
            "install_manifest",
            "schema, identity, source, and count are invalid",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for record in &manifest.records {
        validate_installed_record(record)?;
        if !seen.insert(record.content_address.as_str()) {
            return Err(delivery_error(
                "install_manifest.records",
                "duplicate content_address",
            ));
        }
    }
    Ok(())
}

pub fn validate_delivery_record(record: &DeliveredGameRecord) -> Result<(), EngineError> {
    validate_single_line("delivery.project_id", &record.project_id)?;
    validate_single_line("delivery.package_id", &record.package_id)?;
    validate_single_line("delivery.publish_id", &record.publish_id)?;
    let sha256 = content_address_sha256(&record.content_address)?;
    if record.media_type != LITDGAME_PACKAGE_MEDIA_TYPE
        || record.digest != format!("sha256:{sha256}")
        || record.package_sha256 != sha256
        || record.stored_blob_sha256 != sha256
        || record.package_bytes == 0
        || record.stored_blob_bytes != record.package_bytes
        || record.runtime_id != LITD_RUNTIME_ID
        || record.lock_in_policy != LITD_LOCK_IN_POLICY
    {
        return Err(delivery_error(
            "delivery.record",
            "package digest, bytes, media type, or runtime lock are invalid",
        ));
    }
    if record.store_relative_path != blob_relative_path(&sha256)?.display().to_string() {
        return Err(delivery_error(
            "delivery.store_relative_path",
            "store path must be derived from package digest",
        ));
    }
    validate_single_line("delivery.source_package_path", &record.source_package_path)?;
    validate_single_line("delivery.stored_blob_path", &record.stored_blob_path)?;
    validate_single_line("delivery.delivered_by", &record.delivered_by)
}

pub fn validate_installed_record(record: &InstalledGameRecord) -> Result<(), EngineError> {
    let sha256 = content_address_sha256(&record.content_address)?;
    if record.installed_package_sha256 != sha256
        || record.installed_package_bytes == 0
        || record.runtime_id != LITD_RUNTIME_ID
        || record.lock_in_policy != LITD_LOCK_IN_POLICY
        || !record.world_hash_matches
        || !record.runtime_lock_verified
        // #1513: an installed package must be self-contained — every declared asset byte
        // materialized co-located and verified (true for a zero-asset package).
        || !record.asset_bytes_verified
    {
        return Err(delivery_error(
            "install.record",
            "installed package digest, bytes, runtime readback, or asset-byte verification are invalid",
        ));
    }
    validate_single_line("install.project_id", &record.project_id)?;
    validate_single_line("install.package_id", &record.package_id)?;
    validate_single_line("install.publish_id", &record.publish_id)?;
    validate_single_line("install.source_blob_path", &record.source_blob_path)?;
    validate_single_line(
        "install.installed_package_path",
        &record.installed_package_path,
    )?;
    validate_single_line("install.installed_by", &record.installed_by)
}

pub fn validate_litdgame_package_path(path: &Path) -> Result<(), EngineError> {
    validate_path("package_path", path)?;
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return Err(delivery_error(
            "package_path",
            "package path has no filename",
        ));
    };
    if !(name.ends_with(".litdgame") || name.ends_with(".litdgame.json")) {
        return Err(delivery_error(
            "package_path",
            "package path must end with .litdgame or .litdgame.json",
        ));
    }
    Ok(())
}

pub fn verify_package_bytes(
    bytes: &[u8],
    expected_len: usize,
    expected_sha256: &str,
) -> Result<(), EngineError> {
    if bytes.is_empty() || bytes.len() != expected_len {
        return Err(delivery_error(
            "package.bytes",
            format!("expected {expected_len} bytes, got {}", bytes.len()),
        ));
    }
    let actual = sha256_hex(bytes);
    if actual != expected_sha256 {
        return Err(delivery_error(
            "package.sha256",
            format!("expected {expected_sha256}, got {actual}"),
        ));
    }
    Ok(())
}

pub fn content_address_sha256(content_address: &str) -> Result<String, EngineError> {
    let Some(sha256) = content_address.strip_prefix("litdgame.sha256.") else {
        return Err(delivery_error(
            "content_address",
            "content address must be litdgame.sha256.<64 lowercase hex>",
        ));
    };
    validate_sha256_hex(sha256)?;
    Ok(sha256.to_string())
}

pub fn validate_sha256_hex(value: &str) -> Result<(), EngineError> {
    if !litd_hash::is_sha256_hex(value) {
        return Err(delivery_error(
            "sha256",
            "SHA-256 must be 64 lowercase hex characters",
        ));
    }
    Ok(())
}

pub fn blob_relative_path(sha256: &str) -> Result<PathBuf, EngineError> {
    validate_sha256_hex(sha256)?;
    Ok(PathBuf::from("blobs").join("sha256").join(sha256))
}

pub fn delivery_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    let field = format!("editor.mcp_project_tools.delivery.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        target: "litd_editor::mcp_project_tools::delivery",
        field = %field,
        reason = %reason,
        code = "LITD-DELIVERY-VALIDATION",
        "litdgame delivery rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}
