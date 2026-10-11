//! CDLOD / quadtree GPU-driven terrain RUNTIME (#1739).
//!
//! The editor authors terrain (`litd-editor` `terrain_*` modules) and `procgen`
//! lowers a world heightfield into per-tile meshes; this module is the RUNTIME
//! that decides, every frame, WHICH tiles at WHICH level of detail to draw for a
//! given camera -- with a constant vertex budget regardless of world size, and
//! seam-free geomorphing so there is no LOD popping.
//!
//! # Design lineage (researched, not vendored -- permissive-only, #1073)
//!
//! - CDLOD (Filip Strugar, "Continuous Distance-Dependent Level of Detail for
//!   Rendering Heightmaps", J. Graphics/GPU/Game Tools 2010): an implicit
//!   quadtree of uniform vertex-grid tiles, per-node min/max height for
//!   bounding, distance-sphere LOD selection, and per-vertex geomorphing that
//!   snaps odd grid vertices onto the next-coarser LOD so adjacent LODs stay C0
//!   continuous. `select` mirrors Strugar's `LODSelect`; `morph` mirrors his
//!   vertex morph. Source studied: <https://aggrobird.com/files/cdlod_latest.pdf>.
//! - UDLOD / `bevy_terrain` (Kurt Kuehnert, BSc thesis, Rust/wgpu-native,
//!   <https://d-nb.info/1383361673/34>): GPU-driven quadtree subdivision + a node
//!   atlas with a clipped quadtree lookup and graceful coarse-tile fallback while
//!   streaming lags. `streaming::NodeAtlas` follows that residency + fallback model.
//! - Witcher-3-style vertical-error / adaptive-density metric: per-node max height
//!   error lets flat ground stop subdividing early (~2x raster savings on plains).
//!   `quadtree` stores per-node vertical error; `select` honours a flatness cutoff.
//!
//! Per the render-tech skip list (#1746) this is CPU-side selection feeding ONE
//! indirect-style draw of a shared grid; the geomorph vertex program is
//! first-party WGSL (`cdlod_morph.wgsl`), no mesh shaders, no runtime splat bake.
//! Selection is deterministic (seeded/no wall-clock, valve #725): identical inputs
//! yield an identical draw list, so it is Calyx-ingestible and MCP-queryable.

mod config;
mod morph;
mod quadtree;
mod runtime;
mod select;
mod streaming;

#[cfg(test)]
mod tests;

pub use config::{HeightField, TerrainConfig};
pub use morph::{MorphParams, morph_factor, morph_grid_xz, morph_height};
pub use quadtree::{NodeId, QuadNode, TerrainQuadtree};
pub use runtime::{FrameSelection, FrameStats, SelectedTile, TerrainRuntime};
pub use select::TileResidency;
pub use streaming::{NodeAtlas, StreamRequest};

/// First-party CDLOD geomorph + splat-blend vertex program. Kept as a string so
/// FSV can assert its structure headlessly and the GPU path can create a module
/// from the identical bytes (mirrors `lighting::FORWARD_PLUS_WGSL`).
pub const CDLOD_MORPH_WGSL: &str = include_str!("cdlod_morph.wgsl");
