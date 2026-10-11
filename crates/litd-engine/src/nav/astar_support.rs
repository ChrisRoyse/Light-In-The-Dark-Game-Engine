//! Cost, hashing, neighbour-lookup and validation helpers backing the navmesh A* search
//! (#1466 split of `astar.rs`). Pure functions over a `&BakedNavMesh` + `&NavIndex`; the
//! search loop itself lives in `astar.rs` and the value types in `astar_types.rs`.

use std::collections::BTreeMap;

use litd_hash::Fnv1a64;

use crate::{
    EngineError,
    nav::{BakedNavCell, BakedNavMesh},
};

use super::astar_types::{NavCellCoord, NavIndex, NavPath, NavPathNode, NavPathQuery};

pub(super) fn compute_nav_path_fnv1a64(path: &NavPath) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, &path.schema);
    update_str(&mut hasher, &path.navmesh_fnv1a64);
    hash_coord(&mut hasher, path.from);
    hash_coord(&mut hasher, path.to);
    hasher.update_u64_le(path.query_max_expansions as u64);
    hasher.update_u64_le(path.expanded_nodes as u64);
    hasher.update_u64_le(path.frontier_peak as u64);
    hasher.update_i64_le(path.total_cost_fp);
    hasher.update_u64_le(path.nodes.len() as u64);
    for node in &path.nodes {
        update_str(&mut hasher, &node.cell_id);
        hasher.update_i64_le(i64::from(node.x));
        hasher.update_i64_le(i64::from(node.z));
        hasher.update_i64_le(node.step_cost_fp);
        hasher.update_i64_le(node.total_cost_fp);
    }
    hasher.finish_hex()
}

pub(super) fn query_cell<'a>(
    navmesh: &'a BakedNavMesh,
    index: &NavIndex,
    coord: NavCellCoord,
    field: &str,
) -> Result<&'a BakedNavCell, EngineError> {
    cell(navmesh, index, coord).ok_or_else(|| EngineError::NavInvalidConfig {
        field: field.to_string(),
        reason: format!("coordinate {} is outside the navmesh", coord_label(coord)),
    })
}

pub(super) fn cell<'a>(
    navmesh: &'a BakedNavMesh,
    index: &NavIndex,
    coord: NavCellCoord,
) -> Option<&'a BakedNavCell> {
    index
        .cell_index(coord)
        .map(|cell_index| &navmesh.cells[cell_index])
}

pub(super) fn walkable_neighbors(
    navmesh: &BakedNavMesh,
    index: &NavIndex,
    coord: NavCellCoord,
) -> Vec<NavCellCoord> {
    // Sort neighbours by cell-id rank — the same order the old `cell_id` string sort
    // produced, but with an integer key and no string comparisons (#1486 #2).
    let mut neighbors = Vec::new();
    for candidate in neighbor_coords(coord) {
        if let Some(cell_index) = index.cell_index(candidate)
            && navmesh.cells[cell_index].walkable
        {
            neighbors.push((index.rank(cell_index), candidate));
        }
    }
    neighbors.sort_by_key(|(rank, _)| *rank);
    neighbors.into_iter().map(|(_, coord)| coord).collect()
}

fn neighbor_coords(coord: NavCellCoord) -> Vec<NavCellCoord> {
    let mut coords = Vec::with_capacity(4);
    if let Some(z) = coord.z.checked_sub(1) {
        coords.push(NavCellCoord::new(coord.x, z));
    }
    if let Some(x) = coord.x.checked_sub(1) {
        coords.push(NavCellCoord::new(x, coord.z));
    }
    if let Some(x) = coord.x.checked_add(1) {
        coords.push(NavCellCoord::new(x, coord.z));
    }
    if let Some(z) = coord.z.checked_add(1) {
        coords.push(NavCellCoord::new(coord.x, z));
    }
    coords
}

pub(super) fn predecessor_is_lower(
    index: &NavIndex,
    came_from: &BTreeMap<NavCellCoord, NavCellCoord>,
    neighbor: NavCellCoord,
    candidate_parent: NavCellCoord,
) -> bool {
    let Some(existing_parent) = came_from.get(&neighbor).copied() else {
        return true;
    };
    // Rank order equals cell-id order, so this is the same tie-break as comparing the
    // parents' cell-id strings (#1486 #2).
    let candidate_rank = index.rank(index.cell_index(candidate_parent).unwrap());
    let existing_rank = index.rank(index.cell_index(existing_parent).unwrap());
    candidate_rank < existing_rank
}

pub(super) fn heuristic_cost(
    navmesh: &BakedNavMesh,
    from: NavCellCoord,
    to: NavCellCoord,
) -> Result<i64, EngineError> {
    let dx = i128::from((i64::from(from.x) - i64::from(to.x)).abs());
    let dz = i128::from((i64::from(from.z) - i64::from(to.z)).abs());
    fit_cost(
        (dx + dz) * i128::from(navmesh.cell_size_fp),
        "nav_path.heuristic",
    )
}

pub(super) fn step_cost(
    navmesh: &BakedNavMesh,
    index: &NavIndex,
    from: NavCellCoord,
    to: NavCellCoord,
) -> Result<i64, EngineError> {
    let from_cell = cell(navmesh, index, from).unwrap();
    let to_cell = cell(navmesh, index, to).unwrap();
    let height_delta = (i128::from(from_cell.height_fp) - i128::from(to_cell.height_fp)).abs();
    fit_cost(
        i128::from(navmesh.cell_size_fp) + height_delta,
        "nav_path.step_cost",
    )
}

pub(super) fn validate_query(query: NavPathQuery) -> Result<(), EngineError> {
    if query.max_expansions == 0 {
        return invalid_config("query.max_expansions", "must be greater than zero");
    }
    Ok(())
}

pub(super) fn coord_of(node: &NavPathNode) -> NavCellCoord {
    NavCellCoord::new(node.x, node.z)
}

pub(super) fn checked_add_cost(
    left: i64,
    right: i64,
    operation: &'static str,
) -> Result<i64, EngineError> {
    left.checked_add(right)
        .ok_or(EngineError::FixedPointOverflow { operation })
}

fn fit_cost(value: i128, operation: &'static str) -> Result<i64, EngineError> {
    if value < i64::MIN as i128 || value > i64::MAX as i128 {
        return Err(EngineError::FixedPointOverflow { operation });
    }
    Ok(value as i64)
}

pub(super) fn not_found(query: NavPathQuery, expansions: usize) -> EngineError {
    EngineError::NavPathNotFound {
        from: coord_label(query.from),
        to: coord_label(query.to),
        expansions,
    }
}

pub(super) fn coord_label(coord: NavCellCoord) -> String {
    format!("{},{}", coord.x, coord.z)
}

fn hash_coord(hasher: &mut Fnv1a64, coord: NavCellCoord) {
    hasher.update_i64_le(i64::from(coord.x));
    hasher.update_i64_le(i64::from(coord.z));
}

fn update_str(hasher: &mut Fnv1a64, value: &str) {
    hasher.update_u64_le(value.len() as u64);
    hasher.update(value.as_bytes());
}

pub(super) fn invalid_config<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::NavInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}
