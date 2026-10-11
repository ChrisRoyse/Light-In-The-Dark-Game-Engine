//! Generic #829 lowering-valve plumbing shared by the #823/#824/#826/#828 object-data
//! editors. One valve, four record families: each editor supplies a serde
//! `LoweredArtifactDoc` (schema version + fingerprint slot + content validation) and this
//! module fingerprints it (fnv1a64 over the fingerprint-cleared TOML preimage), writes the
//! project-confined TOML, and re-reads it from disk with `SerializedFingerprintMismatch` on
//! any drift. Mirrors the #818 unit valve exactly so persist-time and lowering-time
//! validation share one canonical contract. Also hosts the shared authored-property parse
//! helpers so every field-set module range/enum/flag checks against the SAME code.

use std::{fs, path::Path};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::validation::object_editor_error;
use crate::history::EditorObject;

/// Lowering version stamped into every #823/#824/#826/#828 lowered artifact provenance.
pub const OBJECT_EDITOR_P2_LOWERING_VERSION: &str = "litd-editor.object_editor.p2.v1";

/// Shared lowering provenance: where the persisted record came from + the valve version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweringProvenance {
    pub source_document_id: String,
    pub source_revision_id: String,
    pub source_object_id: String,
    pub lowering_version: String,
}

impl LoweringProvenance {
    pub(crate) fn new(
        document: &crate::history::EditorDocument,
        object_id: &str,
    ) -> LoweringProvenance {
        LoweringProvenance {
            source_document_id: document.calyx_document_id.clone(),
            source_revision_id: document.revision_id.clone(),
            source_object_id: object_id.to_string(),
            lowering_version: OBJECT_EDITOR_P2_LOWERING_VERSION.to_string(),
        }
    }
}

/// A serde lowered-artifact document with a fingerprint slot and a content validator.
pub trait LoweredArtifactDoc: Serialize + DeserializeOwned + Clone {
    const SCHEMA_VERSION: u32;
    fn schema_version(&self) -> u32;
    fn fingerprint(&self) -> &str;
    fn set_fingerprint(&mut self, fingerprint: String);
    fn provenance(&self) -> &LoweringProvenance;
    fn validate_content(&self) -> Result<(), EngineError>;
}

/// Canonical TOML + its fnv1a64 content fingerprint, ready to persist to disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredToml {
    pub canonical_toml: String,
    pub fingerprint: String,
}

/// A lowered artifact re-read from disk with byte length + sha256 + verified fingerprint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoweredTomlReadback<T> {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub fingerprint: String,
    pub document: T,
}

/// fnv1a64 over the canonical TOML of the document with its fingerprint slot cleared.
pub fn lowered_fingerprint<T: LoweredArtifactDoc>(document: &T) -> Result<String, EngineError> {
    let mut preimage = document.clone();
    preimage.set_fingerprint(String::new());
    let text = toml::to_string(&preimage).map_err(|error| EngineError::SerializationFailed {
        format: "toml",
        reason: format!("encode lowered object-editor p2 fingerprint preimage failed: {error}"),
    })?;
    Ok(litd_hash::fnv1a64_hex(text.as_bytes()))
}

/// Stamp the fingerprint into the document and encode it to canonical TOML.
pub fn finalize_lowering<T: LoweredArtifactDoc>(
    mut document: T,
) -> Result<LoweredToml, EngineError> {
    let fingerprint = lowered_fingerprint(&document)?;
    document.set_fingerprint(fingerprint.clone());
    let canonical_toml =
        toml::to_string(&document).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("encode lowered object-editor p2 TOML failed: {error}"),
        })?;
    Ok(LoweredToml {
        canonical_toml,
        fingerprint,
    })
}

/// Parse + validate a lowered artifact from TOML text, failing loud on drift.
pub fn verify_lowered<T: LoweredArtifactDoc>(
    canonical_toml: &str,
    expected_fingerprint: &str,
) -> Result<T, EngineError> {
    if expected_fingerprint.trim().is_empty() {
        return Err(object_editor_error(
            "toml.fingerprint",
            "expected fingerprint must be non-empty",
        ));
    }
    let document: T =
        toml::from_str(canonical_toml).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("parse lowered object-editor p2 TOML failed: {error}"),
        })?;
    if document.schema_version() != T::SCHEMA_VERSION {
        return Err(object_editor_error(
            "toml.schema_version",
            format!("expected schema version {}", T::SCHEMA_VERSION),
        ));
    }
    if document.provenance().lowering_version != OBJECT_EDITOR_P2_LOWERING_VERSION {
        return Err(object_editor_error(
            "toml.lowering_version",
            "lowered artifact carries an unexpected lowering version",
        ));
    }
    document.validate_content()?;
    if document.fingerprint() != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual: document.fingerprint().to_string(),
        });
    }
    let actual = lowered_fingerprint(&document)?;
    if actual != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual,
        });
    }
    Ok(document)
}

/// Write the canonical TOML to a project-confined path, then re-read + verify it from disk.
pub fn write_lowered<T: LoweredArtifactDoc>(
    path: impl AsRef<Path>,
    artifact: &LoweredToml,
) -> Result<LoweredTomlReadback<T>, EngineError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            object_editor_error(
                "toml.parent",
                format!("create {}: {error}", parent.display()),
            )
        })?;
    }
    fs::write(path, artifact.canonical_toml.as_bytes()).map_err(|error| {
        object_editor_error("toml.write", format!("write {}: {error}", path.display()))
    })?;
    read_lowered(path, &artifact.fingerprint)
}

/// Re-read a lowered artifact from disk with fingerprint verification.
pub fn read_lowered<T: LoweredArtifactDoc>(
    path: impl AsRef<Path>,
    expected_fingerprint: &str,
) -> Result<LoweredTomlReadback<T>, EngineError> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|error| {
        object_editor_error("toml.read", format!("read {}: {error}", path.display()))
    })?;
    let text = String::from_utf8(bytes.clone()).map_err(|error| {
        object_editor_error(
            "toml.utf8",
            format!("{} is not UTF-8: {error}", path.display()),
        )
    })?;
    let document: T = verify_lowered(&text, expected_fingerprint)?;
    Ok(LoweredTomlReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: litd_hash::sha256_hex(&bytes),
        fingerprint: expected_fingerprint.to_string(),
        document,
    })
}

// ---- Shared authored-property parse helpers (single source for every field module) ----

pub(crate) fn u32_field(
    object: &EditorObject,
    key: &str,
    min: u32,
    max: u32,
    default: u32,
) -> Result<u32, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(default);
    };
    let parsed = raw
        .trim()
        .parse::<u32>()
        .map_err(|_| object_editor_error(key, format!("must be an integer in {min}..={max}")))?;
    check_range(key, parsed, min, max)?;
    Ok(parsed)
}

pub(crate) fn string_field(object: &EditorObject, key: &str, default: &str) -> String {
    object
        .properties
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

pub(crate) fn enum_field(
    object: &EditorObject,
    key: &str,
    allowed: &[&str],
    default: &str,
) -> Result<String, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(default.to_string());
    };
    let value = raw.trim().to_string();
    check_enum(key, &value, allowed)?;
    Ok(value)
}

pub(crate) fn flag_list_field(
    object: &EditorObject,
    key: &str,
    allowed: &[&str],
    default: &str,
) -> Result<Vec<String>, EngineError> {
    let raw = object
        .properties
        .get(key)
        .map(String::as_str)
        .unwrap_or(default);
    let flags = split_list(raw);
    if flags.is_empty() {
        return Err(object_editor_error(key, "at least one flag is required"));
    }
    check_flags(key, &flags, allowed)?;
    Ok(flags)
}

pub(crate) fn id_list_field(object: &EditorObject, key: &str) -> Result<Vec<String>, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(Vec::new());
    };
    let ids = split_list(raw);
    let mut seen = std::collections::BTreeSet::new();
    for id in &ids {
        if !seen.insert(id.as_str()) {
            return Err(object_editor_error(key, format!("duplicate id {id}")));
        }
    }
    Ok(ids)
}

pub(crate) fn i32_list_field(object: &EditorObject, key: &str) -> Result<Vec<i32>, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(Vec::new());
    };
    split_list(raw)
        .iter()
        .map(|token| {
            token
                .parse::<i32>()
                .map_err(|_| object_editor_error(key, format!("{token} is not an integer")))
        })
        .collect()
}

pub(crate) fn u32_list_field(object: &EditorObject, key: &str) -> Result<Vec<u32>, EngineError> {
    let Some(raw) = object.properties.get(key) else {
        return Ok(Vec::new());
    };
    split_list(raw)
        .iter()
        .map(|token| {
            token.parse::<u32>().map_err(|_| {
                object_editor_error(key, format!("{token} is not a non-negative integer"))
            })
        })
        .collect()
}

pub(crate) fn split_list(raw: &str) -> Vec<String> {
    raw.split([',', '|'])
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

pub(crate) fn check_range(key: &str, value: u32, min: u32, max: u32) -> Result<(), EngineError> {
    if value < min || value > max {
        return Err(object_editor_error(
            key,
            format!("must be in {min}..={max}; got {value}"),
        ));
    }
    Ok(())
}

pub(crate) fn check_enum(key: &str, value: &str, allowed: &[&str]) -> Result<(), EngineError> {
    if !allowed.contains(&value) {
        return Err(object_editor_error(
            key,
            format!("{value} is not one of {allowed:?}"),
        ));
    }
    Ok(())
}

pub(crate) fn check_flags(
    key: &str,
    flags: &[String],
    allowed: &[&str],
) -> Result<(), EngineError> {
    let mut seen = std::collections::BTreeSet::new();
    for flag in flags {
        if !allowed.contains(&flag.as_str()) {
            return Err(object_editor_error(
                key,
                format!("{flag} is not one of {allowed:?}"),
            ));
        }
        if !seen.insert(flag.as_str()) {
            return Err(object_editor_error(key, format!("duplicate flag {flag}")));
        }
    }
    Ok(())
}
