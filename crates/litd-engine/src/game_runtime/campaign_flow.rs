//! #1852: the single-player campaign FLOW RUNNER -- the deterministic host state machine
//! that advances a validated [`RuntimeCampaignManifest`] chapter-by-chapter.
//!
//! On a chapter's victory it (a) persists hero carry-over for every ACTIVE hero slot
//! (honoring each slot's carry flags, #853, and skipping #1853 FALLEN / past-
//! `active_through_chapter` slots), (b) records the chapter as unlocked/completed
//! in the campaign KV store, (c) resolves the next chapter from `branch_on_victory` (an
//! explicit victory-condition edge wins) falling back to the `unlock_after` DAG (the
//! lowest-order chapter whose every predecessor is now completed), and (d) persists the
//! whole campaign progress + hero payload into a save slot reusing [`GameSaveFile`] (the
//! runtime half of #850). A chapter BOOTS its match by instantiating a
//! [`MatchRuntimeSession`] from the chapter's embedded lowered runtime datasets (#1852).
//!
//! #1853 permanent death: mission script (or authoring `active_through_chapter`) writes a
//! FALLEN tombstone under `hero_roster`; chapter seeding and rehydrate skip those slots,
//! and revival refuses. No silent reintroduction of a canonically dead hero.
//!
//! Fail-loud on invalid campaign TOPOLOGY: a circular `unlock_after` graph (the "key
//! locked inside the box" invalidity -- a dependency cycle can never be satisfied, so the
//! chapter is unreachable), a chapter with no embedded datasets asked to boot a match, or
//! a hero carry-over slot mismatch (progress for an undeclared slot, a missing active
//! slot, or a hero-key that disagrees with the slot). No silent fallback, ever.

use std::collections::BTreeSet;
use std::path::Path;

use crate::EngineError;
use crate::game_package::{
    RuntimeCampaignChapter, RuntimeCampaignHeroSlot, RuntimeCampaignManifest,
};
use crate::sim::SimConfig;

use super::campaign_flow_topology::{detect_unlock_cycle, flow_error};
use super::hero_carryover::{
    assert_revival_allowed, is_hero_fallen, persist_hero_carryover, rehydrate_hero_carryover,
    slot_active_for_chapter, tombstone_hero,
};
use super::session::MatchRuntimeSession;
use super::time_of_day::{TimeOfDayConfig, TimeOfDayState};
use super::{CampaignKvStore, GameRuntime, GameSaveFile, HeroProgress};

/// KV category the campaign-progress head (current chapter + completed count) lives in.
pub const CAMPAIGN_PROGRESS_CATEGORY: &str = "campaign_progress";
/// KV category recording each completed chapter (key = chapter_id, value = its order).
pub const CHAPTER_UNLOCK_CATEGORY: &str = "chapter_unlock";
const PROGRESS_CURRENT_CHAPTER_KEY: &str = "current_chapter";
const PROGRESS_COMPLETED_COUNT_KEY: &str = "completed_count";
// The campaign-progress save reuses the #850 GameSaveFile whose deterministic KV store is
// the campaign progress; the sim snapshot is a minimal empty runtime (no match bodies --
// the per-chapter match sim is re-instantiated fresh each chapter, never carried in the
// progress save). These constants only shape that empty snapshot.
const CAMPAIGN_SAVE_TICK_RATE: u32 = 64;
const CAMPAIGN_SAVE_DAY_LENGTH_TICKS: u64 = 100;
const CAMPAIGN_SAVE_BASE_SIGHT_MM: u32 = 12_000;
const CAMPAIGN_SAVE_NIGHT_PENALTY_PERMILLE: u16 = 500;

/// The outcome of completing one chapter: which chapter finished, the resolved next
/// chapter (if any), and whether that completion ended the campaign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterAdvance {
    pub completed_chapter_id: String,
    pub next_chapter_id: Option<String>,
    pub campaign_complete: bool,
}

/// The deterministic campaign flow state machine.
#[derive(Debug)]
pub struct CampaignFlow {
    manifest: RuntimeCampaignManifest,
    save_slot: u32,
    current_chapter_id: String,
    completed_chapters: Vec<String>,
    kv: CampaignKvStore,
}

impl CampaignFlow {
    /// Boot the flow at the campaign's entry chapter. The manifest must already be
    /// package-validated (chapter refs / fingerprints); this additionally fails loud on a
    /// circular `unlock_after` topology, which package validation does NOT catch.
    pub fn boot(manifest: RuntimeCampaignManifest, save_slot: u32) -> Result<Self, EngineError> {
        detect_unlock_cycle(&manifest)?;
        // The entry chapter is validated to exist by package validation; re-check here so a
        // directly-constructed (unvalidated) manifest still fails loud instead of panicking.
        if !manifest
            .chapters
            .iter()
            .any(|chapter| chapter.chapter_id == manifest.entry_chapter_id)
        {
            return Err(flow_error(
                "boot.entry_chapter",
                format!(
                    "entry chapter {} is not a declared chapter",
                    manifest.entry_chapter_id
                ),
            ));
        }
        let kv = CampaignKvStore::new(&manifest.campaign_id, save_slot)
            .map_err(|error| flow_error("boot.kv", error.to_string()))?;
        let mut flow = Self {
            current_chapter_id: manifest.entry_chapter_id.clone(),
            manifest,
            save_slot,
            completed_chapters: Vec::new(),
            kv,
        };
        flow.write_progress_head()?;
        Ok(flow)
    }

    pub fn current_chapter_id(&self) -> &str {
        &self.current_chapter_id
    }

    pub fn completed_chapters(&self) -> &[String] {
        &self.completed_chapters
    }

    pub fn manifest(&self) -> &RuntimeCampaignManifest {
        &self.manifest
    }

    pub fn save_slot(&self) -> u32 {
        self.save_slot
    }

    pub fn kv(&self) -> &CampaignKvStore {
        &self.kv
    }

    pub fn current_chapter(&self) -> Result<&RuntimeCampaignChapter, EngineError> {
        self.chapter(&self.current_chapter_id)
    }

    fn chapter(&self, chapter_id: &str) -> Result<&RuntimeCampaignChapter, EngineError> {
        self.manifest
            .chapters
            .iter()
            .find(|chapter| chapter.chapter_id == chapter_id)
            .ok_or_else(|| {
                flow_error(
                    "chapter.missing",
                    format!("chapter {chapter_id} is not declared in the campaign manifest"),
                )
            })
    }

    /// Whether the current chapter embeds runtime datasets (i.e. is a combat chapter that
    /// boots a match, as opposed to a cinematic / interlude chapter that does not).
    pub fn current_chapter_boots_match(&self) -> bool {
        self.current_chapter()
            .ok()
            .map(|chapter| {
                self.manifest
                    .chapter_runtime_datasets(&chapter.map_ref)
                    .is_some()
            })
            .unwrap_or(false)
    }

    /// Instantiate the current chapter's [`MatchRuntimeSession`] from its embedded lowered
    /// runtime datasets. Fails loud if the chapter embeds none (a cinematic chapter has no
    /// match to boot -- callers that tolerate that use [`Self::current_chapter_boots_match`]).
    pub fn instantiate_current_session(&self) -> Result<MatchRuntimeSession, EngineError> {
        self.instantiate_session(&self.current_chapter_id)
    }

    pub fn instantiate_session(
        &self,
        chapter_id: &str,
    ) -> Result<MatchRuntimeSession, EngineError> {
        let chapter = self.chapter(chapter_id)?;
        let datasets = self
            .manifest
            .chapter_runtime_datasets(&chapter.map_ref)
            .ok_or_else(|| {
                flow_error(
                    "session.datasets_missing",
                    format!(
                        "chapter {} (map {}) embeds no runtime datasets to boot a match session",
                        chapter.chapter_id, chapter.map_ref
                    ),
                )
            })?;
        MatchRuntimeSession::from_manifest(datasets)
            .map_err(|error| flow_error("session.build", error.to_string()))
    }

    pub fn hero_slots(&self) -> &[RuntimeCampaignHeroSlot] {
        &self.manifest.hero_carryover_slots
    }

    /// #1853: write the permanent-death tombstone for a declared hero slot.
    /// Surfaces in the campaign save under `hero_roster` / FALLEN.
    pub fn tombstone_hero(&mut self, slot_id: &str) -> Result<(), EngineError> {
        let _slot = self.hero_slot(slot_id)?;
        tombstone_hero(&mut self.kv, slot_id)
            .map_err(|error| flow_error("hero.tombstone", error.to_string()))
    }

    /// #1853: whether the campaign roster marks the slot FALLEN.
    pub fn is_hero_fallen(&self, slot_id: &str) -> Result<bool, EngineError> {
        let _slot = self.hero_slot(slot_id)?;
        is_hero_fallen(&self.kv, slot_id)
            .map_err(|error| flow_error("hero.fallen_query", error.to_string()))
    }

    /// #1853: revival entry point -- refuses if the slot is FALLEN.
    pub fn assert_revival_allowed(&self, slot_id: &str) -> Result<(), EngineError> {
        let _slot = self.hero_slot(slot_id)?;
        assert_revival_allowed(&self.kv, slot_id)
            .map_err(|error| flow_error("hero.revival", error.to_string()))
    }

    /// Slot ids that seed into `chapter_id`: not FALLEN and within
    /// `active_through_chapter` (inclusive). This is the chapter-seeding roster.
    pub fn active_hero_slot_ids_for_chapter(
        &self,
        chapter_id: &str,
    ) -> Result<Vec<String>, EngineError> {
        let mut active = Vec::new();
        for slot in &self.manifest.hero_carryover_slots {
            if is_hero_fallen(&self.kv, &slot.slot_id)
                .map_err(|error| flow_error("hero.seed.fallen", error.to_string()))?
            {
                continue;
            }
            let within_bound = slot_active_for_chapter(slot, chapter_id, |id| {
                self.manifest
                    .chapters
                    .iter()
                    .find(|chapter| chapter.chapter_id == id)
                    .map(|chapter| chapter.order)
            })
            .map_err(|error| flow_error("hero.seed.bound", error.to_string()))?;
            if within_bound {
                active.push(slot.slot_id.clone());
            }
        }
        Ok(active)
    }

    /// Convenience: active slots for the current chapter (chapter-start seeding).
    pub fn active_hero_slot_ids_for_current_chapter(&self) -> Result<Vec<String>, EngineError> {
        self.active_hero_slot_ids_for_chapter(&self.current_chapter_id)
    }

    /// Rehydrate a hero's carried-over progress for the given slot (chapter start).
    /// Fails loud if the slot is FALLEN or not authored-active for the current chapter.
    pub fn rehydrate_hero(&self, slot_id: &str) -> Result<HeroProgress, EngineError> {
        let slot = self.hero_slot(slot_id)?;
        let active = self.active_hero_slot_ids_for_current_chapter()?;
        if !active.iter().any(|id| id == slot_id) {
            return Err(flow_error(
                "hero.rehydrate.inactive",
                format!(
                    "slot {slot_id} is not active for chapter {} (FALLEN or past active_through_chapter)",
                    self.current_chapter_id
                ),
            ));
        }
        rehydrate_hero_carryover(&self.kv, slot)
            .map_err(|error| flow_error("hero.rehydrate", error.to_string()))
    }

    fn hero_slot(&self, slot_id: &str) -> Result<&RuntimeCampaignHeroSlot, EngineError> {
        self.manifest
            .hero_carryover_slots
            .iter()
            .find(|slot| slot.slot_id == slot_id)
            .ok_or_else(|| {
                flow_error(
                    "hero.slot_missing",
                    format!("campaign declares no hero carry-over slot {slot_id}"),
                )
            })
    }

    /// Complete the current chapter on victory: persist hero carry-over for every ACTIVE
    /// slot (skipping FALLEN / past-bound slots), record the chapter unlock, resolve the
    /// next chapter, advance, and update the persisted progress head. `hero_progress`
    /// supplies each active slot's chapter-end live progress exactly once; a
    /// missing/duplicate/undeclared/inactive slot fails loud.
    pub fn complete_chapter(
        &mut self,
        victory_condition: Option<&str>,
        hero_progress: &[(String, HeroProgress)],
    ) -> Result<ChapterAdvance, EngineError> {
        let completed_id = self.current_chapter_id.clone();
        if self.completed_chapters.iter().any(|id| id == &completed_id) {
            return Err(flow_error(
                "complete.already_completed",
                format!("chapter {completed_id} was already completed"),
            ));
        }
        self.persist_active_hero_slots(&completed_id, hero_progress)?;
        // #1853: slots whose authoring bound ends on this chapter auto-tombstone so the
        // next chapter's seed roster and the save file both surface the absence.
        self.tombstone_slots_ending_on(&completed_id)?;
        let order = self.chapter(&completed_id)?.order;
        self.completed_chapters.push(completed_id.clone());
        self.kv
            .put_integer(CHAPTER_UNLOCK_CATEGORY, &completed_id, i64::from(order))
            .map_err(|error| flow_error("complete.unlock", error.to_string()))?;
        let next = self.resolve_next_chapter(&completed_id, victory_condition)?;
        if let Some(next_id) = &next {
            self.current_chapter_id = next_id.clone();
        }
        self.write_progress_head()?;
        Ok(ChapterAdvance {
            completed_chapter_id: completed_id,
            next_chapter_id: next.clone(),
            campaign_complete: next.is_none(),
        })
    }

    fn tombstone_slots_ending_on(&mut self, chapter_id: &str) -> Result<(), EngineError> {
        let ending: Vec<String> = self
            .manifest
            .hero_carryover_slots
            .iter()
            .filter(|slot| slot.active_through_chapter.as_deref() == Some(chapter_id))
            .map(|slot| slot.slot_id.clone())
            .collect();
        for slot_id in ending {
            if !is_hero_fallen(&self.kv, &slot_id)
                .map_err(|error| flow_error("hero.auto_tombstone.query", error.to_string()))?
            {
                tombstone_hero(&mut self.kv, &slot_id)
                    .map_err(|error| flow_error("hero.auto_tombstone", error.to_string()))?;
            }
        }
        Ok(())
    }

    fn persist_active_hero_slots(
        &mut self,
        chapter_id: &str,
        hero_progress: &[(String, HeroProgress)],
    ) -> Result<(), EngineError> {
        let required = self.active_hero_slot_ids_for_chapter(chapter_id)?;
        let required_set: BTreeSet<&str> = required.iter().map(String::as_str).collect();
        let declared: BTreeSet<&str> = self
            .manifest
            .hero_carryover_slots
            .iter()
            .map(|slot| slot.slot_id.as_str())
            .collect();
        let mut supplied: BTreeSet<String> = BTreeSet::new();
        for (slot_id, progress) in hero_progress {
            if !declared.contains(slot_id.as_str()) {
                return Err(flow_error(
                    "hero.unknown_slot",
                    format!(
                        "hero progress supplied for slot {slot_id} which the campaign does not declare"
                    ),
                ));
            }
            if !required_set.contains(slot_id.as_str()) {
                return Err(flow_error(
                    "hero.inactive_slot",
                    format!(
                        "hero progress supplied for inactive slot {slot_id} on chapter {chapter_id} (FALLEN or past active_through_chapter)"
                    ),
                ));
            }
            if !supplied.insert(slot_id.clone()) {
                return Err(flow_error(
                    "hero.duplicate_slot",
                    format!("hero progress supplied more than once for slot {slot_id}"),
                ));
            }
            // Cloning the slot decouples the immutable manifest borrow from the mutable KV
            // borrow below; the slot's carry flags + hero-key mismatch are enforced by
            // `persist_hero_carryover` (fail loud).
            let slot = self.hero_slot(slot_id)?.clone();
            persist_hero_carryover(&mut self.kv, &slot, progress)
                .map_err(|error| flow_error("hero.persist", error.to_string()))?;
        }
        for slot_id in &required {
            if !supplied.contains(slot_id) {
                return Err(flow_error(
                    "hero.missing_slot",
                    format!("chapter completed without hero progress for active slot {slot_id}"),
                ));
            }
        }
        Ok(())
    }

    fn resolve_next_chapter(
        &self,
        completed_id: &str,
        victory_condition: Option<&str>,
    ) -> Result<Option<String>, EngineError> {
        let completed_chapter = self.chapter(completed_id)?;
        // `branch_on_victory` precedence: an explicit victory-condition edge routes flow.
        if let Some(condition) = victory_condition {
            for branch in &completed_chapter.branch_on_victory {
                if branch.victory_condition == condition {
                    // Re-check the target exists (package validation guarantees it; this
                    // fails loud for a directly-constructed manifest instead of dangling).
                    self.chapter(&branch.next_chapter_id)?;
                    return Ok(Some(branch.next_chapter_id.clone()));
                }
            }
        }
        // `unlock_after` DAG: the lowest-order not-yet-completed chapter every one of whose
        // predecessors is now completed (AND semantics -- WC3-style multi-prerequisite).
        let completed: BTreeSet<&str> =
            self.completed_chapters.iter().map(String::as_str).collect();
        let mut candidates: Vec<&RuntimeCampaignChapter> = self
            .manifest
            .chapters
            .iter()
            .filter(|chapter| {
                !completed.contains(chapter.chapter_id.as_str())
                    && !chapter.unlock_after.is_empty()
                    && chapter
                        .unlock_after
                        .iter()
                        .all(|predecessor| completed.contains(predecessor.as_str()))
            })
            .collect();
        candidates.sort_by(|a, b| a.order.cmp(&b.order).then(a.chapter_id.cmp(&b.chapter_id)));
        Ok(candidates.first().map(|chapter| chapter.chapter_id.clone()))
    }

    fn write_progress_head(&mut self) -> Result<(), EngineError> {
        self.kv
            .put_text(
                CAMPAIGN_PROGRESS_CATEGORY,
                PROGRESS_CURRENT_CHAPTER_KEY,
                &self.current_chapter_id,
            )
            .map_err(|error| flow_error("progress.current", error.to_string()))?;
        self.kv
            .put_integer(
                CAMPAIGN_PROGRESS_CATEGORY,
                PROGRESS_COMPLETED_COUNT_KEY,
                self.completed_chapters.len() as i64,
            )
            .map_err(|error| flow_error("progress.count", error.to_string()))?;
        Ok(())
    }

    /// Capture the campaign progress + hero payload into a fingerprinted [`GameSaveFile`]
    /// (the save slot's KV store IS the campaign progress). Reuses the #850 save format.
    pub fn capture_save(&self) -> Result<GameSaveFile, EngineError> {
        let config = SimConfig::new(CAMPAIGN_SAVE_TICK_RATE)
            .map_err(|error| flow_error("save.sim_config", error.to_string()))?;
        let time_of_day = TimeOfDayState::new(
            TimeOfDayConfig {
                day_length_ticks: CAMPAIGN_SAVE_DAY_LENGTH_TICKS,
                base_sight_radius_mm: CAMPAIGN_SAVE_BASE_SIGHT_MM,
                night_sight_penalty_permille: CAMPAIGN_SAVE_NIGHT_PENALTY_PERMILLE,
            },
            0,
            0,
        )
        .map_err(|error| flow_error("save.time_of_day", error.to_string()))?;
        let runtime = GameRuntime::<4, 4>::new(
            config,
            u64::from(self.save_slot),
            time_of_day,
            self.kv.clone(),
        )
        .map_err(|error| flow_error("save.runtime", error.to_string()))?;
        runtime
            .capture_save()
            .map_err(|error| flow_error("save.capture", error.to_string()))
    }

    /// Persist the campaign progress save to disk and return the captured save file.
    pub fn save_to_path(&self, path: &Path) -> Result<GameSaveFile, EngineError> {
        let save = self.capture_save()?;
        save.save_to_path(path)
            .map_err(|error| flow_error("save.write", error.to_string()))?;
        Ok(save)
    }
}

#[cfg(test)]
#[path = "campaign_flow_tests.rs"]
mod tests;
