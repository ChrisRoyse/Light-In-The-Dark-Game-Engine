//! Deterministic runtime navigation bake.
//!
//! The editor owns authoring. This module owns the lowered, fixed-point source
//! of truth consumed by sim/pathfinding.

mod astar;
mod astar_support;
mod astar_types;
mod avoidance;
mod avoidance_store;
mod bake;
mod path_store;
mod store;

pub use astar::{
    NAV_PATH_SCHEMA, NavCellCoord, NavIndex, NavPath, NavPathNode, NavPathQuery, find_path,
    find_path_with_budget, find_path_with_index, verify_nav_path_integrity,
};
pub use avoidance::{
    NAV_AVOIDANCE_SCHEMA, NavAvoidanceAgent, NavAvoidanceAgentResolution, NavAvoidanceResolution,
    compute_avoidance_fnv1a64, resolve_local_avoidance, verify_avoidance_resolution,
};
pub use avoidance_store::{NavAvoidanceAsterReadback, load_avoidance_aster, write_avoidance_aster};
pub use bake::{bake_navmesh, compute_navmesh_fnv1a64, rebake_navmesh, verify_navmesh_integrity};
pub use path_store::{NavPathAsterReadback, load_nav_path_aster, write_nav_path_aster};
pub use store::{NavMeshAsterReadback, load_navmesh_aster, write_navmesh_aster};

use serde::{Deserialize, Serialize};

pub const NAV_BAKE_INPUT_SCHEMA: &str = "litd.navmesh.bake_input.v1";
pub const NAVMESH_SCHEMA: &str = "litd.navmesh.grid.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavPathingFlags {
    pub walkable: bool,
    pub buildable: bool,
    pub flyable: bool,
    pub blocked: bool,
}

impl Default for NavPathingFlags {
    fn default() -> Self {
        Self {
            walkable: true,
            buildable: false,
            flyable: true,
            blocked: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavTerrainCellInput {
    pub id: String,
    pub x: i32,
    pub z: i32,
    pub height_fp: i64,
    pub flags: NavPathingFlags,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavStaticBlockerInput {
    pub id: String,
    pub min_x: i32,
    pub min_z: i32,
    pub max_x: i32,
    pub max_z: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavBakeInput {
    pub schema: String,
    pub cell_size_fp: i64,
    pub terrain_cells: Vec<NavTerrainCellInput>,
    pub static_blockers: Vec<NavStaticBlockerInput>,
}

impl NavBakeInput {
    pub fn new(
        cell_size_fp: i64,
        terrain_cells: Vec<NavTerrainCellInput>,
        static_blockers: Vec<NavStaticBlockerInput>,
    ) -> Self {
        Self {
            schema: NAV_BAKE_INPUT_SCHEMA.to_string(),
            cell_size_fp,
            terrain_cells,
            static_blockers,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BakedNavBounds {
    pub min_x: i32,
    pub max_x: i32,
    pub min_z: i32,
    pub max_z: i32,
    pub width: u64,
    pub depth: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BakedNavCell {
    pub id: String,
    pub x: i32,
    pub z: i32,
    pub height_fp: i64,
    pub walkable: bool,
    pub buildable: bool,
    pub flyable: bool,
    pub blocked_by_pathing: bool,
    pub static_blocker_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BakedNavMesh {
    pub schema: String,
    pub source_input_schema: String,
    pub cell_size_fp: i64,
    pub bounds: BakedNavBounds,
    pub terrain_cell_count: usize,
    pub static_blocker_count: usize,
    pub walkable_cell_count: usize,
    pub blocked_cell_count: usize,
    pub boundary_matches_input: bool,
    pub cells: Vec<BakedNavCell>,
    pub fnv1a64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavRebakeReport {
    pub previous_fnv1a64: Option<String>,
    pub current_fnv1a64: String,
    pub changed: bool,
    pub navmesh: BakedNavMesh,
}

#[cfg(test)]
mod astar_tests;
#[cfg(test)]
mod avoidance_tests;
#[cfg(test)]
mod tests;
