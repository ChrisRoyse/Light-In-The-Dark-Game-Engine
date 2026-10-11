use std::{
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};

use litd_engine::EngineError;
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::{
    LitdWorldProject, MCP_LITDWORLD_SOURCE_OF_TRUTH,
    mcp_project_tools_validation::validate_project_file,
};
use crate::history::stable_fingerprint;

const MANIFEST_ENTRY: &str = "manifest.json";
const PROJECT_ENTRY: &str = "project.json";
const ARCHIVE_FORMAT: &str = "zip";
const ARCHIVE_VERSION: &str = "1";
const MIN_ENGINE_VERSION: &str = "0.0.1";
const CURRENT_ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_ENTRY_BYTES: usize = 16 * 1024 * 1024;

pub const LITDWORLD_ARCHIVE_SCHEMA: &str = "litd.worldarchive.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LitdWorldArchiveEntry {
    pub path: String,
    pub bytes: usize,
    pub sha256: String,
    pub media_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LitdWorldArchiveManifest {
    pub schema: String,
    pub issue: u32,
    pub archive_format: String,
    pub archive_version: String,
    pub compression: String,
    pub engine_version: String,
    pub min_engine_version: String,
    pub project_entry: String,
    pub project_bytes: usize,
    pub project_sha256: String,
    pub document_fingerprint: String,
    pub aggregate_sha256: String,
    pub entries: Vec<LitdWorldArchiveEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LitdWorldArchiveReadback {
    pub path: String,
    pub bytes: usize,
    pub sha256: String,
    pub zip_magic: String,
    pub zip_entry_count: usize,
    pub zip_entries: Vec<String>,
    pub manifest: LitdWorldArchiveManifest,
    pub project: LitdWorldProject,
}

#[derive(Serialize)]
struct AggregateInput<'a> {
    schema: &'a str,
    archive_format: &'a str,
    archive_version: &'a str,
    engine_version: &'a str,
    min_engine_version: &'a str,
    project_entry: &'a str,
    project_bytes: usize,
    project_sha256: &'a str,
    document_fingerprint: &'a str,
}

pub fn write_litdworld_archive(
    path: impl AsRef<Path>,
    project: &LitdWorldProject,
) -> Result<LitdWorldArchiveReadback, EngineError> {
    validate_project_file(project)?;
    let path = path.as_ref();
    if path.exists() && path.is_dir() {
        return Err(archive_error(
            "archive.path",
            format!(
                "{} is a directory, expected a .litdworld file",
                path.display()
            ),
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            archive_error("archive.parent", format!("{}: {error}", parent.display()))
        })?;
    }

    let project_bytes =
        serde_json::to_vec_pretty(project).map_err(|error| EngineError::SerializationFailed {
            format: "litdworld-project-json",
            reason: format!("serialize archive project entry: {error}"),
        })?;
    let project_sha256 = sha256_hex(&project_bytes);
    let document_fingerprint = stable_fingerprint(&project.document).map_err(|error| {
        EngineError::SerializationFailed {
            format: "litdworld-document-ron",
            reason: format!("fingerprint archive project document: {error}"),
        }
    })?;
    if project.document_fingerprint != document_fingerprint {
        return Err(archive_error(
            "project.document_fingerprint",
            "project fingerprint does not match recomputed document fingerprint",
        ));
    }
    let manifest = LitdWorldArchiveManifest {
        schema: LITDWORLD_ARCHIVE_SCHEMA.to_string(),
        issue: 1316,
        archive_format: ARCHIVE_FORMAT.to_string(),
        archive_version: ARCHIVE_VERSION.to_string(),
        compression: "deflate".to_string(),
        engine_version: CURRENT_ENGINE_VERSION.to_string(),
        min_engine_version: MIN_ENGINE_VERSION.to_string(),
        project_entry: PROJECT_ENTRY.to_string(),
        project_bytes: project_bytes.len(),
        project_sha256: project_sha256.clone(),
        document_fingerprint,
        aggregate_sha256: aggregate_sha256(
            CURRENT_ENGINE_VERSION,
            MIN_ENGINE_VERSION,
            project_bytes.len(),
            &project_sha256,
            &project.document_fingerprint,
        )?,
        entries: vec![LitdWorldArchiveEntry {
            path: PROJECT_ENTRY.to_string(),
            bytes: project_bytes.len(),
            sha256: project_sha256,
            media_type: "application/json".to_string(),
        }],
    };
    let manifest_bytes =
        serde_json::to_vec_pretty(&manifest).map_err(|error| EngineError::SerializationFailed {
            format: "litdworld-manifest-json",
            reason: format!("serialize archive manifest: {error}"),
        })?;

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    writer
        .start_file(MANIFEST_ENTRY, options)
        .map_err(|error| archive_error("archive.write.manifest", error))?;
    writer
        .write_all(&manifest_bytes)
        .map_err(|error| archive_error("archive.write.manifest", error))?;
    writer
        .start_file(PROJECT_ENTRY, options)
        .map_err(|error| archive_error("archive.write.project", error))?;
    writer
        .write_all(&project_bytes)
        .map_err(|error| archive_error("archive.write.project", error))?;
    let archive_bytes = writer
        .finish()
        .map_err(|error| archive_error("archive.finish", error))?
        .into_inner();
    fs::write(path, &archive_bytes)
        .map_err(|error| archive_error("archive.write", format!("{}: {error}", path.display())))?;

    let readback = inspect_litdworld_archive(path)?;
    if readback.project != *project {
        return Err(archive_error(
            "archive.readback",
            "archive project differed immediately after write",
        ));
    }
    Ok(readback)
}

pub fn inspect_litdworld_archive(
    path: impl AsRef<Path>,
) -> Result<LitdWorldArchiveReadback, EngineError> {
    let path = path.as_ref();
    let archive_bytes = fs::read(path)
        .map_err(|error| archive_error("archive.read", format!("{}: {error}", path.display())))?;
    if !archive_bytes.starts_with(b"PK\x03\x04") {
        tracing::error!(
            path = %path.display(),
            code = "LITD-WORLDARCHIVE-SIGNATURE",
            "litdworld archive rejected: missing ZIP local-file-header signature"
        );
        return Err(archive_error(
            "archive.signature",
            "canonical .litdworld files must be ZIP archives starting with PK\\x03\\x04",
        ));
    }
    let mut archive = ZipArchive::new(Cursor::new(&archive_bytes))
        .map_err(|error| archive_error("archive.central_directory", error))?;
    let names = archive
        .file_names()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    validate_archive_entries(&names)?;
    let manifest_bytes = read_zip_entry(&mut archive, MANIFEST_ENTRY)?;
    let project_bytes = read_zip_entry(&mut archive, PROJECT_ENTRY)?;
    let manifest: LitdWorldArchiveManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|error| {
            EngineError::SerializationFailed {
                format: "litdworld-manifest-json",
                reason: format!("parse archive manifest: {error}"),
            }
        })?;
    validate_manifest(&manifest, &project_bytes)?;
    let project: LitdWorldProject = serde_json::from_slice(&project_bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "litdworld-project-json",
            reason: format!("parse archive project entry: {error}"),
        }
    })?;
    validate_project_file(&project)?;
    validate_project_matches_manifest(&project, &manifest)?;
    Ok(LitdWorldArchiveReadback {
        path: path.display().to_string(),
        bytes: archive_bytes.len(),
        sha256: sha256_hex(&archive_bytes),
        zip_magic: "PK0304".to_string(),
        zip_entry_count: names.len(),
        zip_entries: names,
        manifest,
        project,
    })
}

fn validate_manifest(
    manifest: &LitdWorldArchiveManifest,
    project_bytes: &[u8],
) -> Result<(), EngineError> {
    if manifest.schema != LITDWORLD_ARCHIVE_SCHEMA
        || manifest.issue != 1316
        || manifest.archive_format != ARCHIVE_FORMAT
        || manifest.archive_version != ARCHIVE_VERSION
        || manifest.compression != "deflate"
        || manifest.project_entry != PROJECT_ENTRY
    {
        tracing::error!(
            schema = %manifest.schema,
            archive_format = %manifest.archive_format,
            archive_version = %manifest.archive_version,
            compression = %manifest.compression,
            code = "LITD-WORLDARCHIVE-MANIFEST-CONTRACT",
            "litdworld archive rejected: manifest contract mismatch"
        );
        return Err(archive_error(
            "manifest.contract",
            "manifest schema, format, version, compression, and project_entry must match the canonical .litdworld contract",
        ));
    }
    if manifest.engine_version.trim().is_empty() || manifest.min_engine_version.trim().is_empty() {
        return Err(archive_error(
            "manifest.engine_version",
            "engine_version and min_engine_version must be stamped",
        ));
    }
    validate_engine_version_range(&manifest.engine_version, &manifest.min_engine_version)?;
    let project_sha256 = sha256_hex(project_bytes);
    if manifest.project_sha256 != project_sha256
        || manifest.project_bytes != project_bytes.len()
        || manifest.entries.len() != 1
        || manifest.entries[0].path != PROJECT_ENTRY
        || manifest.entries[0].sha256 != project_sha256
        || manifest.entries[0].bytes != project_bytes.len()
    {
        tracing::error!(
            expected_project_sha256 = %project_sha256,
            manifest_project_sha256 = %manifest.project_sha256,
            code = "LITD-WORLDARCHIVE-PROJECT-HASH",
            "litdworld archive rejected: project entry hash or bytes mismatch"
        );
        return Err(archive_error(
            "manifest.project_sha256",
            "project entry bytes must match manifest project_sha256, bytes, and entry record",
        ));
    }
    let aggregate = aggregate_sha256(
        &manifest.engine_version,
        &manifest.min_engine_version,
        project_bytes.len(),
        &project_sha256,
        &manifest.document_fingerprint,
    )?;
    if manifest.aggregate_sha256 != aggregate {
        return Err(archive_error(
            "manifest.aggregate_sha256",
            "aggregate_sha256 does not match manifest content and project entry",
        ));
    }
    Ok(())
}

fn validate_project_matches_manifest(
    project: &LitdWorldProject,
    manifest: &LitdWorldArchiveManifest,
) -> Result<(), EngineError> {
    let document_fingerprint = stable_fingerprint(&project.document).map_err(|error| {
        EngineError::SerializationFailed {
            format: "litdworld-document-ron",
            reason: format!("fingerprint archive project document: {error}"),
        }
    })?;
    if project.source_of_truth != MCP_LITDWORLD_SOURCE_OF_TRUTH
        || project.document_fingerprint != manifest.document_fingerprint
        || project.document_fingerprint != document_fingerprint
    {
        return Err(archive_error(
            "project.document_fingerprint",
            "project source and document fingerprint must match manifest and recomputed document fingerprint",
        ));
    }
    Ok(())
}

fn validate_archive_entries(names: &[String]) -> Result<(), EngineError> {
    if names.len() != 2
        || !names.iter().any(|name| name == MANIFEST_ENTRY)
        || !names.iter().any(|name| name == PROJECT_ENTRY)
        || names.iter().any(|name| {
            name.contains('\\')
                || name.contains("..")
                || name.starts_with('/')
                || name.starts_with('.')
        })
    {
        tracing::error!(
            entries = ?names,
            code = "LITD-WORLDARCHIVE-ENTRY-SET",
            "litdworld archive rejected: entry set is not the canonical manifest/project pair"
        );
        return Err(archive_error(
            "archive.entries",
            "canonical .litdworld archives must contain exactly manifest.json and project.json",
        ));
    }
    Ok(())
}

fn read_zip_entry<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>, EngineError> {
    let mut file = archive
        .by_name(name)
        .map_err(|error| archive_error("archive.entry", format!("{name}: {error}")))?;
    if file.is_dir() || file.compression() != CompressionMethod::Deflated {
        return Err(archive_error(
            "archive.entry.compression",
            format!("{name} must be a deflated file entry"),
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| archive_error("archive.entry.read", format!("{name}: {error}")))?;
    if bytes.is_empty() || bytes.len() > MAX_ENTRY_BYTES {
        return Err(archive_error(
            "archive.entry.size",
            format!(
                "{name} bytes must be in 1..={MAX_ENTRY_BYTES}, got {}",
                bytes.len()
            ),
        ));
    }
    Ok(bytes)
}

fn aggregate_sha256(
    engine_version: &str,
    min_engine_version: &str,
    project_bytes: usize,
    project_sha256: &str,
    document_fingerprint: &str,
) -> Result<String, EngineError> {
    let input = AggregateInput {
        schema: LITDWORLD_ARCHIVE_SCHEMA,
        archive_format: ARCHIVE_FORMAT,
        archive_version: ARCHIVE_VERSION,
        engine_version,
        min_engine_version,
        project_entry: PROJECT_ENTRY,
        project_bytes,
        project_sha256,
        document_fingerprint,
    };
    let bytes = serde_json::to_vec(&input).map_err(|error| EngineError::SerializationFailed {
        format: "litdworld-aggregate-json",
        reason: format!("serialize aggregate input: {error}"),
    })?;
    Ok(sha256_hex(&bytes))
}

fn validate_engine_version_range(
    engine_version: &str,
    min_engine_version: &str,
) -> Result<(), EngineError> {
    let archive_engine = parse_semver_triplet("manifest.engine_version", engine_version)?;
    let min_engine = parse_semver_triplet("manifest.min_engine_version", min_engine_version)?;
    let current = parse_semver_triplet("current_engine_version", CURRENT_ENGINE_VERSION)?;
    if current < min_engine {
        return Err(archive_error(
            "manifest.min_engine_version",
            format!(
                "archive requires engine >= {min_engine_version}, current engine is {CURRENT_ENGINE_VERSION}"
            ),
        ));
    }
    if archive_engine < min_engine {
        return Err(archive_error(
            "manifest.engine_version",
            "engine_version must be greater than or equal to min_engine_version",
        ));
    }
    Ok(())
}

fn parse_semver_triplet(field: &str, value: &str) -> Result<(u64, u64, u64), EngineError> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(archive_error(field, "expected major.minor.patch"));
    }
    let major = parse_semver_part(field, parts[0])?;
    let minor = parse_semver_part(field, parts[1])?;
    let patch = parse_semver_part(field, parts[2])?;
    Ok((major, minor, patch))
}

fn parse_semver_part(field: &str, value: &str) -> Result<u64, EngineError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(archive_error(field, "expected numeric semver components"));
    }
    value
        .parse::<u64>()
        .map_err(|error| archive_error(field, error))
}

fn archive_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    EngineError::SerializationFailed {
        format: "litdworld-archive",
        reason: format!("{}: {}", field.into(), reason.to_string()),
    }
}
