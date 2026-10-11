//! Deterministic fixed-point simulation loop for replay/netplay and headless FSV.

use std::collections::{HashMap, VecDeque};

use crate::{
    EngineError,
    determinism_boundary::{HotPathPhase, enter_hot_path},
    math::{Fixed, FixedVec3},
};
use litd_hash::Fnv1a64;

#[path = "sim_types.rs"]
mod sim_types;

// #1466: sim value types + RNG + state-hash helpers live in `sim_types`. Re-export the public
// model so every `crate::sim::Sim*` / `DeterministicRng` / `ATTACK_EVENT_RETENTION` path is
// unchanged; bring the crate-private hash helpers into scope for the tick/hash methods below.
pub use sim_types::*;
use sim_types::{fold_attack_event, hash_u64, hash_vec3};

pub struct FixedTickSim<const MAX_BODIES: usize> {
    config: SimConfig,
    fixed_delta: Fixed,
    tick: u64,
    next_body_id: u32,
    body_count: usize,
    bodies: Vec<Option<SimBody>>,
    /// Index from body id to its slot in `bodies`, for O(1) `body_state` /
    /// `body_mut` lookups instead of an O(MAX_BODIES) linear scan per applied
    /// order (#1463). Used only for point lookups — `state_hash` still iterates
    /// `bodies` in slot order, so this map's iteration order never affects
    /// determinism. Slot assignment is unchanged (lowest free slot), so hashes
    /// are unaffected.
    body_slots: HashMap<u32, usize>,
    /// Bounded window of the most recent attack events (see
    /// [`ATTACK_EVENT_RETENTION`]). NOT the determinism source of truth — that
    /// is `attack_events_digest`.
    attack_events: VecDeque<SimAttackEvent>,
    /// Persistent order-dependent digest of EVERY attack ever recorded, folded
    /// incrementally on each push. `state_hash` mixes this digest instead of
    /// re-hashing the whole `attack_events` log every tick, so per-tick hashing
    /// is O(1) in the attack count rather than O(total attacks ever) — the
    /// #1463 quadratic-per-session growth. Full-history coverage is preserved,
    /// so lockstep replay verification is not weakened.
    attack_events_digest: u64,
    /// Persistent count of every attack ever recorded (independent of how many
    /// are retained in `attack_events`).
    attack_events_total: u64,
    rng: DeterministicRng,
}

impl<const MAX_BODIES: usize> FixedTickSim<MAX_BODIES> {
    pub fn new(config: SimConfig, seed: u64) -> Result<Self, EngineError> {
        let mut bodies = Vec::with_capacity(MAX_BODIES);
        bodies.resize(MAX_BODIES, None);
        Ok(Self {
            config,
            fixed_delta: config.fixed_delta()?,
            tick: 0,
            next_body_id: 0,
            body_count: 0,
            bodies,
            body_slots: HashMap::new(),
            attack_events: VecDeque::new(),
            attack_events_digest: 0,
            attack_events_total: 0,
            rng: DeterministicRng::new(seed),
        })
    }

    pub const fn config(&self) -> SimConfig {
        self.config
    }

    pub const fn fixed_delta(&self) -> Fixed {
        self.fixed_delta
    }

    pub const fn tick(&self) -> u64 {
        self.tick
    }

    pub const fn capacity(&self) -> usize {
        MAX_BODIES
    }

    pub const fn body_count(&self) -> usize {
        self.body_count
    }

    pub const fn rng_state(&self) -> u64 {
        self.rng.state()
    }

    pub fn spawn_body(
        &mut self,
        position: FixedVec3,
        velocity: FixedVec3,
        acceleration: FixedVec3,
    ) -> Result<SimBodyId, EngineError> {
        if self.body_count == MAX_BODIES {
            return Err(EngineError::SimCapacityExceeded {
                capacity: MAX_BODIES,
            });
        }
        let id = SimBodyId::from_raw(self.next_body_id);
        self.next_body_id = self.next_body_id.checked_add(1).ok_or({
            EngineError::SimInvalidConfig {
                field: "body_id".to_string(),
                reason: "body id counter overflowed".to_string(),
            }
        })?;
        let body = SimBody {
            id,
            position,
            velocity,
            acceleration,
        };
        for (index, slot) in self.bodies.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(body);
                self.body_slots.insert(id.index(), index);
                self.body_count += 1;
                return Ok(id);
            }
        }
        Err(EngineError::SimCapacityExceeded {
            capacity: MAX_BODIES,
        })
    }

    pub fn from_snapshot_parts(
        config: SimConfig,
        tick: u64,
        next_body_id: u32,
        rng_state: u64,
        bodies: &[SimBodyState],
    ) -> Result<Self, EngineError> {
        Self::from_snapshot_parts_with_attacks(
            config,
            tick,
            next_body_id,
            rng_state,
            bodies,
            &[],
            0,
            0,
        )
    }

    /// Restore a sim from snapshot parts. `attack_events_digest` and
    /// `attack_events_total` are the persistent determinism state (set directly
    /// from the snapshot); `attack_events` is the retained window (a snapshot of
    /// a long-running sim may hold fewer events than `attack_events_total`).
    #[allow(clippy::too_many_arguments)]
    pub fn from_snapshot_parts_with_attacks(
        config: SimConfig,
        tick: u64,
        next_body_id: u32,
        rng_state: u64,
        bodies: &[SimBodyState],
        attack_events: &[SimAttackEvent],
        attack_events_digest: u64,
        attack_events_total: u64,
    ) -> Result<Self, EngineError> {
        if bodies.len() > MAX_BODIES {
            return Err(EngineError::SimCapacityExceeded {
                capacity: MAX_BODIES,
            });
        }
        if (attack_events.len() as u64) > attack_events_total {
            return Err(EngineError::SimInvalidConfig {
                field: "attack_events".to_string(),
                reason: "retained attack window cannot exceed the total attack count".to_string(),
            });
        }
        if attack_events.len() > ATTACK_EVENT_RETENTION {
            return Err(EngineError::SimInvalidConfig {
                field: "attack_events".to_string(),
                reason: "retained attack window exceeds retention capacity".to_string(),
            });
        }
        let mut sim = Self::new(config, 1)?;
        sim.tick = tick;
        sim.next_body_id = next_body_id;
        sim.rng = DeterministicRng::from_state(rng_state);
        sim.attack_events_digest = attack_events_digest;
        sim.attack_events_total = attack_events_total;
        for body in bodies {
            if body.id.index() >= next_body_id {
                return Err(EngineError::SimInvalidConfig {
                    field: "next_body_id".to_string(),
                    reason: "must be greater than every restored body id".to_string(),
                });
            }
            for (index, slot) in sim.bodies.iter_mut().enumerate() {
                if slot.is_none() {
                    *slot = Some(SimBody {
                        id: body.id,
                        position: body.position,
                        velocity: body.velocity,
                        acceleration: body.acceleration,
                    });
                    sim.body_slots.insert(body.id.index(), index);
                    sim.body_count += 1;
                    break;
                }
            }
        }
        for event in attack_events {
            if event.tick > tick {
                return Err(EngineError::SimInvalidConfig {
                    field: "attack.tick".to_string(),
                    reason: "restored attack events cannot be newer than the snapshot tick"
                        .to_string(),
                });
            }
            sim.validate_attack_event(*event)?;
            sim.retain_attack_event(*event);
        }
        Ok(sim)
    }

    pub fn body_state(&self, id: SimBodyId) -> Option<SimBodyState> {
        let slot_index = *self.body_slots.get(&id.index())?;
        self.bodies[slot_index].as_ref().map(|body| SimBodyState {
            id: body.id,
            position: body.position,
            velocity: body.velocity,
            acceleration: body.acceleration,
        })
    }

    pub fn despawn_body(&mut self, id: SimBodyId) -> Result<SimBodyState, EngineError> {
        let slot_index = self
            .body_slots
            .remove(&id.index())
            .ok_or(EngineError::SimBodyMissing { id: id.index() })?;
        let body = self.bodies[slot_index]
            .take()
            .ok_or(EngineError::SimBodyMissing { id: id.index() })?;
        self.body_count -= 1;
        Ok(SimBodyState {
            id: body.id,
            position: body.position,
            velocity: body.velocity,
            acceleration: body.acceleration,
        })
    }

    pub fn body_states(&self) -> impl Iterator<Item = SimBodyState> + '_ {
        self.bodies.iter().flatten().map(|body| SimBodyState {
            id: body.id,
            position: body.position,
            velocity: body.velocity,
            acceleration: body.acceleration,
        })
    }

    /// The retained window of recent attack events (bounded by
    /// [`ATTACK_EVENT_RETENTION`]); for sessions with more than that many
    /// attacks this is a suffix of the full history, not all of it. Use
    /// [`Self::attack_event_count`] for the full total.
    pub fn attack_events(&self) -> impl Iterator<Item = SimAttackEvent> + '_ {
        self.attack_events.iter().copied()
    }

    /// The full attack total as a `u64` (for snapshot capture).
    pub fn attack_events_total(&self) -> u64 {
        self.attack_events_total
    }

    /// Total attacks recorded over the whole session, independent of how many
    /// events are retained in the `attack_events` log.
    pub fn attack_event_count(&self) -> usize {
        self.attack_events_total as usize
    }

    /// Persistent order-dependent digest of every attack ever recorded.
    pub fn attack_events_digest(&self) -> u64 {
        self.attack_events_digest
    }

    pub fn set_body_velocity(
        &mut self,
        id: SimBodyId,
        velocity: FixedVec3,
    ) -> Result<(), EngineError> {
        self.body_mut(id).map(|body| {
            body.velocity = velocity;
        })
    }

    pub fn set_body_acceleration(
        &mut self,
        id: SimBodyId,
        acceleration: FixedVec3,
    ) -> Result<(), EngineError> {
        self.body_mut(id).map(|body| {
            body.acceleration = acceleration;
        })
    }

    pub fn add_body_velocity(
        &mut self,
        id: SimBodyId,
        delta: FixedVec3,
    ) -> Result<(), EngineError> {
        let body = self.body_mut(id)?;
        body.velocity = body.velocity.checked_add(delta)?;
        Ok(())
    }

    pub fn record_attack_at(
        &mut self,
        tick: u64,
        attacker: SimBodyId,
        target: SimBodyId,
        damage_fp: i64,
    ) -> Result<(), EngineError> {
        let event = SimAttackEvent {
            tick,
            attacker,
            target,
            damage_fp,
        };
        self.validate_attack_event(event)?;
        self.push_attack_event(event)?;
        Ok(())
    }

    /// Record a live attack: fold it into the persistent digest, bump the
    /// persistent total, and retain it in the bounded window. The digest is the
    /// determinism source of truth for attacks; `attack_events` is a bounded
    /// log for the animation system and evidence readback.
    fn push_attack_event(&mut self, event: SimAttackEvent) -> Result<(), EngineError> {
        self.attack_events_digest = fold_attack_event(self.attack_events_digest, event);
        self.attack_events_total = self
            .attack_events_total
            .checked_add(1)
            .ok_or(EngineError::SimTickOverflow)?;
        self.retain_attack_event(event);
        Ok(())
    }

    /// Append an attack to the bounded retention window, evicting the oldest if
    /// over capacity. Does NOT touch the digest or total — used both by the live
    /// path (after folding) and by snapshot restore (which sets digest/total
    /// directly from the snapshot).
    fn retain_attack_event(&mut self, event: SimAttackEvent) {
        self.attack_events.push_back(event);
        while self.attack_events.len() > ATTACK_EVENT_RETENTION {
            self.attack_events.pop_front();
        }
    }

    pub fn step(&mut self) -> Result<SimStepReport, EngineError> {
        let _hot_path = enter_hot_path(HotPathPhase::SimTick);
        self.tick = self
            .tick
            .checked_add(1)
            .ok_or(EngineError::SimTickOverflow)?;
        let mut updated = 0;
        for slot in &mut self.bodies {
            let Some(body) = slot else {
                continue;
            };
            body.velocity = body
                .velocity
                .checked_add(body.acceleration.checked_mul_scalar(self.fixed_delta)?)?;
            body.position = body
                .position
                .checked_add(body.velocity.checked_mul_scalar(self.fixed_delta)?)?;
            updated += 1;
        }
        Ok(SimStepReport {
            tick: self.tick,
            bodies_updated: updated,
            state_hash: self.state_hash(),
        })
    }

    pub fn run_ticks(&mut self, ticks: u64) -> Result<SimRunReport, EngineError> {
        let start_tick = self.tick;
        let mut bodies_updated = 0_u64;
        for _ in 0..ticks {
            bodies_updated = bodies_updated
                .checked_add(self.step()?.bodies_updated as u64)
                .ok_or(EngineError::SimTickOverflow)?;
        }
        Ok(SimRunReport {
            start_tick,
            final_tick: self.tick,
            steps: ticks,
            bodies_updated,
            state_hash: self.state_hash(),
        })
    }

    pub fn run_until(
        &mut self,
        max_ticks: u64,
        mut predicate: impl FnMut(&Self) -> bool,
    ) -> Result<SimRunReport, EngineError> {
        let start_tick = self.tick;
        let mut bodies_updated = 0_u64;
        let mut steps = 0_u64;
        while steps < max_ticks && !predicate(self) {
            let report = self.step()?;
            bodies_updated = bodies_updated
                .checked_add(report.bodies_updated as u64)
                .ok_or(EngineError::SimTickOverflow)?;
            steps += 1;
        }
        Ok(SimRunReport {
            start_tick,
            final_tick: self.tick,
            steps,
            bodies_updated,
            state_hash: self.state_hash(),
        })
    }

    pub fn state_hash(&self) -> u64 {
        let mut hasher = Fnv1a64::new();
        hash_u64(&mut hasher, self.tick);
        hash_u64(&mut hasher, u64::from(self.config.ticks_per_second));
        hash_u64(&mut hasher, self.rng.state());
        hash_u64(&mut hasher, self.body_count as u64);
        for body in self.bodies.iter().flatten() {
            hash_u64(&mut hasher, u64::from(body.id.index()));
            hash_vec3(&mut hasher, body.position);
            hash_vec3(&mut hasher, body.velocity);
            hash_vec3(&mut hasher, body.acceleration);
        }
        // O(1) in the attack count: mix the persistent total + digest rather than
        // re-hashing the whole retained log every tick (#1463). The digest folds
        // every attack ever recorded, so full-history determinism is preserved.
        hash_u64(&mut hasher, self.attack_events_total);
        hash_u64(&mut hasher, self.attack_events_digest);
        hasher.finish()
    }

    fn body_mut(&mut self, id: SimBodyId) -> Result<&mut SimBody, EngineError> {
        let slot_index = *self
            .body_slots
            .get(&id.index())
            .ok_or(EngineError::SimBodyMissing { id: id.index() })?;
        self.bodies[slot_index]
            .as_mut()
            .ok_or(EngineError::SimBodyMissing { id: id.index() })
    }

    fn validate_attack_event(&self, event: SimAttackEvent) -> Result<(), EngineError> {
        if event.tick > self.tick.saturating_add(1) {
            return Err(EngineError::SimInvalidConfig {
                field: "attack.tick".to_string(),
                reason: "attack events cannot be recorded beyond the next sim tick".to_string(),
            });
        }
        if event.damage_fp <= 0 {
            return Err(EngineError::SimInvalidConfig {
                field: "attack.damage_fp".to_string(),
                reason: "attack damage must be positive fixed-point damage".to_string(),
            });
        }
        if self.body_state(event.attacker).is_none() {
            return Err(EngineError::SimBodyMissing {
                id: event.attacker.index(),
            });
        }
        if self.body_state(event.target).is_none() {
            return Err(EngineError::SimBodyMissing {
                id: event.target.index(),
            });
        }
        Ok(())
    }
}
