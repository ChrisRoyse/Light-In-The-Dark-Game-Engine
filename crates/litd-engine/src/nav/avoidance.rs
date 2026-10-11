use std::collections::{BTreeMap, BTreeSet};

use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    nav::{
        BakedNavCell, BakedNavMesh, NavCellCoord, NavPath, verify_nav_path_integrity,
        verify_navmesh_integrity,
    },
};

pub const NAV_AVOIDANCE_SCHEMA: &str = "litd.navmesh.avoidance.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavAvoidanceAgent {
    pub id: String,
    pub current: NavCellCoord,
    pub path: NavPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavAvoidanceAgentResolution {
    pub id: String,
    pub from: NavCellCoord,
    pub desired: NavCellCoord,
    pub assigned: NavCellCoord,
    pub velocity_x_fp: i64,
    pub velocity_z_fp: i64,
    pub moved: bool,
    pub blocked_by_agent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavAvoidanceResolution {
    pub schema: String,
    pub navmesh_fnv1a64: String,
    pub tick: u64,
    pub cell_size_fp: i64,
    pub agent_count: usize,
    pub final_unique: bool,
    pub agents: Vec<NavAvoidanceAgentResolution>,
    pub state_fnv1a64: String,
}

pub fn resolve_local_avoidance(
    navmesh: &BakedNavMesh,
    tick: u64,
    agents: &[NavAvoidanceAgent],
) -> Result<NavAvoidanceResolution, EngineError> {
    verify_navmesh_integrity(navmesh)?;
    if agents.is_empty() {
        return invalid_config("agents", "must contain at least one agent");
    }
    let nav_index = nav_index(navmesh)?;
    let sorted = sorted_agents(navmesh, &nav_index, agents)?;
    let initial_occupancy = initial_occupancy(&sorted)?;
    let mut processed = BTreeSet::new();
    let mut reserved = BTreeMap::new();
    let mut resolutions = Vec::with_capacity(sorted.len());

    for agent in sorted {
        let desired = desired_coord(agent)?;
        let (assigned, blocked_by_agent_id) = match availability(
            desired,
            &agent.id,
            &reserved,
            &initial_occupancy,
            &processed,
        ) {
            Ok(()) => (desired, None),
            Err(blocker) => {
                let assigned = fallback_coord(
                    navmesh,
                    &nav_index,
                    agent.current,
                    &agent.id,
                    &reserved,
                    &initial_occupancy,
                    &processed,
                )?;
                (assigned, Some(blocker))
            }
        };
        reserved.insert(assigned, agent.id.clone());
        processed.insert(agent.id.clone());
        resolutions.push(agent_resolution(
            navmesh,
            agent,
            desired,
            assigned,
            blocked_by_agent_id,
        )?);
    }

    let final_unique = reserved.len() == agents.len();
    if !final_unique {
        return invalid_config("agents.assigned", "resolved final positions must be unique");
    }
    let mut resolution = NavAvoidanceResolution {
        schema: NAV_AVOIDANCE_SCHEMA.to_string(),
        navmesh_fnv1a64: navmesh.fnv1a64.clone(),
        tick,
        cell_size_fp: navmesh.cell_size_fp,
        agent_count: resolutions.len(),
        final_unique,
        agents: resolutions,
        state_fnv1a64: String::new(),
    };
    resolution.state_fnv1a64 = compute_avoidance_fnv1a64(&resolution);
    verify_avoidance_resolution(&resolution)?;
    Ok(resolution)
}

pub fn verify_avoidance_resolution(resolution: &NavAvoidanceResolution) -> Result<(), EngineError> {
    if resolution.schema != NAV_AVOIDANCE_SCHEMA {
        return invalid_config("avoidance.schema", "must be litd.navmesh.avoidance.v1");
    }
    if resolution.cell_size_fp <= 0 {
        return invalid_config("avoidance.cell_size_fp", "must be positive");
    }
    if resolution.agent_count != resolution.agents.len() {
        return invalid_config("avoidance.agent_count", "must match resolution agent count");
    }
    if resolution.agents.is_empty() {
        return invalid_config("avoidance.agents", "must contain at least one resolution");
    }
    let mut final_cells = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for agent in &resolution.agents {
        validate_id("avoidance.agents.id", &agent.id)?;
        if !ids.insert(agent.id.as_str()) {
            return invalid_config("avoidance.agents.id", format!("duplicate id {}", agent.id));
        }
        if !final_cells.insert(agent.assigned) {
            return invalid_config("avoidance.assigned", "duplicate final cell");
        }
    }
    if !resolution.final_unique {
        return invalid_config("avoidance.final_unique", "must be true after resolution");
    }
    let actual = compute_avoidance_fnv1a64(resolution);
    if actual != resolution.state_fnv1a64 {
        return Err(EngineError::NavReadbackMismatch {
            field: "avoidance.state_fnv1a64".to_string(),
            expected: resolution.state_fnv1a64.clone(),
            actual,
        });
    }
    Ok(())
}

pub fn compute_avoidance_fnv1a64(resolution: &NavAvoidanceResolution) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, &resolution.schema);
    update_str(&mut hasher, &resolution.navmesh_fnv1a64);
    hasher.update_u64_le(resolution.tick);
    hasher.update_i64_le(resolution.cell_size_fp);
    hasher.update_u64_le(resolution.agent_count as u64);
    hasher.update_u8(u8::from(resolution.final_unique));
    hasher.update_u64_le(resolution.agents.len() as u64);
    for agent in &resolution.agents {
        update_str(&mut hasher, &agent.id);
        hash_coord(&mut hasher, agent.from);
        hash_coord(&mut hasher, agent.desired);
        hash_coord(&mut hasher, agent.assigned);
        hasher.update_i64_le(agent.velocity_x_fp);
        hasher.update_i64_le(agent.velocity_z_fp);
        hasher.update_u8(u8::from(agent.moved));
        match &agent.blocked_by_agent_id {
            Some(id) => {
                hasher.update_u8(1);
                update_str(&mut hasher, id);
            }
            None => {
                hasher.update_u8(0);
            }
        }
    }
    hasher.finish_hex()
}

fn sorted_agents<'a>(
    navmesh: &BakedNavMesh,
    nav_index: &BTreeMap<NavCellCoord, usize>,
    agents: &'a [NavAvoidanceAgent],
) -> Result<Vec<&'a NavAvoidanceAgent>, EngineError> {
    let mut ids = BTreeSet::new();
    let mut sorted = agents.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|agent| agent.id.as_str());
    for agent in &sorted {
        validate_id("agents.id", &agent.id)?;
        if !ids.insert(agent.id.as_str()) {
            return invalid_config("agents.id", format!("duplicate id {}", agent.id));
        }
        validate_agent_path(navmesh, nav_index, agent)?;
    }
    Ok(sorted)
}

fn validate_agent_path(
    navmesh: &BakedNavMesh,
    nav_index: &BTreeMap<NavCellCoord, usize>,
    agent: &NavAvoidanceAgent,
) -> Result<(), EngineError> {
    verify_nav_path_integrity(&agent.path)?;
    if agent.path.navmesh_fnv1a64 != navmesh.fnv1a64 {
        return invalid_config(
            format!("agents.{}.path.navmesh_fnv1a64", agent.id),
            "path hash must match the navmesh source of truth",
        );
    }
    let current =
        cell(navmesh, nav_index, agent.current).ok_or_else(|| EngineError::NavInvalidConfig {
            field: format!("agents.{}.current", agent.id),
            reason: "current cell is outside the navmesh".to_string(),
        })?;
    if !current.walkable {
        return invalid_config(
            format!("agents.{}.current", agent.id),
            "current cell must be walkable",
        );
    }
    for window in agent.path.nodes.windows(2) {
        let from = coord_of(&window[0]);
        let to = coord_of(&window[1]);
        if manhattan(from, to) != 1 {
            return invalid_config(
                format!("agents.{}.path.nodes", agent.id),
                "consecutive path nodes must be adjacent grid cells",
            );
        }
    }
    for node in &agent.path.nodes {
        let coord = coord_of(node);
        let Some(nav_cell) = cell(navmesh, nav_index, coord) else {
            return invalid_config(
                format!("agents.{}.path.nodes", agent.id),
                "path node is outside the navmesh",
            );
        };
        if !nav_cell.walkable {
            return invalid_config(
                format!("agents.{}.path.nodes", agent.id),
                "path node must be walkable",
            );
        }
    }
    if !agent
        .path
        .nodes
        .iter()
        .any(|node| coord_of(node) == agent.current)
    {
        return invalid_config(
            format!("agents.{}.current", agent.id),
            "current cell must appear in the agent path",
        );
    }
    Ok(())
}

fn initial_occupancy(
    agents: &[&NavAvoidanceAgent],
) -> Result<BTreeMap<NavCellCoord, String>, EngineError> {
    let mut occupancy = BTreeMap::new();
    for agent in agents {
        if let Some(existing) = occupancy.insert(agent.current, agent.id.clone()) {
            return invalid_config(
                "agents.current",
                format!("agents {existing} and {} share a start cell", agent.id),
            );
        }
    }
    Ok(occupancy)
}

fn desired_coord(agent: &NavAvoidanceAgent) -> Result<NavCellCoord, EngineError> {
    let index = agent
        .path
        .nodes
        .iter()
        .position(|node| coord_of(node) == agent.current)
        .ok_or_else(|| EngineError::NavInvalidConfig {
            field: format!("agents.{}.current", agent.id),
            reason: "current cell must appear in path".to_string(),
        })?;
    Ok(agent
        .path
        .nodes
        .get(index + 1)
        .map(coord_of)
        .unwrap_or(agent.current))
}

fn fallback_coord(
    navmesh: &BakedNavMesh,
    nav_index: &BTreeMap<NavCellCoord, usize>,
    current: NavCellCoord,
    agent_id: &str,
    reserved: &BTreeMap<NavCellCoord, String>,
    initial_occupancy: &BTreeMap<NavCellCoord, String>,
    processed: &BTreeSet<String>,
) -> Result<NavCellCoord, EngineError> {
    if availability(current, agent_id, reserved, initial_occupancy, processed).is_ok() {
        return Ok(current);
    }
    for neighbor in walkable_neighbors(navmesh, nav_index, current) {
        if availability(neighbor, agent_id, reserved, initial_occupancy, processed).is_ok() {
            return Ok(neighbor);
        }
    }
    invalid_config(
        format!("agents.{agent_id}.reservation"),
        "no collision-free hold or adjacent cell is available",
    )
}

fn availability(
    coord: NavCellCoord,
    agent_id: &str,
    reserved: &BTreeMap<NavCellCoord, String>,
    initial_occupancy: &BTreeMap<NavCellCoord, String>,
    processed: &BTreeSet<String>,
) -> Result<(), String> {
    if let Some(blocker) = reserved.get(&coord)
        && blocker != agent_id
    {
        return Err(blocker.clone());
    }
    if let Some(blocker) = initial_occupancy.get(&coord)
        && blocker != agent_id
        && !processed.contains(blocker)
    {
        return Err(blocker.clone());
    }
    Ok(())
}

fn agent_resolution(
    navmesh: &BakedNavMesh,
    agent: &NavAvoidanceAgent,
    desired: NavCellCoord,
    assigned: NavCellCoord,
    blocked_by_agent_id: Option<String>,
) -> Result<NavAvoidanceAgentResolution, EngineError> {
    let velocity_x_fp = coord_delta_fp(assigned.x, agent.current.x, navmesh.cell_size_fp)?;
    let velocity_z_fp = coord_delta_fp(assigned.z, agent.current.z, navmesh.cell_size_fp)?;
    Ok(NavAvoidanceAgentResolution {
        id: agent.id.clone(),
        from: agent.current,
        desired,
        assigned,
        velocity_x_fp,
        velocity_z_fp,
        moved: assigned != agent.current,
        blocked_by_agent_id,
    })
}

fn coord_delta_fp(to: i32, from: i32, cell_size_fp: i64) -> Result<i64, EngineError> {
    let delta = i128::from(to) - i128::from(from);
    let value = delta * i128::from(cell_size_fp);
    if value < i64::MIN as i128 || value > i64::MAX as i128 {
        return Err(EngineError::FixedPointOverflow {
            operation: "nav_avoidance.velocity",
        });
    }
    Ok(value as i64)
}

fn nav_index(navmesh: &BakedNavMesh) -> Result<BTreeMap<NavCellCoord, usize>, EngineError> {
    let mut index = BTreeMap::new();
    for (cell_index, cell) in navmesh.cells.iter().enumerate() {
        if index
            .insert(NavCellCoord::new(cell.x, cell.z), cell_index)
            .is_some()
        {
            return invalid_config("navmesh.cells", "duplicate coordinate in navmesh");
        }
    }
    Ok(index)
}

fn walkable_neighbors(
    navmesh: &BakedNavMesh,
    nav_index: &BTreeMap<NavCellCoord, usize>,
    coord: NavCellCoord,
) -> Vec<NavCellCoord> {
    let mut neighbors = Vec::new();
    for candidate in neighbor_coords(coord) {
        if let Some(candidate_cell) = cell(navmesh, nav_index, candidate)
            && candidate_cell.walkable
        {
            neighbors.push((candidate_cell.id.as_str(), candidate));
        }
    }
    neighbors.sort_by_key(|(cell_id, _)| *cell_id);
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

fn cell<'a>(
    navmesh: &'a BakedNavMesh,
    nav_index: &BTreeMap<NavCellCoord, usize>,
    coord: NavCellCoord,
) -> Option<&'a BakedNavCell> {
    nav_index
        .get(&coord)
        .map(|cell_index| &navmesh.cells[*cell_index])
}

fn coord_of(node: &crate::nav::NavPathNode) -> NavCellCoord {
    NavCellCoord::new(node.x, node.z)
}

fn manhattan(left: NavCellCoord, right: NavCellCoord) -> i64 {
    (i64::from(left.x) - i64::from(right.x)).abs() + (i64::from(left.z) - i64::from(right.z)).abs()
}

fn hash_coord(hasher: &mut Fnv1a64, coord: NavCellCoord) {
    hasher.update_i64_le(i64::from(coord.x));
    hasher.update_i64_le(i64::from(coord.z));
}

fn update_str(hasher: &mut Fnv1a64, value: &str) {
    hasher.update_u64_le(value.len() as u64);
    hasher.update(value.as_bytes());
}

fn validate_id(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_config(field, "must be non-empty");
    }
    if value.bytes().any(|byte| byte.is_ascii_control()) {
        return invalid_config(field, "must not contain control characters");
    }
    Ok(())
}

fn invalid_config<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::NavInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}
