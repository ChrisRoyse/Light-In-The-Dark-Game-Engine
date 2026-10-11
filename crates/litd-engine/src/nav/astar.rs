//! Deterministic A* over a baked navmesh.
//!
//! Split (#1466) into three cohesive modules; this one owns the search itself:
//!   - `astar_types.rs`   — path/query value types + the prebuilt `NavIndex`
//!   - `astar_support.rs` — cost, hashing, neighbour lookup and validation helpers
//!   - `astar.rs`         — the A* loop, path reconstruction and integrity verification
//!
//! The types are re-exported here so `astar::{NavPath, ...}` (and therefore `nav/mod.rs`'s
//! `pub use astar::{...}`) resolves exactly as it did before the split.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
};

use crate::{EngineError, nav::BakedNavMesh};

use super::astar_support::cell;
use super::astar_support::{
    checked_add_cost, compute_nav_path_fnv1a64, coord_label, coord_of, heuristic_cost,
    invalid_config, not_found, predecessor_is_lower, query_cell, step_cost, validate_query,
    walkable_neighbors,
};
use super::astar_types::OpenEntry;

pub use super::astar_types::{
    NAV_PATH_SCHEMA, NavCellCoord, NavIndex, NavPath, NavPathNode, NavPathQuery,
};

pub fn find_path(
    navmesh: &BakedNavMesh,
    from: NavCellCoord,
    to: NavCellCoord,
) -> Result<NavPath, EngineError> {
    find_path_with_budget(
        navmesh,
        NavPathQuery::new(from, to, navmesh.cells.len().max(1)),
    )
}

pub fn find_path_with_budget(
    navmesh: &BakedNavMesh,
    query: NavPathQuery,
) -> Result<NavPath, EngineError> {
    // Convenience/one-off path: build the verified index for this single query. Callers
    // that issue many queries against an immutable navmesh (e.g. the AI controller, once
    // per chasing agent per decision tick) build the NavIndex ONCE and call
    // find_path_with_index — the index build is where the per-query re-verify/re-hash/
    // re-index cost was concentrated (#1486 #1).
    let index = NavIndex::build(navmesh)?;
    find_path_with_index(navmesh, &index, query)
}

/// A* over a navmesh with a prebuilt [`NavIndex`]. Does NOT re-verify or re-index the
/// navmesh — that happened once when the index was built. Deterministic: given the same
/// navmesh + query it returns a byte-identical [`NavPath`].
pub fn find_path_with_index(
    navmesh: &BakedNavMesh,
    index: &NavIndex,
    query: NavPathQuery,
) -> Result<NavPath, EngineError> {
    validate_query(query)?;
    let start = query_cell(navmesh, index, query.from, "query.from")?;
    let goal = query_cell(navmesh, index, query.to, "query.to")?;
    if !start.walkable || !goal.walkable {
        return Err(not_found(query, 0));
    }
    if query.from == query.to {
        return build_path(navmesh, query, index, &BTreeMap::new(), 0, 0);
    }

    // Min-priority queue via Reverse (BinaryHeap is a max-heap). Ordering key holds the
    // integer cell-id rank, not a cloned String (#1486 #2). We push a new entry only when
    // a strictly-shorter g is found or a coord is first reached; a same-g tie only rewrites
    // came_from, and the coord's existing open entry (guaranteed still queued — a popped
    // coord is closed and skipped) already carries that priority. This reproduces exactly
    // the old BTreeSet's dedup of identical entries, so frontier_peak and the whole path
    // stay byte-identical.
    let mut open: BinaryHeap<Reverse<OpenEntry>> = BinaryHeap::new();
    let mut g_score = BTreeMap::new();
    let mut came_from = BTreeMap::new();
    let start_h = heuristic_cost(navmesh, query.from, query.to)?;
    let start_index = index.cell_index(query.from).unwrap();
    open.push(Reverse(OpenEntry {
        f_cost_fp: start_h,
        h_cost_fp: start_h,
        cell_id_rank: index.rank(start_index),
        g_cost_fp: 0,
        coord: query.from,
    }));
    g_score.insert(query.from, 0_i64);
    let mut closed = BTreeSet::new();
    let mut expanded = 0_usize;
    let mut frontier_peak = open.len();

    while let Some(Reverse(entry)) = open.pop() {
        if g_score.get(&entry.coord).copied() != Some(entry.g_cost_fp) {
            continue;
        }
        if !closed.insert(entry.coord) {
            continue;
        }
        if entry.coord == query.to {
            return build_path(navmesh, query, index, &came_from, expanded, frontier_peak);
        }
        if expanded == query.max_expansions {
            return Err(EngineError::NavPathBudgetExceeded {
                max_expansions: query.max_expansions,
                expanded,
            });
        }
        expanded += 1;
        for neighbor in walkable_neighbors(navmesh, index, entry.coord) {
            if closed.contains(&neighbor) {
                continue;
            }
            let tentative_g = checked_add_cost(
                entry.g_cost_fp,
                step_cost(navmesh, index, entry.coord, neighbor)?,
                "nav_path.g_cost",
            )?;
            let existing_g = g_score.get(&neighbor).copied();
            let should_update = match existing_g {
                None => true,
                Some(existing) if tentative_g < existing => true,
                Some(existing) if tentative_g == existing => {
                    predecessor_is_lower(index, &came_from, neighbor, entry.coord)
                }
                _ => false,
            };
            if should_update {
                came_from.insert(neighbor, entry.coord);
                g_score.insert(neighbor, tentative_g);
                // Re-queue only on a first visit or a strictly-shorter path; a same-g
                // parent swap leaves the already-queued entry's priority unchanged.
                let requeue = existing_g != Some(tentative_g);
                if requeue {
                    let h_cost = heuristic_cost(navmesh, neighbor, query.to)?;
                    let f_cost = checked_add_cost(tentative_g, h_cost, "nav_path.f_cost")?;
                    let neighbor_index = index.cell_index(neighbor).unwrap();
                    open.push(Reverse(OpenEntry {
                        f_cost_fp: f_cost,
                        h_cost_fp: h_cost,
                        cell_id_rank: index.rank(neighbor_index),
                        g_cost_fp: tentative_g,
                        coord: neighbor,
                    }));
                    frontier_peak = frontier_peak.max(open.len());
                }
            }
        }
    }
    Err(not_found(query, expanded))
}

pub fn verify_nav_path_integrity(nav_path: &NavPath) -> Result<(), EngineError> {
    if nav_path.schema != NAV_PATH_SCHEMA {
        return invalid_config("path.schema", "must be litd.navmesh.path.v1");
    }
    if nav_path.nodes.is_empty() {
        return invalid_config("path.nodes", "must contain at least the start node");
    }
    if nav_path.nodes.first().map(coord_of) != Some(nav_path.from) {
        return invalid_config("path.nodes.first", "must match query.from");
    }
    if nav_path.nodes.last().map(coord_of) != Some(nav_path.to) {
        return invalid_config("path.nodes.last", "must match query.to");
    }
    let mut total = 0_i64;
    for (index, node) in nav_path.nodes.iter().enumerate() {
        if index == 0 && node.step_cost_fp != 0 {
            return invalid_config("path.nodes[0].step_cost_fp", "must be zero");
        }
        total = checked_add_cost(total, node.step_cost_fp, "nav_path.verify_total")?;
        if node.total_cost_fp != total {
            return invalid_config("path.nodes.total_cost_fp", "must be cumulative");
        }
    }
    if total != nav_path.total_cost_fp {
        return invalid_config("path.total_cost_fp", "must match final node total");
    }
    let actual = compute_nav_path_fnv1a64(nav_path);
    if actual != nav_path.fnv1a64 {
        return Err(EngineError::NavReadbackMismatch {
            field: "nav_path.fnv1a64".to_string(),
            expected: nav_path.fnv1a64.clone(),
            actual,
        });
    }
    Ok(())
}

fn build_path(
    navmesh: &BakedNavMesh,
    query: NavPathQuery,
    index: &NavIndex,
    came_from: &BTreeMap<NavCellCoord, NavCellCoord>,
    expanded_nodes: usize,
    frontier_peak: usize,
) -> Result<NavPath, EngineError> {
    let mut coords = vec![query.to];
    let mut cursor = query.to;
    while cursor != query.from {
        cursor = *came_from
            .get(&cursor)
            .ok_or_else(|| EngineError::NavPathNotFound {
                from: coord_label(query.from),
                to: coord_label(query.to),
                expansions: expanded_nodes,
            })?;
        coords.push(cursor);
    }
    coords.reverse();
    let mut nodes = Vec::with_capacity(coords.len());
    let mut total_cost_fp = 0_i64;
    let mut previous = None;
    for coord in coords {
        let cell = cell(navmesh, index, coord).unwrap();
        let step_cost_fp = match previous {
            Some(prev) => step_cost(navmesh, index, prev, coord)?,
            None => 0,
        };
        total_cost_fp = checked_add_cost(total_cost_fp, step_cost_fp, "nav_path.total_cost")?;
        nodes.push(NavPathNode {
            cell_id: cell.id.clone(),
            x: coord.x,
            z: coord.z,
            step_cost_fp,
            total_cost_fp,
        });
        previous = Some(coord);
    }
    let mut path = NavPath {
        schema: NAV_PATH_SCHEMA.to_string(),
        navmesh_fnv1a64: navmesh.fnv1a64.clone(),
        from: query.from,
        to: query.to,
        query_max_expansions: query.max_expansions,
        expanded_nodes,
        frontier_peak,
        total_cost_fp,
        nodes,
        fnv1a64: String::new(),
    };
    path.fnv1a64 = compute_nav_path_fnv1a64(&path);
    verify_nav_path_integrity(&path)?;
    Ok(path)
}
