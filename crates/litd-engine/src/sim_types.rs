//! Deterministic-sim value types, RNG, and state-hash helpers (#1466 split from `sim.rs`).
//!
//! Pure data + hashing for the fixed-point sim loop: config, body/state/event records, run
//! reports, the deterministic RNG, and the FNV-1a state-hash primitives. `FixedTickSim` and the
//! tick loop stay in `sim`. Re-exported by `sim` via `pub use sim_types::*` so every existing
//! `crate::sim::Sim*` / `DeterministicRng` / `ATTACK_EVENT_RETENTION` path is unchanged.

use crate::{
    EngineError,
    math::{Fixed, FixedVec3},
};
use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

/// Maximum number of recent attack events retained in memory for the animation
/// system and evidence readback. The determinism source of truth for attacks is
/// the persistent `attack_events_digest` (full history, O(1) space), so the
/// retained log can be a bounded window without weakening replay verification
/// (#1463). Must exceed the maximum attacks recordable in a single tick so the
/// animation system (which reads current-tick attacks) never loses one; a sim
/// order queue applies at most its capacity of orders per tick, far below this.
pub const ATTACK_EVENT_RETENTION: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimConfig {
    pub ticks_per_second: u32,
}

impl SimConfig {
    pub fn new(ticks_per_second: u32) -> Result<Self, EngineError> {
        if ticks_per_second == 0 {
            return Err(EngineError::SimInvalidConfig {
                field: "ticks_per_second".to_string(),
                reason: "must be non-zero".to_string(),
            });
        }
        Ok(Self { ticks_per_second })
    }

    pub fn fixed_delta(self) -> Result<Fixed, EngineError> {
        Fixed::from_ratio(1, i64::from(self.ticks_per_second))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimBodyId(u32);

impl SimBodyId {
    pub const fn index(self) -> u32 {
        self.0
    }

    pub const fn from_raw(index: u32) -> Self {
        Self(index)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimBody {
    pub id: SimBodyId,
    pub position: FixedVec3,
    pub velocity: FixedVec3,
    pub acceleration: FixedVec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimBodyState {
    pub id: SimBodyId,
    pub position: FixedVec3,
    pub velocity: FixedVec3,
    pub acceleration: FixedVec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimAttackEvent {
    pub tick: u64,
    pub attacker: SimBodyId,
    pub target: SimBodyId,
    pub damage_fp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimStepReport {
    pub tick: u64,
    pub bodies_updated: usize,
    pub state_hash: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimRunReport {
    pub start_tick: u64,
    pub final_tick: u64,
    pub steps: u64,
    pub bodies_updated: u64,
    pub state_hash: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    pub fn new(seed: u64) -> Self {
        let state = if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        };
        Self { state }
    }

    pub fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.state >> 32) as u32
    }

    pub fn next_fixed_unit(&mut self) -> Fixed {
        Fixed::from_raw(i64::from(self.next_u32()))
    }

    pub const fn state(self) -> u64 {
        self.state
    }

    pub const fn from_state(state: u64) -> Self {
        Self { state }
    }
}

/// Fold one attack event into the running attack digest. Order-dependent and
/// deterministic: chaining the prior digest as the first word makes the result
/// depend on the exact sequence of attacks, matching the previous per-event
/// hash contribution's ordering guarantee in O(1) space.
pub(crate) fn fold_attack_event(digest: u64, event: SimAttackEvent) -> u64 {
    let mut hasher = Fnv1a64::new();
    hasher.update_u64_le(digest);
    hasher.update_u64_le(event.tick);
    hasher.update_u64_le(u64::from(event.attacker.index()));
    hasher.update_u64_le(u64::from(event.target.index()));
    hasher.update_i64_le(event.damage_fp);
    hasher.finish()
}

pub(crate) fn hash_vec3(hasher: &mut Fnv1a64, value: FixedVec3) {
    hash_i64(hasher, value.x.raw());
    hash_i64(hasher, value.y.raw());
    hash_i64(hasher, value.z.raw());
}

pub(crate) fn hash_i64(hasher: &mut Fnv1a64, value: i64) {
    hasher.update_i64_le(value);
}

pub(crate) fn hash_u64(hasher: &mut Fnv1a64, value: u64) {
    hasher.update_u64_le(value);
}
