//! Workstation-first graphics quality scale and supporting #1148 evidence.

use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    EngineError,
    fsv_artifacts::{FsvReportReadback, begin_issue_run},
    render::{
        FrameBudget,
        reference::{ReferenceSceneSettings, render_reference_scene_gpu},
        upscale::UpscaleSettings,
    },
};
#[path = "performance_quality_upscale.rs"]
mod performance_quality_upscale;

pub const ISSUE1148_PERFORMANCE_SCHEMA: &str = "litd.engine.issue1148.performance_quality.v1";
pub const ISSUE1148_PERFORMANCE_SLUG: &str = "issue1148_performance_quality";
pub const ULTRA_MAX_DRAW_CALLS: u32 = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum GraphicsQualityTier {
    Low,
    Medium,
    High,
    #[default]
    Ultra,
}

impl GraphicsQualityTier {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }

    pub const fn ordered() -> [Self; 4] {
        [Self::Low, Self::Medium, Self::High, Self::Ultra]
    }

    pub fn settings(self) -> GraphicsQualitySettings {
        let (
            target_width,
            target_height,
            render_scale_percent,
            target_fps,
            max_draw_calls_per_frame,
            max_visible_instances,
            shadow_map_size,
            max_texture_mebibytes_per_scene,
            vfx_particle_budget,
            bloom_passes,
        ) = match self {
            Self::Low => (1920, 1080, 50, 60, 512, 5_000, 1024, 512, 8_192, 0),
            Self::Medium => (1920, 1080, 85, 60, 1_024, 10_000, 2048, 1_024, 16_384, 1),
            Self::High => (2560, 1440, 100, 90, 2_048, 25_000, 4096, 2_048, 65_536, 2),
            Self::Ultra => (
                3840,
                2160,
                100,
                120,
                ULTRA_MAX_DRAW_CALLS,
                50_000,
                8192,
                8_192,
                262_144,
                3,
            ),
        };
        settings(SettingsArgs {
            tier: self,
            target_width,
            target_height,
            render_scale_percent,
            target_fps,
            max_draw_calls_per_frame,
            max_visible_instances,
            shadow_map_size,
            max_texture_mebibytes_per_scene,
            vfx_particle_budget,
            bloom_passes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkstationPerformanceTarget {
    pub source_issue: u32,
    pub cpu: String,
    pub gpu: String,
    pub gpu_vram_gb: u32,
    pub ram_gb: u32,
    pub os: String,
    pub default_tier: GraphicsQualityTier,
    pub policy: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GraphicsQualitySettings {
    pub tier: GraphicsQualityTier,
    pub target_width: u32,
    pub target_height: u32,
    pub render_scale_percent: u32,
    pub target_fps: u32,
    pub target_frame_ns: u64,
    pub max_draw_calls_per_frame: u32,
    pub max_visible_instances: u32,
    pub shadow_map_size: u32,
    pub max_texture_mebibytes_per_scene: u32,
    pub vfx_particle_budget: u32,
    pub bloom_passes: u32,
    pub hzb_enabled: bool,
    pub post_process_enabled: bool,
    /// FSR temporal upscale config (#1741, default ON per #928); the runtime reads
    /// it via `internal_render_dimensions` (performance_quality_upscale.rs).
    pub upscale: UpscaleSettings,
}

impl GraphicsQualitySettings {
    pub fn validate(self) -> Result<(), EngineError> {
        if self.target_width == 0 || self.target_height == 0 {
            return invalid_quality("target_resolution", "width and height must be non-zero");
        }
        if !(25..=100).contains(&self.render_scale_percent) {
            return invalid_quality("render_scale_percent", "must be in 25..=100");
        }
        if self.target_fps == 0 || self.target_frame_ns == 0 {
            return invalid_quality("target_fps", "target FPS and frame time must be positive");
        }
        if self.max_draw_calls_per_frame == 0
            || self.max_visible_instances == 0
            || self.shadow_map_size == 0
            || self.max_texture_mebibytes_per_scene == 0
            || self.vfx_particle_budget == 0
        {
            return invalid_quality("budget", "quality budgets must be positive");
        }
        self.upscale.validate()?;
        Ok(())
    }

    pub const fn target_pixels(self) -> u64 {
        self.target_width as u64 * self.target_height as u64
    }

    pub const fn frame_budget(self) -> FrameBudget {
        FrameBudget {
            max_draw_calls: self.max_draw_calls_per_frame,
            // #1532: use the SAME real per-frame allocation ceiling as `FrameBudget::default()`
            // (measured MAX_FRAME_ALLOCATIONS) instead of the old hardcoded `0`. While allocations
            // were hardcoded `0` at every frame site, `0 <= 0` was a decorative false-green; once
            // #1532 fed the REAL per-frame allocation delta into `FrameStats.allocations`, a `0`
            // ceiling here made `budget_ok` FALSE on EVERY `--quality` game frame (measured ~163
            // allocs/frame at quality low >> 0) — a false-RED regression on the shipping min-spec
            // path. The default budget and every quality tier now share one real, failable ceiling.
            // Per-tier allocation budgets can follow if/when real min-spec measurements justify
            // them (#1470); until then a single measured ceiling is the honest, non-fabricated value.
            max_allocations: crate::render::MAX_FRAME_ALLOCATIONS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityScaleValidation {
    pub tier_count: usize,
    pub default_tier: GraphicsQualityTier,
    pub ultra_is_default: bool,
    pub target_pixels_monotonic: bool,
    pub draw_budget_monotonic: bool,
    pub vfx_budget_monotonic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityBudgetReadback {
    pub tier: GraphicsQualityTier,
    pub observed_draw_calls: u32,
    pub max_draw_calls_per_frame: u32,
    pub draw_calls_within_budget: bool,
}

pub fn workstation_performance_target() -> WorkstationPerformanceTarget {
    WorkstationPerformanceTarget {
        source_issue: 928,
        cpu: "AMD Ryzen 9 9950X3D, 16C/32T, Zen 5, 128 MB 3D V-Cache".to_string(),
        gpu: "NVIDIA RTX 5090, Blackwell".to_string(),
        gpu_vram_gb: 32,
        ram_gb: 128,
        os: "Windows 11 Pro".to_string(),
        default_tier: GraphicsQualityTier::default(),
        policy: "workstation-first; ultra default; lower tiers preserve stylized-PBR readability"
            .to_string(),
    }
}

pub fn default_quality_scale() -> Vec<GraphicsQualitySettings> {
    GraphicsQualityTier::ordered()
        .into_iter()
        .map(GraphicsQualityTier::settings)
        .collect()
}

pub fn validate_quality_scale(
    scale: &[GraphicsQualitySettings],
) -> Result<QualityScaleValidation, EngineError> {
    if scale.len() != GraphicsQualityTier::ordered().len() {
        return invalid_quality(
            "scale",
            "must contain exactly low, medium, high, and ultra tiers",
        );
    }
    let mut ordered = Vec::with_capacity(scale.len());
    for tier in GraphicsQualityTier::ordered() {
        let Some(settings) = scale.iter().find(|settings| settings.tier == tier) else {
            return invalid_quality("scale.tier", format!("missing {} tier", tier.as_str()));
        };
        settings.validate()?;
        ordered.push(*settings);
    }
    for pair in ordered.windows(2) {
        let [lower, higher] = pair else {
            continue;
        };
        if higher.target_pixels() < lower.target_pixels()
            || higher.max_draw_calls_per_frame < lower.max_draw_calls_per_frame
            || higher.vfx_particle_budget < lower.vfx_particle_budget
        {
            return invalid_quality(
                "scale.monotonic",
                "higher quality tiers must not reduce pixels, draw budget, or VFX budget",
            );
        }
    }
    Ok(QualityScaleValidation {
        tier_count: scale.len(),
        default_tier: GraphicsQualityTier::default(),
        ultra_is_default: GraphicsQualityTier::default() == GraphicsQualityTier::Ultra,
        target_pixels_monotonic: true,
        draw_budget_monotonic: true,
        vfx_budget_monotonic: true,
    })
}

pub fn evaluate_draw_budget(
    settings: GraphicsQualitySettings,
    observed_draw_calls: u32,
) -> Result<QualityBudgetReadback, EngineError> {
    settings.validate()?;
    if observed_draw_calls > settings.max_draw_calls_per_frame {
        return Err(EngineError::BatchingDrawLimitExceeded {
            draw_count: observed_draw_calls as usize,
            max_draws: settings.max_draw_calls_per_frame as usize,
        });
    }
    Ok(QualityBudgetReadback {
        tier: settings.tier,
        observed_draw_calls,
        max_draw_calls_per_frame: settings.max_draw_calls_per_frame,
        draw_calls_within_budget: true,
    })
}

pub async fn run_issue1148_performance_quality_harness() -> Result<FsvReportReadback, EngineError> {
    let run = begin_issue_run(ISSUE1148_PERFORMANCE_SLUG)
        .map_err(|error| harness_error("fsv.begin_issue_run", error))?;
    let bmp_path = run
        .artifact_path("ultra-reference-4k.bmp")
        .map_err(|error| harness_error("fsv.bmp_path", error))?;
    let target = workstation_performance_target();
    let scale = default_quality_scale();
    let validation = validate_quality_scale(&scale)?;
    let ultra = GraphicsQualityTier::Ultra.settings();
    let render = render_ultra_probe(ultra, &bmp_path).await?;
    let edges = edge_proofs()?;
    let report = json!({
        "schema": ISSUE1148_PERFORMANCE_SCHEMA,
        "schema_version": 1,
        "issue": 1148,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "render_bmp_path": bmp_path.display().to_string(),
            "manual_gate": "supporting automated evidence only; manual Synapse/HWND/GPU trace FSV is still required"
        },
        "research_basis": [
            "PIX timing captures gather CPU/GPU timing from the running game",
            "Nsight GPU Trace captures frame boundaries and GPU work over selected frames",
            "GPUOpen performance guidance favors explicit budgets and scalable quality knobs"
        ],
        "reference_images_reviewed": ["docs/readme/ED11-world-view.png"],
        "happy_path": {
            "target": target,
            "quality_scale": scale,
            "validation": validation,
            "ultra_render_probe": render,
        },
        "edges": edges,
    });
    run.write_report(&report)
        .map_err(|error| harness_error("fsv.write_report", error))
}

async fn render_ultra_probe(
    settings: GraphicsQualitySettings,
    bmp_path: &Path,
) -> Result<Value, EngineError> {
    let before = json!({
        "tier": settings.tier,
        "target_width": settings.target_width,
        "target_height": settings.target_height,
        "target_fps": settings.target_fps,
        "target_frame_ns": settings.target_frame_ns,
        "max_draw_calls_per_frame": settings.max_draw_calls_per_frame,
        "timing_gate": "wall-clock observed only; real close gate requires running-frame GPU trace through Synapse",
    });
    let started = Instant::now();
    let frame = render_reference_scene_gpu(ReferenceSceneSettings {
        width: settings.target_width,
        height: settings.target_height,
    })
    .await?;
    let wall_clock = started.elapsed();
    let budget = evaluate_draw_budget(settings, frame.draw_calls)?;
    frame.image.write_bmp(bmp_path)?;
    Ok(json!({
        "before": before,
        "after": {
            "gpu_context_id": frame.gpu_context_id,
            "adapter_name": frame.adapter_name,
            "backend": frame.backend,
            "texture_format": frame.texture_format,
            "mesh_count": frame.mesh_count,
            "instance_count": frame.instance_count,
            "draw_calls": frame.draw_calls,
            "draw_budget": budget,
            "readback_bytes": frame.readback_bytes,
            "row_pitch": frame.row_pitch,
            "wall_clock_ns": duration_ns(wall_clock),
            "comparison_pass": frame.comparison.pass,
            "comparison_checksum": frame.comparison.checksum,
            "bmp_readback": file_readback(bmp_path)?,
        }
    }))
}

fn edge_proofs() -> Result<Value, EngineError> {
    Ok(json!({
        "invalid_zero_fps": invalid_zero_fps_edge(),
        "missing_tier": missing_tier_edge(),
        "non_monotonic_budget": non_monotonic_budget_edge(),
        "over_draw_budget": over_draw_budget_edge()?,
    }))
}

fn invalid_zero_fps_edge() -> Value {
    let mut settings = GraphicsQualityTier::Ultra.settings();
    settings.target_fps = 0;
    settings.target_frame_ns = 0;
    let error = settings.validate().unwrap_err();
    json!({ "before": settings, "after": engine_error_report(&error) })
}

fn missing_tier_edge() -> Value {
    let scale = vec![
        GraphicsQualityTier::Medium.settings(),
        GraphicsQualityTier::High.settings(),
        GraphicsQualityTier::Ultra.settings(),
    ];
    let error = validate_quality_scale(&scale).unwrap_err();
    json!({ "before": { "tier_count": scale.len() }, "after": engine_error_report(&error) })
}

fn non_monotonic_budget_edge() -> Value {
    let mut scale = default_quality_scale();
    if let Some(high) = scale
        .iter_mut()
        .find(|settings| settings.tier == GraphicsQualityTier::High)
    {
        high.max_draw_calls_per_frame = 8_192;
    }
    let error = validate_quality_scale(&scale).unwrap_err();
    json!({ "before": scale, "after": engine_error_report(&error) })
}

fn over_draw_budget_edge() -> Result<Value, EngineError> {
    let settings = GraphicsQualityTier::Low.settings();
    let observed = settings.max_draw_calls_per_frame + 1;
    let error = evaluate_draw_budget(settings, observed).unwrap_err();
    Ok(json!({
        "before": {
            "tier": settings.tier,
            "observed_draw_calls": observed,
            "max_draw_calls_per_frame": settings.max_draw_calls_per_frame,
        },
        "after": engine_error_report(&error)
    }))
}

/// Cohesive per-tier graphics-quality inputs (#1545). Groups the eleven values
/// that `settings()` previously took positionally so each tier's call site reads
/// as named fields and the builder stays under the clippy argument threshold.
struct SettingsArgs {
    tier: GraphicsQualityTier,
    target_width: u32,
    target_height: u32,
    render_scale_percent: u32,
    target_fps: u32,
    max_draw_calls_per_frame: u32,
    max_visible_instances: u32,
    shadow_map_size: u32,
    max_texture_mebibytes_per_scene: u32,
    vfx_particle_budget: u32,
    bloom_passes: u32,
}

fn settings(args: SettingsArgs) -> GraphicsQualitySettings {
    let SettingsArgs {
        tier,
        target_width,
        target_height,
        render_scale_percent,
        target_fps,
        max_draw_calls_per_frame,
        max_visible_instances,
        shadow_map_size,
        max_texture_mebibytes_per_scene,
        vfx_particle_budget,
        bloom_passes,
    } = args;
    GraphicsQualitySettings {
        tier,
        target_width,
        target_height,
        render_scale_percent,
        target_fps,
        target_frame_ns: 1_000_000_000 / u64::from(target_fps),
        max_draw_calls_per_frame,
        max_visible_instances,
        shadow_map_size,
        max_texture_mebibytes_per_scene,
        vfx_particle_budget,
        bloom_passes,
        hzb_enabled: true,
        post_process_enabled: true,
        upscale: UpscaleSettings::default(),
    }
}

fn file_readback(path: &Path) -> Result<Value, EngineError> {
    let bytes = fs::read(path).map_err(|error| harness_error("file.readback", error))?;
    Ok(json!({
        "path": path.display().to_string(),
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
    }))
}

fn duration_ns(duration: Duration) -> u128 {
    duration.as_nanos()
}

fn engine_error_report(error: &EngineError) -> Value {
    let report = error.report();
    json!({
        "error_code": report.code,
        "message": report.message,
        "remediation": report.remediation,
        "source_error": report.source_error,
    })
}

fn invalid_quality<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::GamePackageInvalidConfig {
        field: format!("performance_quality.{}", field.into()),
        reason: reason.into(),
    })
}

fn harness_error(op: &'static str, error: impl std::fmt::Display) -> EngineError {
    EngineError::SerializationFailed {
        format: "issue1148-performance-quality",
        reason: format!("{op}: {error}"),
    }
}
