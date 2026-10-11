//! Value types for the terrain lowering valve (#806/#807/#811). The lowering
//! logic lives in `terrain_lowering.rs`; these are the frozen, fingerprinted
//! artifact shapes it emits for the CDLOD runtime and the nav bake/A* path.

use litd_engine::{
    nav::NavBakeInput,
    render::terrain::{HeightField, TerrainConfig},
};
use serde::{Deserialize, Serialize};

pub const TERRAIN_HEIGHTFIELD_LOWERED_SCHEMA: &str = "litd.terrain.lowered.heightfield.v1";
pub const TERRAIN_TILE_SPLAT_LOWERED_SCHEMA: &str = "litd.terrain.lowered.tile_splat.v1";
pub const TERRAIN_NAV_LOWERED_SCHEMA: &str = "litd.terrain.lowered.nav_input.v1";

/// Lowering knobs for the CDLOD height field. The authored tile grid is resampled
/// onto a `leaf_nodes_per_side * leaf_sample_span + 1` square sample grid so every
/// leaf tile owns an exact integer patch (the CDLOD invariant).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeightFieldLowerParams {
    pub leaf_nodes_per_side: u32,
    pub leaf_sample_span: u32,
    pub cell_size: f32,
    pub grid_dim: u32,
    pub lod0_range: f32,
    pub morph_start_ratio: f32,
    pub flat_error_cutoff: f32,
    /// World-height added per authored cliff level (WC3-style stepped cliffs).
    pub cliff_step: f32,
}

impl Default for HeightFieldLowerParams {
    fn default() -> Self {
        Self {
            leaf_nodes_per_side: 16,
            leaf_sample_span: 4,
            cell_size: 1.0,
            grid_dim: 9,
            lod0_range: 6.0,
            morph_start_ratio: 0.66,
            flat_error_cutoff: 0.05,
            cliff_step: 4.0,
        }
    }
}

/// Lowered CDLOD terrain: a dense height field + runtime config (#807).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoweredHeightField {
    pub schema: String,
    pub source_fingerprint: String,
    pub field: HeightField,
    pub config: TerrainConfig,
    pub tiles_x: u32,
    pub tiles_z: u32,
    pub min_height: f32,
    pub max_height: f32,
    pub sample_count: usize,
    pub fingerprint: String,
}

/// Lowered tile-splat palette + per-tile index buffer (#806).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredTileSplat {
    pub schema: String,
    pub source_fingerprint: String,
    /// Palette: sorted-unique authored tile ids. `> 1` means a mixed/custom set.
    pub palette: Vec<String>,
    pub min_x: i32,
    pub min_z: i32,
    pub tiles_x: u32,
    pub tiles_z: u32,
    /// Row-major (`tiles_z` rows of `tiles_x`) palette index per authored tile.
    pub indices: Vec<u16>,
    pub cell_count: usize,
    pub fingerprint: String,
}

impl LoweredTileSplat {
    /// Reconstruct the authored tile id at a tile coordinate from the lowered
    /// buffer -- the render-consumption readback: `palette[indices[..]]`.
    pub fn tile_id_at(&self, x: i32, z: i32) -> Option<&str> {
        if x < self.min_x || z < self.min_z {
            return None;
        }
        let lx = (x - self.min_x) as u32;
        let lz = (z - self.min_z) as u32;
        if lx >= self.tiles_x || lz >= self.tiles_z {
            return None;
        }
        let flat = (lz * self.tiles_x + lx) as usize;
        let palette_index = *self.indices.get(flat)? as usize;
        self.palette.get(palette_index).map(String::as_str)
    }
}

/// Lowered nav bake input (#811): pathing flags + blockers ready for `bake_navmesh`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoweredNavInput {
    pub schema: String,
    pub source_fingerprint: String,
    pub input: NavBakeInput,
    pub cell_count: usize,
    pub blocker_count: usize,
    pub walkable_count: usize,
    pub blocked_count: usize,
    pub fingerprint: String,
}

/// All three lowered artifacts for one document (the MCP tool payload).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoweredTerrainBundle {
    pub source_fingerprint: String,
    pub height_field: LoweredHeightField,
    pub tile_splat: LoweredTileSplat,
    pub nav_input: LoweredNavInput,
}
