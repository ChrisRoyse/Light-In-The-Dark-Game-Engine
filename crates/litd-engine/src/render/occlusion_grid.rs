//! Broad-phase screen-space occlusion grid (issue #1524 item 2).
//!
//! `build_batch_plan` used to test every instance against every occluder
//! (`O(instances x occluders)`) despite the "HZB" name. This module replaces that
//! linear scan with a low-resolution uniform grid over normalized screen space
//! `[0,1]^2`. Each occluder is bucketed into every cell its screen rect overlaps; an
//! instance is then tested only against the occluders in the single cell that
//! contains the instance's min corner.
//!
//! Correctness (no false negatives): if an occluder *contains* an instance's rect then
//! `occluder.min <= instance.min <= instance.max <= occluder.max`, so the instance's
//! min-corner point lies inside the occluder, which means the occluder was bucketed
//! into that corner cell. The exact per-candidate `contains` + depth test is still run,
//! so no false positives are introduced either — the emitted `BatchPlan` is identical
//! to the old full scan, bit for bit.
//!
//! This is the "low-resolution occlusion grid" broad phase (Hey / Tobler /
//! Purgathofer, EGWR 2001) layered over the existing HZB screen-rect + depth test.

use super::batching::{BatchingSettings, HzbOccluder, RenderInstance};

/// Upper bound on grid resolution per axis. Bounds the worst-case build cost of a
/// single screen-filling occluder (which must be bucketed into `dim*dim` cells) while
/// still giving small, spread-out occluders near-`O(1)` candidate lists.
const MAX_GRID_DIM: usize = 128;

pub(super) struct OcclusionGrid {
    dim: usize,
    /// CSR row offsets: cell `c` owns `entries[cell_starts[c]..cell_starts[c + 1]]`.
    cell_starts: Vec<u32>,
    /// Occluder indices grouped by cell.
    entries: Vec<u32>,
    bias: f32,
}

impl OcclusionGrid {
    /// Builds the grid over `occluders`. Returns `None` when occlusion is disabled or
    /// there is nothing to test, so the caller skips culling entirely (matching the old
    /// `settings.hzb_enabled` short-circuit).
    pub(super) fn build(settings: BatchingSettings, occluders: &[HzbOccluder]) -> Option<Self> {
        if !settings.hzb_enabled || occluders.is_empty() {
            return None;
        }
        // Resolution ~ sqrt(n): keeps average occupancy per cell roughly constant as
        // the occluder count grows, so per-instance candidate lists stay short.
        let dim = (occluders.len() as f64).sqrt().ceil() as usize;
        let dim = dim.clamp(1, MAX_GRID_DIM);
        let cell_count = dim * dim;

        // Pass 1: count occluders per cell.
        let mut counts = vec![0u32; cell_count + 1];
        for occluder in occluders {
            let (x0, y0, x1, y1) = cell_span(dim, occluder);
            for cy in y0..=y1 {
                for cx in x0..=x1 {
                    counts[cy * dim + cx] += 1;
                }
            }
        }

        // Prefix sum -> CSR starts.
        let mut cell_starts = vec![0u32; cell_count + 1];
        let mut running = 0u32;
        for cell in 0..cell_count {
            cell_starts[cell] = running;
            running += counts[cell];
        }
        cell_starts[cell_count] = running;

        // Pass 2: scatter occluder indices into their cells using a moving cursor.
        let mut cursor = cell_starts.clone();
        let mut entries = vec![0u32; running as usize];
        for (index, occluder) in occluders.iter().enumerate() {
            let (x0, y0, x1, y1) = cell_span(dim, occluder);
            for cy in y0..=y1 {
                for cx in x0..=x1 {
                    let cell = cy * dim + cx;
                    entries[cursor[cell] as usize] = index as u32;
                    cursor[cell] += 1;
                }
            }
        }

        Some(Self {
            dim,
            cell_starts,
            entries,
            bias: settings.occlusion_bias,
        })
    }

    /// Returns `true` when some occluder fully contains `instance` and is strictly
    /// nearer than it (occluder depth + bias). Only the occluders bucketed into the
    /// instance's min-corner cell are examined.
    pub(super) fn is_occluded(&self, occluders: &[HzbOccluder], instance: &RenderInstance) -> bool {
        let cx = axis_cell(self.dim, instance.screen_rect.min.x);
        let cy = axis_cell(self.dim, instance.screen_rect.min.y);
        let cell = cy * self.dim + cx;
        let range = self.cell_starts[cell] as usize..self.cell_starts[cell + 1] as usize;
        self.entries[range].iter().any(|&index| {
            let occluder = &occluders[index as usize];
            occluder.screen_rect.contains(instance.screen_rect)
                && instance.nearest_depth > occluder.depth + self.bias
        })
    }
}

/// Maps a normalized `[0,1]` coordinate to a cell column/row, clamped into range.
fn axis_cell(dim: usize, value: f32) -> usize {
    let scaled = (value.clamp(0.0, 1.0) * dim as f32).floor() as isize;
    scaled.clamp(0, dim as isize - 1) as usize
}

/// The inclusive cell span `(x0, y0, x1, y1)` an occluder's screen rect overlaps.
fn cell_span(dim: usize, occluder: &HzbOccluder) -> (usize, usize, usize, usize) {
    let rect = occluder.screen_rect;
    (
        axis_cell(dim, rect.min.x),
        axis_cell(dim, rect.min.y),
        axis_cell(dim, rect.max.x),
        axis_cell(dim, rect.max.y),
    )
}
