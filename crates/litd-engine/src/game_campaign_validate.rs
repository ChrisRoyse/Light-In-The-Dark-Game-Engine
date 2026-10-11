//! #853/#1852: fail-loud validation of the embedded campaign manifest.
//!
//! Split out of `game_campaign.rs` (the data model) so both files stay under the #800
//! sec.6 <500-line ratchet. Runs at PACKAGE VALIDATION time in both the editor (before
//! write) and `litd-game` (before play): a missing/tampered chapter map, a dangling
//! unlock/branch target, a hero slot that names a non-Hero datum, a corrupted manifest
//! fingerprint, or a swapped/sabotaged per-chapter runtime datasets section all fail loud
//! here -- never at play time.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::EngineError;
use crate::game_runtime::datasets::validate_runtime_datasets;

use super::super::game_package_support::{
    game_package_error, invalid_game_package, validate_token,
};
use super::super::{LITD_LOWERED_DOCUMENT_SCHEMA, stable_package_fingerprint};
use super::{
    CAMPAIGN_WINNER_CAMPAIGN_SHARED, CAMPAIGN_WINNER_MAP_LOCAL, LITD_CAMPAIGN_MANIFEST_SCHEMA,
    RuntimeCampaignChapter, RuntimeCampaignHeroSlot, RuntimeCampaignManifest,
    RuntimeCampaignSharedDatum, RuntimeSharedObjectCategory,
};

/// Fail-loud validation of a campaign manifest against the envelope's lowered
/// document. Runs at package validation time in both the editor (before write) and
/// `litd-game` (before play). Every chapter map reference must resolve to an
/// embedded, fingerprint-matched lowered map; a missing one is a hard error.
pub fn validate_campaign_manifest(
    manifest: &RuntimeCampaignManifest,
    document: &Value,
    document_fingerprint: &str,
) -> Result<(), EngineError> {
    if manifest.schema != LITD_CAMPAIGN_MANIFEST_SCHEMA {
        return invalid_game_package(
            "campaign.schema",
            format!("campaign manifest schema must be {LITD_CAMPAIGN_MANIFEST_SCHEMA}"),
        );
    }
    validate_token("campaign.campaign_id", &manifest.campaign_id)?;
    validate_token("campaign.title", &manifest.title)?;
    validate_token("campaign.description", &manifest.description)?;
    validate_token("campaign.entry_chapter_id", &manifest.entry_chapter_id)?;
    validate_difficulty_tiers(&manifest.difficulty_tiers)?;
    validate_shared_object_data(&manifest.shared_object_data)?;
    let shared_keys = shared_key_set(&manifest.shared_object_data);
    validate_hero_slots(&manifest.hero_carryover_slots, &manifest.shared_object_data)?;
    validate_chapters(manifest, document, document_fingerprint, &shared_keys)?;
    validate_chapter_runtime_datasets(manifest)?;

    let expected = manifest.compute_fingerprint()?;
    if manifest.manifest_fingerprint != expected {
        return invalid_game_package(
            "campaign.manifest_fingerprint",
            "campaign manifest fingerprint does not match its contents (corrupted manifest)",
        );
    }
    Ok(())
}

fn validate_difficulty_tiers(tiers: &[String]) -> Result<(), EngineError> {
    if tiers.is_empty() {
        return invalid_game_package(
            "campaign.difficulty_tiers",
            "campaign must declare at least one difficulty tier",
        );
    }
    let mut seen = BTreeSet::new();
    for tier in tiers {
        validate_token("campaign.difficulty_tier", tier)?;
        if !seen.insert(tier.clone()) {
            return invalid_game_package(
                "campaign.difficulty_tiers",
                format!("duplicate difficulty tier {tier}"),
            );
        }
    }
    Ok(())
}

fn validate_shared_object_data(data: &[RuntimeCampaignSharedDatum]) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for datum in data {
        validate_token("campaign.shared_object_data.key", &datum.key)?;
        if !seen.insert(datum.key.clone()) {
            return invalid_game_package(
                "campaign.shared_object_data",
                format!("duplicate shared object datum key {}", datum.key),
            );
        }
        let Some(object) = datum.definition.as_object() else {
            return invalid_game_package(
                "campaign.shared_object_data.definition",
                format!(
                    "shared datum {} definition must be a JSON object",
                    datum.key
                ),
            );
        };
        if object.is_empty() {
            return invalid_game_package(
                "campaign.shared_object_data.definition",
                format!("shared datum {} definition must be non-empty", datum.key),
            );
        }
    }
    Ok(())
}

fn shared_key_set(data: &[RuntimeCampaignSharedDatum]) -> BTreeSet<String> {
    data.iter().map(|datum| datum.key.clone()).collect()
}

fn validate_hero_slots(
    slots: &[RuntimeCampaignHeroSlot],
    data: &[RuntimeCampaignSharedDatum],
) -> Result<(), EngineError> {
    let hero_keys: BTreeSet<&str> = data
        .iter()
        .filter(|datum| datum.category == RuntimeSharedObjectCategory::Hero)
        .map(|datum| datum.key.as_str())
        .collect();
    let mut seen = BTreeSet::new();
    for slot in slots {
        validate_token("campaign.hero_slot.slot_id", &slot.slot_id)?;
        validate_token("campaign.hero_slot.hero_key", &slot.hero_key)?;
        if !seen.insert(slot.slot_id.clone()) {
            return invalid_game_package(
                "campaign.hero_carryover_slots",
                format!("duplicate hero carry-over slot id {}", slot.slot_id),
            );
        }
        if !hero_keys.contains(slot.hero_key.as_str()) {
            return invalid_game_package(
                "campaign.hero_slot.hero_key",
                format!(
                    "hero carry-over slot {} references {} which is not a shared Hero datum",
                    slot.slot_id, slot.hero_key
                ),
            );
        }
    }
    Ok(())
}

fn validate_chapters(
    manifest: &RuntimeCampaignManifest,
    document: &Value,
    document_fingerprint: &str,
    shared_keys: &BTreeSet<String>,
) -> Result<(), EngineError> {
    if manifest.chapters.is_empty() {
        return invalid_game_package(
            "campaign.chapters",
            "campaign must declare at least one chapter",
        );
    }
    let chapter_ids: BTreeSet<&str> = manifest
        .chapters
        .iter()
        .map(|chapter| chapter.chapter_id.as_str())
        .collect();
    if chapter_ids.len() != manifest.chapters.len() {
        return invalid_game_package("campaign.chapters", "chapter ids must be unique");
    }
    let mut orders = BTreeSet::new();
    for chapter in &manifest.chapters {
        if !orders.insert(chapter.order) {
            return invalid_game_package(
                "campaign.chapter.order",
                format!("duplicate chapter order {}", chapter.order),
            );
        }
    }
    if !chapter_ids.contains(manifest.entry_chapter_id.as_str()) {
        return invalid_game_package(
            "campaign.entry_chapter_id",
            format!(
                "entry chapter {} is not a declared chapter",
                manifest.entry_chapter_id
            ),
        );
    }
    for chapter in &manifest.chapters {
        validate_chapter(chapter, manifest, &chapter_ids, shared_keys)?;
    }
    validate_entry_chapter(manifest, document, document_fingerprint)
}

fn validate_chapter(
    chapter: &RuntimeCampaignChapter,
    manifest: &RuntimeCampaignManifest,
    chapter_ids: &BTreeSet<&str>,
    shared_keys: &BTreeSet<String>,
) -> Result<(), EngineError> {
    validate_token("campaign.chapter.chapter_id", &chapter.chapter_id)?;
    validate_token("campaign.chapter.title", &chapter.title)?;
    validate_token("campaign.chapter.map_ref", &chapter.map_ref)?;
    validate_token(
        "campaign.chapter.map_document_fingerprint",
        &chapter.map_document_fingerprint,
    )?;
    let Some(map_document) = manifest.maps.get(&chapter.map_ref) else {
        return invalid_game_package(
            "campaign.chapter.map_ref",
            format!(
                "chapter {} references map {} which is not embedded in the campaign package",
                chapter.chapter_id, chapter.map_ref
            ),
        );
    };
    validate_embedded_map(map_document, &chapter.map_ref)?;
    let actual = stable_package_fingerprint(map_document)?;
    if actual != chapter.map_document_fingerprint {
        return invalid_game_package(
            "campaign.chapter.map_document_fingerprint",
            format!(
                "chapter {} embedded map {} fingerprint {} does not match declared {}",
                chapter.chapter_id, chapter.map_ref, actual, chapter.map_document_fingerprint
            ),
        );
    }
    for predecessor in &chapter.unlock_after {
        if predecessor == &chapter.chapter_id {
            return invalid_game_package(
                "campaign.chapter.unlock_after",
                format!("chapter {} cannot unlock after itself", chapter.chapter_id),
            );
        }
        if !chapter_ids.contains(predecessor.as_str()) {
            return invalid_game_package(
                "campaign.chapter.unlock_after",
                format!(
                    "chapter {} unlock predecessor {} does not exist",
                    chapter.chapter_id, predecessor
                ),
            );
        }
    }
    for branch in &chapter.branch_on_victory {
        validate_token(
            "campaign.chapter.branch.victory_condition",
            &branch.victory_condition,
        )?;
        if !chapter_ids.contains(branch.next_chapter_id.as_str()) {
            return invalid_game_package(
                "campaign.chapter.branch.next_chapter_id",
                format!(
                    "chapter {} branch target {} does not exist",
                    chapter.chapter_id, branch.next_chapter_id
                ),
            );
        }
    }
    validate_chapter_resolution(chapter, shared_keys)
}

fn validate_chapter_resolution(
    chapter: &RuntimeCampaignChapter,
    shared_keys: &BTreeSet<String>,
) -> Result<(), EngineError> {
    for resolution in &chapter.shared_data_resolution {
        if !shared_keys.contains(&resolution.key) {
            return invalid_game_package(
                "campaign.chapter.shared_data_resolution",
                format!(
                    "chapter {} resolves unknown shared key {}",
                    chapter.chapter_id, resolution.key
                ),
            );
        }
        if resolution.winner != CAMPAIGN_WINNER_MAP_LOCAL
            && resolution.winner != CAMPAIGN_WINNER_CAMPAIGN_SHARED
        {
            return invalid_game_package(
                "campaign.chapter.shared_data_resolution.winner",
                format!(
                    "chapter {} shared key {} has unknown winner {}",
                    chapter.chapter_id, resolution.key, resolution.winner
                ),
            );
        }
        validate_token(
            "campaign.chapter.shared_data_resolution.source",
            &resolution.source,
        )?;
    }
    Ok(())
}

/// #1852: validate the per-chapter embedded runtime datasets. Each datasets entry must be
/// keyed by a declared chapter `map_ref`, and each manifest must independently re-validate
/// (economy/blueprint/constants fingerprints re-derived) -- a swapped or sabotaged datasets
/// section is rejected here at package validation time, never at play time.
fn validate_chapter_runtime_datasets(
    manifest: &RuntimeCampaignManifest,
) -> Result<(), EngineError> {
    let chapter_map_refs: BTreeSet<&str> = manifest
        .chapters
        .iter()
        .map(|chapter| chapter.map_ref.as_str())
        .collect();
    for (map_ref, datasets) in &manifest.chapter_runtime_datasets {
        if !chapter_map_refs.contains(map_ref.as_str()) {
            return invalid_game_package(
                "campaign.chapter_runtime_datasets.map_ref",
                format!(
                    "runtime datasets keyed by {map_ref} which is not a declared chapter map_ref"
                ),
            );
        }
        validate_runtime_datasets(datasets)?;
    }
    Ok(())
}

fn validate_embedded_map(map_document: &Value, map_ref: &str) -> Result<(), EngineError> {
    let Some(object) = map_document.as_object() else {
        return invalid_game_package(
            "campaign.maps",
            format!("embedded map {map_ref} must be a lowered document object"),
        );
    };
    if map_schema(object) != Some(LITD_LOWERED_DOCUMENT_SCHEMA) {
        return invalid_game_package(
            "campaign.maps.schema",
            format!("embedded map {map_ref} schema must be {LITD_LOWERED_DOCUMENT_SCHEMA}"),
        );
    }
    Ok(())
}

fn map_schema(object: &Map<String, Value>) -> Option<&str> {
    object.get("schema").and_then(Value::as_str)
}

fn validate_entry_chapter(
    manifest: &RuntimeCampaignManifest,
    document: &Value,
    document_fingerprint: &str,
) -> Result<(), EngineError> {
    let entry = manifest
        .chapters
        .iter()
        .find(|chapter| chapter.chapter_id == manifest.entry_chapter_id)
        .ok_or_else(|| {
            game_package_error(
                "campaign.entry_chapter",
                "entry chapter is not present in chapters",
            )
        })?;
    if !entry.unlock_after.is_empty() {
        return invalid_game_package(
            "campaign.entry_chapter.unlock_after",
            "entry chapter must not depend on any predecessor chapter",
        );
    }
    if entry.map_document_fingerprint != document_fingerprint {
        return invalid_game_package(
            "campaign.entry_chapter.document",
            "entry chapter map fingerprint must equal the runtime boot document fingerprint",
        );
    }
    let entry_map = manifest.maps.get(&entry.map_ref).ok_or_else(|| {
        game_package_error(
            "campaign.entry_chapter.map",
            "entry chapter map is not embedded",
        )
    })?;
    if entry_map != document {
        return invalid_game_package(
            "campaign.entry_chapter.document",
            "entry chapter embedded map must equal the runtime boot document",
        );
    }
    Ok(())
}
