//! #825 tech-tree lowering through the #829 valve. Collect every authored `Tech` object
//! from the persisted (Aster) document, build graph-validated requirement records, and emit
//! a single fnv1a64-fingerprinted litd/data TOML re-read from disk. Split out of
//! `object_editor` so each lowering file stays under the 500-line ratchet.

use std::{fs, path::Path};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::tech_fields::{
    LoweredTechTree, TechRequirementRecord, build_tech_record, validate_lowered_tech_tree,
    validate_tech_graph,
};
use super::validation::{invalid_object_editor, object_editor_error};
use crate::history::EditorDocument;
use crate::mcp_object_tools::read_mcp_object_document;

pub const OBJECT_EDITOR_TECH_TOML_SCHEMA_VERSION: u32 = 1;
pub const OBJECT_EDITOR_TECH_LOWERING_VERSION: &str = "litd-editor.object_editor.tech.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredTechDocument {
    pub schema_version: u32,
    pub lowering_fingerprint: String,
    pub tech_tree: LoweredTechTree,
    pub validation: ObjectEditorTechValidation,
    pub provenance: ObjectEditorTechProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorTechValidation {
    pub graph_valid: bool,
    pub node_count: usize,
    pub edge_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorTechProvenance {
    pub source_document_id: String,
    pub source_revision_id: String,
    pub lowering_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredTechToml {
    pub canonical_toml: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorTechReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub fingerprint: String,
    pub document: ObjectEditorLoweredTechDocument,
}

/// Lower every `Tech` object in the document into one graph-validated, fingerprinted tree.
pub fn lower_tech_from_document(
    document: &EditorDocument,
) -> Result<ObjectEditorLoweredTechToml, EngineError> {
    let mut records: Vec<TechRequirementRecord> = document
        .objects
        .values()
        .filter(|object| object.kind == "Tech")
        .map(build_tech_record)
        .collect::<Result<Vec<_>, _>>()?;
    if records.is_empty() {
        return invalid_object_editor("tech.records", "document has no Tech requirement records");
    }
    records.sort_by(|left, right| left.tech_id.cmp(&right.tech_id));
    let edge_count = validate_tech_graph(&records)?;
    let node_count = records.len();
    let tech_tree = LoweredTechTree {
        records,
        node_count,
        edge_count,
    };
    let mut lowered = ObjectEditorLoweredTechDocument {
        schema_version: OBJECT_EDITOR_TECH_TOML_SCHEMA_VERSION,
        lowering_fingerprint: String::new(),
        tech_tree,
        validation: ObjectEditorTechValidation {
            graph_valid: true,
            node_count,
            edge_count,
        },
        provenance: ObjectEditorTechProvenance {
            source_document_id: document.calyx_document_id.clone(),
            source_revision_id: document.revision_id.clone(),
            lowering_version: OBJECT_EDITOR_TECH_LOWERING_VERSION.to_string(),
        },
    };
    lowered.lowering_fingerprint = tech_fingerprint(&lowered)?;
    let canonical_toml =
        toml::to_string(&lowered).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("encode lowered tech TOML failed: {error}"),
        })?;
    Ok(ObjectEditorLoweredTechToml {
        canonical_toml,
        fingerprint: lowered.lowering_fingerprint,
    })
}

pub fn lower_tech_from_document_path(
    document_path: impl AsRef<Path>,
) -> Result<ObjectEditorLoweredTechToml, EngineError> {
    let readback = read_mcp_object_document(document_path)?;
    lower_tech_from_document(&readback.document)
}

pub fn write_lowered_tech_toml(
    path: impl AsRef<Path>,
    artifact: &ObjectEditorLoweredTechToml,
) -> Result<ObjectEditorTechReadback, EngineError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            object_editor_error(
                "tech.parent",
                format!("create {}: {error}", parent.display()),
            )
        })?;
    }
    fs::write(path, artifact.canonical_toml.as_bytes()).map_err(|error| {
        object_editor_error("tech.write", format!("write {}: {error}", path.display()))
    })?;
    read_lowered_tech_toml(path, &artifact.fingerprint)
}

pub fn read_lowered_tech_toml(
    path: impl AsRef<Path>,
    expected_fingerprint: &str,
) -> Result<ObjectEditorTechReadback, EngineError> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|error| {
        object_editor_error("tech.read", format!("read {}: {error}", path.display()))
    })?;
    let text = String::from_utf8(bytes.clone()).map_err(|error| {
        object_editor_error(
            "tech.utf8",
            format!("{} is not UTF-8: {error}", path.display()),
        )
    })?;
    let document = verify_lowered_tech_toml(&text, expected_fingerprint)?;
    Ok(ObjectEditorTechReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: litd_hash::sha256_hex(&bytes),
        fingerprint: expected_fingerprint.to_string(),
        document,
    })
}

pub fn verify_lowered_tech_toml(
    canonical_toml: &str,
    expected_fingerprint: &str,
) -> Result<ObjectEditorLoweredTechDocument, EngineError> {
    if expected_fingerprint.trim().is_empty() {
        return invalid_object_editor("tech.fingerprint", "expected fingerprint must be non-empty");
    }
    let document: ObjectEditorLoweredTechDocument =
        toml::from_str(canonical_toml).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("parse lowered tech TOML failed: {error}"),
        })?;
    if document.schema_version != OBJECT_EDITOR_TECH_TOML_SCHEMA_VERSION
        || !document.validation.graph_valid
        || document.provenance.lowering_version != OBJECT_EDITOR_TECH_LOWERING_VERSION
    {
        return invalid_object_editor(
            "tech.required",
            "lowered tech TOML is missing required data",
        );
    }
    validate_lowered_tech_tree(&document.tech_tree)?;
    if document.validation.node_count != document.tech_tree.node_count
        || document.validation.edge_count != document.tech_tree.edge_count
    {
        return invalid_object_editor("tech.validation", "lowered tech counts are inconsistent");
    }
    if document.lowering_fingerprint != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual: document.lowering_fingerprint,
        });
    }
    let actual = tech_fingerprint(&document)?;
    if actual != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual,
        });
    }
    Ok(document)
}

fn tech_fingerprint(document: &ObjectEditorLoweredTechDocument) -> Result<String, EngineError> {
    let mut preimage = document.clone();
    preimage.lowering_fingerprint.clear();
    let text = toml::to_string(&preimage).map_err(|error| EngineError::SerializationFailed {
        format: "toml",
        reason: format!("encode lowered tech fingerprint preimage failed: {error}"),
    })?;
    Ok(litd_hash::fnv1a64_hex(text.as_bytes()))
}
