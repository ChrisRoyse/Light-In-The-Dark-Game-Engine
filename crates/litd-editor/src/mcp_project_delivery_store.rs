use std::{fs, path::Path};

use litd_engine::EngineError;
use litd_engine::game_package::load_runtime_locked_package;
use litd_hash::sha256_hex;

use crate::asset_vault::AssetVaultCatalog;
use crate::quick_share::{
    LITD_LOCK_IN_POLICY, LITD_RUNTIME_ID, load_quick_share_package, parse_quick_share_package,
};

use super::super::mcp_project_aster_store::{
    AsterJsonSpec, read_json_record, read_json_record_optional, write_json_record,
};
use super::super::mcp_project_tools_validation::validate_path;
use super::mcp_project_delivery_types::{
    BlobWriteback, DeliverLitdGameArgs, DeliveredGameRecord, DeliveryStoreManifest,
    DeliveryStoreManifestReadback, InstalledGameRecord, LITDGAME_INSTALL_MANIFEST_SCHEMA,
    LitdGameInstallManifest, LitdGameInstallManifestReadback,
};
use super::mcp_project_delivery_validation::{
    blob_relative_path, delivery_error, validate_delivery_store_manifest,
    validate_install_manifest, validate_installed_record, verify_package_bytes,
};

#[path = "mcp_project_delivery_store_write.rs"]
mod mcp_project_delivery_store_write;
use mcp_project_delivery_store_write::{write_bytes_create_new, write_bytes_replace};

const DELIVERY_STORE_SOURCE_OF_TRUTH: &str =
    "Calyx Aster delivery manifest + content-addressed .litdgame blob store";
const INSTALL_MANIFEST_SOURCE_OF_TRUTH: &str = "Calyx Aster installed .litdgame package manifest";
const DELIVERY_STORE_SPEC: AsterJsonSpec = AsterJsonSpec {
    label: "delivery_store_manifest",
    collection: "litd_project_delivery_store",
    row: b"mcp_project/delivery_store/current",
    column: b"delivery_store_manifest_json",
    format: "litd-delivery-store-manifest-json",
};
const INSTALL_MANIFEST_SPEC: AsterJsonSpec = AsterJsonSpec {
    label: "install_manifest",
    collection: "litd_project_install_manifest",
    row: b"mcp_project/install_manifest/current",
    column: b"litdgame_install_manifest_json",
    format: "litd-install-manifest-json",
};

pub fn read_delivery_store_manifest(
    path: impl AsRef<Path>,
) -> Result<DeliveryStoreManifestReadback, EngineError> {
    let path = path.as_ref();
    validate_path("delivery_manifest_path", path)?;
    let readback = read_json_record(path, &DELIVERY_STORE_SPEC, validate_delivery_store_manifest)?;
    Ok(DeliveryStoreManifestReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        manifest: readback.value,
    })
}

pub fn read_litdgame_install_manifest(
    path: impl AsRef<Path>,
) -> Result<LitdGameInstallManifestReadback, EngineError> {
    let path = path.as_ref();
    validate_path("install_manifest_path", path)?;
    let readback = read_json_record(path, &INSTALL_MANIFEST_SPEC, validate_install_manifest)?;
    Ok(LitdGameInstallManifestReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        manifest: readback.value,
    })
}

pub fn load_or_create_delivery_store_manifest(
    path: impl AsRef<Path>,
    store_id: &str,
    blob_root_path: &str,
) -> Result<DeliveryStoreManifest, EngineError> {
    let path = path.as_ref();
    validate_path("delivery_manifest_path", path)?;
    if let Some(readback) =
        read_json_record_optional(path, &DELIVERY_STORE_SPEC, validate_delivery_store_manifest)?
    {
        let manifest = readback.value;
        if manifest.store_id != store_id || manifest.blob_root_path != blob_root_path {
            return Err(delivery_error(
                "delivery_manifest.identity",
                "delivery store id or blob root mismatch",
            ));
        }
        Ok(manifest)
    } else {
        let manifest = DeliveryStoreManifest {
            schema: super::mcp_project_delivery_types::DELIVERY_STORE_SCHEMA.to_string(),
            issue: 1158,
            store_id: store_id.to_string(),
            source_of_truth: DELIVERY_STORE_SOURCE_OF_TRUTH.to_string(),
            blob_root_path: blob_root_path.to_string(),
            entry_count: 0,
            entries: Vec::new(),
        };
        validate_delivery_store_manifest(&manifest)?;
        Ok(manifest)
    }
}

pub fn load_or_create_install_manifest(
    path: impl AsRef<Path>,
    install_id: &str,
    install_root_path: &str,
) -> Result<LitdGameInstallManifest, EngineError> {
    let path = path.as_ref();
    validate_path("install_manifest_path", path)?;
    if let Some(readback) =
        read_json_record_optional(path, &INSTALL_MANIFEST_SPEC, validate_install_manifest)?
    {
        let manifest = readback.value;
        if manifest.install_id != install_id || manifest.install_root_path != install_root_path {
            return Err(delivery_error(
                "install_manifest.identity",
                "install id or root mismatch",
            ));
        }
        Ok(manifest)
    } else {
        let manifest = LitdGameInstallManifest {
            schema: LITDGAME_INSTALL_MANIFEST_SCHEMA.to_string(),
            issue: 1158,
            install_id: install_id.to_string(),
            source_of_truth: INSTALL_MANIFEST_SOURCE_OF_TRUTH.to_string(),
            install_root_path: install_root_path.to_string(),
            record_count: 0,
            records: Vec::new(),
        };
        validate_install_manifest(&manifest)?;
        Ok(manifest)
    }
}

pub fn write_delivery_store_manifest(
    path: impl AsRef<Path>,
    manifest: &DeliveryStoreManifest,
) -> Result<DeliveryStoreManifestReadback, EngineError> {
    validate_delivery_store_manifest(manifest)?;
    let readback = write_json_record(
        path.as_ref(),
        &DELIVERY_STORE_SPEC,
        manifest,
        validate_delivery_store_manifest,
    )?;
    Ok(DeliveryStoreManifestReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        manifest: readback.value,
    })
}

pub fn write_litdgame_install_manifest(
    path: impl AsRef<Path>,
    manifest: &LitdGameInstallManifest,
) -> Result<LitdGameInstallManifestReadback, EngineError> {
    validate_install_manifest(manifest)?;
    let readback = write_json_record(
        path.as_ref(),
        &INSTALL_MANIFEST_SPEC,
        manifest,
        validate_install_manifest,
    )?;
    Ok(LitdGameInstallManifestReadback {
        path: readback.path,
        bytes: readback.bytes,
        sha256: readback.sha256,
        manifest: readback.value,
    })
}

pub fn write_or_verify_blob(
    root: &Path,
    sha256: &str,
    bytes: &[u8],
) -> Result<BlobWriteback, EngineError> {
    validate_path("blob_root_path", root)?;
    let relative_path = blob_relative_path(sha256)?;
    let path = root.join(&relative_path);
    let deduplicated = if path.exists() {
        let existing = fs::read(&path).map_err(|error| {
            delivery_error(
                "blob.existing.read",
                format!("read {}: {error}", path.display()),
            )
        })?;
        if existing != bytes {
            return Err(delivery_error(
                "blob.existing.bytes",
                format!(
                    "{} exists for sha256={sha256} but bytes differ",
                    path.display()
                ),
            ));
        }
        true
    } else {
        write_bytes_create_new(&path, bytes, "blob")?;
        false
    };
    let readback = fs::read(&path).map_err(|error| {
        delivery_error("blob.readback", format!("read {}: {error}", path.display()))
    })?;
    verify_package_bytes(&readback, bytes.len(), sha256)?;
    Ok(BlobWriteback {
        path: path.display().to_string(),
        relative_path: relative_path.display().to_string(),
        bytes: readback.len(),
        sha256: sha256_hex(&readback),
        deduplicated,
    })
}

/// #1513: bundle each declared asset's real bytes into the content-addressed blob store so
/// an installed package can be materialized self-contained. The bytes are read from the
/// editor asset vault (the content-addressed source of truth), re-verified against the
/// package's declared `source_sha256`/`source_bytes`, and stored keyed by `source_sha256`
/// (dedup). Any missing vault record, provenance mismatch, or byte mismatch fails loud — no
/// fallback that ships an unverifiable package. Returns (asset count, total declared bytes).
pub(crate) fn bundle_delivery_assets(
    blob_root: &Path,
    package_bytes: &[u8],
) -> Result<(usize, usize), EngineError> {
    let package = parse_quick_share_package(package_bytes)?;
    if package.assets.is_empty() {
        return Ok((0, 0));
    }
    let vault = AssetVaultCatalog::load_default_cached()?;
    let mut count = 0_usize;
    let mut total_bytes = 0_usize;
    for asset in &package.assets {
        let record = vault
            .records()
            .iter()
            .find(|record| record.relative_path == asset.source_relative_path)
            .ok_or_else(|| {
                delivery_error(
                    "bundle.asset.vault_record",
                    format!(
                        "declared asset {} ({}) has no asset-vault record to bundle",
                        asset.product_id, asset.source_relative_path
                    ),
                )
            })?;
        if record.sha256 != asset.source_sha256 || record.bytes_on_disk != asset.source_bytes {
            return Err(delivery_error(
                "bundle.asset.provenance",
                format!(
                    "asset {} vault provenance sha256={} bytes={} does not match declared sha256={} bytes={}",
                    asset.product_id,
                    record.sha256,
                    record.bytes_on_disk,
                    asset.source_sha256,
                    asset.source_bytes
                ),
            ));
        }
        let bytes = fs::read(&record.absolute_path).map_err(|error| {
            delivery_error(
                "bundle.asset.read",
                format!("read {}: {error}", record.absolute_path.display()),
            )
        })?;
        if bytes.len() as u64 != asset.source_bytes || sha256_hex(&bytes) != asset.source_sha256 {
            return Err(delivery_error(
                "bundle.asset.bytes",
                format!(
                    "asset {} bytes at {} do not match declared sha256/bytes",
                    asset.product_id,
                    record.absolute_path.display()
                ),
            ));
        }
        write_or_verify_blob(blob_root, &asset.source_sha256, &bytes)?;
        count += 1;
        total_bytes += bytes.len();
    }
    Ok((count, total_bytes))
}

/// #1513: materialize each declared asset's bundled bytes co-located with the installed
/// package (at `source_relative_path` under the install dir), reading from the
/// content-addressed blob store and re-verifying against `source_sha256`/`source_bytes`
/// before and after the atomic write. This makes the installed package self-contained so
/// the runtime's own `verify_package_assets` can byte-verify it and fail loud on tampering.
/// Returns the number of asset payloads materialized.
fn materialize_installed_assets(
    blob_root: &Path,
    install_dir: &Path,
    package_bytes: &[u8],
) -> Result<usize, EngineError> {
    let package = parse_quick_share_package(package_bytes)?;
    let mut count = 0_usize;
    for asset in &package.assets {
        let blob_rel = blob_relative_path(&asset.source_sha256)?;
        let blob_path = blob_root.join(&blob_rel);
        let bytes = fs::read(&blob_path).map_err(|error| {
            delivery_error(
                "install.asset.blob_read",
                format!(
                    "read bundled asset blob {} for {}: {error}",
                    blob_path.display(),
                    asset.product_id
                ),
            )
        })?;
        if bytes.len() as u64 != asset.source_bytes || sha256_hex(&bytes) != asset.source_sha256 {
            return Err(delivery_error(
                "install.asset.blob_bytes",
                format!(
                    "bundled asset blob for {} does not match declared sha256/bytes",
                    asset.product_id
                ),
            ));
        }
        let dest = install_dir.join(&asset.source_relative_path);
        write_bytes_replace(&dest, &bytes, "install_asset")?;
        count += 1;
    }
    Ok(count)
}

pub fn install_litdgame_from_blob(
    args: &DeliverLitdGameArgs,
    delivered: &DeliveredGameRecord,
) -> Result<InstalledGameRecord, EngineError> {
    let install_path = Path::new(&args.install_root_path)
        .join(&delivered.publish_id)
        .join(format!("{}.litdgame.json", delivered.package_id));
    let blob_bytes = fs::read(&delivered.stored_blob_path).map_err(|error| {
        delivery_error(
            "install.source_blob.read",
            format!("read {}: {error}", delivered.stored_blob_path),
        )
    })?;
    verify_package_bytes(
        &blob_bytes,
        delivered.package_bytes,
        &delivered.package_sha256,
    )?;
    write_bytes_replace(&install_path, &blob_bytes, "install_package")?;
    let installed_bytes = fs::read(&install_path).map_err(|error| {
        delivery_error(
            "install.readback",
            format!("read {}: {error}", install_path.display()),
        )
    })?;
    verify_package_bytes(
        &installed_bytes,
        delivered.package_bytes,
        &delivered.package_sha256,
    )?;
    // #1513: materialize the bundled asset bytes co-located with the installed package so it
    // is self-contained, then independently load it through the runtime verifier and REQUIRE
    // every declared asset byte to be present and verified. A package that cannot be made
    // self-contained (a missing/mismatched bundled asset) fails the install loud rather than
    // installing an unverifiable game.
    let install_dir = install_path.parent().ok_or_else(|| {
        delivery_error(
            "install.dir",
            format!("{} has no parent directory", install_path.display()),
        )
    })?;
    let materialized_asset_count = materialize_installed_assets(
        Path::new(&args.blob_root_path),
        install_dir,
        &installed_bytes,
    )?;
    let verified =
        load_runtime_locked_package(&install_path, "mcp.project.delivery.install_asset_verify")?;
    let asset_bytes_verified = verified.assets.is_empty() || verified.asset_bytes_verified;
    if !asset_bytes_verified {
        return Err(delivery_error(
            "install.asset_bytes_verified",
            format!(
                "installed package {} declares {} assets but is not byte-verified after materializing {} — refusing to install an unverifiable game",
                install_path.display(),
                verified.assets.len(),
                materialized_asset_count
            ),
        ));
    }
    let client = load_quick_share_package(
        &install_path,
        LITD_RUNTIME_ID,
        "mcp.project.delivery.install_readback",
    )?;
    let record = InstalledGameRecord {
        project_id: delivered.project_id.clone(),
        package_id: delivered.package_id.clone(),
        publish_id: delivered.publish_id.clone(),
        content_address: delivered.content_address.clone(),
        source_blob_path: delivered.stored_blob_path.clone(),
        installed_package_path: install_path.display().to_string(),
        installed_package_sha256: sha256_hex(&installed_bytes),
        installed_package_bytes: installed_bytes.len(),
        runtime_id: LITD_RUNTIME_ID.to_string(),
        lock_in_policy: LITD_LOCK_IN_POLICY.to_string(),
        world_hash_matches: client.world_hash_matches,
        runtime_lock_verified: client.runtime_lock_verified,
        installed_by: args.installed_by.clone(),
        asset_bytes_verified,
        materialized_asset_count,
    };
    validate_installed_record(&record)?;
    Ok(record)
}
