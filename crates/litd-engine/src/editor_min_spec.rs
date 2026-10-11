//! Editor min-spec model (#1793 / #1790 sec 3): adapter-class classification,
//! per-class degrade knobs (shadow atlas size + viewport resolution scale), and the
//! measured editor startup/interactive budget. Every knob fails loud on an invalid
//! value -- there is NO silent clamp. The budget struct records real measured numbers
//! and gates them against fail-loud ceilings so "fast start" is a tracked FSV number,
//! not a vibe.

use serde::{Deserialize, Serialize};

use crate::{error::EngineError, gpu::AdapterReport};

/// The class of GPU adapter, derived from wgpu's reported device type. The editor
/// runs on every class; the class only chooses sensible degrade DEFAULTS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterClass {
    Discrete,
    Integrated,
    Virtual,
    Cpu,
    Unknown,
}

impl AdapterClass {
    /// Classify from wgpu's `AdapterInfo.device_type` debug string (as stored in
    /// [`AdapterReport`]). Unknown strings map to [`AdapterClass::Unknown`], which
    /// takes the conservative integrated-class defaults -- never a discrete default.
    pub fn from_device_type_str(device_type: &str) -> Self {
        match device_type {
            "DiscreteGpu" => Self::Discrete,
            "IntegratedGpu" => Self::Integrated,
            "VirtualGpu" => Self::Virtual,
            "Cpu" => Self::Cpu,
            _ => Self::Unknown,
        }
    }

    pub fn from_report(report: &AdapterReport) -> Self {
        Self::from_device_type_str(&report.device_type)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Discrete => "discrete",
            Self::Integrated => "integrated",
            Self::Virtual => "virtual",
            Self::Cpu => "cpu",
            Self::Unknown => "unknown",
        }
    }

    /// True for the classes that get the min-spec (integrated) degrade defaults:
    /// integrated, cpu, virtual, and unknown all default conservatively.
    const fn is_min_spec_class(self) -> bool {
        !matches!(self, Self::Discrete)
    }
}

/// Editor degrade knobs surfaced as settings (#1793 deliverable c). Both fail loud
/// on invalid values so an operator cannot silently ship a broken viewport or a
/// non-power-of-two shadow atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorDegradeSettings {
    /// Adapter class these knobs were defaulted for (provenance for the FSV record).
    pub adapter_class: AdapterClass,
    /// Square shadow-atlas edge in texels. Power of two in `[256, 8192]`. Bounds
    /// shadow VRAM; the min-spec floor is 512-1024, discrete scales to 4096+.
    pub shadow_atlas_size: u32,
    /// Internal viewport render scale as a percentage of the editor surface, in
    /// `[25, 100]`. Below 100 the editor renders smaller and upscales for present.
    pub viewport_resolution_scale_percent: u32,
}

pub const MIN_SHADOW_ATLAS_SIZE: u32 = 256;
pub const MAX_SHADOW_ATLAS_SIZE: u32 = 8192;
pub const MIN_VIEWPORT_SCALE_PERCENT: u32 = 25;
pub const MAX_VIEWPORT_SCALE_PERCENT: u32 = 100;

impl EditorDegradeSettings {
    /// Per-adapter-class defaults. Discrete gets full-quality knobs; every other
    /// class (integrated/cpu/virtual/unknown) gets the min-spec floor.
    pub fn defaults_for(adapter_class: AdapterClass) -> Self {
        if adapter_class.is_min_spec_class() {
            Self {
                adapter_class,
                shadow_atlas_size: 1024,
                viewport_resolution_scale_percent: 75,
            }
        } else {
            Self {
                adapter_class,
                shadow_atlas_size: 4096,
                viewport_resolution_scale_percent: 100,
            }
        }
    }

    /// The lowest-tier forced settings used by the FSV harness: smallest atlas and
    /// half-resolution viewport. Exercises the degrade path at its floor.
    pub fn forced_low_tier(adapter_class: AdapterClass) -> Self {
        Self {
            adapter_class,
            shadow_atlas_size: 512,
            viewport_resolution_scale_percent: 50,
        }
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if !(MIN_SHADOW_ATLAS_SIZE..=MAX_SHADOW_ATLAS_SIZE).contains(&self.shadow_atlas_size) {
            return Err(degrade_error(
                "shadow_atlas_size",
                format!(
                    "must be in {MIN_SHADOW_ATLAS_SIZE}..={MAX_SHADOW_ATLAS_SIZE}, got {}",
                    self.shadow_atlas_size
                ),
            ));
        }
        if !self.shadow_atlas_size.is_power_of_two() {
            return Err(degrade_error(
                "shadow_atlas_size",
                format!(
                    "must be a power of two, got {} (a non-po2 atlas breaks tile addressing)",
                    self.shadow_atlas_size
                ),
            ));
        }
        if !(MIN_VIEWPORT_SCALE_PERCENT..=MAX_VIEWPORT_SCALE_PERCENT)
            .contains(&self.viewport_resolution_scale_percent)
        {
            return Err(degrade_error(
                "viewport_resolution_scale_percent",
                format!(
                    "must be in {MIN_VIEWPORT_SCALE_PERCENT}..={MAX_VIEWPORT_SCALE_PERCENT}, got {}",
                    self.viewport_resolution_scale_percent
                ),
            ));
        }
        Ok(())
    }

    /// Internal render dimensions for a base editor surface after the viewport
    /// scale is applied. Fails loud on a zero base surface.
    pub fn scaled_dimensions(
        &self,
        base_width: u32,
        base_height: u32,
    ) -> Result<(u32, u32), EngineError> {
        self.validate()?;
        if base_width == 0 || base_height == 0 {
            return Err(degrade_error(
                "surface",
                "base surface dimensions must be non-zero",
            ));
        }
        let scale = self.viewport_resolution_scale_percent;
        let width = (base_width * scale / 100).max(1);
        let height = (base_height * scale / 100).max(1);
        Ok((width, height))
    }

    /// Deterministic VRAM footprint the degrade knobs control: the shadow atlas
    /// (Depth32) plus the scaled viewport's colour (RGBA8), depth (Depth32) and HDR
    /// intermediate (RGBA16F). Reproducible from the settings alone -- the number
    /// the knobs exist to bound.
    pub fn derived_vram_footprint_bytes(
        &self,
        base_width: u32,
        base_height: u32,
    ) -> Result<u64, EngineError> {
        let (width, height) = self.scaled_dimensions(base_width, base_height)?;
        let atlas = u64::from(self.shadow_atlas_size) * u64::from(self.shadow_atlas_size) * 4;
        let pixels = u64::from(width) * u64::from(height);
        let color = pixels * 4;
        let depth = pixels * 4;
        let hdr = pixels * 8;
        Ok(atlas + color + depth + hdr)
    }
}

/// Fail-loud ceilings for the editor min-spec budget. Measured numbers above any
/// ceiling are a real regression, not a tuning knob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorMinSpecCeilings {
    pub cold_start_budget_ns: u128,
    pub interactive_frame_budget_ns: u128,
    pub working_set_budget_bytes: u64,
    pub vram_footprint_budget_bytes: u64,
}

impl EditorMinSpecCeilings {
    /// The #1790 sec 3 editor baseline: interactive (>= 30 fps) frames, a fast cold
    /// start, and a modest RAM/VRAM footprint at the forced low tier.
    pub const fn editor_min_spec() -> Self {
        Self {
            // Cold Vulkan device + pipeline compile + scene upload on integrated-class HW.
            cold_start_budget_ns: 30_000_000_000,
            // 30 fps interactive floor per #1790 sec 1.
            interactive_frame_budget_ns: 33_333_333,
            // 2 GiB editor base process working set.
            working_set_budget_bytes: 2 * 1024 * 1024 * 1024,
            // 1 GiB derived VRAM footprint at the forced low tier (<= 8 GB VRAM machines).
            vram_footprint_budget_bytes: 1024 * 1024 * 1024,
        }
    }
}

/// The measured editor min-spec budget for one forced-adapter, forced-low-tier run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorMinSpecBudget {
    pub adapter_class: AdapterClass,
    pub adapter_name: String,
    pub backend: String,
    pub forced_preference: String,
    pub degrade: EditorDegradeSettings,
    pub base_surface_width: u32,
    pub base_surface_height: u32,
    pub internal_render_width: u32,
    pub internal_render_height: u32,
    pub cold_start_ns: u128,
    pub startup_working_set_bytes: u64,
    pub derived_vram_footprint_bytes: u64,
    pub interactive_frame_ns_p50: u128,
    pub interactive_frame_ns_max: u128,
    pub structural_edit_apply_ns: u128,
    pub frames_measured: u32,
    pub ceilings: EditorMinSpecCeilings,
}

impl EditorMinSpecBudget {
    /// Gate every measured number against its ceiling. Fails loud naming the first
    /// breach -- this is the FSV verdict's pass/fail.
    pub fn validate_within_budget(&self) -> Result<(), EngineError> {
        self.degrade.validate()?;
        if self.frames_measured == 0 {
            return Err(budget_error(
                "frames_measured",
                "no interactive frames were measured",
            ));
        }
        if self.cold_start_ns > self.ceilings.cold_start_budget_ns {
            return Err(budget_error(
                "cold_start_ns",
                format!(
                    "{} ns exceeds cold-start budget {} ns",
                    self.cold_start_ns, self.ceilings.cold_start_budget_ns
                ),
            ));
        }
        if self.interactive_frame_ns_p50 > self.ceilings.interactive_frame_budget_ns {
            return Err(budget_error(
                "interactive_frame_ns_p50",
                format!(
                    "p50 {} ns exceeds interactive frame budget {} ns",
                    self.interactive_frame_ns_p50, self.ceilings.interactive_frame_budget_ns
                ),
            ));
        }
        if self.startup_working_set_bytes > self.ceilings.working_set_budget_bytes {
            return Err(budget_error(
                "startup_working_set_bytes",
                format!(
                    "{} bytes exceeds working-set budget {} bytes",
                    self.startup_working_set_bytes, self.ceilings.working_set_budget_bytes
                ),
            ));
        }
        if self.derived_vram_footprint_bytes > self.ceilings.vram_footprint_budget_bytes {
            return Err(budget_error(
                "derived_vram_footprint_bytes",
                format!(
                    "{} bytes exceeds VRAM footprint budget {} bytes",
                    self.derived_vram_footprint_bytes, self.ceilings.vram_footprint_budget_bytes
                ),
            ));
        }
        Ok(())
    }
}

/// The current process's resident working-set size in bytes. Real measurement via
/// `K32GetProcessMemoryInfo` (exported by kernel32; no extra crate). Fails loud on
/// unsupported platforms and on API failure -- never a fabricated number.
#[cfg(windows)]
pub fn process_working_set_bytes() -> Result<u64, EngineError> {
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut core::ffi::c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }
    let mut counters = ProcessMemoryCounters {
        cb: core::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };
    // SAFETY: `counters` is a valid, correctly sized PROCESS_MEMORY_COUNTERS and the
    // pseudo-handle from GetCurrentProcess is always valid for query.
    let ok = unsafe {
        K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            core::mem::size_of::<ProcessMemoryCounters>() as u32,
        )
    };
    if ok == 0 {
        return Err(budget_error(
            "process_working_set",
            "K32GetProcessMemoryInfo failed",
        ));
    }
    Ok(counters.working_set_size as u64)
}

#[cfg(not(windows))]
pub fn process_working_set_bytes() -> Result<u64, EngineError> {
    Err(budget_error(
        "process_working_set",
        "working-set measurement is only implemented on Windows (the editor min-spec target)",
    ))
}

fn degrade_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor_min_spec.degrade.{field}"),
        reason: reason.into(),
    }
}

fn budget_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor_min_spec.budget.{field}"),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "editor_min_spec_tests.rs"]
mod editor_min_spec_tests;
