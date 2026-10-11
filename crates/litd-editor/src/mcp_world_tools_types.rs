use std::path::PathBuf;

use litd_engine::math::Vec3;
use serde::{Deserialize, Serialize};

use crate::{
    history::{EditorDocument, EditorNamedCamera, EditorObject, EditorRegion},
    play::PlayCameraConfig,
    terrain_advanced_tools::TerrainBrushSpec,
    terrain_bulk_ops::{TerrainBulkCatalog, TerrainBulkScope},
    terrain_pathing::{TerrainPathingFlags, TerrainPathingPaintMode},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaintTilesArgs {
    pub document_path: PathBuf,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub initial_document: Option<EditorDocument>,
    pub brush: TerrainBrushSpec,
    pub pathing: Option<WorldPathingPaint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BulkEditTerrainArgs {
    pub document_path: PathBuf,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub initial_document: Option<EditorDocument>,
    pub edit: WorldBulkEdit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceActorRegionCameraArgs {
    pub document_path: PathBuf,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub initial_document: Option<EditorDocument>,
    pub objects: Vec<EditorObject>,
    pub regions: Vec<EditorRegion>,
    pub camera: Option<PlayCameraConfig>,
    /// #813: named cameras to place/update (top-down default + cinematic).
    #[serde(default)]
    pub named_cameras: Vec<EditorNamedCamera>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetStartLocationsArgs {
    pub document_path: PathBuf,
    pub ledger_path: PathBuf,
    pub ledger_id: String,
    pub request_id: String,
    pub requested_by: String,
    pub initial_document: Option<EditorDocument>,
    pub starts: Vec<WorldStartLocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadWorldArgs {
    pub document_path: PathBuf,
}

/// #812: read-only evaluation of the deterministic region enter/leave events a
/// `previous` -> `next` movement produces against the stored world document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluateRegionTransitionsArgs {
    pub document_path: PathBuf,
    pub previous: Vec3,
    pub next: Vec3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldPathingPaint {
    pub cell_ids: Vec<String>,
    pub mode: TerrainPathingPaintMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorldBulkEdit {
    ReplaceTiles {
        catalog: TerrainBulkCatalog,
        old_tile_id: String,
        new_tile_id: String,
        scope: TerrainBulkScope,
    },
    ReplaceCliffType {
        catalog: TerrainBulkCatalog,
        old_cliff_type: String,
        new_cliff_type: String,
        scope: TerrainBulkScope,
    },
    AdjustCliffLevels {
        delta: i16,
        scope: TerrainBulkScope,
    },
    ResetHeightField {
        base_height: f32,
        scope: TerrainBulkScope,
    },
    FloodFillTile {
        seed_cell_id: String,
        new_tile_id: String,
    },
    SetGlobalTilePathing {
        tile_id: String,
        flags: TerrainPathingFlags,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldStartLocation {
    /// Zero-based player slot index. Slots must be unique across the set and
    /// contiguous from 0 (0..starts.len()) so runtime melee/template wiring can
    /// index players positionally (#817).
    pub slot: u32,
    pub player_id: String,
    pub object_id: String,
    pub position: Vec3,
    pub yaw_radians: f32,
}
