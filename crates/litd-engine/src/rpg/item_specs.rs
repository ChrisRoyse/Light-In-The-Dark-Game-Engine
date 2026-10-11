//! Validation + readback for the core item data model (#1682/#1683/#1684).
//!
//! `validate_item_spec` is the loud-fail gate (structured
//! `LITD-RPG-STATS-INVALID-CONFIG`, never a silent skip, #1348 doctrine). It
//! composes the module-level validators: core identity/level/weight/material/
//! flags, the 5-channel AC + weapon-dice cross-checks against the kind payload
//! (#1682/#1684), the wear-slot + racial-gating form check (#1683), the
//! kind-specific payload check (#1684), the affect list, and the identify
//! reveal split.

use std::collections::BTreeSet;

use litd_hash::sha256_hex;

use crate::EngineError;

use super::item_affects::validate_item_affects;
use super::item_flags::validate_item_flags;
use super::item_kinds::{ItemKind, validate_item_kind_payload};
use super::item_materials::validate_item_material;
use super::item_specs_types::*;
use super::item_weapons::validate_weapon_profile;
use super::item_wear_slots::{WearSlot, validate_wear_gating};

pub fn item_spec_readback(spec: &ItemSpec) -> Result<ItemSpecReadback, EngineError> {
    validate_item_spec(spec)?;
    Ok(ItemSpecReadback {
        schema: spec.schema.clone(),
        item_id: spec.item_id.clone(),
        display_name: spec.display_name.clone(),
        source_area_id: spec.source_area_id.clone(),
        kind: spec.kind_payload.kind().as_id().to_string(),
        wear_slot: spec.wear.slot.as_id().to_string(),
        gating_race_count: spec.wear.gating_race_ids.len(),
        level: spec.level,
        weight_total_ounces: spec.weight.total_ounces(),
        value_coin: spec.value_coin,
        material: spec.material.clone(),
        flag_count: spec.flags.len(),
        has_armor: spec.armor.is_some(),
        has_weapon: spec.weapon.is_some(),
        affect_count: spec.affects.len(),
        breakage_prone: spec.condition.breakage_prone,
        restrung: spec.restring.is_some(),
        identify_reveal_count: spec.identify.identify_reveals.len(),
        deep_lore_reveal_count: spec.identify.deep_lore_reveals.len(),
        source_issue: spec.source_issue,
        item_spec_sha256: spec_hash(spec)?,
    })
}

pub fn validate_item_spec(spec: &ItemSpec) -> Result<(), EngineError> {
    if spec.schema != RPG_ITEM_SPEC_SCHEMA {
        return invalid("schema", format!("expected {RPG_ITEM_SPEC_SCHEMA}"));
    }
    validate_token("item_id", &spec.item_id)?;
    validate_text("display_name", &spec.display_name)?;
    if spec.keywords.is_empty() {
        return invalid("keywords", "at least one keyword is required");
    }
    for keyword in &spec.keywords {
        validate_text("keywords", keyword)?;
    }
    validate_token("source_area_id", &spec.source_area_id)?;
    validate_text("source_area_name", &spec.source_area_name)?;
    if spec.source_issue == 0 {
        return invalid("source_issue", "must be non-zero");
    }
    if spec.canon_issues.is_empty() || !spec.canon_issues.contains(&spec.source_issue) {
        return invalid("canon_issues", "must include the source issue");
    }
    if !spec
        .mechanics_source_url
        .starts_with("https://carrionfields.net/")
    {
        return invalid(
            "mechanics_source_url",
            "must point to the approved CF factual mechanics source",
        );
    }
    if spec.level == 0 || spec.level > 100 {
        return invalid("level", "must be inside 1..=100");
    }
    if spec.weight.ounces > 15 {
        return invalid("weight.ounces", "must be inside 0..=15 (16 oz per pound)");
    }
    // Material (#1685): must be in the closed CF itemsearch vocabulary.
    validate_item_material("material", &spec.material)?;
    // Flags (#1685): resolved to the typed CF flag vocabulary, failing loud on
    // any unknown flag, duplicate, or contradictory gate combination.
    validate_item_flags("flags", &spec.flags)?;

    // Wear-slot + racial-gating form check (#1683).
    validate_wear_gating(&spec.wear)?;
    // Kind-specific payload check (#1684).
    validate_item_kind_payload(&spec.kind_payload)?;
    // Cross-field checks that need the whole spec (#1682/#1684).
    validate_kind_cross_fields(spec)?;

    validate_item_affects("affects", &spec.affects)?;
    validate_identify(&spec.identify)?;
    if let Some(restring) = &spec.restring {
        validate_text("restring.restrung_name", &restring.restrung_name)?;
        validate_text(
            "restring.restrung_description",
            &restring.restrung_description,
        )?;
        validate_text("restring.reward_source", &restring.reward_source)?;
    }
    if spec.implementation_notes.is_empty() {
        return invalid("implementation_notes", "must include source notes");
    }
    Ok(())
}

/// Cross-field kind consistency: a weapon payload requires a weapon profile (and
/// no armour); an armour payload requires the 5-channel AC (and no weapon dice).
/// Non-combat kinds must carry neither. Fails LOUD on any mismatch.
fn validate_kind_cross_fields(spec: &ItemSpec) -> Result<(), EngineError> {
    let kind = spec.kind_payload.kind();
    match kind {
        ItemKind::Weapon => {
            let Some(weapon) = &spec.weapon else {
                return invalid("weapon", "a weapon item must carry a weapon profile");
            };
            validate_weapon_profile(weapon)?;
            if spec.armor.is_some() {
                return invalid("armor", "a weapon item must not carry armor class");
            }
            if !matches!(spec.wear.slot, WearSlot::Wield | WearSlot::Hold) {
                return invalid("wear.slot", "a weapon must be wielded or held");
            }
        }
        ItemKind::Armor => {
            if spec.armor.is_none() {
                return invalid("armor", "an armor item must carry 5-channel armor class");
            }
            if spec.weapon.is_some() {
                return invalid("weapon", "an armor item must not carry weapon dice");
            }
        }
        _ => {
            if spec.weapon.is_some() {
                return invalid(
                    "weapon",
                    format!("a {} item must not carry weapon dice", kind.as_id()),
                );
            }
            if spec.armor.is_some() {
                return invalid(
                    "armor",
                    format!("a {} item must not carry armor class", kind.as_id()),
                );
            }
        }
    }
    // The payload's own kind tag and the ItemKindPayload discriminant always
    // agree by construction; this asserts the invariant for defence in depth.
    debug_assert_eq!(spec.kind_payload.kind(), kind);
    Ok(())
}

/// The identify reveal split must reveal SOMETHING via identify, must not list
/// the same property in both tiers (a property is either identify-visible or
/// deep-lore-hidden, not both), and must not duplicate within a tier.
fn validate_identify(identify: &IdentifyReveal) -> Result<(), EngineError> {
    if identify.identify_reveals.is_empty() {
        return invalid(
            "identify.identify_reveals",
            "identify must reveal at least one property (helpfile 430)",
        );
    }
    let mut identify_set = BTreeSet::new();
    for property in &identify.identify_reveals {
        if !identify_set.insert(property.as_id()) {
            return invalid(
                "identify.identify_reveals",
                format!("duplicate identify property {}", property.as_id()),
            );
        }
    }
    let mut deep_set = BTreeSet::new();
    for property in &identify.deep_lore_reveals {
        if !deep_set.insert(property.as_id()) {
            return invalid(
                "identify.deep_lore_reveals",
                format!("duplicate deep-lore property {}", property.as_id()),
            );
        }
        if identify_set.contains(property.as_id()) {
            return invalid(
                "identify.deep_lore_reveals",
                format!(
                    "property {} cannot be both identify-visible and deep-lore-hidden",
                    property.as_id()
                ),
            );
        }
    }
    Ok(())
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid(field, "must be non-empty and <= 96 bytes");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash",
        );
    }
    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    Ok(())
}

fn spec_hash(spec: &ItemSpec) -> Result<String, EngineError> {
    serde_json::to_vec(spec)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize item spec: {error}"),
        })
}

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(EngineError::RpgStatsInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    })
}
