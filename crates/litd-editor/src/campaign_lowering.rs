//! #853: lower an authored `Campaign` to a fingerprinted runtime manifest and build
//! a campaign `.litdgame` package that `litd-game` validates.
//!
//! Each chapter's `.litdworld` map is lowered through the real quick-share pipeline
//! (`build_quick_share_package`), so a campaign package embeds only genuinely lowered
//! worlds. The entry chapter's lowered world is the package boot `document`; every
//! chapter's lowered map is embedded in the manifest keyed by `map_ref`.
//!
//! Shared-data precedence (documented): a map that authors an object whose id
//! collides with a campaign shared-datum key OVERRIDES the campaign datum on that
//! map (`map_local` wins); elsewhere the campaign shared datum applies
//! (`campaign_shared`). The per-chapter resolution is recorded so readback proves it.

use std::collections::BTreeMap;

use litd_engine::EngineError;
use litd_engine::game_package::{
    CAMPAIGN_WINNER_CAMPAIGN_SHARED, CAMPAIGN_WINNER_MAP_LOCAL, LITD_CAMPAIGN_MANIFEST_SCHEMA,
    RuntimeCampaignChapter, RuntimeCampaignManifest, RuntimeCampaignSharedDatum,
    RuntimeCampaignSharedResolution, validate_campaign_manifest,
};
use serde_json::Value;

use crate::history::EditorDocument;
use crate::quick_share::{
    QuickShareBuildRequest, QuickShareEndpoint, QuickSharePackage, build_quick_share_package,
};

use super::{Campaign, invalid_campaign};

/// The result of building a campaign `.litdgame`: the sealed package (carrying the
/// embedded campaign manifest) plus the manifest itself for readback.
#[derive(Debug, Clone, PartialEq)]
pub struct CampaignPackageBuild {
    pub package: QuickSharePackage,
    pub manifest: RuntimeCampaignManifest,
}

/// Resolve the documented shared-data precedence for one chapter against its lowered
/// map document. A shared key present as an object id in the map is overridden
/// locally; every other shared key is served by the campaign datum.
pub fn resolve_shared_data_for_chapter(
    shared_object_data: &[RuntimeCampaignSharedDatum],
    map_document: &Value,
) -> Vec<RuntimeCampaignSharedResolution> {
    let map_object_ids = map_document
        .get("objects")
        .and_then(Value::as_object)
        .map(|objects| objects.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    shared_object_data
        .iter()
        .map(|datum| {
            if map_object_ids.iter().any(|id| id == &datum.key) {
                RuntimeCampaignSharedResolution {
                    key: datum.key.clone(),
                    winner: CAMPAIGN_WINNER_MAP_LOCAL.to_string(),
                    source: format!("map object {} overrides campaign shared datum", datum.key),
                }
            } else {
                RuntimeCampaignSharedResolution {
                    key: datum.key.clone(),
                    winner: CAMPAIGN_WINNER_CAMPAIGN_SHARED.to_string(),
                    source: "campaign shared datum applies (no map-local override)".to_string(),
                }
            }
        })
        .collect()
}

/// Lower an authored campaign to a runtime manifest, embedding each chapter's lowered
/// map. `chapter_maps` maps a `map_ref` to the quick-share package produced for that
/// map. Fails loud if a chapter references a map that was not lowered.
pub fn lower_campaign(
    campaign: &Campaign,
    chapter_maps: &BTreeMap<String, QuickSharePackage>,
) -> Result<RuntimeCampaignManifest, EngineError> {
    campaign.validate()?;
    let mut chapters = Vec::new();
    let mut maps: BTreeMap<String, Value> = BTreeMap::new();
    // #1852: per-chapter lowered runtime datasets so the campaign flow runner can BOOT each
    // chapter as a match. Inference resolves the value type from the struct field below.
    let mut chapter_runtime_datasets = BTreeMap::new();
    for chapter in &campaign.chapters {
        let Some(map_package) = chapter_maps.get(&chapter.map_ref) else {
            return Err(invalid_campaign(
                "lower.map_ref",
                format!(
                    "chapter {} references map {} which was not lowered",
                    chapter.chapter_id, chapter.map_ref
                ),
            ));
        };
        let resolution =
            resolve_shared_data_for_chapter(&campaign.shared_object_data, &map_package.document);
        chapters.push(RuntimeCampaignChapter {
            chapter_id: chapter.chapter_id.clone(),
            order: chapter.order,
            title: chapter.title.clone(),
            map_ref: chapter.map_ref.clone(),
            map_document_fingerprint: map_package.document_fingerprint.clone(),
            unlock_after: chapter.unlock_after.clone(),
            branch_on_victory: chapter.branch_on_victory.clone(),
            shared_data_resolution: resolution,
        });
        maps.insert(chapter.map_ref.clone(), map_package.document.clone());
        // A chapter whose authored world carried object-data that lowered to a runnable
        // economy+blueprint+constants set embeds it here (keyed by map_ref); a cinematic
        // chapter whose world carried none embeds nothing (serde(default) empty).
        if let Some(runtime) = &map_package.runtime {
            chapter_runtime_datasets.insert(chapter.map_ref.clone(), runtime.clone());
        }
    }
    let mut manifest = RuntimeCampaignManifest {
        schema: LITD_CAMPAIGN_MANIFEST_SCHEMA.to_string(),
        campaign_id: campaign.campaign_id.clone(),
        title: campaign.metadata.title.clone(),
        description: campaign.metadata.description.clone(),
        difficulty_tiers: campaign.metadata.difficulty_tiers.clone(),
        entry_chapter_id: campaign.entry_chapter_id.clone(),
        chapters,
        shared_object_data: campaign.shared_object_data.clone(),
        hero_carryover_slots: campaign.hero_carryover_slots.clone(),
        maps,
        chapter_runtime_datasets,
        manifest_fingerprint: String::new(),
    };
    manifest.manifest_fingerprint = manifest.compute_fingerprint()?;
    Ok(manifest)
}

/// Build a campaign `.litdgame` package from authored chapters and their `.litdworld`
/// maps. `chapter_maps` maps each chapter `map_ref` to its authored `EditorDocument`.
/// The entry chapter's lowered world becomes the boot `document`; the full campaign
/// manifest is embedded and validated before the package is returned.
pub fn build_campaign_litdgame(
    campaign: &Campaign,
    chapter_maps: &BTreeMap<String, EditorDocument>,
    request: &QuickShareBuildRequest,
) -> Result<CampaignPackageBuild, EngineError> {
    campaign.validate()?;
    let entry_chapter = campaign
        .chapters
        .iter()
        .find(|chapter| chapter.chapter_id == campaign.entry_chapter_id)
        .ok_or_else(|| invalid_campaign("build.entry", "entry chapter is not present"))?;

    let mut lowered_maps: BTreeMap<String, QuickSharePackage> = BTreeMap::new();
    let mut base_package: Option<QuickSharePackage> = None;
    for chapter in &campaign.chapters {
        if lowered_maps.contains_key(&chapter.map_ref) {
            continue;
        }
        let document = chapter_maps.get(&chapter.map_ref).ok_or_else(|| {
            invalid_campaign(
                "build.map_ref",
                format!(
                    "chapter {} references map {} which has no authored world",
                    chapter.chapter_id, chapter.map_ref
                ),
            )
        })?;
        let package = if chapter.map_ref == entry_chapter.map_ref {
            let package = build_quick_share_package(document, request)?;
            base_package = Some(package.clone());
            package
        } else {
            let chapter_request = derive_chapter_request(request, &chapter.map_ref);
            build_quick_share_package(document, &chapter_request)?
        };
        lowered_maps.insert(chapter.map_ref.clone(), package);
    }

    let base = base_package.ok_or_else(|| {
        invalid_campaign("build.base", "entry chapter map produced no boot package")
    })?;
    let manifest = lower_campaign(campaign, &lowered_maps)?;
    // Fail loud at PACKAGE VALIDATION time (not play time): the same validation
    // litd-game runs on load, run here so a bad campaign never reaches disk.
    validate_campaign_manifest(&manifest, &base.document, &base.document_fingerprint)?;
    let package = QuickSharePackage {
        campaign: Some(manifest.clone()),
        ..base
    };
    Ok(CampaignPackageBuild { package, manifest })
}

fn derive_chapter_request(
    request: &QuickShareBuildRequest,
    map_ref: &str,
) -> QuickShareBuildRequest {
    let share_id = format!("{}-{}", request.share_id, sanitize(map_ref));
    QuickShareBuildRequest {
        share_id: share_id.clone(),
        created_by: request.created_by.clone(),
        endpoint: QuickShareEndpoint::loopback(&share_id, request.endpoint.port),
        runtime_id: request.runtime_id.clone(),
        max_package_bytes: request.max_package_bytes,
    }
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}
