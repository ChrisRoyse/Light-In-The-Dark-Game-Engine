//! Object-editor unit lowering and runtime readback for copy/edit workflows.

use std::{fs, path::Path};

use litd_engine::{EngineError, math::Transform};

use crate::{
    history::{EditorDocument, EditorObject},
    mcp_object_tools::{ObjectBaseTemplate, read_mcp_object_document},
};

#[path = "object_editor_ability.rs"]
mod ability;
#[path = "object_editor_ability_fields.rs"]
mod ability_fields;
#[path = "object_editor_document.rs"]
mod document;
#[path = "object_editor_field_extract.rs"]
mod field_extract;
#[path = "object_editor_field_helpers.rs"]
mod field_helpers;
#[path = "object_editor_hero.rs"]
mod hero;
#[path = "object_editor_hero_fields.rs"]
mod hero_fields;
#[path = "object_editor_item.rs"]
mod item;
#[path = "object_editor_item_fields.rs"]
mod item_fields;
#[path = "object_editor_shop.rs"]
mod shop;
#[path = "object_editor_shop_fields.rs"]
mod shop_fields;
#[path = "object_editor_unit_fields.rs"]
mod unit_fields;
#[path = "object_editor_validation.rs"]
mod validation;
// #825 tech-tree requirement records + #827 gameplay constants, each a typed field-set module
// paired with a #829-valve lowering module, mirroring the #818 unit split above.
#[path = "object_editor_constants_fields.rs"]
mod constants_fields;
#[path = "object_editor_constants_lowering.rs"]
mod constants_lowering;
#[path = "object_editor_tech_fields.rs"]
mod tech_fields;
#[path = "object_editor_tech_lowering.rs"]
mod tech_lowering;

// #823/#824/#826/#828 p2 object-data editors: each lives in its own field-set module and
// lowers through the shared generic #829 valve in `lower`. Mounted here so `super` resolves to
// `object_editor` (matching the #818 unit split) and the field modules can reach `unit_fields`
// (canonical `WC3_TARGET_FLAGS`), `validation` (typed error), and `lower` (parse helpers).
#[path = "object_editor_buff_fields.rs"]
mod buff_fields;
#[path = "object_editor_interface_fields.rs"]
mod interface_fields;
#[path = "object_editor_lower.rs"]
pub mod lower;
#[path = "object_editor_p2_lowering.rs"]
mod p2_lowering;
#[path = "object_editor_scatter_fields.rs"]
mod scatter_fields;
#[path = "object_editor_upgrade_fields.rs"]
mod upgrade_fields;

// #1466: data model + version constants live in `document`; validation/property/lowering helpers
// live in `validation`. Re-export the data model so every existing `object_editor::ObjectEditor*`
// path is unchanged; bring the crate-private helpers into scope so the public API below calls them
// by name exactly as before.
pub use ability::*;
pub use ability_fields::{
    ABILITY_AOE_MAX, ABILITY_COOLDOWN_MAX, ABILITY_DURATION_MAX, ABILITY_KINDS, ABILITY_LEVELS_MAX,
    ABILITY_LEVELS_MIN, ABILITY_MANA_MAX, ABILITY_PROPERTY_KEYS, ABILITY_RANGE_MAX,
    ABILITY_SCALE_MAX, ABILITY_TARGET_TYPES, AbilityFields, AbilityScaling,
};
pub use document::*;
pub use hero::*;
pub use hero_fields::{
    HERO_ATTR_BASE_MAX, HERO_ATTR_GAIN_MILLI_MAX, HERO_FLAGS, HERO_LEVEL_MIN, HERO_PRIMARY_ATTRS,
    HERO_PROPERTY_KEYS, HERO_REVIVE_COST_MAX, HERO_REVIVE_TIME_MAX, HeroFields, HeroLearnableSlot,
};
// #818 unit WC3 numeric bounds: single source for persist-time MCP validation (#1837).
// Enum/flag sets stay re-exported via `document::*` (WC3_*).
pub use unit_fields::{
    UNIT_ARMOR_MAX, UNIT_ATTACK_COOLDOWN_MS_MAX, UNIT_ATTACK_COOLDOWN_MS_MIN,
    UNIT_ATTACK_RANGE_MAX, UNIT_BUILD_TIME_SECS_MAX, UNIT_DAMAGE_BASE_MAX, UNIT_DAMAGE_DICE_MAX,
    UNIT_DAMAGE_DICE_MIN, UNIT_FOOD_COST_MAX, UNIT_GOLD_COST_MAX, UNIT_LUMBER_COST_MAX,
    UNIT_MANA_MAX, UNIT_SCALE_PCT_MAX, UNIT_SCALE_PCT_MIN, UNIT_TURN_RATE_PCT_MAX,
};
// #821 item authoring surface + #822 commerce (recipe/shop/drop-table) authoring surface,
// re-exported so `crate::object_editor::*` paths mirror the #818 unit surface. The enum/flag/
// range constant sets are the SINGLE SOURCE consumed by BOTH persist-time validation
// (`mcp_object_tools_validation`) and lowering-time validation.
pub use item::*;
pub use item_fields::{
    ITEM_BONUS_ARMOR_MAX, ITEM_BONUS_ATTACK_MAX, ITEM_BONUS_ATTR_MAX, ITEM_BONUS_HP_MAX,
    ITEM_BONUS_MANA_MAX, ITEM_CHARGES_MAX, ITEM_CLASSES, ITEM_COST_MAX, ITEM_DROP_ON_DEATH,
    ITEM_INTERVAL_MAX, ITEM_INVENTORY_FLAGS, ITEM_PICKUP_RANGE_MAX, ITEM_PROPERTY_KEYS,
    ITEM_STOCK_MAX, ItemAbilities, ItemArt, ItemBonuses, ItemCharges, ItemClassification,
    ItemEconomy, ItemPickupDrop, ItemStock, ItemWc3Fields,
};
pub use shop::*;
pub use shop_fields::{
    COMMERCE_KINDS, COMMERCE_PROPERTY_KEYS, CommerceRecord, DROP_MAX_ENTRIES, DROP_WEIGHT_MAX,
    DROP_WEIGHT_SUM_MAX, DropEntry, DropTableRecord, RECIPE_MAX_COMPONENTS, RecipeRecord,
    SHOP_INTERVAL_MAX, SHOP_MAX_STOCK_ITEMS, SHOP_STOCK_MAX, ShopRecord,
};
use validation::*;
// Re-export the tech + constants field/lowering surfaces so callers reach them at
// `crate::object_editor::*` exactly like the unit surface.
pub use constants_fields::{
    CONVERSION_RATE_MAX, CYCLE_LEN_MAX, CYCLE_LEN_MIN, DROP_RATE_PCT_MAX, FOOD_CAP_MAX,
    GAMEPLAY_CONSTANT_PROPERTY_KEYS, HERO_LEVEL_MAX, LoweredGameplayConstants, REVIVE_GOLD_MAX,
    REVIVE_SECS_MAX, XP_GAIN_BASE_MAX, XP_GAIN_PCT_MAX, XP_MAX_LEVELS, XP_THRESHOLD_MAX,
    gameplay_constants_base_properties,
};
pub use constants_lowering::{
    ObjectEditorConstantsReadback, ObjectEditorLoweredConstantsDocument,
    ObjectEditorLoweredConstantsToml, lower_constants_from_document,
    lower_constants_from_document_path, read_lowered_constants_toml, verify_lowered_constants_toml,
    write_lowered_constants_toml,
};
pub use tech_fields::{
    LoweredTechTree, TECH_MAX_EQUIVALENTS, TECH_MAX_REQUIRES, TECH_PROPERTY_KEYS, TECH_TIER_MAX,
    TechRequirementRecord,
};
pub use tech_lowering::{
    ObjectEditorLoweredTechDocument, ObjectEditorLoweredTechToml, ObjectEditorTechReadback,
    lower_tech_from_document, lower_tech_from_document_path, read_lowered_tech_toml,
    verify_lowered_tech_toml, write_lowered_tech_toml,
};

// Re-export the p2 editors' canonical constant sets (the single source of truth shared with
// persist-time validation), lowered document types, and public lowering entry points.
pub use buff_fields::{
    BUFF_AURA_RADIUS_MAX, BUFF_DURATION_SECS_MAX, BuffRecord, ObjectEditorLoweredBuffDocument,
    WC3_BUFF_DURATION_SEMANTICS, WC3_BUFF_POLARITIES, WC3_BUFF_PROPERTY_KEYS, WC3_BUFF_STACK_RULES,
};
pub use interface_fields::{
    CommandCardCell, INTERFACE_CMD_PREFIX, INTERFACE_COLOR_PREFIX, INTERFACE_ICON_PREFIX,
    INTERFACE_SCALAR_PROPERTY_KEYS, INTERFACE_STRING_PREFIX, InterfaceRecord,
    MESSAGE_DURATION_SECS_MAX, MESSAGE_FONT_PCT_MAX, MESSAGE_FONT_PCT_MIN,
    ObjectEditorLoweredInterfaceDocument, parse_cell, validate_hex_color,
};
pub use p2_lowering::{
    lower_buff_from_document_path, lower_interface_from_document_path,
    lower_scatter_from_document_path, lower_upgrade_from_document_path, read_lowered_buff_toml,
    read_lowered_interface_toml, read_lowered_scatter_toml, read_lowered_upgrade_toml,
    write_lowered_buff_toml, write_lowered_interface_toml, write_lowered_scatter_toml,
    write_lowered_upgrade_toml,
};
pub use scatter_fields::{
    DESTRUCTIBLE_HP_MAX, DOODAD_SCALE_MILLI_MAX, DOODAD_VARIATIONS_MAX, DestructibleTypeRecord,
    DoodadTypeRecord, ObjectEditorLoweredScatterDocument, WC3_DESTRUCTIBLE_ARMOR_TYPES,
    WC3_DESTRUCTIBLE_KINDS, WC3_DESTRUCTIBLE_PROPERTY_KEYS, WC3_DOODAD_PROPERTY_KEYS,
};
pub use upgrade_fields::{
    ObjectEditorLoweredUpgradeDocument, UPGRADE_COST_MAX, UPGRADE_LEVEL_COUNT_MAX,
    UPGRADE_RESEARCH_SECS_MAX, UpgradeRecord, WC3_UPGRADE_EFFECT_KINDS, WC3_UPGRADE_PROPERTY_KEYS,
};

pub fn issue732_base_document() -> EditorDocument {
    let mut document = EditorDocument::new(
        "calyx://world/issue732-object-editor",
        "issue732-object-editor-rev-1",
    )
    .expect("issue732 base document must validate");
    document
        .upsert_object(
            EditorObject::new("base.unit.footman", "Unit", None, Transform::IDENTITY)
                .with_property("object.rawcode", "uFtm")
                .with_property("object.display_name", "Footman")
                .with_property("stats.hit_points", "420")
                .with_property("stats.move_speed", "270")
                .with_property("art.model", "units/human/Footman/Footman.mdl")
                .with_property("abilities.normal", "ability.issue732.defend"),
        )
        .expect("issue732 base unit must validate");
    document
        .upsert_object(
            EditorObject::new(
                "ability.issue732.defend",
                "Ability",
                None,
                Transform::IDENTITY,
            )
            .with_property("object.rawcode", "Adef")
            .with_property("object.display_name", "Defend")
            .with_property("ability.mana_cost", "0"),
        )
        .expect("issue732 defend ability must validate");
    document
        .upsert_object(
            EditorObject::new(
                "ability.issue732.guardian_leap",
                "Ability",
                None,
                Transform::IDENTITY,
            )
            .with_property("object.rawcode", "Aglp")
            .with_property("object.display_name", "Guardian Leap")
            .with_property("ability.mana_cost", "35"),
        )
        .expect("issue732 leap ability must validate");
    document
}

pub fn object_editor_base_templates(
    document: &EditorDocument,
) -> Result<Vec<ObjectBaseTemplate>, EngineError> {
    let mut templates = document
        .objects
        .values()
        .filter_map(|object| {
            object_data_kind(&object.kind).map(|kind| ObjectBaseTemplate {
                base_object_id: object.id.clone(),
                kind,
                display_name: object
                    .properties
                    .get("object.display_name")
                    .cloned()
                    .unwrap_or_else(|| object.id.clone()),
                properties: editable_properties(object),
            })
        })
        .collect::<Vec<_>>();
    templates.sort_by(|left, right| left.base_object_id.cmp(&right.base_object_id));
    if templates.is_empty() {
        return invalid_object_editor("base_templates", "no supported base objects found");
    }
    Ok(templates)
}

pub fn lower_unit_from_object(
    document: &EditorDocument,
    object_id: &str,
) -> Result<ObjectEditorLoweredUnitToml, EngineError> {
    let object = document.objects.get(object_id).ok_or_else(|| {
        object_editor_error("unit.object_id", format!("missing object {object_id}"))
    })?;
    let validation = validate_unit_object_for_lowering(object);
    if !validation.valid {
        let first = validation
            .fields
            .iter()
            .find(|field| !field.valid)
            .map(|field| format!("{}: {}", field.field, field.message))
            .unwrap_or_else(|| "unit failed LowerUnit validation".to_string());
        return invalid_object_editor("unit.lower", first);
    }
    let unit = lowered_unit_from_object(object)?;
    let mut document = ObjectEditorLoweredUnitDocument {
        schema_version: OBJECT_EDITOR_UNIT_TOML_SCHEMA_VERSION,
        lowering_fingerprint: String::new(),
        validation: ObjectEditorLoweringValidation {
            lower_unit_valid: true,
            range_checked: true,
            ability_count: unit.abilities.len(),
            field_count: 7 + WC3_UNIT_PROPERTY_KEYS.len(),
        },
        provenance: ObjectEditorLoweringProvenance {
            source_document_id: document.calyx_document_id.clone(),
            source_revision_id: document.revision_id.clone(),
            source_object_id: object_id.to_string(),
            lowering_version: OBJECT_EDITOR_LOWERING_VERSION.to_string(),
        },
        unit,
    };
    document.lowering_fingerprint = lowered_unit_fingerprint(&document)?;
    let canonical_toml =
        toml::to_string(&document).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("encode lowered object-editor unit TOML failed: {error}"),
        })?;
    Ok(ObjectEditorLoweredUnitToml {
        canonical_toml,
        fingerprint: document.lowering_fingerprint,
    })
}

pub fn lower_unit_from_document_path(
    document_path: impl AsRef<Path>,
    object_id: &str,
) -> Result<ObjectEditorLoweredUnitToml, EngineError> {
    let readback = read_mcp_object_document(document_path)?;
    lower_unit_from_object(&readback.document, object_id)
}

pub fn verify_lowered_unit_toml(
    canonical_toml: &str,
    expected_fingerprint: &str,
) -> Result<ObjectEditorLoweredUnitDocument, EngineError> {
    if expected_fingerprint.trim().is_empty() {
        return invalid_object_editor("toml.fingerprint", "expected fingerprint must be non-empty");
    }
    let document: ObjectEditorLoweredUnitDocument =
        toml::from_str(canonical_toml).map_err(|error| EngineError::SerializationFailed {
            format: "toml",
            reason: format!("parse lowered object-editor unit TOML failed: {error}"),
        })?;
    validate_lowered_unit_document(&document)?;
    if document.lowering_fingerprint != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual: document.lowering_fingerprint,
        });
    }
    let actual = lowered_unit_fingerprint(&document)?;
    if actual != expected_fingerprint {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: expected_fingerprint.to_string(),
            actual,
        });
    }
    Ok(document)
}

pub fn write_lowered_unit_toml(
    path: impl AsRef<Path>,
    artifact: &ObjectEditorLoweredUnitToml,
) -> Result<ObjectEditorTomlReadback, EngineError> {
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
    read_lowered_unit_toml(path, &artifact.fingerprint)
}

pub fn read_lowered_unit_toml(
    path: impl AsRef<Path>,
    expected_fingerprint: &str,
) -> Result<ObjectEditorTomlReadback, EngineError> {
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
    let document = verify_lowered_unit_toml(&text, expected_fingerprint)?;
    Ok(ObjectEditorTomlReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: litd_hash::sha256_hex(&bytes),
        fingerprint: expected_fingerprint.to_string(),
        document,
    })
}

#[path = "object_editor_unit_runtime.rs"]
mod unit_runtime;
pub use unit_runtime::{
    load_lowered_unit_toml_into_sim, revert_unit_from_toml, validate_unit_object_for_lowering,
};

// #1837 item 3: lowered #826 scatter types -> engine DestructibleSpec / DoodadScatterBrush.
#[path = "object_editor_scatter_runtime.rs"]
mod scatter_runtime;
pub use scatter_runtime::{
    SCATTER_RUNTIME_PROOF_CELL, SCATTER_RUNTIME_PROOF_DENSITY_PERCENT, SCATTER_RUNTIME_PROOF_SEED,
    SCATTER_RUNTIME_PROOF_TILE_X, SCATTER_RUNTIME_PROOF_TILE_Z, SCATTER_RUNTIME_PROOF_YAW_DEG,
    ScatterRuntimeProof, destructible_type_to_runtime_spec, doodad_type_to_runtime_brush,
    load_lowered_scatter_into_runtime,
};

#[cfg(test)]
#[path = "object_editor_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "object_editor_p2_fields_tests.rs"]
mod p2_fields_tests;
