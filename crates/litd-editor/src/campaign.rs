//! #853: single-player campaign editor (WC3 Campaign Editor equivalent).
//!
//! The AUTHORED campaign model: an ordered/branching set of chapters, each
//! referencing a `.litdworld` authored map, plus campaign-level SHARED OBJECT DATA
//! (custom units/heroes/upgrades visible to every member map), chapter flow (unlock
//! progression + optional branch-on-victory), campaign metadata (title, description,
//! difficulty tiers), and hero carry-over slots.
//!
//! Authoring persists via Calyx (`campaign_store`), lowers to a fingerprinted
//! runtime manifest embedded in a `.litdgame` (`campaign_lowering`), and is exposed
//! as MCP tools. The runtime shared-datum/hero-slot/branch types are the engine's
//! (`litd_engine::game_package`) so authored and lowered forms share one schema.

use std::collections::BTreeSet;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

pub use litd_engine::game_package::{
    LITD_CAMPAIGN_MANIFEST_SCHEMA, RuntimeCampaignBranch, RuntimeCampaignHeroSlot,
    RuntimeCampaignManifest, RuntimeCampaignSharedDatum, RuntimeSharedObjectCategory,
};

#[path = "campaign_lowering.rs"]
mod campaign_lowering;
#[path = "campaign_store.rs"]
mod campaign_store;

pub use campaign_lowering::{
    CampaignPackageBuild, build_campaign_litdgame, lower_campaign, resolve_shared_data_for_chapter,
};
pub use campaign_store::{
    CAMPAIGN_STORE_LEDGER_SCHEMA, CampaignReadback, CampaignStoreLedger,
    CampaignStoreLedgerReadback, CampaignStoreRecord, read_campaign, read_campaign_ledger,
    validate_campaign_ledger, write_campaign, write_campaign_ledger,
};

pub const AUTHORED_CAMPAIGN_SCHEMA: &str = "litd.campaign.authored.v1";

/// Campaign presentation metadata surfaced in menus and the campaign browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignMetadata {
    pub title: String,
    pub description: String,
    pub difficulty_tiers: Vec<String>,
}

/// One authored chapter node referencing a `.litdworld` map by its Calyx document
/// path plus flow edges. Lowering resolves `map_document_path` to a real lowered
/// world and embeds it in the campaign manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignChapter {
    pub chapter_id: String,
    pub order: u32,
    pub title: String,
    pub map_ref: String,
    pub map_document_path: String,
    pub unlock_after: Vec<String>,
    pub branch_on_victory: Vec<RuntimeCampaignBranch>,
}

/// The authored campaign aggregate persisted to Calyx and lowered to a manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Campaign {
    pub schema: String,
    pub campaign_id: String,
    pub metadata: CampaignMetadata,
    pub entry_chapter_id: String,
    pub chapters: Vec<CampaignChapter>,
    pub shared_object_data: Vec<RuntimeCampaignSharedDatum>,
    pub hero_carryover_slots: Vec<RuntimeCampaignHeroSlot>,
}

impl Campaign {
    pub fn new(campaign_id: impl Into<String>, metadata: CampaignMetadata) -> Self {
        Self {
            schema: AUTHORED_CAMPAIGN_SCHEMA.to_string(),
            campaign_id: campaign_id.into(),
            metadata,
            entry_chapter_id: String::new(),
            chapters: Vec::new(),
            shared_object_data: Vec::new(),
            hero_carryover_slots: Vec::new(),
        }
    }

    /// Insert or replace a chapter by `chapter_id`, keeping chapters sorted by order.
    pub fn upsert_chapter(&mut self, chapter: CampaignChapter) {
        if let Some(existing) = self
            .chapters
            .iter_mut()
            .find(|existing| existing.chapter_id == chapter.chapter_id)
        {
            *existing = chapter;
        } else {
            self.chapters.push(chapter);
        }
        self.chapters.sort_by(|left, right| {
            left.order
                .cmp(&right.order)
                .then_with(|| left.chapter_id.cmp(&right.chapter_id))
        });
    }

    /// Fail-loud structural validation of the authored campaign. Mirrors the runtime
    /// manifest validation so authoring rejects the same defects before lowering.
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema != AUTHORED_CAMPAIGN_SCHEMA {
            return Err(invalid_campaign(
                "schema",
                format!("authored campaign schema must be {AUTHORED_CAMPAIGN_SCHEMA}"),
            ));
        }
        validate_token("campaign_id", &self.campaign_id)?;
        validate_token("metadata.title", &self.metadata.title)?;
        validate_token("metadata.description", &self.metadata.description)?;
        self.validate_difficulty_tiers()?;
        self.validate_shared_object_data()?;
        self.validate_hero_slots()?;
        self.validate_chapters()?;
        Ok(())
    }

    fn validate_difficulty_tiers(&self) -> Result<(), EngineError> {
        if self.metadata.difficulty_tiers.is_empty() {
            return Err(invalid_campaign(
                "metadata.difficulty_tiers",
                "campaign must declare at least one difficulty tier",
            ));
        }
        let mut seen = BTreeSet::new();
        for tier in &self.metadata.difficulty_tiers {
            validate_token("metadata.difficulty_tier", tier)?;
            if !seen.insert(tier.clone()) {
                return Err(invalid_campaign(
                    "metadata.difficulty_tiers",
                    format!("duplicate difficulty tier {tier}"),
                ));
            }
        }
        Ok(())
    }

    fn validate_shared_object_data(&self) -> Result<(), EngineError> {
        let mut seen = BTreeSet::new();
        for datum in &self.shared_object_data {
            validate_token("shared_object_data.key", &datum.key)?;
            if !seen.insert(datum.key.clone()) {
                return Err(invalid_campaign(
                    "shared_object_data",
                    format!("duplicate shared object datum key {}", datum.key),
                ));
            }
            let is_object = datum
                .definition
                .as_object()
                .is_some_and(|map| !map.is_empty());
            if !is_object {
                return Err(invalid_campaign(
                    "shared_object_data.definition",
                    format!(
                        "shared datum {} definition must be a non-empty object",
                        datum.key
                    ),
                ));
            }
        }
        Ok(())
    }

    fn validate_hero_slots(&self) -> Result<(), EngineError> {
        let hero_keys: BTreeSet<&str> = self
            .shared_object_data
            .iter()
            .filter(|datum| datum.category == RuntimeSharedObjectCategory::Hero)
            .map(|datum| datum.key.as_str())
            .collect();
        let mut seen = BTreeSet::new();
        for slot in &self.hero_carryover_slots {
            validate_token("hero_slot.slot_id", &slot.slot_id)?;
            validate_token("hero_slot.hero_key", &slot.hero_key)?;
            if !seen.insert(slot.slot_id.clone()) {
                return Err(invalid_campaign(
                    "hero_carryover_slots",
                    format!("duplicate hero carry-over slot id {}", slot.slot_id),
                ));
            }
            if !hero_keys.contains(slot.hero_key.as_str()) {
                return Err(invalid_campaign(
                    "hero_slot.hero_key",
                    format!(
                        "hero carry-over slot {} references {} which is not a shared Hero datum",
                        slot.slot_id, slot.hero_key
                    ),
                ));
            }
        }
        Ok(())
    }

    fn validate_chapters(&self) -> Result<(), EngineError> {
        if self.chapters.is_empty() {
            return Err(invalid_campaign(
                "chapters",
                "campaign must declare at least one chapter",
            ));
        }
        validate_token("entry_chapter_id", &self.entry_chapter_id)?;
        let ids: BTreeSet<&str> = self
            .chapters
            .iter()
            .map(|chapter| chapter.chapter_id.as_str())
            .collect();
        if ids.len() != self.chapters.len() {
            return Err(invalid_campaign("chapters", "chapter ids must be unique"));
        }
        if !ids.contains(self.entry_chapter_id.as_str()) {
            return Err(invalid_campaign(
                "entry_chapter_id",
                format!(
                    "entry chapter {} is not a declared chapter",
                    self.entry_chapter_id
                ),
            ));
        }
        let mut orders = BTreeSet::new();
        for chapter in &self.chapters {
            self.validate_chapter(chapter, &ids)?;
            if !orders.insert(chapter.order) {
                return Err(invalid_campaign(
                    "chapter.order",
                    format!("duplicate chapter order {}", chapter.order),
                ));
            }
        }
        let entry = self
            .chapters
            .iter()
            .find(|chapter| chapter.chapter_id == self.entry_chapter_id)
            .expect("entry chapter presence checked above");
        if !entry.unlock_after.is_empty() {
            return Err(invalid_campaign(
                "entry_chapter.unlock_after",
                "entry chapter must not depend on any predecessor",
            ));
        }
        Ok(())
    }

    fn validate_chapter(
        &self,
        chapter: &CampaignChapter,
        ids: &BTreeSet<&str>,
    ) -> Result<(), EngineError> {
        validate_token("chapter.chapter_id", &chapter.chapter_id)?;
        validate_token("chapter.title", &chapter.title)?;
        validate_token("chapter.map_ref", &chapter.map_ref)?;
        validate_token("chapter.map_document_path", &chapter.map_document_path)?;
        for predecessor in &chapter.unlock_after {
            if predecessor == &chapter.chapter_id {
                return Err(invalid_campaign(
                    "chapter.unlock_after",
                    format!("chapter {} cannot unlock after itself", chapter.chapter_id),
                ));
            }
            if !ids.contains(predecessor.as_str()) {
                return Err(invalid_campaign(
                    "chapter.unlock_after",
                    format!(
                        "chapter {} predecessor {} does not exist",
                        chapter.chapter_id, predecessor
                    ),
                ));
            }
        }
        for branch in &chapter.branch_on_victory {
            validate_token(
                "chapter.branch.victory_condition",
                &branch.victory_condition,
            )?;
            if !ids.contains(branch.next_chapter_id.as_str()) {
                return Err(invalid_campaign(
                    "chapter.branch.next_chapter_id",
                    format!(
                        "chapter {} branch target {} does not exist",
                        chapter.chapter_id, branch.next_chapter_id
                    ),
                ));
            }
        }
        Ok(())
    }
}

pub(crate) fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(invalid_campaign(
            field,
            "value must be non-empty single-line text",
        ));
    }
    Ok(())
}

pub(crate) fn invalid_campaign(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let field = format!("editor.campaign.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::campaign",
        field = %field,
        reason = %reason,
        "invalid campaign authoring operation"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

#[cfg(test)]
#[path = "campaign_tests.rs"]
mod tests;
