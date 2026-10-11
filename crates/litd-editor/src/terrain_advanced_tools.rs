use std::collections::{BTreeMap, BTreeSet, VecDeque};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::{
    history::{EditorDocument, SelectionSet, stable_fingerprint},
    terrain_bulk_ops::{EditorTerrainCell, terrain_bulk_summary},
};

const MAX_BRUSH_SIZE_TILES: u32 = 4096;
const HEIGHT_EPSILON: f32 = 0.000_1;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TerrainMagicWandMode {
    SameTile,
    HeightWithin { tolerance: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerrainBrushShape {
    Circle,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerrainBrushFalloff {
    Constant,
    Linear,
    Smooth,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TerrainBrushOperation {
    RaiseHeight { delta: f32 },
    SetHeight { height: f32 },
    PaintTile { tile_id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainBrushSpec {
    pub center_tile_x: i32,
    pub center_tile_z: i32,
    pub size_tiles: u32,
    pub shape: TerrainBrushShape,
    pub falloff: TerrainBrushFalloff,
    pub operation: TerrainBrushOperation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainSelectionReport {
    pub operation: String,
    pub seed_cell_id: String,
    pub selected_cell_ids: Vec<String>,
    pub selected_count: usize,
    pub boundary_cell_ids: Vec<String>,
    pub source_summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainMutationReport {
    pub operation: String,
    pub changed_cell_ids: Vec<String>,
    pub changed_count: usize,
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub before_summary: String,
    pub after_summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainBrushCellWeight {
    pub terrain_cell_id: String,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainBrushReport {
    pub mutation: TerrainMutationReport,
    pub weighted_cells: Vec<TerrainBrushCellWeight>,
    pub max_weight: f32,
    pub min_nonzero_weight: f32,
}

impl TerrainSelectionReport {
    pub fn selection_set(&self) -> SelectionSet {
        let mut selection = SelectionSet::default();
        selection.select_many(self.selected_cell_ids.iter().cloned());
        selection
    }
}

impl TerrainBrushSpec {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.size_tiles == 0 || self.size_tiles > MAX_BRUSH_SIZE_TILES {
            return invalid_advanced(
                "brush.size",
                format!("size_tiles must be 1..={MAX_BRUSH_SIZE_TILES}"),
            );
        }
        match &self.operation {
            TerrainBrushOperation::RaiseHeight { delta } => {
                if !delta.is_finite() || delta.abs() <= HEIGHT_EPSILON {
                    return invalid_advanced(
                        "brush.delta",
                        "height delta must be finite and non-zero",
                    );
                }
            }
            TerrainBrushOperation::SetHeight { height } => {
                if !height.is_finite() {
                    return invalid_advanced("brush.height", "target height must be finite");
                }
            }
            TerrainBrushOperation::PaintTile { tile_id } => {
                if tile_id.trim().is_empty() {
                    return invalid_advanced("brush.tile", "tile id must be non-empty");
                }
            }
        }
        Ok(())
    }
}

pub fn magic_wand_select(
    document: &EditorDocument,
    seed_cell_id: &str,
    mode: TerrainMagicWandMode,
) -> Result<TerrainSelectionReport, EngineError> {
    document.validate()?;
    validate_magic_wand_mode(mode)?;
    let seed = require_cell(document, seed_cell_id)?;
    let cells_by_coord = cells_by_coord(document);
    let mut selected = BTreeSet::new();
    let mut boundary = BTreeSet::new();
    let mut queue = VecDeque::from([seed.id.clone()]);

    while let Some(cell_id) = queue.pop_front() {
        if !selected.insert(cell_id.clone()) {
            continue;
        }
        let coord = coord_for_cell(document, &cell_id)?;
        for neighbor_coord in neighbors(coord) {
            let Some(neighbor_id) = cells_by_coord.get(&neighbor_coord) else {
                continue;
            };
            if selected.contains(neighbor_id) {
                continue;
            }
            let neighbor = require_cell(document, neighbor_id)?;
            if magic_wand_matches(seed, neighbor, mode) {
                queue.push_back(neighbor_id.clone());
            } else {
                boundary.insert(neighbor_id.clone());
            }
        }
    }

    let selected_cell_ids = selected.into_iter().collect::<Vec<_>>();
    if selected_cell_ids.is_empty() {
        return invalid_advanced("selection.empty", "magic wand selected no cells");
    }
    Ok(TerrainSelectionReport {
        operation: format!("magic_wand.{mode:?}"),
        seed_cell_id: seed_cell_id.to_string(),
        selected_count: selected_cell_ids.len(),
        selected_cell_ids,
        boundary_cell_ids: boundary.into_iter().collect(),
        source_summary: terrain_bulk_summary(document),
    })
}

pub fn flood_fill_tile(
    document: &mut EditorDocument,
    seed_cell_id: &str,
    new_tile_id: &str,
) -> Result<TerrainMutationReport, EngineError> {
    if new_tile_id.trim().is_empty() {
        return invalid_advanced("flood_fill.tile", "new tile id must be non-empty");
    }
    let before = document.clone();
    let selection = magic_wand_select(&before, seed_cell_id, TerrainMagicWandMode::SameTile)?;
    let seed_tile = before.terrain_cells[seed_cell_id].tile_id.clone();
    if seed_tile == new_tile_id {
        return invalid_advanced(
            "flood_fill.no_change",
            "new tile must differ from seed tile",
        );
    }
    let mut after = before.clone();
    for id in &selection.selected_cell_ids {
        after.terrain_cells.get_mut(id).expect("known id").tile_id = new_tile_id.to_string();
    }
    finish_mutation(
        document,
        before,
        after,
        format!("flood_fill.tile.{seed_tile}->{new_tile_id}"),
        selection.selected_cell_ids,
    )
}

pub fn apply_terrain_brush(
    document: &mut EditorDocument,
    spec: &TerrainBrushSpec,
) -> Result<TerrainBrushReport, EngineError> {
    spec.validate()?;
    document.validate()?;
    let before = document.clone();
    let mut after = before.clone();
    let weighted_cells = brush_weights(&before, spec)?;
    if weighted_cells.is_empty() {
        return invalid_advanced("brush.matches", "brush matched no terrain cells");
    }

    let mut changed = Vec::new();
    for weighted in &weighted_cells {
        let cell = after
            .terrain_cells
            .get_mut(&weighted.terrain_cell_id)
            .expect("known weighted cell");
        let changed_cell = apply_brush_operation(cell, weighted.weight, &spec.operation)?;
        if changed_cell {
            changed.push(weighted.terrain_cell_id.clone());
        }
    }
    let mutation = finish_mutation(
        document,
        before,
        after,
        format!("brush.{:?}.{:?}", spec.shape, spec.falloff),
        changed,
    )?;
    let max_weight = weighted_cells
        .iter()
        .map(|cell| cell.weight)
        .fold(0.0_f32, f32::max);
    let min_nonzero_weight = weighted_cells
        .iter()
        .filter(|cell| cell.weight > 0.0)
        .map(|cell| cell.weight)
        .fold(f32::INFINITY, f32::min);
    Ok(TerrainBrushReport {
        mutation,
        weighted_cells,
        max_weight,
        min_nonzero_weight,
    })
}

pub fn terrain_advanced_summary(document: &EditorDocument) -> String {
    terrain_bulk_summary(document)
}

fn brush_weights(
    document: &EditorDocument,
    spec: &TerrainBrushSpec,
) -> Result<Vec<TerrainBrushCellWeight>, EngineError> {
    let mut weighted = Vec::new();
    let radius = spec.size_tiles as f32 * 0.5;
    for cell in document.terrain_cells.values() {
        let dx = (cell.tile_x - spec.center_tile_x) as f32;
        let dz = (cell.tile_z - spec.center_tile_z) as f32;
        let distance = match spec.shape {
            TerrainBrushShape::Circle => (dx.mul_add(dx, dz * dz)).sqrt(),
            TerrainBrushShape::Square => dx.abs().max(dz.abs()),
        };
        if distance > radius {
            continue;
        }
        let normalized = if radius <= 0.5 {
            0.0
        } else {
            (distance / radius).min(1.0)
        };
        let weight = match spec.falloff {
            TerrainBrushFalloff::Constant => 1.0,
            TerrainBrushFalloff::Linear => 1.0 - normalized,
            TerrainBrushFalloff::Smooth => {
                let inverse = 1.0 - normalized;
                inverse * inverse * (3.0 - 2.0 * inverse)
            }
        };
        if weight > 0.0 {
            weighted.push(TerrainBrushCellWeight {
                terrain_cell_id: cell.id.clone(),
                weight,
            });
        }
    }
    weighted.sort_by(|left, right| left.terrain_cell_id.cmp(&right.terrain_cell_id));
    Ok(weighted)
}

fn apply_brush_operation(
    cell: &mut EditorTerrainCell,
    weight: f32,
    operation: &TerrainBrushOperation,
) -> Result<bool, EngineError> {
    match operation {
        TerrainBrushOperation::RaiseHeight { delta } => {
            let next = cell.height + delta * weight;
            if !next.is_finite() {
                return invalid_advanced(
                    "brush.height",
                    "raise operation produced non-finite height",
                );
            }
            if (cell.height - next).abs() > HEIGHT_EPSILON {
                cell.height = next;
                return Ok(true);
            }
            Ok(false)
        }
        TerrainBrushOperation::SetHeight { height } => {
            let next = cell.height + (height - cell.height) * weight;
            if !next.is_finite() {
                return invalid_advanced(
                    "brush.height",
                    "set operation produced non-finite height",
                );
            }
            if (cell.height - next).abs() > HEIGHT_EPSILON {
                cell.height = next;
                return Ok(true);
            }
            Ok(false)
        }
        TerrainBrushOperation::PaintTile { tile_id } => {
            if cell.tile_id != *tile_id {
                cell.tile_id = tile_id.clone();
                return Ok(true);
            }
            Ok(false)
        }
    }
}

fn finish_mutation(
    document: &mut EditorDocument,
    before: EditorDocument,
    after: EditorDocument,
    operation: String,
    changed_cell_ids: Vec<String>,
) -> Result<TerrainMutationReport, EngineError> {
    if changed_cell_ids.is_empty() {
        return invalid_advanced("operation.no_change", "terrain operation changed no cells");
    }
    before.validate()?;
    after.validate()?;
    let before_fingerprint = stable_fingerprint(&before)?;
    let after_fingerprint = stable_fingerprint(&after)?;
    if before_fingerprint == after_fingerprint {
        return invalid_advanced(
            "operation.fingerprint",
            "terrain operation did not mutate state",
        );
    }
    let report = TerrainMutationReport {
        operation,
        changed_count: changed_cell_ids.len(),
        changed_cell_ids,
        before_fingerprint,
        after_fingerprint,
        before_summary: terrain_bulk_summary(&before),
        after_summary: terrain_bulk_summary(&after),
    };
    *document = after;
    Ok(report)
}

fn validate_magic_wand_mode(mode: TerrainMagicWandMode) -> Result<(), EngineError> {
    match mode {
        TerrainMagicWandMode::SameTile => Ok(()),
        TerrainMagicWandMode::HeightWithin { tolerance } => {
            if !tolerance.is_finite() || tolerance < 0.0 {
                return invalid_advanced(
                    "selection.height_tolerance",
                    "height tolerance must be finite and non-negative",
                );
            }
            Ok(())
        }
    }
}

fn magic_wand_matches(
    seed: &EditorTerrainCell,
    cell: &EditorTerrainCell,
    mode: TerrainMagicWandMode,
) -> bool {
    match mode {
        TerrainMagicWandMode::SameTile => cell.tile_id == seed.tile_id,
        TerrainMagicWandMode::HeightWithin { tolerance } => {
            (cell.height - seed.height).abs() <= tolerance
        }
    }
}

fn require_cell<'a>(
    document: &'a EditorDocument,
    terrain_cell_id: &str,
) -> Result<&'a EditorTerrainCell, EngineError> {
    document
        .terrain_cells
        .get(terrain_cell_id)
        .ok_or_else(|| advanced_error("cell", format!("missing terrain cell {terrain_cell_id}")))
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

pub(crate) fn invalid_advanced<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(advanced_error(field, reason))
}

pub(crate) fn advanced_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.terrain_advanced_tools.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "terrain_advanced_tools_tests.rs"]
mod tests;
