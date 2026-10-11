//! #827 Gameplay Constants lowering through the #829 valve. Resolve the singleton
//! `GameplayConstant` object from the persisted (Aster) document into the FULL constant set
//! and emit a single fnv1a64-fingerprinted litd/data TOML re-read from disk. Split out of
//! `object_editor` so each lowering file stays under the 500-line ratchet.

use std::{fs, path::Path};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::constants_fields::{
    LoweredGameplayConstants, build_gameplay_constants, validate_gameplay_constants,
};
use super::validation::{invalid_object_editor, object_editor_error};
use crate::history::EditorDocument;
use crate::mcp_object_tools::read_mcp_object_document;

pub const OBJECT_EDITOR_CONSTANTS_TOML_SCHEMA_VERSION: u32 = 1;
pub const OBJECT_EDITOR_CONSTANTS_LOWERING_VERSION: &str = "litd-editor.object_editor.constants.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredConstantsDocument {
    pub schema_version: u32,
    pub lowering_fingerprint: String,
    pub constants: LoweredGameplayConstants,
    pub validation: ObjectEditorConstantsValidation,
    pub provenance: ObjectEditorConstantsProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorConstantsValidation {
    pub resolved: bool,
    pub range_checked: bool,
    pub level_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorConstantsProvenance {
    pub source_document_id: String,
    pub source_revision_id: String,
    pub source_object_id: String,
    pub lowering_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredConstantsToml {
    pub canonical_toml: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorConstantsReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub fingerprint: String,
    pub document: ObjectEditorLoweredConstantsDocument,
}

/// Resolve the singleton `GameplayConstant` object and lower the FULL constant set.
pub fn lower_constants_from_document(
    document: &EditorDocument,
) -> Result<ObjectEditorLoweredConstantsToml, EngineError> {
    let mut singletons = document
        .objects
        .values()
        .filter(|object| object.kind == "GameplayConstant")
        .collect::<Vec<_>>();
    singletons.sort_by(|left, right| left.id.cmp(&right.id));
    let object = match singletons.as_slice() {
        [] => {
            return invalid_object_editor(
                "constants.singleton",
                "document has no GameplayConstant record",
            );
        }
        [single] => *single,
        many => {
            return invalid_object_editor(
                "constants.singleton",
                format!(
                    "GameplayConstant must be a singleton per project; found {}",
                    many.len()
                ),
            );
        }
    };
    let constants = build_gameplay_constants(object)?;
    let level_count = constants.xp_level_thresholds.len();
    let mut lowered = ObjectEditorLoweredConstantsDocument {
        schema_version: OBJECT_EDITOR_CONSTANTS_TOML_SCHEMA_VERSION,
        lowering_fingerprint: String::new(),
        constants,
        validation: ObjectEditorConstantsValidation {
            resolved: true,
            range_checked: true,
            level_count,
        },
        provenance: ObjectEditorConstantsProvenance {
            source_document_id: document.calyx_document_id.clone(),
            source_revision_id: document.revision_id.clone(),
            source_object_id: object.id.clone(),
            lowering_version: OBJECT_EDITOR_CONSTANTS_LOWERING_VERSION.to_string(),
        },
    };
    lowered.lowering_fingerprint = constants_fingerprint(&lowered)?;
    let canonical_toml =
        toml::to_string(&lowered).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("encode lowered constants TOML failed: {error}"),
        })?;
    Ok(ObjectEditorLoweredConstantsToml {
        canonical_toml,
        fingerprint: lowered.lowering_fingerprint,
    })
}

pub fn lower_constants_from_document_path(
    document_path: impl AsRef<Path>,
) -> Result<ObjectEditorLoweredConstantsToml, EngineError> {
    let readback = read_mcp_object_document(document_path)?;
    lower_constants_from_document(&readback.document)
}

pub fn write_lowered_constants_toml(
    path: impl AsRef<Path>,
    artifact: &ObjectEditorLoweredConstantsToml,
) -> Result<ObjectEditorConstantsReadback, EngineError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            object_editor_error(
                "constants.parent",
                format!("create {}: {error}", parent.display()),
            )
        })?;
    }
    fs::write(path, artifact.canonical_toml.as_bytes()).map_err(|error| {
        object_editor_error(
            "constants.write",
            format!("write {}: {error}", path.display()),
        )
    })?;
    read_lowered_constants_toml(path, &artifact.fingerprint)
}

pub fn read_lowered_constants_toml(
    path: impl AsRef<Path>,
    expected_fingerprint: &str,
) -> Result<ObjectEditorConstantsReadback, EngineError> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|error| {
        object_editor_error(
            "constants.read",
            format!("read {}: {error}", path.display()),
        )
    })?;
    let text = String::from_utf8(bytes.clone()).map_err(|error| {
        object_editor_error(
            "constants.utf8",
            format!("{} is not UTF-8: {error}", path.display()),
        )
    })?;
    let document = verify_lowered_constants_toml(&text, expected_fingerprint)?;
    Ok(ObjectEditorConstantsReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: litd_hash::sha256_hex(&bytes),
        fingerprint: expected_fingerprint.to_string(),
        document,
    })
}

pub fn verify_lowered_constants_toml(
    canonical_toml: &str,
    expected_fingerprint: &str,
) -> Result<ObjectEditorLoweredConstantsDocument, EngineError> {
    if expected_fingerprint.trim().is_empty() {
        return invalid_object_editor(
            "constants.fingerprint",
            "expected fingerprint must be non-empty",
        );
    }
    let document: ObjectEditorLoweredConstantsDocument =
        toml::from_str(canonical_toml).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("parse lowered constants TOML failed: {error}"),
        })?;
    if document.schema_version != OBJECT_EDITOR_CONSTANTS_TOML_SCHEMA_VERSION
        || !document.validation.resolved
        || !document.validation.range_checked
        || document.provenance.lowering_version != OBJECT_EDITOR_CONSTANTS_LOWERING_VERSION
    {
        return invalid_object_editor(
            "constants.required",
            "lowered constants TOML is missing required data",
        );
    }
    validate_gameplay_constants(&document.constants)?;
    if document.validation.level_count != document.constants.xp_level_thresholds.len() {
        return invalid_object_editor(
            "constants.validation",
            "lowered constants level_count is inconsistent",
        );
    }
    if document.lowering_fingerprint != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual: document.lowering_fingerprint,
        });
    }
    let actual = constants_fingerprint(&document)?;
    if actual != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual,
        });
    }
    Ok(document)
}

fn constants_fingerprint(
    document: &ObjectEditorLoweredConstantsDocument,
) -> Result<String, EngineError> {
    let mut preimage = document.clone();
    preimage.lowering_fingerprint.clear();
    let text = toml::to_string(&preimage).map_err(|error| EngineError::SerializationFailed {
        format: "toml",
        reason: format!("encode lowered constants fingerprint preimage failed: {error}"),
    })?;
    Ok(litd_hash::fnv1a64_hex(text.as_bytes()))
}
