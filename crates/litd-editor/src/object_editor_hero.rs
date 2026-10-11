//! #820 hero lowering + fingerprinted litd/data TOML readback, through the #829 valve. A hero
//! is a unit extension: the lowered document carries the full unit core + WC3 field set (reused
//! from the #818 unit lowering) plus the hero-specific field set. Cross-record validation at
//! lowering time: every learnable slot must reference a PERSISTED ability record of kind
//! `hero-learnable`; a dangling or wrong-kind reference fails lowering with a typed error.

use std::{fs, path::Path};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::hero_fields::{HeroFields, HeroLearnableSlot, build_hero_fields, validate_hero_fields};
use super::unit_fields::{UnitWc3Fields, build_wc3_fields, validate_wc3_fields};
use super::validation::{ability_list, object_editor_error, parse_u32_property, required_property};
use super::{
    OBJECT_EDITOR_LOWERING_VERSION, ObjectEditorLoweringProvenance, UNIT_HIT_POINTS_MAX,
    UNIT_HIT_POINTS_MIN, UNIT_MOVE_SPEED_MAX, UNIT_MOVE_SPEED_MIN,
};
use crate::{
    history::{EditorDocument, EditorObject},
    mcp_object_tools::read_mcp_object_document,
};

pub const OBJECT_EDITOR_HERO_TOML_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredHeroDocument {
    pub schema_version: u32,
    pub lowering_fingerprint: String,
    pub hero: ObjectEditorLoweredHero,
    pub validation: ObjectEditorHeroValidation,
    pub provenance: ObjectEditorLoweringProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredHero {
    pub object_id: String,
    pub rawcode: String,
    pub base_object_id: String,
    pub display_name: String,
    pub hit_points: u32,
    pub move_speed: u32,
    pub model: String,
    pub abilities: Vec<String>,
    pub wc3: UnitWc3Fields,
    pub hero: HeroFields,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorHeroValidation {
    pub lower_hero_valid: bool,
    pub range_checked: bool,
    pub learnable_refs_checked: bool,
    pub learnable_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredHeroToml {
    pub canonical_toml: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorHeroTomlReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub fingerprint: String,
    pub document: ObjectEditorLoweredHeroDocument,
}

pub fn lower_hero_from_object(
    document: &EditorDocument,
    object_id: &str,
) -> Result<ObjectEditorLoweredHeroToml, EngineError> {
    let object = document.objects.get(object_id).ok_or_else(|| {
        object_editor_error("hero.object_id", format!("missing object {object_id}"))
    })?;
    if object.kind != "Hero" {
        return Err(object_editor_error(
            "hero.kind",
            format!("{object_id} is kind {} not Hero", object.kind),
        ));
    }
    let hero = lowered_hero_from_object(object)?;
    // Cross-record validation (#820): learnable slots must reference PERSISTED hero-learnable
    // ability records. This is why lowering reads from the whole persisted document, not memory.
    validate_learnable_refs(document, &hero.hero.learnable)?;
    let mut lowered_document = ObjectEditorLoweredHeroDocument {
        schema_version: OBJECT_EDITOR_HERO_TOML_SCHEMA_VERSION,
        lowering_fingerprint: String::new(),
        validation: ObjectEditorHeroValidation {
            lower_hero_valid: true,
            range_checked: true,
            learnable_refs_checked: true,
            learnable_count: hero.hero.learnable.len(),
        },
        provenance: ObjectEditorLoweringProvenance {
            source_document_id: document.calyx_document_id.clone(),
            source_revision_id: document.revision_id.clone(),
            source_object_id: object_id.to_string(),
            lowering_version: OBJECT_EDITOR_LOWERING_VERSION.to_string(),
        },
        hero,
    };
    lowered_document.lowering_fingerprint = hero_fingerprint(&lowered_document)?;
    let canonical_toml =
        toml::to_string(&lowered_document).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("encode lowered object-editor hero TOML failed: {error}"),
        })?;
    Ok(ObjectEditorLoweredHeroToml {
        canonical_toml,
        fingerprint: lowered_document.lowering_fingerprint,
    })
}

pub fn lower_hero_from_document_path(
    document_path: impl AsRef<Path>,
    object_id: &str,
) -> Result<ObjectEditorLoweredHeroToml, EngineError> {
    let readback = read_mcp_object_document(document_path)?;
    lower_hero_from_object(&readback.document, object_id)
}

fn lowered_hero_from_object(object: &EditorObject) -> Result<ObjectEditorLoweredHero, EngineError> {
    Ok(ObjectEditorLoweredHero {
        object_id: object.id.clone(),
        rawcode: required_property(object, "object.rawcode")?,
        base_object_id: required_property(object, "object.base_id")?,
        display_name: required_property(object, "object.display_name")?,
        hit_points: parse_u32_property(
            object,
            "stats.hit_points",
            UNIT_HIT_POINTS_MIN,
            UNIT_HIT_POINTS_MAX,
        )?,
        move_speed: parse_u32_property(
            object,
            "stats.move_speed",
            UNIT_MOVE_SPEED_MIN,
            UNIT_MOVE_SPEED_MAX,
        )?,
        model: required_property(object, "art.model")?,
        abilities: ability_list(
            object
                .properties
                .get("abilities.normal")
                .map(String::as_str),
        )?,
        wc3: build_wc3_fields(object)?,
        hero: build_hero_fields(object)?,
    })
}

/// A learnable slot must reference a PERSISTED ability record of kind `hero-learnable`.
fn validate_learnable_refs(
    document: &EditorDocument,
    learnable: &[HeroLearnableSlot],
) -> Result<(), EngineError> {
    for slot in learnable {
        let referenced = document.objects.get(&slot.ability_id).ok_or_else(|| {
            object_editor_error(
                "hero.learnable",
                format!(
                    "learnable ability {} is not a persisted object",
                    slot.ability_id
                ),
            )
        })?;
        if referenced.kind != "Ability" {
            return Err(object_editor_error(
                "hero.learnable",
                format!(
                    "learnable ref {} is kind {} not Ability",
                    slot.ability_id, referenced.kind
                ),
            ));
        }
        let kind = referenced
            .properties
            .get("ability.kind")
            .map(String::as_str)
            .unwrap_or("");
        if kind != "hero-learnable" {
            return Err(object_editor_error(
                "hero.learnable",
                format!(
                    "learnable ref {} has ability.kind {kind:?} but must be hero-learnable",
                    slot.ability_id
                ),
            ));
        }
    }
    Ok(())
}

pub fn verify_lowered_hero_toml(
    canonical_toml: &str,
    expected_fingerprint: &str,
) -> Result<ObjectEditorLoweredHeroDocument, EngineError> {
    if expected_fingerprint.trim().is_empty() {
        return Err(object_editor_error(
            "toml.fingerprint",
            "expected fingerprint must be non-empty",
        ));
    }
    let document: ObjectEditorLoweredHeroDocument =
        toml::from_str(canonical_toml).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("parse lowered object-editor hero TOML failed: {error}"),
        })?;
    validate_lowered_hero_document(&document)?;
    if document.lowering_fingerprint != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual: document.lowering_fingerprint,
        });
    }
    let actual = hero_fingerprint(&document)?;
    if actual != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual,
        });
    }
    Ok(document)
}

fn validate_lowered_hero_document(
    document: &ObjectEditorLoweredHeroDocument,
) -> Result<(), EngineError> {
    if document.schema_version != OBJECT_EDITOR_HERO_TOML_SCHEMA_VERSION {
        return Err(object_editor_error(
            "toml.schema_version",
            "expected hero schema version 1",
        ));
    }
    if document.lowering_fingerprint.trim().is_empty()
        || document.hero.object_id.trim().is_empty()
        || document.hero.rawcode.len() != 4
        || document.hero.base_object_id.trim().is_empty()
        || document.hero.display_name.trim().is_empty()
        || document.hero.model.trim().is_empty()
        || !document.validation.lower_hero_valid
        || !document.validation.range_checked
        || !document.validation.learnable_refs_checked
        || document.provenance.lowering_version != OBJECT_EDITOR_LOWERING_VERSION
    {
        return Err(object_editor_error(
            "toml.required",
            "lowered hero TOML is missing required data",
        ));
    }
    if document.hero.hit_points < UNIT_HIT_POINTS_MIN
        || document.hero.hit_points > UNIT_HIT_POINTS_MAX
    {
        return Err(object_editor_error(
            "toml.hero.hit_points",
            "hit_points out of range",
        ));
    }
    if document.hero.move_speed < UNIT_MOVE_SPEED_MIN
        || document.hero.move_speed > UNIT_MOVE_SPEED_MAX
    {
        return Err(object_editor_error(
            "toml.hero.move_speed",
            "move_speed out of range",
        ));
    }
    validate_wc3_fields(&document.hero.wc3)?;
    validate_hero_fields(&document.hero.hero)
}

fn hero_fingerprint(document: &ObjectEditorLoweredHeroDocument) -> Result<String, EngineError> {
    let mut preimage = document.clone();
    preimage.lowering_fingerprint.clear();
    let text = toml::to_string(&preimage).map_err(|error| EngineError::SerializationFailed {
        format: "toml",
        reason: format!("encode lowered hero fingerprint preimage failed: {error}"),
    })?;
    Ok(litd_hash::fnv1a64_hex(text.as_bytes()))
}

pub fn write_lowered_hero_toml(
    path: impl AsRef<Path>,
    artifact: &ObjectEditorLoweredHeroToml,
) -> Result<ObjectEditorHeroTomlReadback, EngineError> {
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
    read_lowered_hero_toml(path, &artifact.fingerprint)
}

pub fn read_lowered_hero_toml(
    path: impl AsRef<Path>,
    expected_fingerprint: &str,
) -> Result<ObjectEditorHeroTomlReadback, EngineError> {
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
    let document = verify_lowered_hero_toml(&text, expected_fingerprint)?;
    Ok(ObjectEditorHeroTomlReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: litd_hash::sha256_hex(&bytes),
        fingerprint: expected_fingerprint.to_string(),
        document,
    })
}
