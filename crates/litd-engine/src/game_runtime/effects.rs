//! #844: runtime special effects -- spawn/attach effect models at points or on
//! units, manage their lifetime, and destroy them when their owner unit dies.
//!
//! Effect refs are AUTHORED DATA: the art ref is validated at LOWERING against
//! the packaged art registry (a dangling ref fails loud -- the FSV negative
//! control), so the runtime only ever spawns validated effects.
//!
//! BOUNDARY: effect visuals are presentation, but their LIFECYCLE is driven by
//! deterministic sim facts (the current tick and the set of alive owner bodies),
//! so the manager is tick-driven and reproducible: attach on spawn, expire at a
//! lifetime deadline, and destroy-on-owner-death when the owner leaves the sim.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::math::FixedVec3;

/// Sim body id an effect can be attached to / owned by.
pub type BodyId = u32;

/// Where an effect model is placed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectAttach {
    /// Fixed world point (fixed-point, sim-space).
    Point(FixedVec3),
    /// Attached to a unit body at a named attachment point (e.g. "hand_left").
    Unit { owner: BodyId, attach_point: String },
}

/// An authored effect spawn request before lowering validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectSpawn {
    /// Art ref for the effect model -- validated at lowering.
    pub effect_ref: String,
    pub attach: EffectAttach,
    /// Ticks the effect lives before auto-expiry; `None` = until destroyed.
    pub lifetime_ticks: Option<u64>,
    /// If attached to a unit, destroy the effect when that unit dies.
    pub destroy_on_owner_death: bool,
}

/// A validated spawn (its art ref exists in the registry). Only lowering can
/// mint one, so the runtime cannot spawn a dangling effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredEffectSpawn {
    spawn: EffectSpawn,
}

impl LoweredEffectSpawn {
    pub fn effect_ref(&self) -> &str {
        &self.spawn.effect_ref
    }

    pub fn attach(&self) -> &EffectAttach {
        &self.spawn.attach
    }
}

/// Stable diagnostic-coded lowering error (kept local so `error.rs` stays frozen
/// at its baseline -- mirrors the `CinematicTimelineError` pattern).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("effect spawn field {field} is invalid [{code}]: {reason}")]
pub struct EffectLoweringError {
    pub code: &'static str,
    pub field: String,
    pub reason: String,
}

impl EffectLoweringError {
    pub fn code(&self) -> &'static str {
        self.code
    }
}

/// Runtime allocation/deadline failure for an already-lowered effect spawn.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("effect runtime field {field} is invalid [{code}]: {reason}")]
pub struct EffectRuntimeError {
    pub code: &'static str,
    pub field: String,
    pub reason: String,
}

impl EffectRuntimeError {
    pub fn code(&self) -> &'static str {
        self.code
    }
}

/// The packaged set of valid effect art refs (fingerprinted upstream). Stored
/// sorted+unique so membership is order-independent.
#[derive(Debug, Clone, Default)]
pub struct EffectArtRegistry {
    refs: Vec<String>,
}

impl EffectArtRegistry {
    pub fn new(refs: &[&str]) -> Self {
        let mut refs: Vec<String> = refs.iter().map(|value| value.to_string()).collect();
        refs.sort();
        refs.dedup();
        Self { refs }
    }

    pub fn contains(&self, effect_ref: &str) -> bool {
        self.refs.binary_search(&effect_ref.to_string()).is_ok()
    }
}

/// Validate an authored spawn against the art registry. Fails loud on an empty
/// ref, an empty attachment-point name, or a dangling art ref.
pub fn lower_effect_spawn(
    spawn: &EffectSpawn,
    registry: &EffectArtRegistry,
) -> Result<LoweredEffectSpawn, EffectLoweringError> {
    if spawn.effect_ref.trim().is_empty() {
        return Err(EffectLoweringError {
            code: "EFFECT_REF_EMPTY",
            field: "effect_ref".to_string(),
            reason: "effect art ref must be non-empty".to_string(),
        });
    }
    if let EffectAttach::Unit { attach_point, .. } = &spawn.attach
        && attach_point.trim().is_empty()
    {
        return Err(EffectLoweringError {
            code: "EFFECT_ATTACH_POINT_EMPTY",
            field: "attach.attach_point".to_string(),
            reason: "attachment point name must be non-empty".to_string(),
        });
    }
    if !registry.contains(&spawn.effect_ref) {
        return Err(EffectLoweringError {
            code: "EFFECT_ART_REF_DANGLING",
            field: "effect_ref".to_string(),
            reason: format!(
                "art ref '{}' is not in the packaged registry",
                spawn.effect_ref
            ),
        });
    }
    Ok(LoweredEffectSpawn {
        spawn: spawn.clone(),
    })
}

/// A live effect instance in the runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectInstance {
    pub id: u32,
    pub effect_ref: String,
    pub attach: EffectAttach,
    pub spawn_tick: u64,
    pub lifetime_ticks: Option<u64>,
    pub destroy_on_owner_death: bool,
}

impl EffectInstance {
    /// The owner body this effect is attached to, if any.
    pub fn owner(&self) -> Option<BodyId> {
        match &self.attach {
            EffectAttach::Unit { owner, .. } => Some(*owner),
            EffectAttach::Point(_) => None,
        }
    }

    fn expired_at(&self, current_tick: u64) -> bool {
        match self.lifetime_ticks {
            Some(lifetime) => {
                let deadline = self
                    .spawn_tick
                    .checked_add(lifetime)
                    .expect("effect spawn preflights its lifetime deadline");
                current_tick >= deadline
            }
            None => false,
        }
    }
}

/// Tick-driven manager of live effects.
#[derive(Debug, Clone, Default)]
pub struct EffectRuntime {
    instances: Vec<EffectInstance>,
    next_id: u64,
}

impl EffectRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn a validated effect at `current_tick`; returns its instance id.
    pub fn spawn(
        &mut self,
        lowered: &LoweredEffectSpawn,
        current_tick: u64,
    ) -> Result<u32, EffectRuntimeError> {
        if let Some(lifetime) = lowered.spawn.lifetime_ticks {
            current_tick
                .checked_add(lifetime)
                .ok_or_else(|| EffectRuntimeError {
                    code: "EFFECT_DEADLINE_OVERFLOW",
                    field: "lifetime_ticks".to_string(),
                    reason: format!(
                        "spawn tick {current_tick} plus lifetime {lifetime} exceeds {}",
                        u64::MAX
                    ),
                })?;
        }
        let id = u32::try_from(self.next_id).map_err(|_| EffectRuntimeError {
            code: "EFFECT_ID_EXHAUSTED",
            field: "id".to_string(),
            reason: "effect instance id space is exhausted".to_string(),
        })?;
        let next_id = self
            .next_id
            .checked_add(1)
            .expect("u32 effect id space fits in the u64 allocator");
        self.instances.push(EffectInstance {
            id,
            effect_ref: lowered.spawn.effect_ref.clone(),
            attach: lowered.spawn.attach.clone(),
            spawn_tick: current_tick,
            lifetime_ticks: lowered.spawn.lifetime_ticks,
            destroy_on_owner_death: lowered.spawn.destroy_on_owner_death,
        });
        self.next_id = next_id;
        Ok(id)
    }

    /// Advance one tick: destroy effects whose lifetime elapsed and effects whose
    /// owner has died (left `alive_owners`). Returns the ids destroyed this tick,
    /// in ascending id order (deterministic).
    pub fn tick(&mut self, current_tick: u64, alive_owners: &BTreeSet<BodyId>) -> Vec<u32> {
        let mut destroyed = Vec::new();
        self.instances.retain(|instance| {
            let owner_dead = instance.destroy_on_owner_death
                && instance
                    .owner()
                    .is_some_and(|owner| !alive_owners.contains(&owner));
            let expired = instance.expired_at(current_tick);
            if owner_dead || expired {
                destroyed.push(instance.id);
                false
            } else {
                true
            }
        });
        destroyed.sort_unstable();
        destroyed
    }

    pub fn active_count(&self) -> usize {
        self.instances.len()
    }

    /// Live instance ids in ascending order.
    pub fn active_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.instances.iter().map(|instance| instance.id).collect();
        ids.sort_unstable();
        ids
    }

    pub fn contains(&self, id: u32) -> bool {
        self.instances.iter().any(|instance| instance.id == id)
    }

    pub fn instances(&self) -> &[EffectInstance] {
        &self.instances
    }

    #[cfg(test)]
    pub(super) fn set_next_id_for_test(&mut self, next_id: u64) {
        self.next_id = next_id;
    }
}
