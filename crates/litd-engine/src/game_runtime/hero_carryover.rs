//! #853 runtime follow-up: hero carry-over consumption + #1853 permanent death.
//!
//! At chapter end a hero's live progress (level / XP / HP / inventory) is persisted
//! into the campaign KV store through a [`RuntimeCampaignHeroSlot`] (the authored +
//! lowered #853 data model). At the next chapter's start it is rehydrated. The
//! slot's `carries_level` / `carries_items` flags are honored AT PERSIST time, so a
//! slot that does not carry a field writes the reset baseline -- the rehydrated hero
//! then deterministically reflects exactly what the campaign author declared.
//!
//! #1853 permanent hero death: mission script writes a FALLEN tombstone under the
//! reserved `hero_roster` category. Chapter seeding skips tombstoned heroes; revival
//! paths consult the same key and refuse. No silent reintroduction of a fallen hero.

use crate::game_package::RuntimeCampaignHeroSlot;

use super::{
    error::{GameRuntimeError, invalid},
    kv_store::{CampaignKvStore, CampaignUnitSnapshot},
};

/// The KV category every hero carry-over datum is stored under (keyed by slot id).
pub const HERO_CARRYOVER_CATEGORY: &str = "hero_carryover";

/// #1853: reserved KV category for the campaign hero roster state machine.
/// Keys are slot ids; values are integer roster states (see `HERO_ROSTER_STATE_*`).
pub const HERO_ROSTER_CATEGORY: &str = "hero_roster";

/// Roster state: hero is still alive / available for chapter seeding.
pub const HERO_ROSTER_STATE_ACTIVE: i64 = 0;
/// Roster state: hero is canonically, permanently gone (tombstone).
pub const HERO_ROSTER_STATE_FALLEN: i64 = 1;

/// A hero's live runtime progress at chapter end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroProgress {
    pub hero_key: String,
    pub level: u32,
    pub experience: u64,
    pub hp_milli: u32,
    pub item_keys: Vec<String>,
}

/// Persist a hero's progress into the campaign store for the given carry-over slot,
/// honoring the slot's carry flags. `progress.hero_key` must match the slot.
/// Fails loud if the slot is already FALLEN -- a dead hero cannot carry progress.
pub fn persist_hero_carryover(
    store: &mut CampaignKvStore,
    slot: &RuntimeCampaignHeroSlot,
    progress: &HeroProgress,
) -> Result<(), GameRuntimeError> {
    if is_hero_fallen(store, &slot.slot_id)? {
        return invalid(
            "hero_carryover.fallen",
            format!(
                "slot {} ({}) is FALLEN; cannot persist carry-over for a tombstoned hero",
                slot.slot_id, slot.hero_key
            ),
        );
    }
    if progress.hero_key != slot.hero_key {
        return invalid(
            "hero_carryover.hero_key",
            format!(
                "progress hero {} does not match slot hero {}",
                progress.hero_key, slot.hero_key
            ),
        );
    }
    let snapshot = CampaignUnitSnapshot {
        unit_key: slot.hero_key.clone(),
        level: if slot.carries_level {
            progress.level
        } else {
            1
        },
        experience: if slot.carries_level {
            progress.experience
        } else {
            0
        },
        hp_milli: progress.hp_milli,
        item_keys: if slot.carries_items {
            progress.item_keys.clone()
        } else {
            Vec::new()
        },
    };
    store.put_unit(HERO_CARRYOVER_CATEGORY, &slot.slot_id, snapshot)
}

/// Rehydrate a hero's progress from the campaign store for the given slot. Fails
/// loud if the slot was never persisted, references a different hero, or is FALLEN.
pub fn rehydrate_hero_carryover(
    store: &CampaignKvStore,
    slot: &RuntimeCampaignHeroSlot,
) -> Result<HeroProgress, GameRuntimeError> {
    if is_hero_fallen(store, &slot.slot_id)? {
        return invalid(
            "hero_carryover.fallen",
            format!(
                "slot {} ({}) is FALLEN; cannot rehydrate a tombstoned hero into a chapter",
                slot.slot_id, slot.hero_key
            ),
        );
    }
    let snapshot = store.get_unit(HERO_CARRYOVER_CATEGORY, &slot.slot_id)?;
    if snapshot.unit_key != slot.hero_key {
        return invalid(
            "hero_carryover.unit_key",
            format!(
                "stored hero {} does not match slot hero {}",
                snapshot.unit_key, slot.hero_key
            ),
        );
    }
    Ok(HeroProgress {
        hero_key: snapshot.unit_key.clone(),
        level: snapshot.level,
        experience: snapshot.experience,
        hp_milli: snapshot.hp_milli,
        item_keys: snapshot.item_keys.clone(),
    })
}

/// Write the permanent-death tombstone for `slot_id` under [`HERO_ROSTER_CATEGORY`].
/// Idempotent: re-tombstoning an already-FALLEN slot succeeds (state stays FALLEN).
pub fn tombstone_hero(store: &mut CampaignKvStore, slot_id: &str) -> Result<(), GameRuntimeError> {
    store.put_integer(HERO_ROSTER_CATEGORY, slot_id, HERO_ROSTER_STATE_FALLEN)
}

/// Whether the roster marks `slot_id` as FALLEN. Missing key = not fallen (active by
/// default). Unknown integer states fail loud (no silent coercion).
pub fn is_hero_fallen(store: &CampaignKvStore, slot_id: &str) -> Result<bool, GameRuntimeError> {
    match store.get_integer(HERO_ROSTER_CATEGORY, slot_id) {
        Ok(HERO_ROSTER_STATE_FALLEN) => Ok(true),
        Ok(HERO_ROSTER_STATE_ACTIVE) => Ok(false),
        Ok(other) => invalid(
            "hero_roster.state",
            format!(
                "slot {slot_id} has unknown roster state {other}; expected {HERO_ROSTER_STATE_ACTIVE} (active) or {HERO_ROSTER_STATE_FALLEN} (fallen)"
            ),
        ),
        Err(GameRuntimeError::KvKeyMissing { .. }) => Ok(false),
        Err(error) => Err(error),
    }
}

/// Revival paths MUST call this before restoring a hero. Refuses if the slot is FALLEN.
pub fn assert_revival_allowed(
    store: &CampaignKvStore,
    slot_id: &str,
) -> Result<(), GameRuntimeError> {
    if is_hero_fallen(store, slot_id)? {
        return invalid(
            "hero_roster.revival_refused",
            format!(
                "slot {slot_id} is FALLEN; permanent death forbids revival (consult hero_roster tombstone)"
            ),
        );
    }
    Ok(())
}

/// Whether an authored slot is still active for `chapter_id` given its optional
/// `active_through_chapter` bound and the chapter order table. Inclusive bound:
/// active when `chapter.order <= bound_chapter.order`. Missing bound chapter fails loud.
pub fn slot_active_for_chapter(
    slot: &RuntimeCampaignHeroSlot,
    chapter_id: &str,
    chapter_order: impl Fn(&str) -> Option<u32>,
) -> Result<bool, GameRuntimeError> {
    let Some(bound) = slot.active_through_chapter.as_deref() else {
        return Ok(true);
    };
    let bound_order = chapter_order(bound).ok_or_else(|| GameRuntimeError::Invalid {
        field: "hero_slot.active_through_chapter".to_string(),
        reason: format!(
            "slot {} active_through_chapter={bound} is not a declared campaign chapter",
            slot.slot_id
        ),
    })?;
    let current_order = chapter_order(chapter_id).ok_or_else(|| GameRuntimeError::Invalid {
        field: "hero_slot.chapter".to_string(),
        reason: format!("chapter {chapter_id} is not a declared campaign chapter"),
    })?;
    Ok(current_order <= bound_order)
}

#[cfg(test)]
mod roster_tests {
    use super::*;

    #[test]
    fn unknown_roster_state_fails_loud() {
        let mut store = CampaignKvStore::new("camp.test", 1).unwrap();
        store
            .put_integer(HERO_ROSTER_CATEGORY, "slot_x", 99)
            .unwrap();
        let err = is_hero_fallen(&store, "slot_x").unwrap_err().to_string();
        assert!(err.contains("unknown roster state"), "error was {err}");
    }

    #[test]
    fn missing_roster_key_is_not_fallen() {
        let store = CampaignKvStore::new("camp.test", 1).unwrap();
        assert!(!is_hero_fallen(&store, "slot_x").unwrap());
    }

    #[test]
    fn revival_refuses_fallen() {
        let mut store = CampaignKvStore::new("camp.test", 1).unwrap();
        tombstone_hero(&mut store, "slot_x").unwrap();
        let err = assert_revival_allowed(&store, "slot_x")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("FALLEN") || err.contains("revival"),
            "error was {err}"
        );
    }
}
