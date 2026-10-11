//! Runtime package validation for ecosystem-locked `.litdgame` artifacts.
//!
//! The editor can author packages, but the game runner must independently
//! reject anything that is not bound to the LitD runtime and manifest.

use litd_hash::sha256_hex;
use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::EngineError;
use crate::game_runtime::datasets::{RuntimeDatasetsManifest, validate_runtime_datasets};

#[path = "game_package_support.rs"]
mod game_package_support;
use game_package_support::{
    game_package_error, invalid_game_package, summarize_runtime_document, validate_package_path,
    validate_runtime_package_envelope, validate_token,
};

#[path = "game_campaign.rs"]
mod game_campaign;
pub use game_campaign::{
    CAMPAIGN_WINNER_CAMPAIGN_SHARED, CAMPAIGN_WINNER_MAP_LOCAL, LITD_CAMPAIGN_MANIFEST_SCHEMA,
    RuntimeCampaignBranch, RuntimeCampaignChapter, RuntimeCampaignHeroSlot,
    RuntimeCampaignManifest, RuntimeCampaignSharedDatum, RuntimeCampaignSharedResolution,
    RuntimeSharedObjectCategory, validate_campaign_manifest,
};

pub const LITD_RUNTIME_ID: &str = "litd.engine.runtime.v1";
pub const LITD_LOCK_IN_POLICY: &str = "litd-runtime-only";
pub const LITD_QUICK_SHARE_SCHEMA: &str = "litd.quick_share.playtest_package.v1";
pub const LITD_GAME_READBACK_SCHEMA: &str = "litd.game.runtime_package.readback.v1";
pub const LITD_LOWERED_DOCUMENT_SCHEMA: &str = "litd.lowered_document.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePackageHostMode {
    Loopback,
    Lan,
    Host,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimePackageEndpoint {
    pub mode: RuntimePackageHostMode,
    pub host: String,
    pub port: u16,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimePackageAssetRef {
    pub product_id: String,
    pub source_relative_path: String,
    pub source_sha256: String,
    pub source_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimePackageDocumentSummary {
    pub top_level_key_count: usize,
    pub document_schema: Option<String>,
    pub document_id: Option<String>,
    pub revision_id: Option<String>,
    pub lowered_artifact_count: usize,
    pub object_count: usize,
    pub unit_count: usize,
    pub region_count: usize,
    pub terrain_cell_count: usize,
    pub asset_product_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimePackageReadback {
    pub schema: String,
    pub package_path: String,
    pub package_id: String,
    pub share_id: String,
    pub loaded_by: String,
    pub runtime_id: String,
    pub lock_in_policy: String,
    pub package_sha256: String,
    pub package_bytes: usize,
    pub host_uri: String,
    pub resolved_host: String,
    pub document_fingerprint: String,
    pub play_build_fingerprint: String,
    pub manifest_fingerprint: String,
    pub document: Value,
    pub document_summary: RuntimePackageDocumentSummary,
    pub asset_count: usize,
    pub assets: Vec<RuntimePackageAssetRef>,
    /// #853: present only for single-player campaign packages. When set, the game
    /// runner validated the full multi-map campaign manifest (chapter refs, embedded
    /// map fingerprints, flow edges, shared data, hero slots) at package validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub campaign: Option<RuntimeCampaignManifest>,
    /// #1843: present when the package carries the lowered runtime datasets (economy
    /// ruleset + match blueprint + runtime constants) the game host instantiates the
    /// Wave-4 systems from. Validated fail-loud at package validation, never at play.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeDatasetsManifest>,
    /// Manifest STRUCTURE + envelope fingerprint verified. This does NOT imply
    /// asset-byte integrity — see `asset_bytes_verified` (#1464).
    pub manifest_integrity_verified: bool,
    pub runtime_lock_verified: bool,
    pub playable_on_litd_runtime: bool,
    /// True only when every declared asset was present next to the package and
    /// its bytes hashed to the declared `source_sha256` (#1464). False when the
    /// asset bytes are not co-located with the package (e.g. a manifest-only
    /// bytes inspection, or an installed package that ships no asset bytes — see
    /// the follow-up architecture issue for bundling). A present-but-mismatched
    /// asset fails the load loudly rather than setting this false.
    pub asset_bytes_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeGameRunReport {
    pub schema: String,
    pub runtime_id: String,
    pub package_path: String,
    pub package_sha256: String,
    pub package_bytes: usize,
    pub package_id: String,
    pub share_id: String,
    pub document_fingerprint: String,
    pub play_build_fingerprint: String,
    pub document_summary: RuntimePackageDocumentSummary,
    pub manifest_integrity_verified: bool,
    pub runtime_lock_verified: bool,
    pub playable_on_litd_runtime: bool,
    pub asset_bytes_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RuntimePackageEnvelope {
    schema: String,
    share_id: String,
    package_id: String,
    created_by: String,
    runtime_id: String,
    lock_in_policy: String,
    endpoint: RuntimePackageEndpoint,
    host_uri: String,
    document: Value,
    document_fingerprint: String,
    play_build_fingerprint: String,
    asset_count: usize,
    assets: Vec<RuntimePackageAssetRef>,
    manifest_fingerprint: String,
    #[serde(default)]
    campaign: Option<RuntimeCampaignManifest>,
    #[serde(default)]
    runtime: Option<RuntimeDatasetsManifest>,
}

impl RuntimePackageEndpoint {
    pub fn uri(&self) -> String {
        format!("litd-share://{}:{}{}", self.host, self.port, self.path)
    }

    pub fn resolved_host(&self) -> String {
        match self.mode {
            RuntimePackageHostMode::Loopback => "127.0.0.1".to_string(),
            RuntimePackageHostMode::Lan | RuntimePackageHostMode::Host => self.host.clone(),
        }
    }
}

pub fn load_runtime_locked_package(
    path: &Path,
    loaded_by: impl Into<String>,
) -> Result<RuntimePackageReadback, EngineError> {
    validate_package_path(path)?;
    let loaded_by = loaded_by.into();
    validate_token("loaded_by", &loaded_by)?;
    let bytes = fs::read(path).map_err(|error| {
        game_package_error("package.read", format!("read {}: {error}", path.display()))
    })?;
    let mut readback =
        inspect_runtime_locked_package_bytes(path, &bytes, LITD_RUNTIME_ID, loaded_by)?;
    // #1464: verify asset bytes against their declared source_sha256 when they
    // are co-located with the package. A present-but-mismatched asset (corrupted
    // or substituted) fails the load loudly; an asset that ships no bytes here
    // leaves asset_bytes_verified false rather than silently claiming integrity.
    let package_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let verified = verify_package_assets(package_dir, &readback.assets)?;
    readback.asset_bytes_verified =
        !readback.assets.is_empty() && verified == readback.assets.len();
    Ok(readback)
}

/// Hash each co-located asset file against its declared `source_sha256` and byte
/// count. Returns how many declared assets were present AND verified. A missing
/// asset file is skipped (its bytes are not co-located with the package — the
/// current install ships a manifest only); a present asset whose bytes do not
/// match is a corrupted/substituted asset and fails loud (#1464).
fn verify_package_assets(
    package_dir: &Path,
    assets: &[RuntimePackageAssetRef],
) -> Result<usize, EngineError> {
    let mut verified = 0_usize;
    for asset in assets {
        let asset_path = package_dir.join(&asset.source_relative_path);
        let bytes = match fs::read(&asset_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(game_package_error(
                    "asset.read",
                    format!("read {}: {error}", asset_path.display()),
                ));
            }
        };
        if bytes.len() as u64 != asset.source_bytes {
            return Err(game_package_error(
                "asset.source_bytes",
                format!(
                    "{} byte length {} does not match declared {}",
                    asset.product_id,
                    bytes.len(),
                    asset.source_bytes
                ),
            ));
        }
        let actual = sha256_hex(&bytes);
        if actual != asset.source_sha256 {
            return Err(game_package_error(
                "asset.source_sha256",
                format!(
                    "{} content hash {} does not match declared {} (corrupted or substituted asset)",
                    asset.product_id, actual, asset.source_sha256
                ),
            ));
        }
        verified += 1;
    }
    Ok(verified)
}

pub fn inspect_runtime_locked_package_bytes(
    path: &Path,
    bytes: &[u8],
    expected_runtime_id: &str,
    loaded_by: impl Into<String>,
) -> Result<RuntimePackageReadback, EngineError> {
    validate_package_path(path)?;
    validate_token("expected_runtime_id", expected_runtime_id)?;
    let loaded_by = loaded_by.into();
    validate_token("loaded_by", &loaded_by)?;
    if bytes.is_empty() {
        return invalid_game_package("package.bytes", "package file is empty");
    }
    let value = serde_json::from_slice::<Value>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse runtime package: {error}"),
        }
    })?;
    let document_summary = summarize_runtime_document(&value)?;
    let package = serde_json::from_value::<RuntimePackageEnvelope>(value).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse runtime package envelope: {error}"),
        }
    })?;
    validate_runtime_package_envelope(&package, expected_runtime_id)?;
    // #853: a campaign package fails LOUD at package validation for any invalid or
    // missing chapter map reference, tampered embedded map, or corrupted campaign
    // manifest fingerprint — never at play time.
    if let Some(campaign) = &package.campaign {
        validate_campaign_manifest(campaign, &package.document, &package.document_fingerprint)?;
    }
    // #1843: a runtime-datasets package fails LOUD here for any corrupt/sabotaged
    // economy ruleset, match blueprint, or runtime constant -- every sub-fingerprint
    // is re-derived, so a tampered value is rejected before the host instantiates it.
    if let Some(runtime) = &package.runtime {
        validate_runtime_datasets(runtime)?;
    }
    Ok(RuntimePackageReadback {
        schema: LITD_GAME_READBACK_SCHEMA.to_string(),
        package_path: path.display().to_string(),
        package_id: package.package_id,
        share_id: package.share_id,
        loaded_by,
        runtime_id: expected_runtime_id.to_string(),
        lock_in_policy: LITD_LOCK_IN_POLICY.to_string(),
        package_sha256: sha256_hex(bytes),
        package_bytes: bytes.len(),
        host_uri: package.endpoint.uri(),
        resolved_host: package.endpoint.resolved_host(),
        document_fingerprint: package.document_fingerprint,
        play_build_fingerprint: package.play_build_fingerprint,
        manifest_fingerprint: package.manifest_fingerprint,
        document: package.document,
        document_summary,
        asset_count: package.asset_count,
        assets: package.assets,
        campaign: package.campaign,
        runtime: package.runtime,
        manifest_integrity_verified: true,
        runtime_lock_verified: true,
        playable_on_litd_runtime: true,
        // Byte-only inspection has no filesystem access to the asset files, so it
        // cannot verify asset bytes. `load_runtime_locked_package` upgrades this
        // when the assets are co-located with the package (#1464).
        asset_bytes_verified: false,
    })
}

pub fn build_runtime_game_run_report(readback: &RuntimePackageReadback) -> RuntimeGameRunReport {
    RuntimeGameRunReport {
        schema: "litd.game.runner.readback.v1".to_string(),
        runtime_id: readback.runtime_id.clone(),
        package_path: readback.package_path.clone(),
        package_sha256: readback.package_sha256.clone(),
        package_bytes: readback.package_bytes,
        package_id: readback.package_id.clone(),
        share_id: readback.share_id.clone(),
        document_fingerprint: readback.document_fingerprint.clone(),
        play_build_fingerprint: readback.play_build_fingerprint.clone(),
        document_summary: readback.document_summary.clone(),
        manifest_integrity_verified: readback.manifest_integrity_verified,
        runtime_lock_verified: readback.runtime_lock_verified,
        playable_on_litd_runtime: readback.playable_on_litd_runtime,
        asset_bytes_verified: readback.asset_bytes_verified,
    }
}

/// Verify the immutable package proof required before a guest can join a hosted lobby.
/// Membership mutation stays in the editor; this engine seam owns the runtime identity,
/// byte-address, lock, and playable-state checks so lightweight production callers can reuse it.
pub fn verify_runtime_package_admission(
    path: &Path,
    expected_package_id: &str,
    expected_package_sha256: &str,
    expected_package_bytes: usize,
    loaded_by: impl Into<String>,
) -> Result<RuntimeGameRunReport, EngineError> {
    let package_bytes = fs::read(path).map_err(|error| {
        game_package_error("package.read", format!("read {}: {error}", path.display()))
    })?;
    let actual_sha256 = sha256_hex(&package_bytes);
    if actual_sha256 != expected_package_sha256 || package_bytes.len() != expected_package_bytes {
        return Err(game_package_error(
            "package.admission",
            format!(
                "package bytes do not match expected proof: actual sha256={} bytes={}, expected sha256={} bytes={}",
                actual_sha256,
                package_bytes.len(),
                expected_package_sha256,
                expected_package_bytes
            ),
        ));
    }
    let readback = load_runtime_locked_package(path, loaded_by)?;
    if readback.package_id != expected_package_id
        || readback.package_sha256 != expected_package_sha256
        || readback.package_bytes != expected_package_bytes
    {
        return Err(game_package_error(
            "package.admission",
            format!(
                "package proof mismatch: actual id={} sha256={} bytes={}, expected id={} sha256={} bytes={}",
                readback.package_id,
                readback.package_sha256,
                readback.package_bytes,
                expected_package_id,
                expected_package_sha256,
                expected_package_bytes
            ),
        ));
    }
    let report = build_runtime_game_run_report(&readback);
    if !report.runtime_lock_verified || !report.playable_on_litd_runtime {
        return Err(game_package_error(
            "package.admission",
            "package runtime lock or playable-on-runtime proof is not valid",
        ));
    }
    Ok(report)
}

pub fn compute_runtime_manifest_fingerprint<E, A>(
    share_id: &str,
    created_by: &str,
    runtime_id: &str,
    endpoint: &E,
    document_fingerprint: &str,
    play_build_fingerprint: &str,
    assets: &[A],
) -> Result<String, EngineError>
where
    E: Serialize,
    A: Serialize,
{
    compute_runtime_manifest_fingerprint_bound(
        share_id,
        created_by,
        runtime_id,
        endpoint,
        document_fingerprint,
        play_build_fingerprint,
        assets,
        None,
    )
}

/// Like [`compute_runtime_manifest_fingerprint`], but ALSO binds the lowered runtime
/// datasets (#1843) into the envelope fingerprint via their self-derived
/// `datasets_fingerprint`. A package carrying runtime datasets commits to them here, so a
/// self-consistent substituted runtime section (a different but internally valid manifest)
/// no longer passes envelope validation -- exactly as the campaign manifest is bound to the
/// boot document (#853). When `runtime_datasets_fingerprint` is `None` the result is
/// byte-identical to the un-bound fingerprint, so campaign/plain packages are unaffected.
#[allow(clippy::too_many_arguments)]
pub fn compute_runtime_manifest_fingerprint_bound<E, A>(
    share_id: &str,
    created_by: &str,
    runtime_id: &str,
    endpoint: &E,
    document_fingerprint: &str,
    play_build_fingerprint: &str,
    assets: &[A],
    runtime_datasets_fingerprint: Option<&str>,
) -> Result<String, EngineError>
where
    E: Serialize,
    A: Serialize,
{
    match runtime_datasets_fingerprint {
        None => stable_package_fingerprint(&(
            share_id,
            created_by,
            runtime_id,
            endpoint,
            document_fingerprint,
            play_build_fingerprint,
            assets,
        )),
        Some(runtime_fingerprint) => stable_package_fingerprint(&(
            share_id,
            created_by,
            runtime_id,
            endpoint,
            document_fingerprint,
            play_build_fingerprint,
            assets,
            runtime_fingerprint,
        )),
    }
}

pub fn stable_package_fingerprint<T: Serialize>(value: &T) -> Result<String, EngineError> {
    litd_hash::stable_fingerprint(value).map_err(|error| EngineError::SerializationFailed {
        format: "ron",
        reason: error.to_string(),
    })
}
