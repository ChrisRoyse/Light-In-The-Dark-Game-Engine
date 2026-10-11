//! #819 ability lowering + fingerprinted litd/data TOML readback, through the #829 valve.
//! Mirrors the unit lowering surface (`object_editor`): lower a persisted ability object to a
//! deterministic fnv1a64-fingerprinted TOML document, write it project-confined, and re-read it
//! from disk with fingerprint verification so a corrupted artifact fails loud.

use std::{fs, path::Path};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::ability_fields::{AbilityFields, build_ability_fields, validate_ability_fields};
use super::validation::{object_editor_error, required_property};
use super::{OBJECT_EDITOR_LOWERING_VERSION, ObjectEditorLoweringProvenance};
use crate::{history::EditorObject, mcp_object_tools::read_mcp_object_document};

pub const OBJECT_EDITOR_ABILITY_TOML_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredAbilityDocument {
    pub schema_version: u32,
    pub lowering_fingerprint: String,
    pub ability: ObjectEditorLoweredAbility,
    pub validation: ObjectEditorAbilityValidation,
    pub provenance: ObjectEditorLoweringProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredAbility {
    pub object_id: String,
    pub rawcode: String,
    pub base_object_id: String,
    pub display_name: String,
    pub fields: AbilityFields,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorAbilityValidation {
    pub lower_ability_valid: bool,
    pub range_checked: bool,
    pub levels: u32,
    pub effect_count: usize,
    pub requirement_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredAbilityToml {
    pub canonical_toml: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorAbilityTomlReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub fingerprint: String,
    pub document: ObjectEditorLoweredAbilityDocument,
}

pub fn lower_ability_from_object(
    document: &crate::history::EditorDocument,
    object_id: &str,
) -> Result<ObjectEditorLoweredAbilityToml, EngineError> {
    let object = document.objects.get(object_id).ok_or_else(|| {
        object_editor_error("ability.object_id", format!("missing object {object_id}"))
    })?;
    if object.kind != "Ability" {
        return Err(object_editor_error(
            "ability.kind",
            format!("{object_id} is kind {} not Ability", object.kind),
        ));
    }
    let ability = lowered_ability_from_object(object)?;
    let mut lowered_document = ObjectEditorLoweredAbilityDocument {
        schema_version: OBJECT_EDITOR_ABILITY_TOML_SCHEMA_VERSION,
        lowering_fingerprint: String::new(),
        validation: ObjectEditorAbilityValidation {
            lower_ability_valid: true,
            range_checked: true,
            levels: ability.fields.levels,
            effect_count: ability.fields.effects.len(),
            requirement_count: ability.fields.requirements.len(),
        },
        provenance: ObjectEditorLoweringProvenance {
            source_document_id: document.calyx_document_id.clone(),
            source_revision_id: document.revision_id.clone(),
            source_object_id: object_id.to_string(),
            lowering_version: OBJECT_EDITOR_LOWERING_VERSION.to_string(),
        },
        ability,
    };
    lowered_document.lowering_fingerprint = ability_fingerprint(&lowered_document)?;
    let canonical_toml =
        toml::to_string(&lowered_document).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("encode lowered object-editor ability TOML failed: {error}"),
        })?;
    Ok(ObjectEditorLoweredAbilityToml {
        canonical_toml,
        fingerprint: lowered_document.lowering_fingerprint,
    })
}

pub fn lower_ability_from_document_path(
    document_path: impl AsRef<Path>,
    object_id: &str,
) -> Result<ObjectEditorLoweredAbilityToml, EngineError> {
    let readback = read_mcp_object_document(document_path)?;
    lower_ability_from_object(&readback.document, object_id)
}

fn lowered_ability_from_object(
    object: &EditorObject,
) -> Result<ObjectEditorLoweredAbility, EngineError> {
    Ok(ObjectEditorLoweredAbility {
        object_id: object.id.clone(),
        rawcode: required_property(object, "object.rawcode")?,
        base_object_id: required_property(object, "object.base_id")?,
        display_name: required_property(object, "object.display_name")?,
        fields: build_ability_fields(object)?,
    })
}

pub fn verify_lowered_ability_toml(
    canonical_toml: &str,
    expected_fingerprint: &str,
) -> Result<ObjectEditorLoweredAbilityDocument, EngineError> {
    if expected_fingerprint.trim().is_empty() {
        return Err(object_editor_error(
            "toml.fingerprint",
            "expected fingerprint must be non-empty",
        ));
    }
    let document: ObjectEditorLoweredAbilityDocument =
        toml::from_str(canonical_toml).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("parse lowered object-editor ability TOML failed: {error}"),
        })?;
    validate_lowered_ability_document(&document)?;
    if document.lowering_fingerprint != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual: document.lowering_fingerprint,
        });
    }
    let actual = ability_fingerprint(&document)?;
    if actual != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual,
        });
    }
    Ok(document)
}

fn validate_lowered_ability_document(
    document: &ObjectEditorLoweredAbilityDocument,
) -> Result<(), EngineError> {
    if document.schema_version != OBJECT_EDITOR_ABILITY_TOML_SCHEMA_VERSION {
        return Err(object_editor_error(
            "toml.schema_version",
            "expected ability schema version 1",
        ));
    }
    if document.lowering_fingerprint.trim().is_empty()
        || document.ability.object_id.trim().is_empty()
        || document.ability.rawcode.len() != 4
        || document.ability.base_object_id.trim().is_empty()
        || document.ability.display_name.trim().is_empty()
        || !document.validation.lower_ability_valid
        || !document.validation.range_checked
        || document.provenance.lowering_version != OBJECT_EDITOR_LOWERING_VERSION
    {
        return Err(object_editor_error(
            "toml.required",
            "lowered ability TOML is missing required data",
        ));
    }
    validate_ability_fields(&document.ability.fields)
}

fn ability_fingerprint(
    document: &ObjectEditorLoweredAbilityDocument,
) -> Result<String, EngineError> {
    let mut preimage = document.clone();
    preimage.lowering_fingerprint.clear();
    let text = toml::to_string(&preimage).map_err(|error| EngineError::SerializationFailed {
        format: "toml",
        reason: format!("encode lowered ability fingerprint preimage failed: {error}"),
    })?;
    Ok(litd_hash::fnv1a64_hex(text.as_bytes()))
}

pub fn write_lowered_ability_toml(
    path: impl AsRef<Path>,
    artifact: &ObjectEditorLoweredAbilityToml,
) -> Result<ObjectEditorAbilityTomlReadback, EngineError> {
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
    read_lowered_ability_toml(path, &artifact.fingerprint)
}

pub fn read_lowered_ability_toml(
    path: impl AsRef<Path>,
    expected_fingerprint: &str,
) -> Result<ObjectEditorAbilityTomlReadback, EngineError> {
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
    let document = verify_lowered_ability_toml(&text, expected_fingerprint)?;
    Ok(ObjectEditorAbilityTomlReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: litd_hash::sha256_hex(&bytes),
        fingerprint: expected_fingerprint.to_string(),
        document,
    })
}
