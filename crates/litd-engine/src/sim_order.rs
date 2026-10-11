//! Deterministic lockstep-style order queue for human and lowered-agent input.

use crate::{
    EngineError,
    math::FixedVec3,
    sim::{FixedTickSim, SimBodyId, SimRunReport},
};
use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SimOrderSourceKind {
    Human = 0,
    LoweredAgent = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SimOrderSource {
    pub kind: SimOrderSourceKind,
    pub id: u32,
}

impl SimOrderSource {
    pub const fn human(id: u32) -> Self {
        Self {
            kind: SimOrderSourceKind::Human,
            id,
        }
    }

    pub const fn lowered_agent(id: u32) -> Self {
        Self {
            kind: SimOrderSourceKind::LoweredAgent,
            id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimOrderKind {
    SetVelocity(FixedVec3),
    SetAcceleration(FixedVec3),
    AddVelocity(FixedVec3),
    ApplyAttack { attacker: SimBodyId, damage_fp: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimOrder {
    pub execute_tick: u64,
    pub source: SimOrderSource,
    pub sequence: u64,
    pub target: SimBodyId,
    pub kind: SimOrderKind,
}

impl SimOrder {
    pub const fn new(
        execute_tick: u64,
        source: SimOrderSource,
        sequence: u64,
        target: SimBodyId,
        kind: SimOrderKind,
    ) -> Self {
        Self {
            execute_tick,
            source,
            sequence,
            target,
            kind,
        }
    }

    pub fn fingerprint(self) -> u64 {
        let mut hasher = Fnv1a64::new();
        hash_u64(&mut hasher, self.execute_tick);
        hash_u64(&mut hasher, self.source.kind as u64);
        hash_u64(&mut hasher, u64::from(self.source.id));
        hash_u64(&mut hasher, self.sequence);
        hash_u64(&mut hasher, u64::from(self.target.index()));
        match self.kind {
            SimOrderKind::SetVelocity(value) => {
                hash_u64(&mut hasher, 1);
                hash_vec3(&mut hasher, value);
            }
            SimOrderKind::SetAcceleration(value) => {
                hash_u64(&mut hasher, 2);
                hash_vec3(&mut hasher, value);
            }
            SimOrderKind::AddVelocity(value) => {
                hash_u64(&mut hasher, 3);
                hash_vec3(&mut hasher, value);
            }
            SimOrderKind::ApplyAttack {
                attacker,
                damage_fp,
            } => {
                hash_u64(&mut hasher, 4);
                hash_u64(&mut hasher, u64::from(attacker.index()));
                hash_i64(&mut hasher, damage_fp);
            }
        }
        hasher.finish()
    }

    fn sort_key(self) -> OrderSortKey {
        OrderSortKey {
            execute_tick: self.execute_tick,
            source_kind: self.source.kind,
            source_id: self.source.id,
            sequence: self.sequence,
            target: self.target.index(),
            fingerprint: self.fingerprint(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct OrderSortKey {
    execute_tick: u64,
    source_kind: SimOrderSourceKind,
    source_id: u32,
    sequence: u64,
    target: u32,
    fingerprint: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimOrderStepReport {
    pub tick: u64,
    pub orders_applied: usize,
    pub bodies_updated: usize,
    pub state_hash: u64,
    pub pending_orders: usize,
}

pub struct SimOrderQueue<const MAX_ORDERS: usize> {
    orders: [Option<SimOrder>; MAX_ORDERS],
    order_count: usize,
    applied_total: u64,
}

impl<const MAX_ORDERS: usize> Default for SimOrderQueue<MAX_ORDERS> {
    fn default() -> Self {
        Self {
            orders: [None; MAX_ORDERS],
            order_count: 0,
            applied_total: 0,
        }
    }
}

impl<const MAX_ORDERS: usize> SimOrderQueue<MAX_ORDERS> {
    pub const fn capacity(&self) -> usize {
        MAX_ORDERS
    }

    pub const fn pending_orders(&self) -> usize {
        self.order_count
    }

    pub const fn applied_total(&self) -> u64 {
        self.applied_total
    }

    pub fn pending_order_ticks(&self) -> Vec<u64> {
        let mut ticks = self
            .orders
            .iter()
            .flatten()
            .map(|order| order.execute_tick)
            .collect::<Vec<_>>();
        ticks.sort_unstable();
        ticks
    }

    /// The still-pending orders in deterministic apply order (by `sort_key`).
    /// Additive readback (#850): the save/load spine serializes the exact set of
    /// not-yet-applied orders so a reloaded match resumes with the same queue
    /// position. Iteration order here matches the order `step` would apply them.
    pub fn pending_order_list(&self) -> Vec<SimOrder> {
        let mut orders = self.orders.iter().flatten().copied().collect::<Vec<_>>();
        orders.sort_by_key(|order| order.sort_key());
        orders
    }

    pub fn pending_order_fingerprints(&self) -> Vec<u64> {
        let mut fingerprints = self
            .orders
            .iter()
            .flatten()
            .map(|order| order.fingerprint())
            .collect::<Vec<_>>();
        fingerprints.sort_unstable();
        fingerprints
    }

    pub fn enqueue(&mut self, order: SimOrder) -> Result<u64, EngineError> {
        if self.order_count == MAX_ORDERS {
            return Err(EngineError::SimOrderCapacityExceeded {
                capacity: MAX_ORDERS,
            });
        }
        let fingerprint = order.fingerprint();
        for slot in &mut self.orders {
            if slot.is_none() {
                *slot = Some(order);
                self.order_count += 1;
                return Ok(fingerprint);
            }
        }
        Err(EngineError::SimOrderCapacityExceeded {
            capacity: MAX_ORDERS,
        })
    }

    pub fn step<const MAX_BODIES: usize>(
        &mut self,
        sim: &mut FixedTickSim<MAX_BODIES>,
    ) -> Result<SimOrderStepReport, EngineError> {
        let execute_tick = sim
            .tick()
            .checked_add(1)
            .ok_or(EngineError::SimTickOverflow)?;
        self.reject_past_due_orders(execute_tick)?;
        let orders_applied = self.apply_for_tick(sim, execute_tick)?;
        let step = sim.step()?;
        Ok(SimOrderStepReport {
            tick: step.tick,
            orders_applied,
            bodies_updated: step.bodies_updated,
            state_hash: step.state_hash,
            pending_orders: self.order_count,
        })
    }

    pub fn run_ticks<const MAX_BODIES: usize>(
        &mut self,
        sim: &mut FixedTickSim<MAX_BODIES>,
        ticks: u64,
    ) -> Result<SimRunReport, EngineError> {
        let start_tick = sim.tick();
        let mut bodies_updated = 0_u64;
        for _ in 0..ticks {
            bodies_updated = bodies_updated
                .checked_add(self.step(sim)?.bodies_updated as u64)
                .ok_or(EngineError::SimTickOverflow)?;
        }
        Ok(SimRunReport {
            start_tick,
            final_tick: sim.tick(),
            steps: ticks,
            bodies_updated,
            state_hash: sim.state_hash(),
        })
    }

    fn apply_for_tick<const MAX_BODIES: usize>(
        &mut self,
        sim: &mut FixedTickSim<MAX_BODIES>,
        execute_tick: u64,
    ) -> Result<usize, EngineError> {
        let mut applied = 0;
        while let Some(index) = self.next_order_index(execute_tick) {
            let order = self.orders[index].take().expect("index holds an order");
            self.order_count -= 1;
            apply_order(sim, order)?;
            self.applied_total += 1;
            applied += 1;
        }
        Ok(applied)
    }

    /// Fail loud on any order whose `execute_tick` already elapsed. #1340 pruned
    /// these silently (warn-only); #1463 supersedes that: a past-due order is a
    /// scheduler bug, and under lockstep netplay dropping a late order on one
    /// peer while another applied it on time diverges their state hashes
    /// (#800 §4). The caller must see it and decide how to recover — never a
    /// warn-and-drop. `net_authority` future-dates every order by
    /// `input_delay_ticks`, so in the authoritative path this can only fire on a
    /// genuine scheduler bug; it is reachable only through raw-API misuse.
    fn reject_past_due_orders(&self, execute_tick: u64) -> Result<(), EngineError> {
        let mut orders_past_due = 0_usize;
        let mut earliest_execute_tick = u64::MAX;
        for order in self.orders.iter().flatten() {
            if order.execute_tick < execute_tick {
                orders_past_due += 1;
                earliest_execute_tick = earliest_execute_tick.min(order.execute_tick);
            }
        }
        if orders_past_due == 0 {
            return Ok(());
        }
        Err(EngineError::SimOrderPastDue {
            execute_tick,
            earliest_execute_tick,
            orders_past_due,
        })
    }

    fn next_order_index(&self, execute_tick: u64) -> Option<usize> {
        self.orders
            .iter()
            .enumerate()
            .filter_map(|(index, order)| {
                let order = (*order)?;
                (order.execute_tick == execute_tick).then_some((index, order.sort_key()))
            })
            .min_by_key(|(_, key)| *key)
            .map(|(index, _)| index)
    }
}

fn apply_order<const MAX_BODIES: usize>(
    sim: &mut FixedTickSim<MAX_BODIES>,
    order: SimOrder,
) -> Result<(), EngineError> {
    match order.kind {
        SimOrderKind::SetVelocity(value) => sim.set_body_velocity(order.target, value),
        SimOrderKind::SetAcceleration(value) => sim.set_body_acceleration(order.target, value),
        SimOrderKind::AddVelocity(value) => sim.add_body_velocity(order.target, value),
        SimOrderKind::ApplyAttack {
            attacker,
            damage_fp,
        } => sim.record_attack_at(order.execute_tick, attacker, order.target, damage_fp),
    }
}

fn hash_vec3(hasher: &mut Fnv1a64, value: FixedVec3) {
    hash_i64(hasher, value.x.raw());
    hash_i64(hasher, value.y.raw());
    hash_i64(hasher, value.z.raw());
}

fn hash_i64(hasher: &mut Fnv1a64, value: i64) {
    hasher.update_i64_le(value);
}

fn hash_u64(hasher: &mut Fnv1a64, value: u64) {
    hasher.update_u64_le(value);
}
