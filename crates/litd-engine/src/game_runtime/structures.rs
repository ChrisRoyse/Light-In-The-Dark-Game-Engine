//! Structures: pathing-aware footprint placement, build progress, production/training queues,
//! and rally points (#1785 b/c).
//!
//! Footprint validation consumes the baked navmesh (`nav/`): every cell the structure would
//! occupy must exist, be flagged buildable, and not be pathing-blocked, and the footprint must
//! be reachable from the build origin via an A* path (pathing-aware). Build and training
//! progress are counted in integer ticks -- no wall clock -- so replay is exact.

use std::collections::VecDeque;

use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    nav::{BakedNavMesh, NavCellCoord, NavPathQuery, find_path_with_budget},
};

fn econ_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::GamePackageInvalidConfig {
        field: format!("economy.{field}"),
        reason: reason.into(),
    }
}

/// Where trained units gather. A point target is a grid cell; a unit target follows a
/// sim body (resolved to its live cell at spawn time, #1843). Serde-derived so it can
/// travel in the deterministic session order log (`SetRallyPoint`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RallyPoint {
    Point { x: i32, z: i32 },
    Unit(u32),
}

/// One queued production job. `unit_id` is a lowered unit record id; costs were already paid
/// on enqueue, so a cancel refunds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionItem {
    pub unit_id: String,
    pub ticks_remaining: u32,
    pub food_cost: u32,
    pub gold_cost: u32,
    pub lumber_cost: u32,
}

/// A placed structure. During construction `build_ticks_remaining > 0`; once zero it is
/// `complete` and may run its production queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Structure {
    pub instance_id: u32,
    pub struct_id: String,
    pub origin: NavCellCoord,
    pub footprint_w: u32,
    pub footprint_h: u32,
    pub build_ticks_remaining: u32,
    pub complete: bool,
    pub food_provided: u32,
    pub gold_cost: u32,
    pub lumber_cost: u32,
    pub supply_applied: bool,
    pub rally: RallyPoint,
    pub queue: VecDeque<ProductionItem>,
}

impl Structure {
    pub fn footprint_cells(&self) -> Vec<NavCellCoord> {
        let mut cells = Vec::new();
        for dz in 0..self.footprint_h as i32 {
            for dx in 0..self.footprint_w as i32 {
                cells.push(NavCellCoord::new(self.origin.x + dx, self.origin.z + dz));
            }
        }
        cells
    }

    /// Advance construction by one tick. Returns true on the tick the build completes.
    pub fn tick_build(&mut self) -> bool {
        if self.complete {
            return false;
        }
        self.build_ticks_remaining = self.build_ticks_remaining.saturating_sub(1);
        if self.build_ticks_remaining == 0 {
            self.complete = true;
            return true;
        }
        false
    }

    pub fn hash_into(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(u64::from(self.instance_id));
        hasher.update(self.struct_id.as_bytes());
        hasher.update_i64_le(i64::from(self.origin.x));
        hasher.update_i64_le(i64::from(self.origin.z));
        hasher.update_u64_le(u64::from(self.build_ticks_remaining));
        hasher.update_u64_le(u64::from(self.complete));
        hasher.update_u64_le(u64::from(self.supply_applied));
        match self.rally {
            RallyPoint::Point { x, z } => {
                hasher.update_u64_le(1);
                hasher.update_i64_le(i64::from(x));
                hasher.update_i64_le(i64::from(z));
            }
            RallyPoint::Unit(id) => {
                hasher.update_u64_le(2);
                hasher.update_u64_le(u64::from(id));
            }
        }
        hasher.update_u64_le(self.queue.len() as u64);
        for item in &self.queue {
            hasher.update(item.unit_id.as_bytes());
            hasher.update_u64_le(u64::from(item.ticks_remaining));
            hasher.update_u64_le(u64::from(item.food_cost));
        }
    }
}

/// Validate a structure's footprint against the baked navmesh: existence + buildable +
/// unblocked for every occupied cell, and A*-reachability from `build_origin`.
pub fn validate_footprint(
    navmesh: &BakedNavMesh,
    origin: NavCellCoord,
    footprint_w: u32,
    footprint_h: u32,
    build_origin: NavCellCoord,
) -> Result<(), EngineError> {
    if footprint_w == 0 || footprint_h == 0 {
        return Err(econ_error("place.footprint", "footprint must be non-zero"));
    }
    for dz in 0..footprint_h as i32 {
        for dx in 0..footprint_w as i32 {
            let coord = NavCellCoord::new(origin.x + dx, origin.z + dz);
            let cell = navmesh
                .cells
                .iter()
                .find(|c| c.x == coord.x && c.z == coord.z)
                .ok_or_else(|| {
                    econ_error(
                        "place.oob",
                        format!(
                            "footprint cell ({},{}) is outside the navmesh",
                            coord.x, coord.z
                        ),
                    )
                })?;
            if !cell.buildable {
                return Err(econ_error(
                    "place.unbuildable",
                    format!("footprint cell ({},{}) is not buildable", coord.x, coord.z),
                ));
            }
            if cell.blocked_by_pathing {
                return Err(econ_error(
                    "place.blocked",
                    format!(
                        "footprint cell ({},{}) is pathing-blocked",
                        coord.x, coord.z
                    ),
                ));
            }
        }
    }
    // Pathing-aware: the build origin must be able to reach the structure origin. find_path
    // fails loud with NavPathNotFound if the footprint is walled off.
    let query = NavPathQuery::new(build_origin, origin, navmesh.cells.len().max(1) * 4);
    find_path_with_budget(navmesh, query)?;
    Ok(())
}
