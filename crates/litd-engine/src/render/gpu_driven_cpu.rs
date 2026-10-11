//! CPU two-phase Hi-Z cull (issue #1740): the non-indirect fallback draw path AND
//! the deterministic oracle the GPU compute readback is verified against.
//!
//! Every step mirrors the shaders bit-for-bit in intent so the two draw paths emit
//! the same visible set: seed the base depth level from the previous frame's visible
//! instances (min-combine of nearest depths, `hzb_build.wgsl` primes this), max-reduce
//! the pyramid, then frustum + Hi-Z test every instance and stream-compact the
//! survivors (`two_phase_cull.wgsl`). The float math (`log2`/`ceil`/`floor`) matches
//! the WGSL so mip selection agrees; fixtures keep depths and rect spans away from
//! decision boundaries so no ULP split can diverge the two paths.

use super::batching::{Frustum, RenderInstance, ScreenRect};
use super::gpu_driven::{
    CompactionResult, HzbPyramidDims, TwoPhaseSettings, VisibilityClass, invalid,
};
use crate::error::EngineError;

/// Depth stored for texels no previous-frame occluder covers: the far plane, so an
/// empty region never occludes anything.
const FAR_DEPTH: f32 = 1.0;

/// Seed the finest pyramid level (level 0) from the previous frame's visible set:
/// each covered texel takes the nearest (minimum) depth of the instances over it.
/// This is the "render last frame's visible set / prime the depth" phase-1 input.
pub fn build_base_depth(
    dims: &HzbPyramidDims,
    instances: &[RenderInstance],
    prev_visible: &[bool],
) -> Result<Vec<f32>, EngineError> {
    if prev_visible.len() != instances.len() {
        return Err(invalid(
            "gpu_driven.prev_visible",
            "prev_visible length must equal the instance count",
        ));
    }
    let base_dim = dims.base_dim();
    let mut base = vec![FAR_DEPTH; (base_dim * base_dim) as usize];
    for (instance, &visible) in instances.iter().zip(prev_visible.iter()) {
        if !visible {
            continue;
        }
        let (x0, y0, x1, y1) = rect_texels(base_dim, instance.screen_rect);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let texel = (y * base_dim + x) as usize;
                base[texel] = base[texel].min(instance.nearest_depth);
            }
        }
    }
    Ok(base)
}

/// Max-reduce the seeded base level into the full flat pyramid buffer.
pub fn build_pyramid(dims: &HzbPyramidDims, base: &[f32]) -> Result<Vec<f32>, EngineError> {
    let base_texels = (dims.base_dim() * dims.base_dim()) as usize;
    if base.len() != base_texels {
        return Err(invalid(
            "gpu_driven.base_depth",
            "base depth length must equal base_dim squared",
        ));
    }
    let mut pyramid = vec![FAR_DEPTH; dims.total_texels() as usize];
    pyramid[..base_texels].copy_from_slice(base);
    for level in 1..dims.level_count() {
        let src_dim = dims.level_dim(level - 1);
        let dst_dim = dims.level_dim(level);
        let src_off = dims.level_offset(level - 1) as usize;
        let dst_off = dims.level_offset(level) as usize;
        for dy in 0..dst_dim {
            for dx in 0..dst_dim {
                let sx = dx * 2;
                let sy = dy * 2;
                let x1 = (sx + 1).min(src_dim - 1);
                let y1 = (sy + 1).min(src_dim - 1);
                let a = pyramid[src_off + (sy * src_dim + sx) as usize];
                let b = pyramid[src_off + (sy * src_dim + x1) as usize];
                let c = pyramid[src_off + (y1 * src_dim + sx) as usize];
                let d = pyramid[src_off + (y1 * src_dim + x1) as usize];
                pyramid[dst_off + (dy * dst_dim + dx) as usize] = a.max(b).max(c.max(d));
            }
        }
    }
    Ok(pyramid)
}

/// Max pyramid depth over a screen rect, selecting the mip level whose texels cover
/// the rect in a single (up to 2x2) tap -- identical logic to `two_phase_cull.wgsl`.
fn sample_pyramid_max(dims: &HzbPyramidDims, pyramid: &[f32], rect: ScreenRect) -> f32 {
    let base = dims.base_dim() as f32;
    let span = ((rect.max.x - rect.min.x) * base).max((rect.max.y - rect.min.y) * base);
    let level = (span.max(1.0).log2().ceil() as i32).clamp(0, dims.level_count() as i32 - 1) as u32;
    let dim = dims.level_dim(level);
    let off = dims.level_offset(level) as usize;
    let fdim = dim as f32;
    let last = dim as i32 - 1;
    let x0 = ((rect.min.x * fdim).floor() as i32).clamp(0, last);
    let y0 = ((rect.min.y * fdim).floor() as i32).clamp(0, last);
    let x1 = ((rect.max.x * fdim).floor() as i32).clamp(0, last);
    let y1 = ((rect.max.y * fdim).floor() as i32).clamp(0, last);
    let mut maxd = 0.0f32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            maxd = maxd.max(pyramid[off + (y as u32 * dim + x as u32) as usize]);
        }
    }
    maxd
}

fn is_occluded(
    settings: TwoPhaseSettings,
    dims: &HzbPyramidDims,
    pyramid: &[f32],
    instance: &RenderInstance,
) -> bool {
    if !settings.hzb_enabled {
        return false;
    }
    let maxd = sample_pyramid_max(dims, pyramid, instance.screen_rect);
    instance.nearest_depth > maxd + settings.occlusion_bias
}

/// Run the full CPU two-phase cull and return the compacted result. This is the exact
/// draw set the GPU indirect path must reproduce.
pub fn run_two_phase_cpu(
    settings: TwoPhaseSettings,
    frustum: &Frustum,
    instances: &[RenderInstance],
    prev_visible: &[bool],
) -> Result<CompactionResult, EngineError> {
    settings.validate()?;
    frustum.validate()?;
    for (index, instance) in instances.iter().enumerate() {
        instance.validate(index)?;
    }
    let dims = HzbPyramidDims::new(settings.base_dim)?;
    let base = build_base_depth(&dims, instances, prev_visible)?;
    let pyramid = build_pyramid(&dims, &base)?;

    let mut classes = Vec::with_capacity(instances.len());
    let mut visible_indices = Vec::new();
    let mut phase1_count = 0u32;
    let mut phase2_count = 0u32;
    for (index, instance) in instances.iter().enumerate() {
        let visible = frustum.contains_aabb(instance.world_bounds)
            && !is_occluded(settings, &dims, &pyramid, instance);
        if !visible {
            classes.push(VisibilityClass::Culled);
            continue;
        }
        visible_indices.push(index as u32);
        if prev_visible[index] {
            phase1_count += 1;
            classes.push(VisibilityClass::Phase1);
        } else {
            phase2_count += 1;
            classes.push(VisibilityClass::Phase2);
        }
    }

    Ok(CompactionResult {
        visible_count: visible_indices.len() as u32,
        phase1_count,
        phase2_count,
        visible_indices,
        classes,
    })
}

/// Inclusive texel span `(x0, y0, x1, y1)` a normalized screen rect covers at the
/// base level.
fn rect_texels(base_dim: u32, rect: ScreenRect) -> (u32, u32, u32, u32) {
    let last = base_dim - 1;
    let f = base_dim as f32;
    let clamp_axis = |v: f32| ((v * f).floor() as i32).clamp(0, last as i32) as u32;
    (
        clamp_axis(rect.min.x),
        clamp_axis(rect.min.y),
        clamp_axis(rect.max.x),
        clamp_axis(rect.max.y),
    )
}
