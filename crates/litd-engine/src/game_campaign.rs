//! #853: single-player campaign manifest carried inside a packaged `.litdgame`.
//!
//! A campaign is an ordered/branching set of chapters, each referencing a lowered
//! map document, plus campaign-level SHARED OBJECT DATA (custom units/heroes/
//! upgrades visible to every member map), chapter flow (unlock progression and
//! optional branch-on-victory), campaign metadata, and hero carry-over slots.
//!
//! The manifest is embedded in the runtime package envelope (`game_package.rs`) so
//! `litd-game` validates it at PACKAGE VALIDATION time, not play time: an invalid or
//! missing chapter map reference, a tampered embedded map, a dangling unlock/branch
//! target, or a corrupted `manifest_fingerprint` all fail loud here (#853). The
//! runtime boots into the entry chapter whose lowered map is the envelope
//! `document`, so a campaign package is still a self-consistent single-world package.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::EngineError;
use crate::game_runtime::datasets::RuntimeDatasetsManifest;

use super::stable_package_fingerprint;

#[path = "game_campaign_validate.rs"]
mod validate;
pub use validate::validate_campaign_manifest;

pub const LITD_CAMPAIGN_MANIFEST_SCHEMA: &str = "litd.campaign.manifest.v1";
pub const CAMPAIGN_WINNER_MAP_LOCAL: &str = "map_local";
pub const CAMPAIGN_WINNER_CAMPAIGN_SHARED: &str = "campaign_shared";

/// The three campaign-level object-data kinds a shared datum can carry (WC3 custom
/// units/heroes/upgrades). Hero carry-over slots may only reference `Hero` data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSharedObjectCategory {
    Unit,
    Hero,
    Upgrade,
}

/// One campaign-level custom object definition shared with every member map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCampaignSharedDatum {
    pub key: String,
    pub category: RuntimeSharedObjectCategory,
    pub definition: Value,
}

/// One branch edge: on `victory_condition`, flow proceeds to `next_chapter_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCampaignBranch {
    pub victory_condition: String,
    pub next_chapter_id: String,
}

/// The documented precedence outcome for one shared-data key on a specific map.
///
/// A map may author an object whose id collides with a campaign shared datum key.
/// The documented precedence is MAP-LOCAL OVERRIDE WINS on that map; the campaign
/// shared datum applies only where the map does not override it. This record is
/// what proves which side won on readback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCampaignSharedResolution {
    pub key: String,
    pub winner: String,
    pub source: String,
}

/// One chapter: an ordered node that references a lowered map plus its flow edges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCampaignChapter {
    pub chapter_id: String,
    pub order: u32,
    pub title: String,
    pub map_ref: String,
    pub map_document_fingerprint: String,
    pub unlock_after: Vec<String>,
    pub branch_on_victory: Vec<RuntimeCampaignBranch>,
    pub shared_data_resolution: Vec<RuntimeCampaignSharedResolution>,
}

/// A hero carry-over slot: the data model for persisting a hero's level/items across
/// chapters. Runtime consumption is a follow-up; the authored+lowered model is
/// complete and validated here.
///
/// #1853 permanent death: a runtime tombstone in the campaign KV (`hero_roster`)
/// marks a hero FALLEN so later chapter seeding skips them. Optionally,
/// [`Self::active_through_chapter`] lets the author declare the last chapter the
/// slot may appear in (inclusive); chapters after that bound also skip the slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCampaignHeroSlot {
    pub slot_id: String,
    pub hero_key: String,
    pub carries_level: bool,
    pub carries_items: bool,
    /// Last chapter_id (inclusive) this slot may seed into. `None` means the slot
    /// stays available for every chapter unless a runtime FALLEN tombstone is set.
    /// Additive: older packages without the field deserialize as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_through_chapter: Option<String>,
}

/// The full lowered campaign manifest embedded in the runtime package envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCampaignManifest {
    pub schema: String,
    pub campaign_id: String,
    pub title: String,
    pub description: String,
    pub difficulty_tiers: Vec<String>,
    pub entry_chapter_id: String,
    pub chapters: Vec<RuntimeCampaignChapter>,
    pub shared_object_data: Vec<RuntimeCampaignSharedDatum>,
    pub hero_carryover_slots: Vec<RuntimeCampaignHeroSlot>,
    pub maps: std::collections::BTreeMap<String, Value>,
    /// #1852: per-chapter lowered runtime datasets (economy ruleset + match blueprint +
    /// runtime constants), keyed by chapter `map_ref`, that the `litd-game` campaign flow
    /// runner instantiates each chapter's [`crate::game_runtime::MatchRuntimeSession`] from
    /// so a chapter actually BOOTS as a match. Additive (`serde(default)` so a pre-#1852
    /// campaign package still deserializes; a combat chapter carries datasets, a cinematic
    /// / interlude chapter carries none), and folded into `manifest_fingerprint` so a
    /// swapped or stripped datasets section is rejected fail-loud at package load.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub chapter_runtime_datasets: std::collections::BTreeMap<String, RuntimeDatasetsManifest>,
    pub manifest_fingerprint: String,
}

impl RuntimeCampaignManifest {
    /// Deterministic fingerprint over every field except `manifest_fingerprint`
    /// itself. Reordering chapters, editing shared data, or swapping a map changes
    /// this fingerprint; it is the single value the negative control recomputes to
    /// detect a corrupted embedded manifest.
    pub fn compute_fingerprint(&self) -> Result<String, EngineError> {
        stable_package_fingerprint(&(
            &self.schema,
            &self.campaign_id,
            &self.title,
            &self.description,
            &self.difficulty_tiers,
            &self.entry_chapter_id,
            &self.chapters,
            &self.shared_object_data,
            &self.hero_carryover_slots,
            &self.maps,
            &self.chapter_runtime_datasets,
        ))
    }

    /// The lowered runtime datasets a chapter boots its match session from, if the
    /// chapter is a combat chapter that embedded them (keyed by its `map_ref`).
    pub fn chapter_runtime_datasets(&self, map_ref: &str) -> Option<&RuntimeDatasetsManifest> {
        self.chapter_runtime_datasets.get(map_ref)
    }
}

#[cfg(test)]
#[path = "game_campaign_tests.rs"]
mod tests;
