//! #822 commerce-record lowering: lower a persisted recipe / shop / creep-drop-table object to
//! a fingerprinted litd/data TOML artifact and re-read it from disk with fingerprint
//! verification. Cross-record validation (dangling item refs, recipe self-reference/cycles,
//! drop-weight sums) runs at lowering time against the WHOLE document (see
//! `object_editor_shop_fields`). Mirrors the #818 unit / #821 item lowering surfaces.
//!
//! The lowered commerce record is serialized as a flat struct with a `record_kind` tag and
//! three optional sub-tables (recipe/shop/drop_table) rather than a serde-tagged enum, because
//! the `toml` encoder is table-oriented and this keeps the artifact unambiguous and re-parsable.

use std::{fs, path::Path};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::{history::EditorDocument, mcp_object_tools::read_mcp_object_document};

use super::document::ObjectEditorLoweringProvenance;
use super::shop_fields::{
    COMMERCE_KINDS, CommerceRecord, DropTableRecord, RecipeRecord, ShopRecord,
    build_commerce_record, item_id_set, recipe_graph_from_document, validate_commerce_cross_record,
    validate_commerce_shape,
};
use super::validation::{invalid_object_editor, object_editor_error, required_property};

pub const OBJECT_EDITOR_SHOP_TOML_SCHEMA_VERSION: u32 = 1;
pub const OBJECT_EDITOR_SHOP_LOWERING_VERSION: &str = "litd-editor.object_editor.shop.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredShopDocument {
    pub schema_version: u32,
    pub lowering_fingerprint: String,
    pub commerce: ObjectEditorLoweredCommerce,
    pub validation: ObjectEditorShopLoweringValidation,
    pub provenance: ObjectEditorLoweringProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredCommerce {
    pub object_id: String,
    pub rawcode: String,
    pub base_object_id: String,
    pub display_name: String,
    pub record_kind: String,
    pub recipe: Option<RecipeRecord>,
    pub shop: Option<ShopRecord>,
    pub drop_table: Option<DropTableRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorShopLoweringValidation {
    pub cross_record_valid: bool,
    pub resolved_item_ref_count: usize,
    pub record_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorLoweredShopToml {
    pub canonical_toml: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectEditorShopTomlReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub fingerprint: String,
    pub document: ObjectEditorLoweredShopDocument,
}

impl ObjectEditorLoweredCommerce {
    fn from_record(
        object_id: String,
        rawcode: String,
        base_object_id: String,
        display_name: String,
        record: &CommerceRecord,
    ) -> Self {
        let (recipe, shop, drop_table) = match record {
            CommerceRecord::Recipe(recipe) => (Some(recipe.clone()), None, None),
            CommerceRecord::Shop(shop) => (None, Some(shop.clone()), None),
            CommerceRecord::DropTable(table) => (None, None, Some(table.clone())),
        };
        Self {
            object_id,
            rawcode,
            base_object_id,
            display_name,
            record_kind: record.kind_str().to_string(),
            recipe,
            shop,
            drop_table,
        }
    }

    fn to_record(&self) -> Result<CommerceRecord, EngineError> {
        match (
            self.record_kind.as_str(),
            &self.recipe,
            &self.shop,
            &self.drop_table,
        ) {
            ("recipe", Some(recipe), None, None) => Ok(CommerceRecord::Recipe(recipe.clone())),
            ("shop", None, Some(shop), None) => Ok(CommerceRecord::Shop(shop.clone())),
            ("drop_table", None, None, Some(table)) => Ok(CommerceRecord::DropTable(table.clone())),
            _ => invalid_object_editor(
                "shop.toml.record_kind",
                "record_kind does not match exactly one populated commerce sub-record",
            ),
        }
    }
}

pub fn lower_shop_from_document_path(
    document_path: impl AsRef<Path>,
    object_id: &str,
) -> Result<ObjectEditorLoweredShopToml, EngineError> {
    let readback = read_mcp_object_document(document_path)?;
    lower_shop_from_document(&readback.document, object_id)
}

pub fn lower_shop_from_document(
    document: &EditorDocument,
    object_id: &str,
) -> Result<ObjectEditorLoweredShopToml, EngineError> {
    let object = document.objects.get(object_id).ok_or_else(|| {
        object_editor_error("shop.object_id", format!("missing object {object_id}"))
    })?;
    if !COMMERCE_KINDS.contains(&object.kind.as_str()) {
        return invalid_object_editor(
            "shop.kind",
            format!(
                "object {object_id} is kind {} not one of {COMMERCE_KINDS:?}",
                object.kind
            ),
        );
    }
    let record = build_commerce_record(object)?;
    // Cross-record validation over the WHOLE document -- dangling refs / cycles fail loud here.
    let item_ids = item_id_set(document);
    let recipe_graph = recipe_graph_from_document(document);
    let resolved = validate_commerce_cross_record(&record, &item_ids, &recipe_graph)?;

    let commerce = ObjectEditorLoweredCommerce::from_record(
        object.id.clone(),
        required_property(object, "object.rawcode")?,
        required_property(object, "object.base_id")?,
        required_property(object, "object.display_name")?,
        &record,
    );
    let mut lowered = ObjectEditorLoweredShopDocument {
        schema_version: OBJECT_EDITOR_SHOP_TOML_SCHEMA_VERSION,
        lowering_fingerprint: String::new(),
        validation: ObjectEditorShopLoweringValidation {
            cross_record_valid: true,
            resolved_item_ref_count: resolved,
            record_kind: record.kind_str().to_string(),
        },
        provenance: ObjectEditorLoweringProvenance {
            source_document_id: document.calyx_document_id.clone(),
            source_revision_id: document.revision_id.clone(),
            source_object_id: object_id.to_string(),
            lowering_version: OBJECT_EDITOR_SHOP_LOWERING_VERSION.to_string(),
        },
        commerce,
    };
    lowered.lowering_fingerprint = lowered_shop_fingerprint(&lowered)?;
    let canonical_toml =
        toml::to_string(&lowered).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("encode lowered commerce TOML failed: {error}"),
        })?;
    Ok(ObjectEditorLoweredShopToml {
        canonical_toml,
        fingerprint: lowered.lowering_fingerprint,
    })
}

pub fn write_lowered_shop_toml(
    path: impl AsRef<Path>,
    artifact: &ObjectEditorLoweredShopToml,
) -> Result<ObjectEditorShopTomlReadback, EngineError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            object_editor_error(
                "shop.toml.parent",
                format!("create {}: {error}", parent.display()),
            )
        })?;
    }
    fs::write(path, artifact.canonical_toml.as_bytes()).map_err(|error| {
        object_editor_error(
            "shop.toml.write",
            format!("write {}: {error}", path.display()),
        )
    })?;
    read_lowered_shop_toml(path, &artifact.fingerprint)
}

pub fn read_lowered_shop_toml(
    path: impl AsRef<Path>,
    expected_fingerprint: &str,
) -> Result<ObjectEditorShopTomlReadback, EngineError> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|error| {
        object_editor_error(
            "shop.toml.read",
            format!("read {}: {error}", path.display()),
        )
    })?;
    let text = String::from_utf8(bytes.clone()).map_err(|error| {
        object_editor_error(
            "shop.toml.utf8",
            format!("{} is not UTF-8: {error}", path.display()),
        )
    })?;
    let document = verify_lowered_shop_toml(&text, expected_fingerprint)?;
    Ok(ObjectEditorShopTomlReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: litd_hash::sha256_hex(&bytes),
        fingerprint: expected_fingerprint.to_string(),
        document,
    })
}

pub fn verify_lowered_shop_toml(
    canonical_toml: &str,
    expected_fingerprint: &str,
) -> Result<ObjectEditorLoweredShopDocument, EngineError> {
    if expected_fingerprint.trim().is_empty() {
        return invalid_object_editor(
            "shop.toml.fingerprint",
            "expected fingerprint must be non-empty",
        );
    }
    let document: ObjectEditorLoweredShopDocument =
        toml::from_str(canonical_toml).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("parse lowered commerce TOML failed: {error}"),
        })?;
    validate_lowered_shop_document(&document)?;
    if document.lowering_fingerprint != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual: document.lowering_fingerprint,
        });
    }
    let actual = lowered_shop_fingerprint(&document)?;
    if actual != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual,
        });
    }
    Ok(document)
}

fn validate_lowered_shop_document(
    document: &ObjectEditorLoweredShopDocument,
) -> Result<(), EngineError> {
    if document.schema_version != OBJECT_EDITOR_SHOP_TOML_SCHEMA_VERSION {
        return invalid_object_editor("shop.toml.schema_version", "expected schema version 1");
    }
    if document.lowering_fingerprint.trim().is_empty()
        || document.commerce.object_id.trim().is_empty()
        || document.commerce.rawcode.len() != 4
        || document.commerce.base_object_id.trim().is_empty()
        || document.commerce.display_name.trim().is_empty()
        || !document.validation.cross_record_valid
        || document.provenance.lowering_version != OBJECT_EDITOR_SHOP_LOWERING_VERSION
    {
        return invalid_object_editor(
            "shop.toml.required",
            "lowered commerce TOML is missing required data",
        );
    }
    let record = document.commerce.to_record()?;
    if record.kind_str() != document.commerce.record_kind
        || record.kind_str() != document.validation.record_kind
    {
        return invalid_object_editor(
            "shop.toml.record_kind",
            "record_kind is inconsistent across the lowered commerce document",
        );
    }
    validate_commerce_shape(&record)
}

fn lowered_shop_fingerprint(
    document: &ObjectEditorLoweredShopDocument,
) -> Result<String, EngineError> {
    let mut preimage = document.clone();
    preimage.lowering_fingerprint.clear();
    let text = toml::to_string(&preimage).map_err(|error| EngineError::SerializationFailed {
        format: "toml",
        reason: format!("encode lowered commerce fingerprint preimage failed: {error}"),
    })?;
    Ok(litd_hash::fnv1a64_hex(text.as_bytes()))
}
