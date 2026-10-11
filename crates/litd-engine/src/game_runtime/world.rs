//! The deterministic base-building & economy world (#1785/#837). Ties the lowered ruleset,
//! baked navmesh, player accounts, workers, structures, and resource nodes together and runs
//! them on an integer tick loop. Trained units are spawned as real bodies in a
//! [`FixedTickSim`] at their structure's rally point, so "trains N units to a rally point" is
//! observable as N sim bodies at the rally cell. The world's `state_hash` mixes the economy
//! digest with the sim hash: an identical order stream reproduces an identical hash (#85).

use std::collections::BTreeMap;

use litd_hash::Fnv1a64;

use crate::{
    EngineError,
    nav::{BakedNavMesh, NavCellCoord, NavPath, NavPathQuery, find_path_with_budget},
    sim::{FixedTickSim, SimConfig},
};

use super::economy::{PlayerAccounts, upkeep_tier_for};
use super::harvest::{Worker, WorkerPhase};
use super::lowered::LoweredEconomyRuleset;
use super::structures::{ProductionItem, RallyPoint, Structure, validate_footprint};

#[path = "world_tick.rs"]
mod world_tick;

#[cfg(test)]
#[path = "world_test_support.rs"]
mod world_test_support;

/// Body capacity for trained units in the economy world's sim.
pub const ECONOMY_MAX_UNITS: usize = 256;

fn econ_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::GamePackageInvalidConfig {
        field: format!("economy.{field}"),
        reason: reason.into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickReport {
    pub tick: u64,
    pub deposits_applied: u32,
    pub units_trained: u32,
    pub builds_completed: u32,
}

pub struct EconomyWorld {
    ruleset: LoweredEconomyRuleset,
    navmesh: BakedNavMesh,
    accounts: PlayerAccounts,
    depot: NavCellCoord,
    build_origin: NavCellCoord,
    workers: Vec<Worker>,
    structures: Vec<Structure>,
    node_remaining: BTreeMap<String, u64>,
    sim: FixedTickSim<ECONOMY_MAX_UNITS>,
    next_instance_id: u32,
    tick: u64,
}

impl EconomyWorld {
    pub fn new(
        ruleset: LoweredEconomyRuleset,
        navmesh: BakedNavMesh,
        depot: NavCellCoord,
        build_origin: NavCellCoord,
        seed: u64,
    ) -> Result<Self, EngineError> {
        let accounts = PlayerAccounts::from_ruleset(&ruleset);
        let mut node_remaining = BTreeMap::new();
        for node in &ruleset.nodes {
            node_remaining.insert(node.node_id.clone(), node.amount_total);
        }
        let sim = FixedTickSim::<ECONOMY_MAX_UNITS>::new(SimConfig::new(ruleset.tick_rate)?, seed)?;
        Ok(Self {
            ruleset,
            navmesh,
            accounts,
            depot,
            build_origin,
            workers: Vec::new(),
            structures: Vec::new(),
            node_remaining,
            sim,
            next_instance_id: 0,
            tick: 0,
        })
    }

    pub fn accounts(&self) -> &PlayerAccounts {
        &self.accounts
    }

    /// Whether the lowered economy ruleset defines a unit type with this id
    /// (#1859 spawn validation against lowered unit records).
    pub fn has_unit_type(&self, unit_id: &str) -> bool {
        self.ruleset.unit(unit_id).is_some()
    }

    pub fn tick_count(&self) -> u64 {
        self.tick
    }

    pub fn unit_body_count(&self) -> usize {
        self.sim.body_count()
    }

    pub fn structure(&self, instance_id: u32) -> Option<&Structure> {
        self.structures
            .iter()
            .find(|s| s.instance_id == instance_id)
    }

    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    pub fn worker(&self, worker_id: u32) -> Option<&Worker> {
        self.workers.iter().find(|worker| worker.id == worker_id)
    }

    pub fn node_remaining(&self, node_id: &str) -> u64 {
        self.node_remaining.get(node_id).copied().unwrap_or(0)
    }

    /// The current upkeep tier's income multiplier in permille (parts per thousand), a pure
    /// function of current food usage. Observable so the FSV can show a tier transition change
    /// the income rate (#837). 1000 = full income; a lower value = penalized upkeep.
    pub fn current_income_permille(&self) -> u64 {
        match upkeep_tier_for(&self.ruleset.upkeep_tiers, self.accounts.used_food()) {
            Some(tier) => {
                u64::from(tier.income_numerator) * 1000 / u64::from(tier.income_denominator.max(1))
            }
            None => 1000,
        }
    }

    /// Reserve food/supply against the cap (trigger API for summoned/granted units, #837).
    /// Fails loud at the cap. Used by the runtime's own training path and available to triggers.
    pub fn reserve_supply(&mut self, food: u32) -> Result<u32, EngineError> {
        self.accounts.reserve_food(food)
    }

    /// Grant supply cap directly (trigger API, #837).
    pub fn grant_supply_cap(&mut self, provided: u32) -> Result<u32, EngineError> {
        self.accounts.add_supply_cap(provided)
    }

    /// Spawn a worker at the depot and route it to its assigned node.
    pub fn add_worker(&mut self, id: u32, node_id: &str) -> Result<(), EngineError> {
        if self.workers.iter().any(|worker| worker.id == id) {
            return Err(econ_error(
                "worker.duplicate",
                format!("worker id {id} already exists"),
            ));
        }
        let node = self
            .ruleset
            .nodes
            .iter()
            .find(|n| n.node_id == node_id)
            .ok_or_else(|| econ_error("worker.node", format!("unknown node {node_id}")))?;
        let mut worker = Worker::new(
            id,
            self.depot,
            node_id.to_string(),
            node.resource_key.clone(),
        );
        let target = NavCellCoord::new(node.cell_x, node.cell_z);
        let path = self.path_cells(self.depot, target)?;
        worker.set_path(path, WorkerPhase::ToNode);
        self.workers.push(worker);
        self.workers.sort_by_key(|w| w.id);
        Ok(())
    }

    /// Directly grant resources (trigger API, #837).
    pub fn grant(&mut self, key: &str, amount: u64) -> Result<u64, EngineError> {
        self.accounts.grant(key, amount)
    }

    /// Directly spend resources (trigger API, #837).
    pub fn spend(&mut self, key: &str, amount: u64) -> Result<u64, EngineError> {
        if amount == 0 {
            return Err(econ_error(
                "spend.amount",
                "spend amount must be greater than zero",
            ));
        }
        self.accounts.spend(key, amount)
    }

    /// Place a structure: validate footprint (pathing-aware), charge its cost, and start the
    /// build countdown. Supply is applied only when the build completes.
    pub fn place_structure(
        &mut self,
        struct_id: &str,
        origin_x: i32,
        origin_z: i32,
    ) -> Result<u32, EngineError> {
        let def = self
            .ruleset
            .structure(struct_id)
            .ok_or_else(|| econ_error("place.struct", format!("unknown structure {struct_id}")))?
            .clone();
        let build_ticks = self
            .secs_to_ticks("structure.build_time_secs", struct_id, def.build_time_secs)?
            .max(1);
        let origin = NavCellCoord::new(origin_x, origin_z);
        validate_footprint(
            &self.navmesh,
            origin,
            def.footprint_w,
            def.footprint_h,
            self.build_origin,
        )?;
        // Preflight the complete transaction before mutating either balance.
        for (key, amount) in [
            ("gold", u64::from(def.gold_cost)),
            ("lumber", u64::from(def.lumber_cost)),
        ] {
            if amount > 0 && !self.accounts.can_afford(key, amount) {
                return Err(econ_error(
                    "spend.insufficient",
                    format!(
                        "cannot spend {amount} {key}; have {}",
                        self.accounts.balance(key)
                    ),
                ));
            }
        }
        if def.gold_cost > 0 {
            self.accounts.spend("gold", u64::from(def.gold_cost))?;
        }
        if def.lumber_cost > 0 {
            self.accounts.spend("lumber", u64::from(def.lumber_cost))?;
        }
        let instance_id = self.next_instance_id;
        self.next_instance_id += 1;
        self.structures.push(Structure {
            instance_id,
            struct_id: struct_id.to_string(),
            origin,
            footprint_w: def.footprint_w,
            footprint_h: def.footprint_h,
            build_ticks_remaining: build_ticks,
            complete: false,
            food_provided: def.food_provided,
            gold_cost: def.gold_cost,
            lumber_cost: def.lumber_cost,
            supply_applied: false,
            rally: RallyPoint::Point {
                x: origin.x + def.footprint_w as i32,
                z: origin.z,
            },
            queue: std::collections::VecDeque::new(),
        });
        self.structures.sort_by_key(|s| s.instance_id);
        Ok(instance_id)
    }

    /// Cancel a structure and refund its gold/lumber cost (#1785 b).
    pub fn cancel_structure(&mut self, instance_id: u32) -> Result<(), EngineError> {
        let index = self
            .structures
            .iter()
            .position(|s| s.instance_id == instance_id)
            .ok_or_else(|| econ_error("cancel.missing", format!("no structure {instance_id}")))?;
        let structure = &self.structures[index];
        if structure.complete {
            return Err(econ_error(
                "cancel.complete",
                "cannot cancel a completed structure",
            ));
        }
        let gold_refund = u64::from(structure.gold_cost);
        let lumber_refund = u64::from(structure.lumber_cost);
        self.accounts.checked_grant_balance("gold", gold_refund)?;
        self.accounts
            .checked_grant_balance("lumber", lumber_refund)?;
        self.structures.remove(index);
        self.accounts.grant("gold", gold_refund)?;
        self.accounts.grant("lumber", lumber_refund)?;
        Ok(())
    }

    pub fn set_rally(&mut self, instance_id: u32, rally: RallyPoint) -> Result<(), EngineError> {
        let complete = self
            .structures
            .iter()
            .find(|s| s.instance_id == instance_id)
            .map(|structure| structure.complete)
            .ok_or_else(|| econ_error("rally.missing", format!("no structure {instance_id}")))?;
        if !complete {
            return Err(econ_error(
                "rally.incomplete",
                format!("structure {instance_id} is not complete"),
            ));
        }
        if let RallyPoint::Point { x, z } = rally
            && !self
                .navmesh
                .cells
                .iter()
                .any(|cell| cell.x == x && cell.z == z)
        {
            return Err(econ_error(
                "rally.point",
                format!("rally point ({x},{z}) is outside the navmesh"),
            ));
        }
        let structure = self
            .structures
            .iter_mut()
            .find(|s| s.instance_id == instance_id)
            .expect("structure existence checked");
        structure.rally = rally;
        Ok(())
    }

    /// Queue a unit for training on a completed structure. Charges cost and reserves food up
    /// front; fails loud (typed) at the supply cap so training blocks observably (#1785 c).
    pub fn train_unit(&mut self, instance_id: u32, unit_id: &str) -> Result<(), EngineError> {
        let unit = self
            .ruleset
            .unit(unit_id)
            .ok_or_else(|| econ_error("train.unit", format!("unknown unit {unit_id}")))?
            .clone();
        let can_train = self
            .structures
            .iter()
            .find(|s| s.instance_id == instance_id)
            .map(|s| {
                s.complete
                    && self
                        .ruleset
                        .structure(&s.struct_id)
                        .is_some_and(|d| d.trains.iter().any(|t| t == unit_id))
            })
            .ok_or_else(|| econ_error("train.missing", format!("no structure {instance_id}")))?;
        if !can_train {
            return Err(econ_error(
                "train.forbidden",
                format!("structure {instance_id} cannot train {unit_id}"),
            ));
        }
        let ticks = self
            .secs_to_ticks("unit.build_time_secs", unit_id, unit.build_time_secs)?
            .max(1);
        let used_food_after = self.accounts.checked_food_reservation(unit.food_cost)?;
        // Validate the complete transaction before mutating either balance. Without
        // this preflight, sufficient gold + insufficient lumber spent gold before
        // failing the queue operation.
        for (key, amount) in [
            ("gold", u64::from(unit.gold_cost)),
            ("lumber", u64::from(unit.lumber_cost)),
        ] {
            if amount > 0 && !self.accounts.can_afford(key, amount) {
                return Err(econ_error(
                    "spend.insufficient",
                    format!(
                        "cannot spend {amount} {key}; have {}",
                        self.accounts.balance(key)
                    ),
                ));
            }
        }
        if unit.gold_cost > 0 {
            self.accounts.spend("gold", u64::from(unit.gold_cost))?;
        }
        if unit.lumber_cost > 0 {
            self.accounts.spend("lumber", u64::from(unit.lumber_cost))?;
        }
        self.accounts
            .commit_checked_food_reservation(used_food_after);
        let structure = self
            .structures
            .iter_mut()
            .find(|s| s.instance_id == instance_id)
            .expect("structure existence checked above");
        structure.queue.push_back(ProductionItem {
            unit_id: unit_id.to_string(),
            ticks_remaining: ticks,
            food_cost: unit.food_cost,
            gold_cost: unit.gold_cost,
            lumber_cost: unit.lumber_cost,
        });
        Ok(())
    }

    /// Reassign a worker to a different node (routes it there via a fresh path).
    pub fn reassign_worker(&mut self, worker_id: u32, node_id: &str) -> Result<(), EngineError> {
        let node = self
            .ruleset
            .nodes
            .iter()
            .find(|n| n.node_id == node_id)
            .ok_or_else(|| econ_error("worker.node", format!("unknown node {node_id}")))?
            .clone();
        let from = self
            .workers
            .iter()
            .find(|w| w.id == worker_id)
            .map(|w| w.cell)
            .ok_or_else(|| econ_error("worker.missing", format!("no worker {worker_id}")))?;
        let path = self.path_cells(from, NavCellCoord::new(node.cell_x, node.cell_z))?;
        let worker = self
            .workers
            .iter_mut()
            .find(|w| w.id == worker_id)
            .expect("worker checked");
        worker.node_id = node_id.to_string();
        worker.resource_key = node.resource_key.clone();
        worker.set_path(path, WorkerPhase::ToNode);
        Ok(())
    }

    /// Advance every system by one deterministic tick: workers, builds, production, income.
    pub fn tick(&mut self) -> Result<TickReport, EngineError> {
        self.preflight_tick_workers()?;
        self.preflight_tick_production()?;
        let supply_cap_after = self.preflight_tick_builds()?;
        self.tick = self
            .tick
            .checked_add(1)
            .ok_or(EngineError::SimTickOverflow)?;
        let deposits_applied = self.tick_workers()?;
        let builds_completed = self.tick_builds(supply_cap_after);
        let units_trained = self.tick_production()?;
        self.sim.step()?;
        Ok(TickReport {
            tick: self.tick,
            deposits_applied,
            units_trained,
            builds_completed,
        })
    }

    fn path_cells(
        &self,
        from: NavCellCoord,
        to: NavCellCoord,
    ) -> Result<Vec<NavCellCoord>, EngineError> {
        if from == to {
            return Ok(vec![from]);
        }
        let query = NavPathQuery::new(from, to, self.navmesh.cells.len().max(1) * 4);
        let path: NavPath = find_path_with_budget(&self.navmesh, query)?;
        Ok(path
            .nodes
            .iter()
            .map(|n| NavCellCoord::new(n.x, n.z))
            .collect())
    }

    fn secs_to_ticks(&self, field: &str, id: &str, secs: u32) -> Result<u32, EngineError> {
        secs.checked_mul(self.ruleset.tick_rate).ok_or_else(|| {
            econ_error(
                field,
                format!(
                    "{id} build time {secs} overflows at tick rate {}",
                    self.ruleset.tick_rate
                ),
            )
        })
    }

    /// Deterministic hash of the entire economy + sim state. Identical order streams reproduce
    /// an identical value (the replay-determinism check).
    pub fn state_hash(&self) -> u64 {
        let mut hasher = Fnv1a64::new();
        hasher.update_u64_le(self.tick);
        hasher.update(self.ruleset.lowering_fingerprint.as_bytes());
        self.accounts.hash_into(&mut hasher);
        hasher.update_u64_le(self.node_remaining.len() as u64);
        for (node_id, remaining) in &self.node_remaining {
            hasher.update(node_id.as_bytes());
            hasher.update_u64_le(*remaining);
        }
        hasher.update_u64_le(self.workers.len() as u64);
        for worker in &self.workers {
            worker.hash_into(&mut hasher);
        }
        hasher.update_u64_le(self.structures.len() as u64);
        for structure in &self.structures {
            structure.hash_into(&mut hasher);
        }
        hasher.update_u64_le(self.sim.state_hash());
        hasher.finish()
    }
}
