//! #821 item-record lowering: lower a persisted WC3 item object to a fingerprinted litd/data
//! TOML artifact and re-read it from disk with fingerprint verification. Mirrors the #818 unit
//! lowering surface (`object_editor` + `object_editor_document`) for the item field set.

use std::{fs, path::Path};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::{history::EditorDocument, mcp_object_tools::read_mcp_object_document};

use super::document::ObjectEditorLoweringProvenance;
use super::item_fields::{ItemWc3Fields, build_item_fields, validate_item_fields};
use super::validation::{invalid_object_editor, object_editor_error, required_property};

pub const OBJECT_EDITOR_ITEM_TOML_SCHEMA_VERSION: u32 = 1;
pub const OBJECT_EDITOR_ITEM_LOWERING_VERSION: &str = "litd-editor.object_editor.item.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredItemDocument {
    pub schema_version: u32,
    pub lowering_fingerprint: String,
    pub item: ObjectEditorLoweredItem,
    pub provenance: ObjectEditorLoweringProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredItem {
    pub object_id: String,
    pub rawcode: String,
    pub base_object_id: String,
    pub display_name: String,
    pub wc3: ItemWc3Fields,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredItemToml {
    pub canonical_toml: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorItemTomlReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub fingerprint: String,
    pub document: ObjectEditorLoweredItemDocument,
}

/// Best-effort resolution of item-ability refs against `Ability`-kind records in the same
/// document (see the #821 contract in `object_editor_item_fields`): reported, never fatal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorItemAbilityResolution {
    pub active_total: usize,
    pub active_resolved: usize,
    pub passive_total: usize,
    pub passive_resolved: usize,
    pub unresolved: Vec<String>,
}

/// Result of lowering an item from a document: the fingerprinted TOML plus the (non-fatal)
/// ability-ref resolution summary the tool surfaces to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectEditorItemLowering {
    pub toml: ObjectEditorLoweredItemToml,
    pub ability_resolution: ObjectEditorItemAbilityResolution,
}

pub fn lower_item_from_document_path(
    document_path: impl AsRef<Path>,
    object_id: &str,
) -> Result<ObjectEditorItemLowering, EngineError> {
    let readback = read_mcp_object_document(document_path)?;
    lower_item_from_document(&readback.document, object_id)
}

pub fn lower_item_from_document(
    document: &EditorDocument,
    object_id: &str,
) -> Result<ObjectEditorItemLowering, EngineError> {
    let object = document.objects.get(object_id).ok_or_else(|| {
        object_editor_error("item.object_id", format!("missing object {object_id}"))
    })?;
    if object.kind != "Item" {
        return invalid_object_editor(
            "item.kind",
            format!("object {object_id} is kind {} not Item", object.kind),
        );
    }
    let item = ObjectEditorLoweredItem {
        object_id: object.id.clone(),
        rawcode: required_property(object, "object.rawcode")?,
        base_object_id: required_property(object, "object.base_id")?,
        display_name: required_property(object, "object.display_name")?,
        wc3: build_item_fields(object)?,
    };
    let mut lowered = ObjectEditorLoweredItemDocument {
        schema_version: OBJECT_EDITOR_ITEM_TOML_SCHEMA_VERSION,
        lowering_fingerprint: String::new(),
        provenance: ObjectEditorLoweringProvenance {
            source_document_id: document.calyx_document_id.clone(),
            source_revision_id: document.revision_id.clone(),
            source_object_id: object_id.to_string(),
            lowering_version: OBJECT_EDITOR_ITEM_LOWERING_VERSION.to_string(),
        },
        item,
    };
    lowered.lowering_fingerprint = lowered_item_fingerprint(&lowered)?;
    let canonical_toml =
        toml::to_string(&lowered).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("encode lowered item TOML failed: {error}"),
        })?;
    let ability_resolution = resolve_item_ability_refs(document, &lowered.item.wc3);
    Ok(ObjectEditorItemLowering {
        toml: ObjectEditorLoweredItemToml {
            canonical_toml,
            fingerprint: lowered.lowering_fingerprint,
        },
        ability_resolution,
    })
}

fn resolve_item_ability_refs(
    document: &EditorDocument,
    wc3: &ItemWc3Fields,
) -> ObjectEditorItemAbilityResolution {
    let is_ability = |id: &String| {
        document
            .objects
            .get(id)
            .map(|object| object.kind == "Ability")
            .unwrap_or(false)
    };
    let active_resolved = wc3
        .abilities
        .active
        .iter()
        .filter(|id| is_ability(id))
        .count();
    let passive_resolved = wc3
        .abilities
        .passive
        .iter()
        .filter(|id| is_ability(id))
        .count();
    let mut unresolved = Vec::new();
    for id in wc3
        .abilities
        .active
        .iter()
        .chain(wc3.abilities.passive.iter())
    {
        if !is_ability(id) {
            unresolved.push(id.clone());
        }
    }
    ObjectEditorItemAbilityResolution {
        active_total: wc3.abilities.active.len(),
        active_resolved,
        passive_total: wc3.abilities.passive.len(),
        passive_resolved,
        unresolved,
    }
}

pub fn write_lowered_item_toml(
    path: impl AsRef<Path>,
    artifact: &ObjectEditorLoweredItemToml,
) -> Result<ObjectEditorItemTomlReadback, EngineError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            object_editor_error(
                "item.toml.parent",
                format!("create {}: {error}", parent.display()),
            )
        })?;
    }
    fs::write(path, artifact.canonical_toml.as_bytes()).map_err(|error| {
        object_editor_error(
            "item.toml.write",
            format!("write {}: {error}", path.display()),
        )
    })?;
    read_lowered_item_toml(path, &artifact.fingerprint)
}

pub fn read_lowered_item_toml(
    path: impl AsRef<Path>,
    expected_fingerprint: &str,
) -> Result<ObjectEditorItemTomlReadback, EngineError> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|error| {
        object_editor_error(
            "item.toml.read",
            format!("read {}: {error}", path.display()),
        )
    })?;
    let text = String::from_utf8(bytes.clone()).map_err(|error| {
        object_editor_error(
            "item.toml.utf8",
            format!("{} is not UTF-8: {error}", path.display()),
        )
    })?;
    let document = verify_lowered_item_toml(&text, expected_fingerprint)?;
    Ok(ObjectEditorItemTomlReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: litd_hash::sha256_hex(&bytes),
        fingerprint: expected_fingerprint.to_string(),
        document,
    })
}

pub fn verify_lowered_item_toml(
    canonical_toml: &str,
    expected_fingerprint: &str,
) -> Result<ObjectEditorLoweredItemDocument, EngineError> {
    if expected_fingerprint.trim().is_empty() {
        return invalid_object_editor(
            "item.toml.fingerprint",
            "expected fingerprint must be non-empty",
        );
    }
    let document: ObjectEditorLoweredItemDocument =
        toml::from_str(canonical_toml).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("parse lowered item TOML failed: {error}"),
        })?;
    validate_lowered_item_document(&document)?;
    if document.lowering_fingerprint != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual: document.lowering_fingerprint,
        });
    }
    let actual = lowered_item_fingerprint(&document)?;
    if actual != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual,
        });
    }
    Ok(document)
}

fn validate_lowered_item_document(
    document: &ObjectEditorLoweredItemDocument,
) -> Result<(), EngineError> {
    if document.schema_version != OBJECT_EDITOR_ITEM_TOML_SCHEMA_VERSION {
        return invalid_object_editor("item.toml.schema_version", "expected schema version 1");
    }
    if document.lowering_fingerprint.trim().is_empty()
        || document.item.object_id.trim().is_empty()
        || document.item.rawcode.len() != 4
        || document.item.base_object_id.trim().is_empty()
        || document.item.display_name.trim().is_empty()
        || document.provenance.lowering_version != OBJECT_EDITOR_ITEM_LOWERING_VERSION
    {
        return invalid_object_editor(
            "item.toml.required",
            "lowered item TOML is missing required data",
        );
    }
    validate_item_fields(&document.item.wc3)
}

fn lowered_item_fingerprint(
    document: &ObjectEditorLoweredItemDocument,
) -> Result<String, EngineError> {
    let mut preimage = document.clone();
    preimage.lowering_fingerprint.clear();
    let text = toml::to_string(&preimage).map_err(|error| EngineError::SerializationFailed {
        format: "toml",
        reason: format!("encode lowered item fingerprint preimage failed: {error}"),
    })?;
    Ok(litd_hash::fnv1a64_hex(text.as_bytes()))
}
