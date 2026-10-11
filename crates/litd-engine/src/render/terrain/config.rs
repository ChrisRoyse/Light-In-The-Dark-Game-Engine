//! Runtime configuration and the world heightfield the quadtree is built over.

use serde::{Deserialize, Serialize};

use crate::EngineError;

fn invalid<T>(field: &str, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::TerrainRuntimeInvalidConfig {
        field: field.to_string(),
        reason: reason.into(),
    })
}

/// A world-space heightmap the terrain runtime streams and draws. `size` is the
/// sample count per side (a square grid); the world spans `(size-1)*cell_size`
/// world units on each of X and Z, centred on the origin, heights on +Y.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeightField {
    /// Samples per side. Must be `leaf_nodes_per_side * grid_dim_cells + 1` so
    /// every leaf tile owns an exact integer patch of samples.
    pub size: u32,
    pub cell_size: f32,
    /// Row-major `size*size` heights.
    pub heights: Vec<f32>,
}

impl HeightField {
    pub fn new(size: u32, cell_size: f32, heights: Vec<f32>) -> Result<Self, EngineError> {
        if size < 2 {
            return invalid("heightfield.size", "need at least 2 samples per side");
        }
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return invalid(
                "heightfield.cell_size",
                "cell_size must be finite and positive",
            );
        }
        let expected = size as usize * size as usize;
        if heights.len() != expected {
            return invalid(
                "heightfield.heights",
                format!(
                    "expected {expected} heights for {size}x{size}, got {}",
                    heights.len()
                ),
            );
        }
        if heights.iter().any(|h| !h.is_finite()) {
            return invalid("heightfield.heights", "every height sample must be finite");
        }
        Ok(Self {
            size,
            cell_size,
            heights,
        })
    }

    /// Clamped sample fetch (edge-extend outside the grid).
    pub fn sample(&self, x: u32, z: u32) -> f32 {
        let x = x.min(self.size - 1);
        let z = z.min(self.size - 1);
        self.heights[(z * self.size + x) as usize]
    }

    /// World span (world units) along one axis.
    pub fn world_span(&self) -> f32 {
        (self.size - 1) as f32 * self.cell_size
    }

    /// Convert a sample coordinate to a centred world coordinate.
    pub fn sample_to_world(&self, sample: u32) -> f32 {
        sample as f32 * self.cell_size - self.world_span() * 0.5
    }
}

/// Static configuration for a terrain runtime. Derives the LOD ranges and morph
/// regions once at construction so per-frame selection is allocation-light.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainConfig {
    /// Leaf tiles per side of the world (power of two). LOD count is
    /// `log2(leaf_nodes_per_side) + 1`; the root is one whole-world tile.
    pub leaf_nodes_per_side: u32,
    /// Vertex-grid resolution of every drawn tile: `grid_dim` vertices per side
    /// (so `grid_dim-1` cells). Constant across LODs -> constant vertex budget.
    pub grid_dim: u32,
    /// Base view distance (world units) at which the finest LOD (0) hands off to
    /// LOD 1. Each coarser level doubles this. Larger -> more detail held closer.
    pub lod0_range: f32,
    /// Fraction of a level's range at which geomorph begins (Strugar's morph
    /// start ratio, typically ~0.66). Below this distance a tile is un-morphed.
    pub morph_start_ratio: f32,
    /// Nodes whose vertical height error is below this (world units) stop
    /// subdividing early -- flat ground draws coarse (Witcher-3 adaptive density).
    pub flat_error_cutoff: f32,
}

impl TerrainConfig {
    pub fn new(
        leaf_nodes_per_side: u32,
        grid_dim: u32,
        lod0_range: f32,
        morph_start_ratio: f32,
        flat_error_cutoff: f32,
    ) -> Result<Self, EngineError> {
        if leaf_nodes_per_side < 1 || !leaf_nodes_per_side.is_power_of_two() {
            return invalid(
                "config.leaf_nodes_per_side",
                "leaf_nodes_per_side must be a power of two >= 1",
            );
        }
        if grid_dim < 2 {
            return invalid("config.grid_dim", "grid_dim must be >= 2 (one quad)");
        }
        if !lod0_range.is_finite() || lod0_range <= 0.0 {
            return invalid(
                "config.lod0_range",
                "lod0_range must be finite and positive",
            );
        }
        if !(0.0..1.0).contains(&morph_start_ratio) {
            return invalid(
                "config.morph_start_ratio",
                "morph_start_ratio must be in [0, 1)",
            );
        }
        if !flat_error_cutoff.is_finite() || flat_error_cutoff < 0.0 {
            return invalid(
                "config.flat_error_cutoff",
                "flat_error_cutoff must be finite and non-negative",
            );
        }
        Ok(Self {
            leaf_nodes_per_side,
            grid_dim,
            lod0_range,
            morph_start_ratio,
            flat_error_cutoff,
        })
    }

    /// Number of LOD levels: 0 = finest (leaf), `lod_count-1` = coarsest (root).
    pub fn lod_count(&self) -> u32 {
        self.leaf_nodes_per_side.trailing_zeros() + 1
    }

    /// LOD hand-off distance for level `lod`: `lod0_range * 2^lod`.
    pub fn lod_range(&self, lod: u32) -> f32 {
        self.lod0_range * (1u32 << lod) as f32
    }

    /// Vertices drawn per tile (`grid_dim^2`); the per-tile vertex budget.
    pub fn tile_vertex_count(&self) -> u32 {
        self.grid_dim * self.grid_dim
    }

    /// Required heightfield sample count per side for a given leaf sample span.
    pub fn required_field_size(&self, leaf_sample_span: u32) -> u32 {
        self.leaf_nodes_per_side * leaf_sample_span + 1
    }

    /// Morph parameters for `lod`, honouring the constraint that a level's morph
    /// must finish (reach 1) no nearer than the previous level's hand-off so the
    /// two grids agree exactly at the seam.
    pub fn morph_params(&self, lod: u32) -> super::morph::MorphParams {
        let end = self.lod_range(lod);
        let mut start = self.morph_start_ratio * end;
        if lod > 0 {
            // Never begin morphing before the finer level has fully handed off,
            // otherwise a gap opens between the two LOD grids.
            start = start.max(self.lod_range(lod - 1));
        }
        super::morph::MorphParams::new(start, end)
    }
}
