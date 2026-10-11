//! Navigation path value types and the prebuilt navmesh lookup index (#1466 split of
//! `astar.rs`). Pure data + the one-time index build; the A* search itself lives in
//! `astar.rs` and the cost/hash helpers in `astar_support.rs`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{EngineError, nav::BakedNavMesh, nav::verify_navmesh_integrity};

use super::astar_support::invalid_config;

pub const NAV_PATH_SCHEMA: &str = "litd.navmesh.path.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NavCellCoord {
    pub x: i32,
    pub z: i32,
}

impl NavCellCoord {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavPathQuery {
    pub from: NavCellCoord,
    pub to: NavCellCoord,
    pub max_expansions: usize,
}

impl NavPathQuery {
    pub const fn new(from: NavCellCoord, to: NavCellCoord, max_expansions: usize) -> Self {
        Self {
            from,
            to,
            max_expansions,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavPathNode {
    pub cell_id: String,
    pub x: i32,
    pub z: i32,
    pub step_cost_fp: i64,
    pub total_cost_fp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavPath {
    pub schema: String,
    pub navmesh_fnv1a64: String,
    pub from: NavCellCoord,
    pub to: NavCellCoord,
    pub query_max_expansions: usize,
    pub expanded_nodes: usize,
    pub frontier_peak: usize,
    pub nodes: Vec<NavPathNode>,
    pub total_cost_fp: i64,
    pub fnv1a64: String,
}

// Ordering is (f, h, cell_id_rank, g, coord). `cell_id_rank` is the cell's position in
// cell_id-sorted order (see NavIndex) — an integer that compares IDENTICALLY to the cell
// id string it replaces, so the A* tie-break is byte-for-byte the same as the old
// String-keyed open set, with no per-insertion String clone (#1486 #2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct OpenEntry {
    pub(super) f_cost_fp: i64,
    pub(super) h_cost_fp: i64,
    pub(super) cell_id_rank: u32,
    pub(super) g_cost_fp: i64,
    pub(super) coord: NavCellCoord,
}

/// Prebuilt, verified navmesh lookup structure (#1486 #1). Building it runs the O(cells)
/// integrity verification + FNV re-hash and the coordinate index ONCE; path queries then
/// take a `&NavIndex` and never re-verify or re-index the immutable navmesh. `cell_id_rank`
/// gives each cell an integer rank matching cell-id lexical order so the A* tie-breaks stay
/// deterministic without cloning cell-id strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavIndex {
    coord_to_cell: BTreeMap<NavCellCoord, usize>,
    cell_id_rank: Vec<u32>,
}

impl NavIndex {
    /// Verify the navmesh and build the coordinate index + cell-id ranks once.
    pub fn build(navmesh: &BakedNavMesh) -> Result<Self, EngineError> {
        verify_navmesh_integrity(navmesh)?;
        let mut coord_to_cell = BTreeMap::new();
        for (cell_index, cell) in navmesh.cells.iter().enumerate() {
            let coord = NavCellCoord::new(cell.x, cell.z);
            if coord_to_cell.insert(coord, cell_index).is_some() {
                return invalid_config("navmesh.cells", "duplicate coordinate in navmesh");
            }
        }
        // Rank cells by their (unique) id so an integer rank orders identically to the id.
        let mut order: Vec<usize> = (0..navmesh.cells.len()).collect();
        order.sort_by(|&a, &b| navmesh.cells[a].id.cmp(&navmesh.cells[b].id));
        let mut cell_id_rank = vec![0_u32; navmesh.cells.len()];
        let mut previous_id: Option<&str> = None;
        for (rank, &cell_index) in order.iter().enumerate() {
            let id = navmesh.cells[cell_index].id.as_str();
            if previous_id == Some(id) {
                return invalid_config("navmesh.cells", "duplicate cell id in navmesh");
            }
            previous_id = Some(id);
            cell_id_rank[cell_index] =
                u32::try_from(rank).map_err(|_| EngineError::NavInvalidConfig {
                    field: "navmesh.cells".to_string(),
                    reason: "navmesh has more cells than u32 can rank".to_string(),
                })?;
        }
        Ok(Self {
            coord_to_cell,
            cell_id_rank,
        })
    }

    pub(super) fn cell_index(&self, coord: NavCellCoord) -> Option<usize> {
        self.coord_to_cell.get(&coord).copied()
    }

    pub(super) fn rank(&self, cell_index: usize) -> u32 {
        self.cell_id_rank[cell_index]
    }
}
