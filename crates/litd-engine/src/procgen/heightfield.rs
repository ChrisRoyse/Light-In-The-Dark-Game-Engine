//! Terrain heightfield -> indexed mesh, with per-tile LOD decimation, a
//! crack-hiding skirt, and per-vertex material splat weights.
//!
//! LOD: `lod` selects a power-of-two sample stride (`1 << lod`); the tile is
//! decimated to `((width-1)/stride + 1) x ((depth-1)/stride + 1)` vertices. The
//! stride must divide both `width-1` and `depth-1` so every LOD shares the tile
//! boundary vertices — the precondition that lets neighbouring tiles of
//! different LOD line up. Where a seam still differs, an optional downward
//! `skirt` (flange) of vertical geometry hides the crack. This matches the
//! chunked-LOD skirt/stitch practice (Ulrich 2002; TUM GPU-terrain 2006). A
//! shader-side CDLOD vertex morph (Strugar 2010) is the renderer's job (#1098)
//! and out of scope for generation.
//!
//! Splat: per-vertex weights for up to 4 material layers, chosen from height
//! bands, so terrain painting has a deterministic default hook (#801).

use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::mesh::{ProcMesh, invalid};
use super::primitives::{push_tri, push_vertex};

/// A material layer keyed to a world-height band. Weights ramp smoothly across
/// `feather` so adjacent bands blend instead of hard-stepping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplatBand {
    pub material_id: String,
    pub height_min: f32,
    pub height_max: f32,
    pub feather: f32,
}

/// Input for [`generate_heightfield`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeightfieldSpec {
    pub tile_id: String,
    /// Sample counts (not cells). A mesh needs at least 2x2 samples (one quad).
    pub width: u32,
    pub depth: u32,
    pub cell_size: f32,
    /// Row-major `width*depth` heights (world units on +Y).
    pub heights: Vec<f32>,
    /// LOD level: sample stride is `1 << lod`.
    pub lod: u32,
    /// Downward skirt depth in world units; `0.0` disables the skirt.
    pub skirt: f32,
    /// Up to 4 material splat bands; empty means a single implicit layer 0.
    pub splat_bands: Vec<SplatBand>,
}

/// Geometry plus the LOD/splat metadata a caller needs for FSV and painting.
#[derive(Debug, Clone)]
pub struct HeightfieldProduct {
    pub mesh: ProcMesh,
    pub lod_stride: u32,
    pub grid_width: u32,
    pub grid_depth: u32,
    pub has_skirt: bool,
    pub splat_layer_count: u32,
    /// Per-vertex 4-layer splat weights, aligned to `mesh.vertices`.
    pub splat_weights: Vec<[f32; 4]>,
}

const MAX_SPLAT_LAYERS: usize = 4;
const MAX_HEIGHTFIELD_SAMPLES: usize = 4096 * 4096;

pub fn generate_heightfield(spec: &HeightfieldSpec) -> Result<HeightfieldProduct, EngineError> {
    validate(spec)?;
    let stride = 1u32 << spec.lod;
    let grid_width = (spec.width - 1) / stride + 1;
    let grid_depth = (spec.depth - 1) / stride + 1;
    let span_x = (spec.width - 1) as f32 * spec.cell_size;
    let span_z = (spec.depth - 1) as f32 * spec.cell_size;

    let mut mesh = ProcMesh::new(spec.tile_id.clone());
    let mut splat_weights = Vec::new();

    // Decimated grid vertices.
    for gz in 0..grid_depth {
        let z_full = gz * stride;
        for gx in 0..grid_width {
            let x_full = gx * stride;
            let height = sample(spec, x_full, z_full);
            let position = [
                x_full as f32 * spec.cell_size - span_x * 0.5,
                height,
                z_full as f32 * spec.cell_size - span_z * 0.5,
            ];
            let normal = height_normal(spec, x_full, z_full);
            let uv = [
                gx as f32 / (grid_width - 1) as f32,
                gz as f32 / (grid_depth - 1) as f32,
            ];
            push_vertex(&mut mesh, position, normal, uv);
            splat_weights.push(splat_at(spec, height));
        }
    }
    for gz in 0..grid_depth - 1 {
        for gx in 0..grid_width - 1 {
            let a = gz * grid_width + gx;
            push_tri(&mut mesh, a, a + grid_width, a + grid_width + 1);
            push_tri(&mut mesh, a, a + grid_width + 1, a + 1);
        }
    }

    let has_skirt = spec.skirt > 0.0;
    if has_skirt {
        append_skirt(
            &mut mesh,
            &mut splat_weights,
            grid_width,
            grid_depth,
            spec.skirt,
        );
    }

    let mesh = mesh.finalize()?;
    debug_assert_eq!(mesh.vertices.len(), splat_weights.len());
    Ok(HeightfieldProduct {
        mesh,
        lod_stride: stride,
        grid_width,
        grid_depth,
        has_skirt,
        splat_layer_count: spec.splat_bands.len().max(1) as u32,
        splat_weights,
    })
}

fn validate(spec: &HeightfieldSpec) -> Result<(), EngineError> {
    if spec.width < 2 || spec.depth < 2 {
        return invalid(
            "heightfield.dimensions",
            format!(
                "a heightfield mesh needs at least 2x2 samples; got {}x{} (a 1x1 tile has no \
                 quad to triangulate)",
                spec.width, spec.depth
            ),
        );
    }
    let sample_count = spec.width as usize * spec.depth as usize;
    if sample_count > MAX_HEIGHTFIELD_SAMPLES {
        return invalid(
            "heightfield.dimensions",
            format!("sample count {sample_count} exceeds the maximum of {MAX_HEIGHTFIELD_SAMPLES}"),
        );
    }
    if spec.heights.len() != sample_count {
        return invalid(
            "heightfield.heights",
            format!(
                "expected {sample_count} heights for {}x{}, got {}",
                spec.width,
                spec.depth,
                spec.heights.len()
            ),
        );
    }
    if !spec.cell_size.is_finite() || spec.cell_size <= 0.0 {
        return invalid(
            "heightfield.cell_size",
            "cell_size must be finite and positive",
        );
    }
    if spec.heights.iter().any(|height| !height.is_finite()) {
        return invalid("heightfield.heights", "every height sample must be finite");
    }
    if spec.lod >= 16 {
        return invalid("heightfield.lod", "lod must be < 16");
    }
    let stride = 1u32 << spec.lod;
    if !(spec.width - 1).is_multiple_of(stride) || !(spec.depth - 1).is_multiple_of(stride) {
        return invalid(
            "heightfield.lod",
            format!(
                "lod stride {stride} must divide both (width-1)={} and (depth-1)={} so tile \
                 boundaries stay shared across LODs",
                spec.width - 1,
                spec.depth - 1
            ),
        );
    }
    if !spec.skirt.is_finite() || spec.skirt < 0.0 {
        return invalid("heightfield.skirt", "skirt must be finite and non-negative");
    }
    if spec.splat_bands.len() > MAX_SPLAT_LAYERS {
        return invalid(
            "heightfield.splat_bands",
            format!("at most {MAX_SPLAT_LAYERS} splat layers are supported"),
        );
    }
    for (layer, band) in spec.splat_bands.iter().enumerate() {
        if !band.height_min.is_finite()
            || !band.height_max.is_finite()
            || !band.feather.is_finite()
            || band.height_max < band.height_min
            || band.feather < 0.0
        {
            return invalid(
                "heightfield.splat_band",
                format!("splat band {layer} has an invalid height range or feather"),
            );
        }
    }
    Ok(())
}

fn sample(spec: &HeightfieldSpec, x: u32, z: u32) -> f32 {
    let x = x.min(spec.width - 1);
    let z = z.min(spec.depth - 1);
    spec.heights[(z * spec.width + x) as usize]
}

fn height_normal(spec: &HeightfieldSpec, x: u32, z: u32) -> [f32; 3] {
    let left = sample(spec, x.saturating_sub(1), z);
    let right = sample(spec, (x + 1).min(spec.width - 1), z);
    let down = sample(spec, x, z.saturating_sub(1));
    let up = sample(spec, x, (z + 1).min(spec.depth - 1));
    let dx = (right - left) / (2.0 * spec.cell_size);
    let dz = (up - down) / (2.0 * spec.cell_size);
    let normal = [-dx, 1.0, -dz];
    let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    [normal[0] / len, normal[1] / len, normal[2] / len]
}

fn splat_at(spec: &HeightfieldSpec, height: f32) -> [f32; 4] {
    if spec.splat_bands.is_empty() {
        return [1.0, 0.0, 0.0, 0.0];
    }
    let mut weights = [0.0f32; 4];
    for (layer, band) in spec.splat_bands.iter().enumerate() {
        weights[layer] = band_weight(band, height);
    }
    let sum: f32 = weights.iter().sum();
    if sum > f32::EPSILON {
        for weight in &mut weights {
            *weight /= sum;
        }
    } else {
        weights[0] = 1.0;
    }
    weights
}

fn band_weight(band: &SplatBand, height: f32) -> f32 {
    if band.feather <= f32::EPSILON {
        return if (band.height_min..=band.height_max).contains(&height) {
            1.0
        } else {
            0.0
        };
    }
    // Ramp up over [min-feather, min], plateau to [max], ramp down to max+feather.
    let rising = smoothstep(band.height_min - band.feather, band.height_min, height);
    let falling = 1.0 - smoothstep(band.height_max, band.height_max + band.feather, height);
    (rising * falling).clamp(0.0, 1.0)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if (edge1 - edge0).abs() <= f32::EPSILON {
        return if value >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Vertical border geometry dropped by `skirt` below the tile edge, hiding LOD
/// cracks against neighbouring tiles. Duplicates edge vertices (dropped) and
/// stitches quads around the perimeter.
fn append_skirt(
    mesh: &mut ProcMesh,
    splat_weights: &mut Vec<[f32; 4]>,
    grid_width: u32,
    grid_depth: u32,
    skirt: f32,
) {
    let perimeter = perimeter_indices(grid_width, grid_depth);
    for pair in perimeter.windows(2) {
        let top_a = pair[0];
        let top_b = pair[1];
        let (pa, na, ua) = vertex_attr(mesh, top_a);
        let (pb, nb, ub) = vertex_attr(mesh, top_b);
        let bottom_a = push_vertex(mesh, [pa[0], pa[1] - skirt, pa[2]], na, ua);
        splat_weights.push(splat_weights[top_a as usize]);
        let bottom_b = push_vertex(mesh, [pb[0], pb[1] - skirt, pb[2]], nb, ub);
        splat_weights.push(splat_weights[top_b as usize]);
        push_tri(mesh, top_a, bottom_a, bottom_b);
        push_tri(mesh, top_a, bottom_b, top_b);
    }
}

fn vertex_attr(mesh: &ProcMesh, index: u32) -> ([f32; 3], [f32; 3], [f32; 2]) {
    let vertex = mesh.vertices[index as usize];
    (vertex.position, vertex.normal, vertex.uv)
}

/// Clockwise perimeter loop of grid vertex indices, closed back to the start.
fn perimeter_indices(grid_width: u32, grid_depth: u32) -> Vec<u32> {
    let idx = |x: u32, z: u32| z * grid_width + x;
    let mut loop_indices = Vec::new();
    for x in 0..grid_width {
        loop_indices.push(idx(x, 0));
    }
    for z in 1..grid_depth {
        loop_indices.push(idx(grid_width - 1, z));
    }
    for x in (0..grid_width - 1).rev() {
        loop_indices.push(idx(x, grid_depth - 1));
    }
    for z in (1..grid_depth - 1).rev() {
        loop_indices.push(idx(0, z));
    }
    loop_indices.push(idx(0, 0));
    loop_indices
}
