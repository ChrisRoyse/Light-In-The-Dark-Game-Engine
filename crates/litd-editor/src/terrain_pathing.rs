use std::collections::{BTreeMap, BTreeSet, VecDeque};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::history::{EditorDocument, stable_fingerprint};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainPathingFlags {
    pub walkable: bool,
    pub buildable: bool,
    pub flyable: bool,
    pub blocked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerrainPathingPaintMode {
    Walkable,
    Buildable,
    Flyable,
    Blocked,
    Erase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerrainPathMovement {
    GroundUnit,
    FlyingUnit,
    BuildingPlacement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorTerrainPathingOverride {
    pub terrain_cell_id: String,
    pub flags: TerrainPathingFlags,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorTilePathingRule {
    pub tile_id: String,
    pub flags: TerrainPathingFlags,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainPathingPaintReport {
    pub operation: String,
    pub changed_cell_ids: Vec<String>,
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub before_summary: String,
    pub after_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainPathQueryReport {
    pub movement: TerrainPathMovement,
    pub start_cell_id: String,
    pub goal_cell_id: String,
    pub path_found: bool,
    pub path_cell_ids: Vec<String>,
    pub visited_count: usize,
    pub blocked_cell_ids: Vec<String>,
    pub source_summary: String,
}

impl Default for TerrainPathingFlags {
    fn default() -> Self {
        Self::land()
    }
}

impl TerrainPathingFlags {
    pub fn land() -> Self {
        Self {
            walkable: true,
            buildable: true,
            flyable: true,
            blocked: false,
        }
    }

    pub fn walkable() -> Self {
        Self {
            walkable: true,
            buildable: false,
            flyable: true,
            blocked: false,
        }
    }

    pub fn buildable() -> Self {
        Self::land()
    }

    pub fn flyable_only() -> Self {
        Self {
            walkable: false,
            buildable: false,
            flyable: true,
            blocked: false,
        }
    }

    pub fn blocked() -> Self {
        Self {
            walkable: false,
            buildable: false,
            flyable: false,
            blocked: true,
        }
    }

    pub fn validate(self) -> Result<(), EngineError> {
        if self.blocked && (self.walkable || self.buildable || self.flyable) {
            return invalid_pathing(
                "flags.blocked",
                "blocked cells cannot also be walkable, buildable, or flyable",
            );
        }
        if self.buildable && !self.walkable {
            return invalid_pathing("flags.buildable", "buildable cells must also be walkable");
        }
        Ok(())
    }

    pub fn allows(self, movement: TerrainPathMovement) -> bool {
        if self.blocked {
            return false;
        }
        match movement {
            TerrainPathMovement::GroundUnit => self.walkable,
            TerrainPathMovement::FlyingUnit => self.flyable,
            TerrainPathMovement::BuildingPlacement => self.buildable,
        }
    }
}

impl TerrainPathingPaintMode {
    fn flags(self) -> Option<TerrainPathingFlags> {
        match self {
            Self::Walkable => Some(TerrainPathingFlags::walkable()),
            Self::Buildable => Some(TerrainPathingFlags::buildable()),
            Self::Flyable => Some(TerrainPathingFlags::flyable_only()),
            Self::Blocked => Some(TerrainPathingFlags::blocked()),
            Self::Erase => None,
        }
    }
}

impl EditorTerrainPathingOverride {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.terrain_cell_id.trim().is_empty() {
            return invalid_pathing("override.cell", "terrain_cell_id must be non-empty");
        }
        self.flags.validate()
    }
}

impl EditorTilePathingRule {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.tile_id.trim().is_empty() {
            return invalid_pathing("tile_rule.tile_id", "tile_id must be non-empty");
        }
        self.flags.validate()
    }
}

pub fn paint_pathing_cells(
    document: &mut EditorDocument,
    cell_ids: impl IntoIterator<Item = String>,
    mode: TerrainPathingPaintMode,
) -> Result<TerrainPathingPaintReport, EngineError> {
    let before = document.clone();
    let mut after = before.clone();
    let mut changed = Vec::new();
    for cell_id in cell_ids {
        require_cell(&after, &cell_id)?;
        let previous = after.terrain_pathing_overrides.get(&cell_id).cloned();
        match mode.flags() {
            Some(flags) => {
                flags.validate()?;
                let next = EditorTerrainPathingOverride {
                    terrain_cell_id: cell_id.clone(),
                    flags,
                };
                if previous.as_ref() != Some(&next) {
                    after
                        .terrain_pathing_overrides
                        .insert(cell_id.clone(), next);
                    changed.push(cell_id);
                }
            }
            None => {
                if after.terrain_pathing_overrides.remove(&cell_id).is_some() {
                    changed.push(cell_id);
                }
            }
        }
    }
    finish_pathing_operation(document, before, after, format!("paint.{mode:?}"), changed)
}

pub fn set_global_tile_pathing(
    document: &mut EditorDocument,
    tile_id: &str,
    flags: TerrainPathingFlags,
) -> Result<TerrainPathingPaintReport, EngineError> {
    if tile_id.trim().is_empty() {
        return invalid_pathing("tile_rule.tile_id", "tile_id must be non-empty");
    }
    flags.validate()?;
    let before = document.clone();
    let mut after = before.clone();
    let rule = EditorTilePathingRule {
        tile_id: tile_id.to_string(),
        flags,
    };
    let changed = after
        .terrain_cells
        .values()
        .filter(|cell| cell.tile_id == tile_id)
        .map(|cell| cell.id.clone())
        .collect::<Vec<_>>();
    if changed.is_empty() {
        return invalid_pathing(
            "tile_rule.matches",
            format!("tile rule {tile_id} matched no terrain cells"),
        );
    }
    after
        .terrain_tile_pathing_rules
        .insert(tile_id.to_string(), rule);
    finish_pathing_operation(
        document,
        before,
        after,
        "global_tile_pathing".to_string(),
        changed,
    )
}

pub fn effective_pathing_for_cell(
    document: &EditorDocument,
    terrain_cell_id: &str,
) -> Result<TerrainPathingFlags, EngineError> {
    let cell = require_cell(document, terrain_cell_id)?;
    if let Some(override_flags) = document.terrain_pathing_overrides.get(terrain_cell_id) {
        override_flags.validate()?;
        return Ok(override_flags.flags);
    }
    if let Some(rule) = document.terrain_tile_pathing_rules.get(&cell.tile_id) {
        rule.validate()?;
        return Ok(rule.flags);
    }
    Ok(TerrainPathingFlags::default())
}

pub fn query_path_between_cells(
    document: &EditorDocument,
    start_cell_id: &str,
    goal_cell_id: &str,
    movement: TerrainPathMovement,
) -> Result<TerrainPathQueryReport, EngineError> {
    document.validate()?;
    require_cell(document, start_cell_id)?;
    require_cell(document, goal_cell_id)?;
    let cells_by_coord = cells_by_coord(document);
    let mut blocked = Vec::new();
    let mut visited = BTreeSet::new();
    let mut came_from = BTreeMap::<String, String>::new();
    let mut queue = VecDeque::new();

    if cell_allows(document, start_cell_id, movement, &mut blocked)? {
        visited.insert(start_cell_id.to_string());
        queue.push_back(start_cell_id.to_string());
    } else {
        blocked.push(start_cell_id.to_string());
    }

    while let Some(current_id) = queue.pop_front() {
        if current_id == goal_cell_id {
            break;
        }
        let current_coord = coord_for_cell(document, &current_id)?;
        for neighbor_coord in neighbors(current_coord) {
            let Some(neighbor_id) = cells_by_coord.get(&neighbor_coord) else {
                continue;
            };
            if visited.contains(neighbor_id) {
                continue;
            }
            if !cell_allows(document, neighbor_id, movement, &mut blocked)? {
                continue;
            }
            visited.insert(neighbor_id.clone());
            came_from.insert(neighbor_id.clone(), current_id.clone());
            queue.push_back(neighbor_id.clone());
        }
    }

    let path_found = visited.contains(goal_cell_id);
    let path_cell_ids = if path_found {
        reconstruct_path(start_cell_id, goal_cell_id, &came_from)?
    } else {
        Vec::new()
    };
    blocked.sort();
    blocked.dedup();
    Ok(TerrainPathQueryReport {
        movement,
        start_cell_id: start_cell_id.to_string(),
        goal_cell_id: goal_cell_id.to_string(),
        path_found,
        path_cell_ids,
        visited_count: visited.len(),
        blocked_cell_ids: blocked,
        source_summary: terrain_pathing_summary(document),
    })
}

pub fn validate_terrain_pathing(document: &EditorDocument) -> Result<(), EngineError> {
    for (id, override_record) in &document.terrain_pathing_overrides {
        if id != &override_record.terrain_cell_id {
            return invalid_pathing(
                "override.key",
                format!("map key {id} does not match terrain_cell_id"),
            );
        }
        require_cell(document, id)?;
        override_record.validate()?;
    }
    for (tile_id, rule) in &document.terrain_tile_pathing_rules {
        if tile_id != &rule.tile_id {
            return invalid_pathing(
                "tile_rule.key",
                format!("map key {tile_id} does not match tile_id"),
            );
        }
        rule.validate()?;
        if !document
            .terrain_cells
            .values()
            .any(|cell| cell.tile_id == *tile_id)
        {
            return invalid_pathing(
                "tile_rule.matches",
                format!("tile rule {tile_id} matched no terrain cells"),
            );
        }
    }
    Ok(())
}

pub fn terrain_pathing_summary(document: &EditorDocument) -> String {
    let rules = document
        .terrain_tile_pathing_rules
        .iter()
        .map(|(tile_id, rule)| format!("{tile_id}={}", flags_label(rule.flags)))
        .collect::<Vec<_>>()
        .join("|");
    let overrides = document
        .terrain_pathing_overrides
        .iter()
        .map(|(cell_id, override_record)| {
            format!("{cell_id}={}", flags_label(override_record.flags))
        })
        .collect::<Vec<_>>()
        .join("|");
    format!("tile_rules=[{rules}];overrides=[{overrides}]")
}

fn finish_pathing_operation(
    document: &mut EditorDocument,
    before: EditorDocument,
    after: EditorDocument,
    operation: String,
    changed_cell_ids: Vec<String>,
) -> Result<TerrainPathingPaintReport, EngineError> {
    if changed_cell_ids.is_empty() {
        return invalid_pathing("operation.no_change", "pathing operation changed no cells");
    }
    before.validate()?;
    after.validate()?;
    let before_fingerprint = stable_fingerprint(&before)?;
    let after_fingerprint = stable_fingerprint(&after)?;
    if before_fingerprint == after_fingerprint {
        return invalid_pathing(
            "operation.fingerprint",
            "pathing operation did not mutate state",
        );
    }
    let report = TerrainPathingPaintReport {
        operation,
        changed_cell_ids,
        before_fingerprint,
        after_fingerprint,
        before_summary: terrain_pathing_summary(&before),
        after_summary: terrain_pathing_summary(&after),
    };
    *document = after;
    Ok(report)
}

fn require_cell<'a>(
    document: &'a EditorDocument,
    terrain_cell_id: &str,
) -> Result<&'a crate::terrain_bulk_ops::EditorTerrainCell, EngineError> {
    document.terrain_cells.get(terrain_cell_id).ok_or_else(|| {
        invalid_pathing_error("cell", format!("missing terrain cell {terrain_cell_id}"))
    })
}

fn cells_by_coord(document: &EditorDocument) -> BTreeMap<(i32, i32), String> {
    document
        .terrain_cells
        .values()
        .map(|cell| ((cell.tile_x, cell.tile_z), cell.id.clone()))
        .collect()
}

fn coord_for_cell(
    document: &EditorDocument,
    terrain_cell_id: &str,
) -> Result<(i32, i32), EngineError> {
    let cell = require_cell(document, terrain_cell_id)?;
    Ok((cell.tile_x, cell.tile_z))
}

fn neighbors((x, z): (i32, i32)) -> [(i32, i32); 4] {
    [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)]
}

fn cell_allows(
    document: &EditorDocument,
    terrain_cell_id: &str,
    movement: TerrainPathMovement,
    blocked: &mut Vec<String>,
) -> Result<bool, EngineError> {
    let flags = effective_pathing_for_cell(document, terrain_cell_id)?;
    let allowed = flags.allows(movement);
    if !allowed {
        blocked.push(terrain_cell_id.to_string());
    }
    Ok(allowed)
}

fn reconstruct_path(
    start_cell_id: &str,
    goal_cell_id: &str,
    came_from: &BTreeMap<String, String>,
) -> Result<Vec<String>, EngineError> {
    let mut path = vec![goal_cell_id.to_string()];
    while path.last().map(String::as_str) != Some(start_cell_id) {
        let current = path.last().expect("path contains goal");
        let previous = came_from.get(current).ok_or_else(|| {
            invalid_pathing_error("query.path", format!("missing predecessor for {current}"))
        })?;
        path.push(previous.clone());
    }
    path.reverse();
    Ok(path)
}

fn flags_label(flags: TerrainPathingFlags) -> String {
    format!(
        "walkable={};buildable={};flyable={};blocked={}",
        flags.walkable, flags.buildable, flags.flyable, flags.blocked
    )
}

pub(crate) fn invalid_pathing<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_pathing_error(field, reason))
}

pub(crate) fn invalid_pathing_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.terrain_pathing.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "terrain_pathing_tests.rs"]
mod tests;
