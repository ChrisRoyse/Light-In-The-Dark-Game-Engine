//! #850(a): the WC3 game-cache / hashtable equivalent -- a typed, deterministic
//! key-value store for triggers and campaign flow.
//!
//! Values are one of three typed variants (integer, text, unit snapshot), keyed by
//! a `(category, key)` pair (WC3's `missionKey` + `key`). The whole store is SCOPED
//! per-campaign and per-save-slot (`campaign_id` + `save_slot`). Iteration order is
//! deterministic (`BTreeMap`) so `state_hash` is stable across processes -- the
//! store participates in the lockstep/replay determinism guarantee (#85).

use std::collections::BTreeMap;

use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

use super::error::{GameRuntimeError, invalid};

pub const CAMPAIGN_KV_SCHEMA: &str = "litd.game_runtime.campaign_kv.v1";

/// A stored unit/hero snapshot (WC3 `StoreUnit`): the durable gameplay state a
/// trigger or the campaign carry-over path persists. All integer fields -- no
/// floats -- so it lowers and hashes deterministically. Doubles as the hero
/// carry-over payload (#853 runtime follow-up).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignUnitSnapshot {
    /// Authoring key of the unit/hero definition (a lowered #818/#820 record id).
    pub unit_key: String,
    pub level: u32,
    pub experience: u64,
    /// Current hit points in fixed-point milli-units (integer, no float).
    pub hp_milli: u32,
    /// Inventory item definition keys (lowered #821 record ids), order-preserving.
    pub item_keys: Vec<String>,
}

/// One typed cell in the store. `serde` tags each variant so the on-disk form is
/// self-describing and a type mismatch on read is a typed error, never a coercion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CampaignValue {
    Integer(i64),
    Text(String),
    Unit(CampaignUnitSnapshot),
}

impl CampaignValue {
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Integer(_) => "integer",
            Self::Text(_) => "text",
            Self::Unit(_) => "unit",
        }
    }
}

/// The per-campaign, per-slot typed KV store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignKvStore {
    pub schema: String,
    pub campaign_id: String,
    pub save_slot: u32,
    /// category -> key -> value. `BTreeMap` at both levels keeps iteration (and
    /// therefore `state_hash`) deterministic.
    entries: BTreeMap<String, BTreeMap<String, CampaignValue>>,
}

impl CampaignKvStore {
    /// Open a fresh store scoped to one campaign + save slot.
    pub fn new(campaign_id: &str, save_slot: u32) -> Result<Self, GameRuntimeError> {
        require_token("kv.campaign_id", campaign_id)?;
        Ok(Self {
            schema: CAMPAIGN_KV_SCHEMA.to_string(),
            campaign_id: campaign_id.to_string(),
            save_slot,
            entries: BTreeMap::new(),
        })
    }

    pub fn campaign_id(&self) -> &str {
        &self.campaign_id
    }

    pub const fn save_slot(&self) -> u32 {
        self.save_slot
    }

    /// Total number of stored cells across every category.
    pub fn len(&self) -> usize {
        self.entries.values().map(BTreeMap::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.values().all(BTreeMap::is_empty)
    }

    fn put(
        &mut self,
        category: &str,
        key: &str,
        value: CampaignValue,
    ) -> Result<(), GameRuntimeError> {
        require_token("kv.category", category)?;
        require_token("kv.key", key)?;
        self.entries
            .entry(category.to_string())
            .or_default()
            .insert(key.to_string(), value);
        Ok(())
    }

    pub fn put_integer(
        &mut self,
        category: &str,
        key: &str,
        value: i64,
    ) -> Result<(), GameRuntimeError> {
        self.put(category, key, CampaignValue::Integer(value))
    }

    pub fn put_text(
        &mut self,
        category: &str,
        key: &str,
        value: &str,
    ) -> Result<(), GameRuntimeError> {
        self.put(category, key, CampaignValue::Text(value.to_string()))
    }

    pub fn put_unit(
        &mut self,
        category: &str,
        key: &str,
        value: CampaignUnitSnapshot,
    ) -> Result<(), GameRuntimeError> {
        self.put(category, key, CampaignValue::Unit(value))
    }

    fn get(&self, category: &str, key: &str) -> Result<&CampaignValue, GameRuntimeError> {
        self.entries
            .get(category)
            .and_then(|bucket| bucket.get(key))
            .ok_or_else(|| GameRuntimeError::KvKeyMissing {
                category: category.to_string(),
                key: key.to_string(),
            })
    }

    pub fn get_integer(&self, category: &str, key: &str) -> Result<i64, GameRuntimeError> {
        match self.get(category, key)? {
            CampaignValue::Integer(value) => Ok(*value),
            other => Err(type_mismatch(category, key, "integer", other)),
        }
    }

    pub fn get_text(&self, category: &str, key: &str) -> Result<&str, GameRuntimeError> {
        match self.get(category, key)? {
            CampaignValue::Text(value) => Ok(value),
            other => Err(type_mismatch(category, key, "text", other)),
        }
    }

    pub fn get_unit(
        &self,
        category: &str,
        key: &str,
    ) -> Result<&CampaignUnitSnapshot, GameRuntimeError> {
        match self.get(category, key)? {
            CampaignValue::Unit(value) => Ok(value),
            other => Err(type_mismatch(category, key, "unit", other)),
        }
    }

    /// Order-independent-of-`HashMap` deterministic digest of the whole store.
    /// Folds schema + scope + every `(category, key, value)` in `BTreeMap` order.
    pub fn state_hash(&self) -> u64 {
        let mut hasher = Fnv1a64::new();
        hasher.update(self.schema.as_bytes());
        hasher.update(self.campaign_id.as_bytes());
        hasher.update_u64_le(u64::from(self.save_slot));
        for (category, bucket) in &self.entries {
            for (key, value) in bucket {
                hasher.update(category.as_bytes());
                hasher.update(key.as_bytes());
                hash_value(&mut hasher, value);
            }
        }
        hasher.finish()
    }
}

fn hash_value(hasher: &mut Fnv1a64, value: &CampaignValue) {
    match value {
        CampaignValue::Integer(int) => {
            hasher.update_u64_le(1);
            hasher.update_i64_le(*int);
        }
        CampaignValue::Text(text) => {
            hasher.update_u64_le(2);
            hasher.update(text.as_bytes());
        }
        CampaignValue::Unit(unit) => {
            hasher.update_u64_le(3);
            hasher.update(unit.unit_key.as_bytes());
            hasher.update_u64_le(u64::from(unit.level));
            hasher.update_u64_le(unit.experience);
            hasher.update_u64_le(u64::from(unit.hp_milli));
            for item in &unit.item_keys {
                hasher.update(item.as_bytes());
            }
        }
    }
}

fn type_mismatch(
    category: &str,
    key: &str,
    expected: &'static str,
    actual: &CampaignValue,
) -> GameRuntimeError {
    GameRuntimeError::KvTypeMismatch {
        category: category.to_string(),
        key: key.to_string(),
        expected,
        actual: actual.type_name(),
    }
}

pub(crate) fn require_token(field: &str, value: &str) -> Result<(), GameRuntimeError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    if value.chars().any(char::is_control) {
        return invalid(field, "must not contain control characters");
    }
    Ok(())
}
