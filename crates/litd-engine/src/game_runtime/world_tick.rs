//! Per-tick system advance for [`super::EconomyWorld`] (#1785). Split out of `world.rs` to
//! hold the <500-line ratchet; this is a CHILD module of `world`, so it may touch the world's
//! private fields directly. Each function is one deterministic system: workers, builds, and
//! production. All movement/progress is integer-tick based -- no wall clock -- so replay is
//! exact.

use std::collections::BTreeMap;

use crate::{
    EngineError,
    math::{Fixed, FixedVec3},
    nav::NavCellCoord,
    sim::SimBodyId,
};

use super::super::economy::{apply_upkeep_multiplier, upkeep_tier_for};
use super::super::harvest::{WorkerEvent, WorkerPhase};
use super::super::structures::RallyPoint;
use super::EconomyWorld;

impl EconomyWorld {
    pub(super) fn preflight_tick_workers(&self) -> Result<(), EngineError> {
        let tier = upkeep_tier_for(&self.ruleset.upkeep_tiers, self.accounts.used_food());
        let mut credits_by_resource = BTreeMap::<String, u64>::new();
        for worker in &self.workers {
            if worker.phase != WorkerPhase::Deposit || worker.deposit_cooldown > 1 {
                continue;
            }
            let credited = match tier {
                Some(tier) => apply_upkeep_multiplier(worker.carried, tier)?,
                None => worker.carried,
            };
            let total = credits_by_resource
                .entry(worker.resource_key.clone())
                .or_default();
            *total = total.checked_add(credited).ok_or_else(|| {
                super::econ_error(
                    "grant.overflow",
                    format!(
                        "same-tick deposits would overflow {} credit",
                        worker.resource_key
                    ),
                )
            })?;
        }
        for (resource_key, credited) in credits_by_resource {
            self.accounts
                .checked_grant_balance(&resource_key, credited)?;
        }
        Ok(())
    }

    pub(super) fn tick_workers(&mut self) -> Result<u32, EngineError> {
        let cells_per_sec = self.ruleset.worker_cells_per_sec;
        let tick_rate = self.ruleset.tick_rate;
        let deposit_ticks = self.ruleset.deposit_ticks;
        let mut deposits = 0_u32;
        let ids: Vec<u32> = self.workers.iter().map(|w| w.id).collect();
        for id in ids {
            let event = {
                let worker = self
                    .workers
                    .iter_mut()
                    .find(|w| w.id == id)
                    .expect("worker id");
                worker.tick(cells_per_sec, tick_rate, deposit_ticks)
            };
            match event {
                WorkerEvent::None => {}
                WorkerEvent::ArrivedAtNode => self.on_worker_at_node(id)?,
                WorkerEvent::ArrivedAtDepot => {}
                WorkerEvent::DepositComplete(amount) => {
                    self.on_worker_deposit(id, amount)?;
                    deposits += 1;
                }
            }
        }
        Ok(deposits)
    }

    fn on_worker_at_node(&mut self, worker_id: u32) -> Result<(), EngineError> {
        let (node_id, resource_key) = {
            let worker = self
                .workers
                .iter()
                .find(|w| w.id == worker_id)
                .expect("worker");
            (worker.node_id.clone(), worker.resource_key.clone())
        };
        let yield_per_trip = self
            .ruleset
            .nodes
            .iter()
            .find(|n| n.node_id == node_id)
            .map(|n| n.yield_per_trip)
            .unwrap_or(0);
        let remaining = self.node_remaining.get(&node_id).copied().unwrap_or(0);
        let taken = yield_per_trip.min(remaining);
        if taken == 0 {
            let worker = self
                .workers
                .iter_mut()
                .find(|w| w.id == worker_id)
                .expect("worker");
            worker.phase = WorkerPhase::Idle;
            return Ok(());
        }
        self.node_remaining.insert(node_id, remaining - taken);
        let path = self.path_cells(self.worker_cell(worker_id), self.depot)?;
        let worker = self
            .workers
            .iter_mut()
            .find(|w| w.id == worker_id)
            .expect("worker");
        worker.carried = taken;
        worker.resource_key = resource_key;
        worker.set_path(path, WorkerPhase::ToDepot);
        Ok(())
    }

    fn on_worker_deposit(&mut self, worker_id: u32, amount: u64) -> Result<(), EngineError> {
        let (resource_key, node_id) = {
            let worker = self
                .workers
                .iter()
                .find(|w| w.id == worker_id)
                .expect("worker");
            (worker.resource_key.clone(), worker.node_id.clone())
        };
        // Upkeep multiplier applies to income at deposit time (food-tier scaled, #837).
        let tier = upkeep_tier_for(&self.ruleset.upkeep_tiers, self.accounts.used_food());
        let credited = match tier {
            Some(tier) => apply_upkeep_multiplier(amount, tier)?,
            None => amount,
        };
        self.accounts.grant(&resource_key, credited)?;
        if self.node_remaining.get(&node_id).copied().unwrap_or(0) == 0 {
            let worker = self
                .workers
                .iter_mut()
                .find(|w| w.id == worker_id)
                .expect("worker");
            worker.phase = WorkerPhase::Idle;
            return Ok(());
        }
        let node_cell = self
            .ruleset
            .nodes
            .iter()
            .find(|n| n.node_id == node_id)
            .map(|n| NavCellCoord::new(n.cell_x, n.cell_z))
            .expect("node cell");
        let path = self.path_cells(self.depot, node_cell)?;
        let worker = self
            .workers
            .iter_mut()
            .find(|w| w.id == worker_id)
            .expect("worker");
        worker.set_path(path, WorkerPhase::ToNode);
        Ok(())
    }

    pub(super) fn preflight_tick_builds(&self) -> Result<u32, EngineError> {
        let mut supply_cap_after = u128::from(self.accounts.supply_cap());
        for structure in &self.structures {
            if !structure.complete
                && structure.build_ticks_remaining <= 1
                && !structure.supply_applied
            {
                supply_cap_after = supply_cap_after
                    .checked_add(u128::from(structure.food_provided))
                    .ok_or_else(|| {
                        super::econ_error(
                            "supply_cap.overflow",
                            "completed structure supply exceeds the supported cap",
                        )
                    })?;
            }
        }
        u32::try_from(supply_cap_after).map_err(|_| {
            super::econ_error(
                "supply_cap.overflow",
                format!(
                    "completed structure supply would raise cap {} above {}",
                    supply_cap_after,
                    u32::MAX
                ),
            )
        })
    }

    pub(super) fn tick_builds(&mut self, supply_cap_after: u32) -> u32 {
        let mut completed = 0_u32;
        for structure in &mut self.structures {
            if structure.tick_build() {
                completed += 1;
                if !structure.supply_applied {
                    structure.supply_applied = true;
                }
            }
        }
        self.accounts.commit_checked_supply_cap(supply_cap_after);
        completed
    }

    pub(super) fn preflight_tick_production(&self) -> Result<(), EngineError> {
        let completions_due = self
            .structures
            .iter()
            .filter(|structure| {
                structure.complete
                    && structure
                        .queue
                        .front()
                        .is_some_and(|item| item.ticks_remaining <= 1)
            })
            .count();
        let capacity = self.sim.capacity();
        let body_count_after = self
            .sim
            .body_count()
            .checked_add(completions_due)
            .ok_or(EngineError::SimCapacityExceeded { capacity })?;
        if body_count_after > capacity {
            return Err(EngineError::SimCapacityExceeded { capacity });
        }
        Ok(())
    }

    pub(super) fn tick_production(&mut self) -> Result<u32, EngineError> {
        // Collect the (rally, structure-origin) pair for every unit that finished
        // this tick BEFORE touching the sim: resolving a `RallyPoint::Unit` reads
        // `self.sim` (the live target body), which cannot alias the `&mut
        // self.structures` iteration below.
        let mut pending: Vec<(RallyPoint, NavCellCoord)> = Vec::new();
        for structure in &mut self.structures {
            if !structure.complete {
                continue;
            }
            let Some(front) = structure.queue.front_mut() else {
                continue;
            };
            front.ticks_remaining = front.ticks_remaining.saturating_sub(1);
            if front.ticks_remaining == 0 {
                structure.queue.pop_front();
                pending.push((structure.rally, structure.origin));
            }
        }
        let mut trained = 0_u32;
        for (rally, origin) in pending {
            let cell = self.resolve_rally_cell(rally, origin);
            self.sim.spawn_body(
                FixedVec3::new(
                    Fixed::from_i32(cell.0),
                    Fixed::ZERO,
                    Fixed::from_i32(cell.1),
                ),
                FixedVec3::ZERO,
                FixedVec3::ZERO,
            )?;
            trained += 1;
        }
        Ok(trained)
    }

    /// Resolve a structure's rally to a concrete grid cell. `RallyPoint::Unit`
    /// follows the LIVE target sim body's current cell (fixing the #1842 gap where
    /// unit-target rally silently fell back to the structure origin); if that body
    /// is gone the rally degrades to the producing structure's origin so a
    /// trained unit is never spawned into hyperspace.
    fn resolve_rally_cell(&self, rally: RallyPoint, origin: NavCellCoord) -> (i32, i32) {
        match rally {
            RallyPoint::Point { x, z } => (x, z),
            RallyPoint::Unit(body_id) => match self.sim.body_state(SimBodyId::from_raw(body_id)) {
                Some(state) => (
                    fixed_to_cell(state.position.x),
                    fixed_to_cell(state.position.z),
                ),
                None => (origin.x, origin.z),
            },
        }
    }

    fn worker_cell(&self, worker_id: u32) -> NavCellCoord {
        self.workers
            .iter()
            .find(|w| w.id == worker_id)
            .map(|w| w.cell)
            .unwrap_or(self.depot)
    }
}

/// Floor a fixed-point world coordinate to its integer grid cell. An arithmetic
/// right shift by the fractional bit count floors toward negative infinity, which
/// matches the cell mapping used elsewhere in the runtime (deterministic, no float).
fn fixed_to_cell(coord: Fixed) -> i32 {
    (coord.raw() >> Fixed::FRACTIONAL_BITS) as i32
}

#[cfg(test)]
mod rally_to_unit_tests {
    //! #1842/#1843 falsifier: a `RallyPoint::Unit` must resolve to the target body's LIVE
    //! cell at production time, not the structure origin and not a spawn-time snapshot.
    //! Under the reviewer's revert-mutation (the `Unit` arm of `resolve_rally_cell` returns
    //! the structure origin) the trained unit lands on the origin and the assertion fails.

    use crate::game_runtime::lowered::{
        EconomyRulesetAuthor, LoweredResourceKind, LoweredStructureType, LoweredUnitEconomy,
        LoweredUpkeepTier,
    };
    use crate::math::{Fixed, FixedVec3};
    use crate::nav::{
        NavBakeInput, NavCellCoord, NavPathingFlags, NavTerrainCellInput, bake_navmesh,
    };
    use crate::sim::SimBodyId;

    use crate::game_runtime::structures::RallyPoint;
    use crate::game_runtime::world::EconomyWorld;

    const TICK_RATE: u32 = 4;
    const GRID: i32 = 16;

    fn rally_world() -> EconomyWorld {
        let ruleset = EconomyRulesetAuthor::new(TICK_RATE, 8, 1)
            .resource(LoweredResourceKind {
                key: "gold".to_string(),
                display_name: "Gold".to_string(),
                starting_balance: 10_000,
            })
            .resource(LoweredResourceKind {
                key: "lumber".to_string(),
                display_name: "Lumber".to_string(),
                starting_balance: 10_000,
            })
            .upkeep_tier(LoweredUpkeepTier {
                name: "no_upkeep".to_string(),
                food_min: 0,
                food_max: 100,
                income_numerator: 1,
                income_denominator: 1,
            })
            .unit(LoweredUnitEconomy {
                unit_id: "footman".to_string(),
                gold_cost: 10,
                lumber_cost: 0,
                food_cost: 0,
                build_time_secs: 1,
                food_provided: 0,
            })
            .structure(LoweredStructureType {
                struct_id: "barracks".to_string(),
                gold_cost: 10,
                lumber_cost: 0,
                build_time_secs: 1,
                footprint_w: 1,
                footprint_h: 1,
                food_provided: 50,
                trains: vec!["footman".to_string()],
            })
            .lower()
            .expect("lower rally ruleset");
        let mut cells = Vec::new();
        for z in 0..GRID {
            for x in 0..GRID {
                cells.push(NavTerrainCellInput {
                    id: format!("c_{x}_{z}"),
                    x,
                    z,
                    height_fp: 0,
                    flags: NavPathingFlags {
                        walkable: true,
                        buildable: true,
                        flyable: true,
                        blocked: false,
                    },
                });
            }
        }
        let navmesh = bake_navmesh(&NavBakeInput::new(65536, cells, Vec::new())).expect("bake");
        EconomyWorld::new(
            ruleset,
            navmesh,
            NavCellCoord::new(0, 0),
            NavCellCoord::new(0, 0),
            0x1842,
        )
        .expect("rally world")
    }

    fn cell_of(world: &EconomyWorld, body: SimBodyId) -> (i32, i32) {
        let state = world.sim.body_state(body).expect("body present");
        (
            super::fixed_to_cell(state.position.x),
            super::fixed_to_cell(state.position.z),
        )
    }

    #[test]
    fn rally_to_unit_follows_live_target_cell() {
        let mut world = rally_world();
        // Place + complete a barracks at (2,2) (the structure origin the mutation falls to).
        let barracks = world.place_structure("barracks", 2, 2).expect("place");
        for _ in 0..TICK_RATE + 1 {
            world.tick().expect("build tick");
        }
        assert!(world.structure(barracks).expect("s").complete);

        // Spawn a target body at cell (5,5) moving +1 cell per tick (velocity 4 cells/sec
        // at tick_rate 4 -> exactly one cell per integer tick). It gets sim body id 0.
        let target = world
            .sim
            .spawn_body(
                FixedVec3::new(Fixed::from_i32(5), Fixed::ZERO, Fixed::from_i32(5)),
                FixedVec3::new(Fixed::from_i32(4), Fixed::ZERO, Fixed::from_i32(4)),
                FixedVec3::ZERO,
            )
            .expect("spawn target");
        assert_eq!(cell_of(&world, target), (5, 5));

        // Rally the barracks at the LIVE target body, then train a footman.
        world
            .set_rally(barracks, RallyPoint::Unit(target.index()))
            .expect("set rally to unit");
        world.train_unit(barracks, "footman").expect("train");

        // Advance until the footman completes (build 1s * tick_rate). Each tick also steps
        // the sim, so the target keeps moving AFTER the rally was set.
        for _ in 0..TICK_RATE {
            world.tick().expect("train tick");
        }

        // The trained footman is sim body id 1 (target was id 0).
        let trained = SimBodyId::from_raw(1);
        let trained_cell = cell_of(&world, trained);
        // At the tick the footman spawned, the target had moved to (8,8); it then stepped
        // on to (9,9). The trained unit must sit on the target's cell at RESOLUTION time.
        assert_eq!(
            trained_cell,
            (8, 8),
            "trained unit rallied to the target body's LIVE cell"
        );
        assert_ne!(
            trained_cell,
            (2, 2),
            "must NOT fall back to the structure origin (the #1842 revert)"
        );
        assert_ne!(
            trained_cell,
            (5, 5),
            "must NOT use the rally-set-time snapshot of the target"
        );
        // The target itself continued past the resolution cell -> proves it kept moving.
        assert_eq!(cell_of(&world, target), (9, 9));
    }
}
