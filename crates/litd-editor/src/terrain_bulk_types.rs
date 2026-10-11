use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::{history::EditorDocument, terrain_variation::TerrainTileVariation};

pub const OBJECT_TYPE_ID_PROPERTY: &str = "type_id";
const MIN_CLIFF_LEVEL: i16 = -16;
const MAX_CLIFF_LEVEL: i16 = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorTerrainCell {
    pub id: String,
    pub tile_x: i32,
    pub tile_z: i32,
    pub tile_id: String,
    pub cliff_type: String,
    pub cliff_level: i16,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainBulkCatalog {
    pub tile_variation_counts: BTreeMap<String, u16>,
    pub cliff_types: BTreeSet<String>,
    pub doodad_types: BTreeSet<String>,
    pub unit_types: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerrainBulkScope {
    WholeMap,
    Selection {
        terrain_cell_ids: BTreeSet<String>,
        object_ids: BTreeSet<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerrainBulkOperation {
    ReplaceTiles,
    ReplaceCliffType,
    ReplaceDoodads,
    ReplaceUnits,
    AdjustCliffLevels,
    ResetHeightField,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainBulkOperationReport {
    pub operation: TerrainBulkOperation,
    pub scope: String,
    pub matched_count: usize,
    pub changed_count: usize,
    pub skipped_count: usize,
    pub affected_ids: Vec<String>,
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub before_summary: String,
    pub after_summary: String,
}

impl EditorTerrainCell {
    pub fn new(
        id: impl Into<String>,
        tile_x: i32,
        tile_z: i32,
        tile_id: impl Into<String>,
        cliff_type: impl Into<String>,
        cliff_level: i16,
        height: f32,
    ) -> Self {
        Self {
            id: id.into(),
            tile_x,
            tile_z,
            tile_id: tile_id.into(),
            cliff_type: cliff_type.into(),
            cliff_level,
            height,
        }
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.id.trim().is_empty()
            || self.tile_id.trim().is_empty()
            || self.cliff_type.trim().is_empty()
        {
            return invalid_bulk("cell", "id, tile_id, and cliff_type must be non-empty");
        }
        validate_cliff_level(self.cliff_level)?;
        if !self.height.is_finite() {
            return invalid_bulk("cell.height", "height must be finite");
        }
        Ok(())
    }
}

impl TerrainBulkCatalog {
    pub fn new(
        tile_variation_counts: impl IntoIterator<Item = (String, u16)>,
        cliff_types: impl IntoIterator<Item = String>,
        doodad_types: impl IntoIterator<Item = String>,
        unit_types: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            tile_variation_counts: tile_variation_counts.into_iter().collect(),
            cliff_types: cliff_types.into_iter().collect(),
            doodad_types: doodad_types.into_iter().collect(),
            unit_types: unit_types.into_iter().collect(),
        }
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.tile_variation_counts.is_empty()
            || self.cliff_types.is_empty()
            || self.doodad_types.is_empty()
            || self.unit_types.is_empty()
        {
            return invalid_bulk("catalog", "all replacement catalogs must be non-empty");
        }
        for (tile_id, variation_count) in &self.tile_variation_counts {
            TerrainTileVariation::new("catalog_probe", 0, 0, tile_id, *variation_count, 0, 0)
                .validate()?;
        }
        validate_nonempty_set("catalog.cliff_types", &self.cliff_types)?;
        validate_nonempty_set("catalog.doodad_types", &self.doodad_types)?;
        validate_nonempty_set("catalog.unit_types", &self.unit_types)?;
        Ok(())
    }

    pub(crate) fn tile_variation_count(&self, tile_id: &str) -> Result<u16, EngineError> {
        self.tile_variation_counts
            .get(tile_id)
            .copied()
            .ok_or_else(|| invalid_bulk_error("catalog.tile", format!("missing tile {tile_id}")))
    }
}

impl TerrainBulkScope {
    pub(crate) fn label(&self) -> String {
        match self {
            TerrainBulkScope::WholeMap => "whole_map".to_string(),
            TerrainBulkScope::Selection {
                terrain_cell_ids,
                object_ids,
            } => format!(
                "selection:terrain_cells={}:objects={}",
                terrain_cell_ids.len(),
                object_ids.len()
            ),
        }
    }
}

pub fn validate_terrain_cells(
    cells: &BTreeMap<String, EditorTerrainCell>,
    variations: &BTreeMap<String, TerrainTileVariation>,
) -> Result<(), EngineError> {
    let mut coords = BTreeSet::new();
    let cells_by_coord = cells
        .values()
        .map(|cell| ((cell.tile_x, cell.tile_z), cell))
        .collect::<BTreeMap<_, _>>();
    for (id, cell) in cells {
        if id != &cell.id {
            return invalid_bulk("cell.id", format!("map key {id} does not match cell id"));
        }
        cell.validate()?;
        if !coords.insert((cell.tile_x, cell.tile_z)) {
            return invalid_bulk(
                "cell.coord",
                format!(
                    "duplicate terrain cell coordinate {},{}",
                    cell.tile_x, cell.tile_z
                ),
            );
        }
    }
    for variation in variations.values() {
        if let Some(cell) = cells_by_coord.get(&(variation.tile_x, variation.tile_z))
            && variation.tileset_id != cell.tile_id
        {
            return invalid_bulk(
                "cell.variation_tileset",
                format!(
                    "variation {} uses {} but cell {} uses {}",
                    variation.id, variation.tileset_id, cell.id, cell.tile_id
                ),
            );
        }
    }
    Ok(())
}

pub fn terrain_bulk_summary(document: &EditorDocument) -> String {
    let cells = document
        .terrain_cells
        .values()
        .map(|cell| {
            format!(
                "{}@{},{}:tile={}:cliff={}/{}:height={:.3}",
                cell.id,
                cell.tile_x,
                cell.tile_z,
                cell.tile_id,
                cell.cliff_type,
                cell.cliff_level,
                cell.height
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    let objects = document
        .objects
        .values()
        .filter(|object| object.kind == "Doodad" || object.kind == "Unit")
        .map(|object| {
            format!(
                "{}:{}:{}",
                object.id,
                object.kind,
                object
                    .properties
                    .get(OBJECT_TYPE_ID_PROPERTY)
                    .map(String::as_str)
                    .unwrap_or("<missing>")
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    format!("cells=[{cells}];objects=[{objects}]")
}

fn validate_nonempty_set(field: &str, values: &BTreeSet<String>) -> Result<(), EngineError> {
    for value in values {
        if value.trim().is_empty() {
            return invalid_bulk(field, "catalog values must be non-empty");
        }
    }
    Ok(())
}

pub(crate) fn validate_cliff_level(value: i16) -> Result<(), EngineError> {
    if !(MIN_CLIFF_LEVEL..=MAX_CLIFF_LEVEL).contains(&value) {
        return invalid_bulk(
            "cliff_level",
            format!("cliff level must be {MIN_CLIFF_LEVEL}..={MAX_CLIFF_LEVEL}"),
        );
    }
    Ok(())
}

pub(crate) fn invalid_bulk<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_bulk_error(field, reason))
}

pub(crate) fn invalid_bulk_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.terrain_bulk_ops.{}", field.into()),
        reason: reason.into(),
    }
}
