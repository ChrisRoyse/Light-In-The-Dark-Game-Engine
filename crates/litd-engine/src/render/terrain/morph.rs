//! CDLOD geomorph math -- the CPU mirror of `cdlod_morph.wgsl`.
//!
//! Geomorphing removes LOD popping. Within a tile drawn at some LOD, as a vertex
//! recedes from the camera its position is snapped toward the next-coarser LOD
//! grid over the morph region `[start, end]`. At `end` (== that LOD's hand-off
//! distance) the morph is complete, so the tile's silhouette is IDENTICAL to the
//! coarser tile that takes over beyond `end`: no popping. Odd-indexed grid
//! vertices (the ones absent from the coarser grid) collapse onto their even
//! neighbour; even vertices are shared by both grids and never move.

/// Morph region for one LOD level: distances at which geomorph starts and ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MorphParams {
    pub start: f32,
    pub end: f32,
}

impl MorphParams {
    pub fn new(start: f32, end: f32) -> Self {
        // `end > start` is guaranteed by `TerrainConfig::morph_params`; clamp
        // defensively so a degenerate region yields a hard (still seam-free) step.
        Self {
            start: start.min(end),
            end,
        }
    }
}

/// Morph factor in `[0, 1]` for a vertex at world distance `dist` from the
/// camera. 0 = full detail (no snapping), 1 = fully collapsed onto the coarser
/// grid. Strugar eq.: `clamp((dist - start) / (end - start), 0, 1)`.
pub fn morph_factor(params: MorphParams, dist: f32) -> f32 {
    let span = params.end - params.start;
    if span <= f32::EPSILON {
        return if dist >= params.end { 1.0 } else { 0.0 };
    }
    ((dist - params.start) / span).clamp(0.0, 1.0)
}

/// Snap a tile-local grid coordinate toward the coarser (even) grid by `morph`.
///
/// `grid` is the integer vertex index on one axis (`0..grid_dim`). The coarser
/// grid keeps only even indices, so an odd vertex must move by one `cell` toward
/// its lower even neighbour as `morph -> 1`. Returned value is the morphed local
/// offset in tile cells (multiply by cell size for world units). Matches the WGSL
/// `fract(gridPos * 0.5) * 2` snap.
pub fn morph_grid_xz(grid: u32, morph: f32) -> f32 {
    let base = grid as f32;
    // 1.0 for odd indices, 0.0 for even -- the amount this vertex is "extra"
    // relative to the coarser grid.
    let is_odd = (grid & 1) as f32;
    base - is_odd * morph
}

/// Blend a fine-LOD height sample toward the coarser-LOD height by `morph`. At
/// `morph == 1` the vertex sits exactly on the coarser surface, matching the
/// neighbouring coarse tile (C0 seam continuity).
pub fn morph_height(fine_height: f32, coarse_height: f32, morph: f32) -> f32 {
    fine_height + (coarse_height - fine_height) * morph
}
