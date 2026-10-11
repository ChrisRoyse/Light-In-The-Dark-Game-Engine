//! Produced-game player min-spec quality target and supporting #1153 evidence.

use std::{fs, path::Path, time::Instant};

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    EngineError,
    fsv_artifacts::{FsvReportReadback, begin_issue_run},
    performance_quality::{GraphicsQualitySettings, GraphicsQualityTier, evaluate_draw_budget},
    render::reference::{
        ReferenceSceneComparison, ReferenceSceneSettings, build_reference_scene_plan,
        compare_reference_scene_image, render_reference_scene_gpu, render_reference_scene_image,
    },
};

pub const ISSUE1153_MIN_SPEC_SCHEMA: &str = "litd.engine.issue1153.player_min_spec.v1";
pub const ISSUE1153_MIN_SPEC_SLUG: &str = "issue1153_player_min_spec";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerMinSpecTarget {
    pub source_issue: u32,
    pub price_band_usd: String,
    pub cpu_class: String,
    pub gpu_floor: String,
    pub gpu_alternate: String,
    pub gpu_vram_floor_gb: u32,
    pub ram_gb: u32,
    pub os: String,
    pub output_width: u32,
    pub output_height: u32,
    pub target_fps: u32,
    pub target_frame_ns: u64,
}

impl PlayerMinSpecTarget {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.output_width != 1920 || self.output_height != 1080 {
            return invalid_min_spec(
                "output_resolution",
                "player min-spec target must be 1920x1080",
            );
        }
        if self.target_fps != 60 || self.target_frame_ns != 16_666_666 {
            return invalid_min_spec(
                "target_fps",
                "player min-spec target must be 60fps/16.666ms",
            );
        }
        if self.gpu_vram_floor_gb < 6 {
            return invalid_min_spec(
                "gpu_vram_floor_gb",
                "GTX 1660 floor requires at least 6GB VRAM",
            );
        }
        if self.ram_gb < 16 {
            return invalid_min_spec(
                "ram_gb",
                "player min-spec target requires at least 16GB RAM",
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinSpecUpscaleContract {
    pub source_issue: u32,
    pub technology: String,
    pub mode: String,
    pub open_source_required: bool,
    pub required_when_render_scale_below_100: bool,
    pub frame_generation_required_for_min_spec: bool,
}

// Embeds `GraphicsQualitySettings`, which carries f32 upscale knobs (#1741) and is
// therefore `PartialEq` but not `Eq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerMinSpecProfile {
    pub target: PlayerMinSpecTarget,
    pub settings: GraphicsQualitySettings,
    pub internal_render_width: u32,
    pub internal_render_height: u32,
    pub output_pixels: u64,
    pub internal_pixels: u64,
    pub internal_pixel_percent: u32,
    pub upscaler: MinSpecUpscaleContract,
}

pub fn player_min_spec_target() -> PlayerMinSpecTarget {
    PlayerMinSpecTarget {
        source_issue: 1153,
        price_band_usd: "$500-$1,000 Windows PC".to_string(),
        cpu_class: "Ryzen 5 class, 6 cores".to_string(),
        gpu_floor: "AMD Radeon RX 6600 class, 8GB GDDR6".to_string(),
        gpu_alternate: "NVIDIA GeForce GTX 1660 class, 6GB GDDR5".to_string(),
        gpu_vram_floor_gb: 6,
        ram_gb: 16,
        os: "Windows 10/11 64-bit".to_string(),
        output_width: 1920,
        output_height: 1080,
        target_fps: 60,
        target_frame_ns: 16_666_666,
    }
}

pub fn low_player_min_spec_profile() -> Result<PlayerMinSpecProfile, EngineError> {
    let target = player_min_spec_target();
    let settings = GraphicsQualityTier::Low.settings();
    let profile = PlayerMinSpecProfile {
        internal_render_width: scaled_dimension(
            settings.target_width,
            settings.render_scale_percent,
        ),
        internal_render_height: scaled_dimension(
            settings.target_height,
            settings.render_scale_percent,
        ),
        output_pixels: settings.target_pixels(),
        internal_pixels: u64::from(scaled_dimension(
            settings.target_width,
            settings.render_scale_percent,
        )) * u64::from(scaled_dimension(
            settings.target_height,
            settings.render_scale_percent,
        )),
        internal_pixel_percent: settings.render_scale_percent * settings.render_scale_percent / 100,
        upscaler: MinSpecUpscaleContract {
            source_issue: 1152,
            technology: "FSR upscale-only or equivalent open-source spatial/temporal upscaler"
                .to_string(),
            mode: "performance_quality_floor".to_string(),
            open_source_required: true,
            required_when_render_scale_below_100: true,
            frame_generation_required_for_min_spec: false,
        },
        target,
        settings,
    };
    validate_player_min_spec_profile(&profile)?;
    Ok(profile)
}

pub fn validate_player_min_spec_profile(profile: &PlayerMinSpecProfile) -> Result<(), EngineError> {
    profile.target.validate()?;
    profile.settings.validate()?;
    if profile.settings.tier != GraphicsQualityTier::Low {
        return invalid_min_spec("tier", "player min-spec profile must use the low tier");
    }
    if profile.settings.target_width != profile.target.output_width
        || profile.settings.target_height != profile.target.output_height
    {
        return invalid_min_spec("output_resolution", "low tier must output 1920x1080");
    }
    if profile.settings.target_fps != profile.target.target_fps {
        return invalid_min_spec("target_fps", "low tier must target 60fps");
    }
    if profile.settings.render_scale_percent < 100
        && (!profile.upscaler.open_source_required
            || !profile.upscaler.required_when_render_scale_below_100
            || profile.upscaler.technology.trim().is_empty())
    {
        return invalid_min_spec(
            "upscaler",
            "sub-native low tier must require an open-source upscaler contract",
        );
    }
    if profile.internal_render_width == 0
        || profile.internal_render_height == 0
        || profile.internal_pixels >= profile.output_pixels
    {
        return invalid_min_spec(
            "internal_resolution",
            "render scale must reduce internal pixels",
        );
    }
    Ok(())
}

pub async fn run_issue1153_player_min_spec_harness() -> Result<FsvReportReadback, EngineError> {
    let run = begin_issue_run(ISSUE1153_MIN_SPEC_SLUG)
        .map_err(|error| harness_error("fsv.begin_issue_run", error))?;
    let bmp_path = run
        .artifact_path("low-1080p-reference.bmp")
        .map_err(|error| harness_error("fsv.bmp_path", error))?;
    let visual_bmp_path = run
        .artifact_path("low-1080p-visual-floor.bmp")
        .map_err(|error| harness_error("fsv.visual_bmp_path", error))?;
    let profile = low_player_min_spec_profile()?;
    let visual = visual_floor_probe(&profile, &visual_bmp_path)?;
    let render = render_low_min_spec_probe(&profile, &bmp_path).await?;
    let edges = edge_proofs()?;
    let report = json!({
        "schema": ISSUE1153_MIN_SPEC_SCHEMA,
        "schema_version": 1,
        "issue": 1153,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "render_bmp_path": bmp_path.display().to_string(),
            "visual_floor_bmp_path": visual_bmp_path.display().to_string(),
            "manual_gate": "supporting automated evidence only; manual Synapse player-runtime FSV on min-spec-class hardware is still required"
        },
        "research_basis": [
            "AMD lists RX 6600 with 8GB GDDR6 and 28 compute units for the min-spec floor",
            "NVIDIA lists GTX 1660 with 6GB memory and 1408 CUDA cores as the alternate floor",
            "GPUOpen FSR guidance prioritizes high-quality upscale-only integration before frame generation",
            "Microsoft PIX GPU captures are the intended close-gate tool for real running-frame GPU work"
        ],
        "reference_images_reviewed": [
            "docs/readme/ED12-play-topdown.png",
            "docs/readme/GP03-combat.png",
            "docs/readme/ED11-world-view.png"
        ],
        "happy_path": {
            "profile": profile,
            "low_visual_floor_probe": visual,
            "low_render_probe": render,
            "sufficiency": "smallest supporting proof: low tier, 1080p output, render-scale/upscaler contract, draw budget, visual comparison, and four fail-closed edge classes"
        },
        "edges": edges,
    });
    run.write_report(&report)
        .map_err(|error| harness_error("fsv.write_report", error))
}

fn visual_floor_probe(
    profile: &PlayerMinSpecProfile,
    bmp_path: &Path,
) -> Result<Value, EngineError> {
    let settings = ReferenceSceneSettings {
        width: profile.internal_render_width,
        height: profile.internal_render_height,
    };
    let plan = build_reference_scene_plan(settings)?;
    let image = render_reference_scene_image(&plan)?;
    let comparison = compare_reference_scene_image(&image);
    if !comparison.pass {
        return Err(EngineError::ReferenceSceneInvalidConfig {
            field: "performance_min_spec.visual_floor".to_string(),
            reason: comparison.band_summary(),
        });
    }
    image.write_bmp(bmp_path)?;
    Ok(json!({
        "before": {
            "width": settings.width,
            "height": settings.height,
            "output_width": profile.target.output_width,
            "output_height": profile.target.output_height,
            "upscale_contract": profile.upscaler,
            "reference_images": plan.reference_image_list(),
        },
        "after": {
            "comparison": comparison_json(&comparison),
            "bmp_readback": file_readback(bmp_path)?,
        }
    }))
}

async fn render_low_min_spec_probe(
    profile: &PlayerMinSpecProfile,
    bmp_path: &Path,
) -> Result<Value, EngineError> {
    let started = Instant::now();
    let frame = render_reference_scene_gpu(ReferenceSceneSettings {
        width: profile.target.output_width,
        height: profile.target.output_height,
    })
    .await?;
    let budget = evaluate_draw_budget(profile.settings, frame.draw_calls)?;
    frame.image.write_bmp(bmp_path)?;
    Ok(json!({
        "before": {
            "tier": profile.settings.tier,
            "output_width": profile.target.output_width,
            "output_height": profile.target.output_height,
            "target_fps": profile.target.target_fps,
            "target_frame_ns": profile.target.target_frame_ns,
            "max_draw_calls_per_frame": profile.settings.max_draw_calls_per_frame,
            "manual_timing_gate": "wall-clock is supporting evidence only; close gate requires real runtime PIX/Synapse readback"
        },
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
            "wall_clock_ns": started.elapsed().as_nanos(),
            "comparison_pass": frame.comparison.pass,
            "comparison_checksum": frame.comparison.checksum,
            "comparison_scope": "diagnostic only for this GPU budget probe; low_visual_floor_probe is the visual metric gate",
            "bmp_readback": file_readback(bmp_path)?,
        }
    }))
}

fn edge_proofs() -> Result<Value, EngineError> {
    Ok(json!({
        "invalid_ram_floor": invalid_ram_floor_edge(),
        "invalid_output_resolution": invalid_output_resolution_edge(),
        "missing_upscaler_for_scaled_low": missing_upscaler_edge(),
        "over_draw_budget": over_draw_budget_edge(),
    }))
}

fn invalid_ram_floor_edge() -> Value {
    let mut target = player_min_spec_target();
    target.ram_gb = 8;
    let error = target.validate().unwrap_err();
    json!({ "before": target, "after": engine_error_report(&error) })
}

fn invalid_output_resolution_edge() -> Value {
    let mut target = player_min_spec_target();
    target.output_width = 1280;
    target.output_height = 720;
    let error = target.validate().unwrap_err();
    json!({ "before": target, "after": engine_error_report(&error) })
}

fn missing_upscaler_edge() -> Value {
    let mut profile = low_player_min_spec_profile().unwrap();
    profile.upscaler.open_source_required = false;
    profile.upscaler.required_when_render_scale_below_100 = false;
    profile.upscaler.technology.clear();
    let error = validate_player_min_spec_profile(&profile).unwrap_err();
    json!({ "before": profile, "after": engine_error_report(&error) })
}

fn over_draw_budget_edge() -> Value {
    let settings = GraphicsQualityTier::Low.settings();
    let observed = settings.max_draw_calls_per_frame + 1;
    let error = evaluate_draw_budget(settings, observed).unwrap_err();
    json!({
        "before": { "observed_draw_calls": observed, "settings": settings },
        "after": engine_error_report(&error)
    })
}

fn scaled_dimension(value: u32, percent: u32) -> u32 {
    (value.saturating_mul(percent).saturating_add(99)) / 100
}

fn file_readback(path: &Path) -> Result<Value, EngineError> {
    let bytes = fs::read(path).map_err(|error| harness_error("file.readback", error))?;
    Ok(json!({
        "path": path.display().to_string(),
        "bytes": bytes.len(),
        "sha256": sha256_hex(&bytes),
    }))
}

fn comparison_json(comparison: &ReferenceSceneComparison) -> Value {
    json!({
        "pass": comparison.pass,
        "checksum": comparison.checksum,
        "metrics": {
            "average_luma": comparison.metrics.average_luma,
            "warm_highlight_ratio": comparison.metrics.warm_highlight_ratio,
            "cool_shadow_ratio": comparison.metrics.cool_shadow_ratio,
            "emissive_ratio": comparison.metrics.emissive_ratio,
            "edge_contrast": comparison.metrics.edge_contrast,
            "hud_gold_ratio": comparison.metrics.hud_gold_ratio,
        },
        "bands": comparison.bands.iter().map(|band| json!({
            "name": band.name,
            "actual": band.actual,
            "min": band.min,
            "max": band.max,
            "pass": band.pass,
        })).collect::<Vec<_>>(),
    })
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

fn invalid_min_spec<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::GamePackageInvalidConfig {
        field: format!("performance_min_spec.{}", field.into()),
        reason: reason.into(),
    })
}

fn harness_error(op: &'static str, error: impl std::fmt::Display) -> EngineError {
    EngineError::SerializationFailed {
        format: "issue1153-player-min-spec",
        reason: format!("{op}: {error}"),
    }
}
