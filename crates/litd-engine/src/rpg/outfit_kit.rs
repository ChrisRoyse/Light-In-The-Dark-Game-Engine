//! Validation + readback for the CF newbie OUTFIT kit (#1687).
//!
//! `validate_outfit_kit` is the loud-fail gate (structured
//! `LITD-RPG-STATS-INVALID-CONFIG`): band/guild-gating consistency, the gear
//! slot + material tokens (against the #1683/#1685 vocabularies), the
//! lowbie-only map-grant rule (helpfile 314), and the breakage/band invariant.

use litd_hash::sha256_hex;

use crate::EngineError;

use super::item_materials::validate_item_material;
use super::item_wear_slots::WearSlot;
use super::outfit_kit_types::*;

pub fn outfit_kit_readback(spec: &OutfitKitSpec) -> Result<OutfitKitReadback, EngineError> {
    validate_outfit_kit(spec)?;
    let (low, high) = spec.band.level_window();
    Ok(OutfitKitReadback {
        schema: spec.schema.clone(),
        outfit_id: spec.outfit_id.clone(),
        band: spec.band.as_id().to_string(),
        guild_gated: spec.band.guild_gated(),
        gear_count: spec.gear.len(),
        provision_count: spec.provisions.len(),
        map_count: spec.maps.len(),
        breakage_prone: spec.breakage_prone,
        level_window_low: low,
        level_window_high: high,
        outfit_kit_sha256: kit_hash(spec)?,
    })
}

pub fn validate_outfit_kit(spec: &OutfitKitSpec) -> Result<(), EngineError> {
    if spec.schema != RPG_OUTFIT_KIT_SCHEMA {
        return invalid("schema", format!("expected {RPG_OUTFIT_KIT_SCHEMA}"));
    }
    validate_token("outfit_id", &spec.outfit_id)?;
    validate_text("display_name", &spec.display_name)?;
    if spec.source_issue == 0 {
        return invalid("source_issue", "must be non-zero");
    }
    if !spec.canon_issues.contains(&spec.source_issue) {
        return invalid("canon_issues", "must include the source issue");
    }
    if spec.recovery_floor_issue == 0 {
        return invalid(
            "recovery_floor_issue",
            "must name the full-loot floor issue",
        );
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
    if !spec.only_fills_empty_slots {
        return invalid(
            "only_fills_empty_slots",
            "helpfile 314: outfit only fills EMPTY slots -- must be true",
        );
    }

    // Gear must be non-empty and every slot/material must resolve.
    if spec.gear.is_empty() {
        return invalid("gear", "an outfit kit must grant at least one gear piece");
    }
    for piece in &spec.gear {
        if WearSlot::from_id(&piece.wear_slot_id).is_none() {
            return invalid(
                "gear.wear_slot_id",
                format!("unknown wear slot {}", piece.wear_slot_id),
            );
        }
        validate_item_material("gear.material_id", &piece.material_id)?;
        validate_text("gear.label", &piece.label)?;
    }
    for provision in &spec.provisions {
        if provision.provision_kind != "drink_container" && provision.provision_kind != "food" {
            return invalid(
                "provisions.provision_kind",
                format!(
                    "unknown provision kind {} (helpfile 314: drink_container | food)",
                    provision.provision_kind
                ),
            );
        }
        validate_text("provisions.label", &provision.label)?;
    }

    // Band vs breakage vs maps consistency (helpfile 314).
    match spec.band {
        OutfitBand::LowbieAnywhere => {
            if spec.breakage_prone {
                return invalid(
                    "breakage_prone",
                    "the levels 1-10 Mayor's-warehouse kit is not the shoddy breakage-prone gear",
                );
            }
        }
        OutfitBand::GuildOnly => {
            if !spec.breakage_prone {
                return invalid(
                    "breakage_prone",
                    "helpfile 314: the guild (L11+) kit is shoddy and breakage-prone",
                );
            }
            if !spec.maps.is_empty() {
                return invalid(
                    "maps",
                    "helpfile 314 grants maps only in the levels 1-10 outfit, not the guild re-gear",
                );
            }
        }
    }
    for map in &spec.maps {
        validate_token("maps.map_region_id", &map.map_region_id)?;
        validate_text("maps.label", &map.label)?;
    }
    if spec.implementation_notes.is_empty() {
        return invalid("implementation_notes", "must include source notes");
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

fn kit_hash(spec: &OutfitKitSpec) -> Result<String, EngineError> {
    serde_json::to_vec(spec)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize outfit kit: {error}"),
        })
}

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(EngineError::RpgStatsInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    })
}
