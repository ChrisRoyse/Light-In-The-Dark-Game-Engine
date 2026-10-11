//! GPU-driven two-phase Hi-Z occlusion culling + indirect-draw compaction
//! (issue #1740).
//!
//! This is the "GPU-driven, not virtualized" middle path sanctioned by the render-
//! tech skip list (#1746): a Hi-Z depth pyramid built from the previous frame's
//! visible set, an all-instance re-test in compute, and stream compaction of the
//! survivors into an indirect draw-command buffer whose length is a GPU-written
//! count buffer (consumed by `multi_draw_indirect_count`). It is deliberately NOT
//! the Nanite path -- no 64-bit-atomic visibility buffer, no compute SW raster, no
//! mesh shaders (all blocked on min-spec DX12 iGPUs, #1746).
//!
//! This module holds the CPU-side surface: the shader sources, the pyramid layout
//! math, the explicit (never silent) GPU-vs-CPU draw-path selection, and the plain
//! data the compute pass and its CPU fallback both consume. The two-phase logic
//! itself lives in `gpu_driven_cpu` (the CPU fallback path + the oracle the GPU
//! readback is checked against); the wgpu compute pass that drives `HZB_BUILD_WGSL`
//! and `TWO_PHASE_CULL_WGSL` on real hardware and reads back the count buffer is the
//! `issue1740_two_phase_cull_fsv` example harness (live render-graph wiring pending).

use serde::{Deserialize, Serialize};

use crate::error::EngineError;

/// Hi-Z pyramid max-reduce build shader (`hzb_build.wgsl`).
pub const HZB_BUILD_WGSL: &str = include_str!("hzb_build.wgsl");
/// Two-phase cull + indirect compaction shader (`two_phase_cull.wgsl`).
pub const TWO_PHASE_CULL_WGSL: &str = include_str!("two_phase_cull.wgsl");

/// Which draw path the frame runs. This is an explicit configuration choice logged
/// at selection time (#1740 hardware-risk mitigation), never a silent fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawPathMode {
    /// GPU-driven: cull + compact in compute, submit via indirect draws.
    GpuIndirect,
    /// CPU-culled, non-indirect: the fallback the quality scale can force, and the
    /// mandated escape for hardware where indirect-draw workloads hang (the July
    /// 2026 Intel UHD 7xx / Iris Xe advisory, #1740 / #1470).
    CpuFallback,
}

/// Why a given [`DrawPathMode`] was selected -- recorded so the choice is auditable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawPathReason {
    /// GPU indirect was requested and the adapter supports indirect execution.
    GpuRequestedAndSupported,
    /// The CPU fallback was explicitly requested (quality scale / hardware policy).
    CpuRequested,
}

/// The audited outcome of [`select_draw_path`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawPathSelection {
    pub requested: DrawPathMode,
    pub selected: DrawPathMode,
    pub indirect_supported: bool,
    pub reason: DrawPathReason,
}

/// Resolve the draw path from the requested mode and the adapter's indirect-execution
/// capability, logging the decision.
///
/// Fail-loud doctrine: requesting [`DrawPathMode::GpuIndirect`] on an adapter without
/// indirect execution is a hard error that names the CPU-fallback flag -- it never
/// silently degrades to the CPU path. Selecting the CPU path must be a deliberate
/// configuration choice, not an accident of missing hardware support.
pub fn select_draw_path(
    requested: DrawPathMode,
    indirect_supported: bool,
) -> Result<DrawPathSelection, EngineError> {
    match requested {
        DrawPathMode::CpuFallback => {
            let selection = DrawPathSelection {
                requested,
                selected: DrawPathMode::CpuFallback,
                indirect_supported,
                reason: DrawPathReason::CpuRequested,
            };
            tracing::info!(
                requested = ?requested,
                selected = ?selection.selected,
                indirect_supported,
                "litd gpu-driven draw path: CPU fallback selected by explicit configuration"
            );
            Ok(selection)
        }
        DrawPathMode::GpuIndirect if indirect_supported => {
            let selection = DrawPathSelection {
                requested,
                selected: DrawPathMode::GpuIndirect,
                indirect_supported,
                reason: DrawPathReason::GpuRequestedAndSupported,
            };
            tracing::info!(
                requested = ?requested,
                selected = ?selection.selected,
                indirect_supported,
                "litd gpu-driven draw path: GPU indirect selected"
            );
            Ok(selection)
        }
        DrawPathMode::GpuIndirect => Err(EngineError::BatchingInvalidConfig {
            field: "render.gpu_driven.draw_path".to_string(),
            reason:
                "GpuIndirect requested but the adapter lacks DownlevelFlags::INDIRECT_EXECUTION; \
                set the draw path to CpuFallback explicitly (quality scale / hardware policy) \
                rather than relying on a silent fallback"
                    .to_string(),
        }),
    }
}

/// Two-phase cull tuning. Mirrors the shader's `CullParams`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TwoPhaseSettings {
    /// Base (finest) Hi-Z pyramid dimension per axis. Must be a power of two.
    pub base_dim: u32,
    /// Whether Hi-Z occlusion is applied (frustum culling always runs).
    pub hzb_enabled: bool,
    /// Depth slack added to the pyramid sample before an instance is called occluded.
    pub occlusion_bias: f32,
}

impl Default for TwoPhaseSettings {
    fn default() -> Self {
        Self {
            base_dim: 64,
            hzb_enabled: true,
            occlusion_bias: 0.001,
        }
    }
}

impl TwoPhaseSettings {
    pub fn validate(&self) -> Result<(), EngineError> {
        if !self.occlusion_bias.is_finite() || self.occlusion_bias < 0.0 {
            return Err(invalid(
                "settings.occlusion_bias",
                "must be finite and non-negative",
            ));
        }
        HzbPyramidDims::new(self.base_dim)?;
        Ok(())
    }
}

/// Flat-buffer layout of the Hi-Z pyramid: every square mip level packed back to
/// back, from the finest `base_dim x base_dim` down to `1 x 1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HzbPyramidDims {
    base_dim: u32,
    level_offsets: Vec<u32>,
    total_texels: u32,
}

impl HzbPyramidDims {
    /// Build the layout for a power-of-two `base_dim` in `1..=4096`.
    pub fn new(base_dim: u32) -> Result<Self, EngineError> {
        if base_dim == 0 || !base_dim.is_power_of_two() {
            return Err(invalid(
                "settings.base_dim",
                "Hi-Z base dimension must be a non-zero power of two",
            ));
        }
        if base_dim > 4096 {
            return Err(invalid(
                "settings.base_dim",
                "Hi-Z base dimension must be <= 4096",
            ));
        }
        let level_count = base_dim.trailing_zeros() + 1;
        let mut level_offsets = Vec::with_capacity(level_count as usize);
        let mut offset = 0u32;
        for level in 0..level_count {
            level_offsets.push(offset);
            let dim = base_dim >> level;
            offset += dim * dim;
        }
        Ok(Self {
            base_dim,
            level_offsets,
            total_texels: offset,
        })
    }

    pub fn base_dim(&self) -> u32 {
        self.base_dim
    }

    /// Number of mip levels (finest through 1x1 inclusive).
    pub fn level_count(&self) -> u32 {
        self.level_offsets.len() as u32
    }

    /// Per-axis dimension of `level` (0 = finest).
    pub fn level_dim(&self, level: u32) -> u32 {
        self.base_dim >> level
    }

    /// Flat-buffer texel offset where `level` begins.
    pub fn level_offset(&self, level: u32) -> u32 {
        self.level_offsets[level as usize]
    }

    /// All level offsets, for upload to the shader's `level_offsets` binding.
    pub fn level_offsets(&self) -> &[u32] {
        &self.level_offsets
    }

    /// Total texels across every level (the pyramid buffer length).
    pub fn total_texels(&self) -> u32 {
        self.total_texels
    }
}

/// Per-instance visibility class the cull pass records (matches the shader).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisibilityClass {
    /// Culled (frustum or Hi-Z occlusion) -- not drawn this frame.
    Culled = 0,
    /// Visible last frame and still visible: drawn in phase 1 (primes the pyramid).
    Phase1 = 1,
    /// Newly disoccluded / newly entered: drawn in phase 2.
    Phase2 = 2,
}

impl VisibilityClass {
    pub fn from_u32(value: u32) -> Result<Self, EngineError> {
        match value {
            0 => Ok(Self::Culled),
            1 => Ok(Self::Phase1),
            2 => Ok(Self::Phase2),
            other => Err(invalid(
                "gpu_driven.visibility_class",
                format!("out-of-range class {other}"),
            )),
        }
    }
}

/// The compacted output of a two-phase cull, produced identically by the GPU pass
/// and the CPU fallback so the two paths draw the same set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactionResult {
    /// Count buffer value: how many instances survived (multi_draw_indirect_count).
    pub visible_count: u32,
    /// Instances drawn in phase 1 (previously visible and still visible).
    pub phase1_count: u32,
    /// Instances drawn in phase 2 (newly disoccluded / newly entered).
    pub phase2_count: u32,
    /// Original instance indices of the survivors (sorted ascending for determinism;
    /// the GPU emits them in atomic-race order and this is its sorted set).
    pub visible_indices: Vec<u32>,
    /// Per-instance class, indexed by original instance index.
    pub classes: Vec<VisibilityClass>,
}

impl CompactionResult {
    /// The set-level invariant the FSV relies on: counts agree with the compacted
    /// set, and phase1 + phase2 partitions the visible set.
    pub fn is_consistent(&self) -> bool {
        self.visible_count as usize == self.visible_indices.len()
            && self.phase1_count + self.phase2_count == self.visible_count
    }
}

pub(crate) fn invalid(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::BatchingInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "gpu_driven_tests.rs"]
mod tests;
