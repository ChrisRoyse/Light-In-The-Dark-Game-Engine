//! `TerrainRuntime`: the per-frame driver that ties the quadtree, CDLOD
//! selection and the streaming atlas into a single draw list with a constant
//! vertex budget. Everything here is deterministic and headless-verifiable; the
//! GPU consumes `FrameSelection.tiles` as one instanced/indirect grid draw.

use crate::EngineError;
use crate::math::Vec3;
use crate::render::camera::CameraFrustum;

use super::config::{HeightField, TerrainConfig};
use super::quadtree::{NodeId, TerrainQuadtree};
use super::select::select_tiles;
use super::streaming::{NodeAtlas, StreamRequest};

pub use super::select::{SelectedTile, TileResidency};

/// Aggregate statistics for one selected frame -- the FSV source of truth.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameStats {
    /// Tiles drawn this frame (== indirect draw count).
    pub tile_count: u32,
    /// Vertices submitted = `tile_count * grid_dim^2`. The budget metric.
    pub total_vertices: u64,
    /// Triangles submitted = `tile_count * (grid_dim-1)^2 * 2`.
    pub total_triangles: u64,
    /// Tiles per LOD level, index = lod.
    pub tiles_per_lod: Vec<u32>,
    /// Distinct LOD levels present (the "LOD rings" the camera spans).
    pub distinct_lods: u32,
    /// Tiles served coarse because their data was still streaming.
    pub fallback_count: u32,
    /// Non-resident tiles requested for streaming this frame.
    pub request_count: u32,
    /// Smallest / largest geomorph factor across drawn tiles (0..1).
    pub min_center_morph: f32,
    pub max_center_morph: f32,
}

/// The full result of selecting one frame.
#[derive(Debug, Clone)]
pub struct FrameSelection {
    pub tiles: Vec<SelectedTile>,
    pub stats: FrameStats,
    /// Nearest-first stream requests for the tiles not yet resident.
    pub requests: Vec<StreamRequest>,
}

/// Owns the immutable tree + config and the mutable streaming atlas.
#[derive(Debug, Clone)]
pub struct TerrainRuntime {
    config: TerrainConfig,
    tree: TerrainQuadtree,
    atlas: NodeAtlas,
}

impl TerrainRuntime {
    /// Build over `field`. The atlas starts with only the root resident and a
    /// generous capacity (every node); use [`Self::with_atlas_capacity`] to model
    /// a bandwidth-limited stream.
    pub fn new(field: &HeightField, config: TerrainConfig) -> Result<Self, EngineError> {
        let tree = TerrainQuadtree::build(field, &config)?;
        let capacity = tree.total_nodes();
        Self::from_tree(config, tree, capacity)
    }

    /// Build with a bounded atlas of `atlas_capacity` tiles (models streaming
    /// pressure -> coarse fallback).
    pub fn with_atlas_capacity(
        field: &HeightField,
        config: TerrainConfig,
        atlas_capacity: usize,
    ) -> Result<Self, EngineError> {
        let tree = TerrainQuadtree::build(field, &config)?;
        Self::from_tree(config, tree, atlas_capacity)
    }

    fn from_tree(
        config: TerrainConfig,
        tree: TerrainQuadtree,
        atlas_capacity: usize,
    ) -> Result<Self, EngineError> {
        let atlas = NodeAtlas::new(atlas_capacity, tree.root().id);
        Ok(Self {
            config,
            tree,
            atlas,
        })
    }

    pub fn config(&self) -> &TerrainConfig {
        &self.config
    }

    pub fn tree(&self) -> &TerrainQuadtree {
        &self.tree
    }

    pub fn atlas(&self) -> &NodeAtlas {
        &self.atlas
    }

    /// Stream a tile's data into the atlas. Returns false if the atlas is full.
    pub fn stream_in(&mut self, id: NodeId) -> bool {
        self.atlas.mark_resident(id)
    }

    /// Select the draw list for a camera. `frustum` is optional: pass `None` to
    /// select the whole in-range set (pure LOD verification without a view
    /// matrix), or a real frustum for view culling.
    pub fn select_frame(
        &mut self,
        camera: Vec3,
        frustum: Option<&CameraFrustum>,
    ) -> FrameSelection {
        let mut tiles = Vec::new();
        select_tiles(
            &self.tree,
            &self.config,
            camera,
            frustum,
            &mut self.atlas,
            &mut tiles,
        );
        let requests = self.atlas.drain_requests();
        let stats = self.compute_stats(&tiles, requests.len() as u32);
        FrameSelection {
            tiles,
            stats,
            requests,
        }
    }

    fn compute_stats(&self, tiles: &[SelectedTile], request_count: u32) -> FrameStats {
        let verts_per_tile = self.config.tile_vertex_count() as u64;
        let cells = (self.config.grid_dim - 1) as u64;
        let tris_per_tile = cells * cells * 2;

        let mut tiles_per_lod = vec![0u32; self.config.lod_count() as usize];
        let mut fallback_count = 0u32;
        let mut min_morph = f32::INFINITY;
        let mut max_morph = f32::NEG_INFINITY;
        for tile in tiles {
            tiles_per_lod[tile.lod as usize] += 1;
            if matches!(tile.residency, TileResidency::Fallback { .. }) {
                fallback_count += 1;
            }
            min_morph = min_morph.min(tile.center_morph);
            max_morph = max_morph.max(tile.center_morph);
        }
        if tiles.is_empty() {
            min_morph = 0.0;
            max_morph = 0.0;
        }
        let tile_count = tiles.len() as u32;
        let distinct_lods = tiles_per_lod.iter().filter(|&&c| c > 0).count() as u32;
        FrameStats {
            tile_count,
            total_vertices: verts_per_tile * tile_count as u64,
            total_triangles: tris_per_tile * tile_count as u64,
            tiles_per_lod,
            distinct_lods,
            fallback_count,
            request_count,
            min_center_morph: min_morph,
            max_center_morph: max_morph,
        }
    }
}
