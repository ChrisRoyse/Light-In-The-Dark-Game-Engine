use litd_hash::sha256_hex;
use std::{fs, path::Path};

use litd_engine::EngineError;

use crate::{
    history::{EditorDocument, stable_fingerprint},
    play::PlaySession,
};

#[path = "quick_share_types.rs"]
mod quick_share_types;

pub use quick_share_types::*;

#[path = "quick_share_lowering.rs"]
mod quick_share_lowering;

#[path = "quick_share_validation.rs"]
mod quick_share_validation;

pub use quick_share_validation::validate_quick_share_ledger;
// #1843: the runtime-datasets attach valve (runtime_datasets_lowering.rs) rebinds the
// envelope fingerprint and re-runs package validation, so it needs these two crate-internal
// helpers.
pub(crate) use quick_share_validation::{
    compute_manifest_fingerprint as compute_package_manifest_fingerprint,
    validate_package as validate_quick_share_package,
};

#[path = "quick_share_store.rs"]
mod quick_share_store;

pub use quick_share_store::{
    QuickShareAsterReadback, parse_quick_share_ledger, read_quick_share_ledger,
    write_quick_share_ledger,
};

/// Hard cap on the size of a `.litdgame` package accepted by the load/readback path
/// (#1487). The write path already bounds output via a caller-supplied
/// `max_package_bytes`; the load path had no equivalent, so an attacker-chosen path
/// to a multi-GB file was an OOM/abort primitive. 64 MiB is far above any legitimate
/// single-world package (test fixtures are a few hundred KiB) while still bounding the
/// allocation.
const MAX_LOADED_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;

use quick_share_lowering::{
    build_source_provenance, collect_verified_assets, lower_document_for_runtime,
};
use quick_share_validation::{
    clean_segment, compute_manifest_fingerprint, invalid_quick_share,
    lowered_play_build_fingerprint, package_bytes, quick_share_error, resolve_validated_endpoint,
    validate_client_readback, validate_package, validate_request, validate_token,
    validate_writeback,
};

pub fn build_quick_share_package(
    document: &EditorDocument,
    request: &QuickShareBuildRequest,
) -> Result<QuickSharePackage, EngineError> {
    validate_request(request)?;
    document.validate()?;
    let assets = collect_verified_assets(document)?;
    let session = PlaySession::start_from_document(document)?;
    let play_build_fingerprint = stable_fingerprint(&session.build)?;
    let source_provenance =
        build_source_provenance(document, session.build.source_fingerprint.clone())?;
    let lowered_document =
        lower_document_for_runtime(document, &session, &assets, &play_build_fingerprint)?;
    let document_fingerprint = stable_fingerprint(&lowered_document)?;
    let package_id = format!("litd.quick_share.{}", clean_segment(&request.share_id));
    let manifest_fingerprint = compute_manifest_fingerprint(
        &request.share_id,
        &request.created_by,
        &request.runtime_id,
        &request.endpoint,
        &document_fingerprint,
        &play_build_fingerprint,
        &assets,
        // The base package carries no runtime datasets; they are attached (and the
        // fingerprint rebound) later via `attach_runtime_datasets` (#1843).
        None,
    )?;
    let package = QuickSharePackage {
        schema: QUICK_SHARE_SCHEMA.to_string(),
        share_id: request.share_id.clone(),
        package_id,
        created_by: request.created_by.clone(),
        runtime_id: request.runtime_id.clone(),
        lock_in_policy: LITD_LOCK_IN_POLICY.to_string(),
        gui_route: QUICK_SHARE_GUI_ROUTE.to_string(),
        tool_name: QUICK_SHARE_TOOL_NAME.to_string(),
        readback_tool_name: QUICK_SHARE_READBACK_TOOL.to_string(),
        endpoint: request.endpoint.clone(),
        host_uri: request.endpoint.uri(),
        document: lowered_document,
        source_provenance,
        document_fingerprint,
        play_build_fingerprint,
        asset_count: assets.len(),
        assets,
        manifest_fingerprint,
        campaign: None,
        runtime: None,
    };
    validate_package(&package)?;
    let bytes = package_bytes(&package)?;
    if bytes.len() > request.max_package_bytes {
        tracing::error!(
            share_id = %request.share_id,
            bytes = bytes.len(),
            max_package_bytes = request.max_package_bytes,
            code = "LITD-QUICK-SHARE-PACKAGE-LIMIT",
            "quick-share package rejected by size limit"
        );
        return invalid_quick_share(
            "package.size",
            format!(
                "package bytes {} exceed limit {}",
                bytes.len(),
                request.max_package_bytes
            ),
        );
    }
    Ok(package)
}

pub fn write_quick_share_package(
    path: &Path,
    package: &QuickSharePackage,
    max_package_bytes: usize,
) -> Result<QuickSharePackageWriteback, EngineError> {
    validate_package(package)?;
    let bytes = package_bytes(package)?;
    if bytes.len() > max_package_bytes {
        return invalid_quick_share(
            "package.write.size",
            format!(
                "package bytes {} exceed limit {max_package_bytes}",
                bytes.len()
            ),
        );
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            quick_share_error(
                "package.mkdir",
                format!("create {}: {error}", parent.display()),
            )
        })?;
    }
    let tmp_path = path.with_extension("litdgame.json.tmp");
    fs::write(&tmp_path, &bytes).map_err(|error| {
        quick_share_error(
            "package.write_tmp",
            format!("write {}: {error}", tmp_path.display()),
        )
    })?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| {
            quick_share_error(
                "package.replace",
                format!("remove {}: {error}", path.display()),
            )
        })?;
    }
    fs::rename(&tmp_path, path).map_err(|error| {
        quick_share_error(
            "package.rename",
            format!(
                "rename {} -> {}: {error}",
                tmp_path.display(),
                path.display()
            ),
        )
    })?;
    let readback = fs::read(path).map_err(|error| {
        quick_share_error(
            "package.readback",
            format!("read {}: {error}", path.display()),
        )
    })?;
    if readback != bytes {
        return invalid_quick_share("package.readback", "package bytes changed after write");
    }
    Ok(QuickSharePackageWriteback {
        package_path: path.display().to_string(),
        bytes: readback.len(),
        sha256: sha256_hex(&readback),
        host_uri: package.host_uri.clone(),
    })
}

pub fn load_quick_share_package(
    path: &Path,
    expected_runtime_id: &str,
    loaded_by: impl Into<String>,
) -> Result<QuickShareClientReadback, EngineError> {
    validate_token("client.expected_runtime_id", expected_runtime_id)?;
    let loaded_by = loaded_by.into();
    validate_token("client.loaded_by", &loaded_by)?;
    // Cap the load/readback path the same way the write path caps output (#1487):
    // paths here can be attacker-chosen, and an unbounded `fs::read` of a multi-GB
    // file is a process-OOM/abort primitive. Check the size via metadata before
    // reading a single byte into memory.
    let metadata = fs::metadata(path).map_err(|error| {
        quick_share_error(
            "package.load.metadata",
            format!("stat {}: {error}", path.display()),
        )
    })?;
    if metadata.len() > MAX_LOADED_PACKAGE_BYTES {
        tracing::error!(
            path = %path.display(),
            bytes = metadata.len(),
            max_loaded_package_bytes = MAX_LOADED_PACKAGE_BYTES,
            code = "LITD-QUICK-SHARE-LOAD-LIMIT",
            "quick-share package load rejected by size limit"
        );
        return invalid_quick_share(
            "package.load.size",
            format!(
                "package file {} is {} bytes, exceeding the load limit {MAX_LOADED_PACKAGE_BYTES}",
                path.display(),
                metadata.len()
            ),
        );
    }
    let bytes = fs::read(path).map_err(|error| {
        quick_share_error("package.load", format!("read {}: {error}", path.display()))
    })?;
    let package = parse_quick_share_package(&bytes)?;
    if package.runtime_id != expected_runtime_id {
        tracing::error!(
            package_id = %package.package_id,
            package_runtime = %package.runtime_id,
            expected_runtime_id,
            code = "LITD-QUICK-SHARE-RUNTIME-LOCK",
            "quick-share package rejected by runtime lock"
        );
        return invalid_quick_share(
            "runtime_lock",
            format!(
                "package runtime {} cannot run on {expected_runtime_id}",
                package.runtime_id
            ),
        );
    }
    let document_fingerprint = stable_fingerprint(&package.document)?;
    let play_build_fingerprint = lowered_play_build_fingerprint(&package.document)?;
    let resolution = resolve_quick_share_endpoint(&package.endpoint)?;
    let world_hash_matches = document_fingerprint == package.document_fingerprint
        && play_build_fingerprint == package.play_build_fingerprint;
    if !world_hash_matches {
        return invalid_quick_share(
            "client.world_hash",
            "loaded client world hash differs from package manifest",
        );
    }
    Ok(QuickShareClientReadback {
        package_id: package.package_id,
        loaded_by,
        runtime_id: expected_runtime_id.to_string(),
        package_sha256: sha256_hex(&bytes),
        document_fingerprint,
        play_build_fingerprint,
        world_hash_matches,
        runtime_lock_verified: true,
        host_resolution: resolution,
    })
}

pub fn parse_quick_share_package(bytes: &[u8]) -> Result<QuickSharePackage, EngineError> {
    let package = serde_json::from_slice::<QuickSharePackage>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse quick-share package: {error}"),
        }
    })?;
    validate_package(&package)?;
    Ok(package)
}

pub fn build_quick_share_ledger(
    ledger_id: impl Into<String>,
    package: &QuickSharePackage,
    writeback: &QuickSharePackageWriteback,
    client: &QuickShareClientReadback,
) -> Result<QuickShareLedger, EngineError> {
    validate_package(package)?;
    validate_writeback(writeback, package)?;
    validate_client_readback(client, package)?;
    let ledger = QuickShareLedger {
        schema: QUICK_SHARE_LEDGER_SCHEMA.to_string(),
        ledger_id: ledger_id.into(),
        share_id: package.share_id.clone(),
        package_id: package.package_id.clone(),
        package_path: writeback.package_path.clone(),
        package_sha256: writeback.sha256.clone(),
        package_bytes: writeback.bytes,
        host_uri: package.host_uri.clone(),
        resolved_host: client.host_resolution.resolved_host.clone(),
        document_fingerprint: package.document_fingerprint.clone(),
        client_document_fingerprint: client.document_fingerprint.clone(),
        play_build_fingerprint: package.play_build_fingerprint.clone(),
        client_play_build_fingerprint: client.play_build_fingerprint.clone(),
        world_hash_matches: client.world_hash_matches,
        runtime_lock_verified: client.runtime_lock_verified,
        asset_count: package.asset_count,
    };
    validate_quick_share_ledger(&ledger)?;
    Ok(ledger)
}

pub fn resolve_quick_share_endpoint(
    endpoint: &QuickShareEndpoint,
) -> Result<QuickShareHostResolution, EngineError> {
    resolve_validated_endpoint(endpoint)
}

#[cfg(test)]
#[path = "quick_share_tests.rs"]
mod tests;
