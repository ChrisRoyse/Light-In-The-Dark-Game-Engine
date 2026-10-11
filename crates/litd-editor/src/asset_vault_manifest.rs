use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use litd_engine::EngineError;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::asset_vault::AssetVaultRecord;

const VERIFIED_DOWNLOAD_STATUSES: &[&str] = &[
    "downloaded",
    "existing_verified",
    "copied_from_manual_acquisition",
];

#[derive(Debug, Deserialize)]
struct RawAssetVaultRecord {
    record_type: Option<String>,
    source: Option<String>,
    asset_id: Option<String>,
    asset_type: Option<String>,
    title: Option<String>,
    license: Option<String>,
    license_url: Option<String>,
    source_url: Option<String>,
    relative_path: Option<String>,
    absolute_path: Option<String>,
    bytes_on_disk: Option<u64>,
    sha256: Option<String>,
    status: Option<String>,
}

pub(super) fn load_manifest_records(
    path: &Path,
    expected_record_type: &str,
) -> Result<Vec<AssetVaultRecord>, EngineError> {
    if !path.exists() {
        return asset_vault_error(
            "manifest.path",
            format!("manifest does not exist: {}", path.display()),
        );
    }
    let text = fs::read_to_string(path).map_err(|error| EngineError::UiRenderInvalidConfig {
        field: "editor.asset_vault.manifest.read".to_string(),
        reason: format!("failed to read {}: {error}", path.display()),
    })?;
    if text.trim().is_empty() {
        return asset_vault_error(
            "manifest.records",
            format!("manifest is empty: {}", path.display()),
        );
    }
    let vault_root =
        path.parent()
            .and_then(Path::parent)
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.asset_vault.manifest.root".to_string(),
                reason: format!(
                    "manifest is not under a vault manifests directory: {}",
                    path.display()
                ),
            })?;
    let vault_root =
        fs::canonicalize(vault_root).map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_vault.manifest.root".to_string(),
            reason: format!("failed to canonicalize {}: {error}", vault_root.display()),
        })?;
    // Phase A -- ALWAYS runs, cheap: parse every record and verify its file exists, is a regular
    // file, and matches the manifested byte length + path consistency. This is O(records)
    // metadata syscalls (no file CONTENT is read), so a missing or truncated fixture still ERRORs
    // immediately and loudly. Each file's (relative_path, size, mtime) is collected to derive the
    // content-addressed verification key.
    let mut records = Vec::new();
    let mut lines = Vec::new();
    let mut fingerprints = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let raw = serde_json::from_str::<RawAssetVaultRecord>(line).map_err(|error| {
            EngineError::UiRenderInvalidConfig {
                field: "editor.asset_vault.manifest.json".to_string(),
                reason: format!(
                    "{}:{} invalid JSONL: {error}",
                    path.display(),
                    line_index + 1
                ),
            }
        })?;
        let record = build_record(path, line_index + 1, raw, expected_record_type)?;
        let modified = validate_record_metadata(path, line_index + 1, &vault_root, &record)?;
        fingerprints.push(super::verify_cache::FileFingerprint {
            relative_path: record.relative_path.clone(),
            size: record.bytes_on_disk,
            mtime_nanos: super::verify_cache::mtime_nanos(modified),
        });
        lines.push(line_index + 1);
        records.push(record);
    }
    if records.is_empty() {
        return asset_vault_error(
            "manifest.records",
            format!("manifest has no nonblank records: {}", path.display()),
        );
    }

    // Phase B -- EXPENSIVE, cache-gated (#1763): the full per-file content sha256 sweep. It runs
    // only when the derived key (manifest bytes + every file's size/mtime) has no matching receipt
    // beside the vault. On a warm shared cache -- e.g. a fresh worktree reusing the primary
    // checkout's receipt -- this is skipped entirely, turning a multi-gigabyte, 900s+ re-hash into
    // a handful of metadata reads. On a miss it verifies content and records a receipt.
    let plan = super::verify_cache::plan(&vault_root, path, text.as_bytes(), &fingerprints);
    if plan.needs_full_hash() {
        for (record, line) in records.iter().zip(&lines) {
            verify_file_sha256(path, *line, record)?;
        }
        plan.record_success(records.len());
    }
    Ok(records)
}

fn build_record(
    path: &Path,
    line: usize,
    raw: RawAssetVaultRecord,
    expected_record_type: &str,
) -> Result<AssetVaultRecord, EngineError> {
    let record_type = required(raw.record_type, path, line, "record_type")?;
    if record_type != expected_record_type {
        return asset_vault_error(
            "manifest.record_type",
            format!(
                "{}:{line} expected record_type '{expected_record_type}', found '{record_type}'",
                path.display()
            ),
        );
    }
    Ok(AssetVaultRecord {
        record_type,
        source: required(raw.source, path, line, "source")?,
        asset_id: required(raw.asset_id, path, line, "asset_id")?,
        asset_type: required(raw.asset_type, path, line, "asset_type")?,
        title: required(raw.title, path, line, "title")?,
        license: required(raw.license, path, line, "license")?,
        license_url: required(raw.license_url, path, line, "license_url")?,
        source_url: required(raw.source_url, path, line, "source_url")?,
        relative_path: required(raw.relative_path, path, line, "relative_path")?,
        absolute_path: PathBuf::from(required(raw.absolute_path, path, line, "absolute_path")?),
        bytes_on_disk: raw
            .bytes_on_disk
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.asset_vault.manifest.bytes_on_disk".to_string(),
                reason: format!("{}:{line} missing bytes_on_disk", path.display()),
            })?,
        sha256: required(raw.sha256, path, line, "sha256")?,
        status: raw.status,
    })
}

/// Cheap, ALWAYS-run per-record checks: verified status, license, sha256 hex FORM (not content),
/// relative-path safety, path consistency, file existence, regular-file kind, and byte length.
/// Returns the file's mtime so the caller can fold it into the content-addressed verification key.
/// The expensive file CONTENT sha256 sweep is a separate, cache-gated step (`verify_file_sha256`).
fn validate_record_metadata(
    manifest_path: &Path,
    line: usize,
    vault_root: &Path,
    record: &AssetVaultRecord,
) -> Result<Option<std::time::SystemTime>, EngineError> {
    if record.record_type == "download" && !is_verified_download_status(record.status.as_deref()) {
        return asset_vault_error(
            "manifest.status",
            format!(
                "{}:{line} download record status {:?} is not one of verified statuses {}",
                manifest_path.display(),
                record.status,
                VERIFIED_DOWNLOAD_STATUSES.join(",")
            ),
        );
    }
    if record.license != "CC0-1.0" {
        return asset_vault_error(
            "manifest.license",
            format!(
                "{}:{line} expected CC0-1.0, found {}",
                manifest_path.display(),
                record.license
            ),
        );
    }
    if !is_sha256_hex(&record.sha256) {
        return asset_vault_error(
            "manifest.sha256",
            format!(
                "{}:{line} invalid sha256 '{}'",
                manifest_path.display(),
                record.sha256
            ),
        );
    }
    validate_relative_path(manifest_path, line, &record.relative_path)?;
    if !record.absolute_path.is_absolute() {
        return asset_vault_error(
            "manifest.absolute_path",
            format!(
                "{}:{line} absolute_path is not absolute",
                manifest_path.display()
            ),
        );
    }
    let expected_absolute = vault_root.join(&record.relative_path);
    if normalized_path(&expected_absolute) != normalized_path(&record.absolute_path) {
        return asset_vault_error(
            "manifest.path_consistency",
            format!(
                "{}:{line} absolute_path does not match vault root plus relative_path",
                manifest_path.display()
            ),
        );
    }
    let metadata = fs::metadata(&record.absolute_path).map_err(|error| {
        EngineError::UiRenderInvalidConfig {
            field: "editor.asset_vault.file.exists".to_string(),
            reason: format!(
                "{}:{line} failed to inspect {}: {error}",
                manifest_path.display(),
                record.absolute_path.display()
            ),
        }
    })?;
    if !metadata.is_file() {
        return asset_vault_error(
            "file.kind",
            format!(
                "{}:{line} path is not a file: {}",
                manifest_path.display(),
                record.absolute_path.display()
            ),
        );
    }
    if metadata.len() != record.bytes_on_disk {
        return asset_vault_error(
            "file.bytes_on_disk",
            format!(
                "{}:{line} {} bytes_on_disk={} actual={}",
                manifest_path.display(),
                record.absolute_path.display(),
                record.bytes_on_disk,
                metadata.len()
            ),
        );
    }
    Ok(metadata.modified().ok())
}

fn verify_file_sha256(
    manifest_path: &Path,
    line: usize,
    record: &AssetVaultRecord,
) -> Result<(), EngineError> {
    let mut file = fs::File::open(&record.absolute_path).map_err(|error| {
        EngineError::UiRenderInvalidConfig {
            field: "editor.asset_vault.file.read".to_string(),
            reason: format!(
                "{}:{line} failed to open {} for sha256 verification: {error}",
                manifest_path.display(),
                record.absolute_path.display()
            ),
        }
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let bytes_read =
            file.read(&mut buffer)
                .map_err(|error| EngineError::UiRenderInvalidConfig {
                    field: "editor.asset_vault.file.read".to_string(),
                    reason: format!(
                        "{}:{line} failed to read {} for sha256 verification: {error}",
                        manifest_path.display(),
                        record.absolute_path.display()
                    ),
                })?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != record.sha256.to_ascii_lowercase() {
        return asset_vault_error(
            "file.sha256",
            format!(
                "{}:{line} {} manifest_sha256={} actual_sha256={}",
                manifest_path.display(),
                record.absolute_path.display(),
                record.sha256,
                actual
            ),
        );
    }
    Ok(())
}

fn validate_relative_path(
    path: &Path,
    line: usize,
    relative_path: &str,
) -> Result<(), EngineError> {
    let relative = Path::new(relative_path);
    if relative.is_absolute() {
        return asset_vault_error(
            "manifest.relative_path",
            format!("{}:{line} relative_path is absolute", path.display()),
        );
    }
    if relative
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return asset_vault_error(
            "manifest.relative_path",
            format!("{}:{line} relative_path escapes the vault", path.display()),
        );
    }
    Ok(())
}

fn required(
    value: Option<String>,
    path: &Path,
    line: usize,
    field: &'static str,
) -> Result<String, EngineError> {
    match value.map(|value| value.trim().to_string()) {
        Some(value) if !value.is_empty() => Ok(value),
        _ => asset_vault_error(
            format!("manifest.{field}"),
            format!("{}:{line} missing {field}", path.display()),
        ),
    }
}

use litd_hash::is_sha256_hex;

pub(super) fn is_verified_download_status(status: Option<&str>) -> bool {
    status
        .map(|status| VERIFIED_DOWNLOAD_STATUSES.contains(&status))
        .unwrap_or(false)
}

fn normalized_path(path: &Path) -> String {
    let mut path = path
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase();
    if let Some(stripped) = path.strip_prefix("//?/") {
        path = stripped.to_string();
    }
    path
}

pub(super) fn asset_vault_error<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_vault.{}", field.into()),
        reason: reason.into(),
    })
}
