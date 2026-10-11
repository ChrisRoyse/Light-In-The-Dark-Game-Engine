//! FSV harness for #1741: FSR 3.1-style temporal upscaler as the default
//! TAA + upscale path (720p -> 1080p).
//!
//! Verdict artifact: target/fsv/issue1741_fsr_upscale/manual/issue1741-fsr-upscale-fsv.json
//! Read it (and its SHA256, and the adapter report) back independently to verify.
//!
//! Proves, from a real GPU readback:
//!  - happy path: a deterministic synthetic scene rendered at 720p, upscaled to
//!    1080p over two frames (temporal accumulation), with resolution + format
//!    asserts, non-degenerate output stats, a 30 dB PSNR floor against a native
//!    1080p reference render, AND a discriminating negative control (a deliberately
//!    broken reconstruction of the same reference must score far below the floor,
//!    proving the PSNR gate has teeth).
//!  - three edges: 1x native (no upscale), max upscale ratio (3.0x ultra
//!    performance), and resize mid-run (stale history rejected fail-loud, then a
//!    reset succeeds at the new resolution).

use std::{env, fs, path::PathBuf, time::Instant};

use litd_engine::gpu::GpuContext;
use litd_engine::{
    EngineError,
    diagnostics::ErrorReport,
    render::upscale::{
        UpscaleGpuInput, UpscaleImage, UpscaleQualityMode, UpscaleSettings,
        render_upscale_gpu_with_context,
    },
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

/// Reconstruction-quality floor in dB. The measured PSNR of the two-frame temporal
/// reconstruction vs the native 1080p reference on this scene is ~37 dB; FSR-class
/// quality-mode upscalers on clean/synthetic content land in the low-to-mid 30s, so
/// 30 dB is a meaningful bar (~7 dB headroom, yet fails real fidelity loss). The old
/// 20 dB floor was near-tautological. `psnr_negative_control` proves this floor has
/// teeth: a deliberately broken reconstruction scores far under it.
const PSNR_FLOOR_DB: f32 = 30.0;

fn main() -> Result<(), EngineError> {
    litd_engine::init_logging();
    pollster::block_on(run())
}

async fn run() -> Result<(), EngineError> {
    let root = fsv_root()?;
    fs::create_dir_all(&root).map_err(|e| fsv_error("root", e))?;
    let report_path = root.join("issue1741-fsr-upscale-fsv.json");

    let context = GpuContext::new_headless().await?;

    let happy = happy_path(&context).await?;
    let edges = json!({
        "native_1x": edge_native(&context).await?,
        "max_upscale_ratio": edge_max_ratio(&context).await?,
        "resize_mid_run": edge_resize_mid_run(&context).await?,
    });

    let report = json!({
        "issue": 1741,
        "source_of_truth": {
            "report_path": report_path.display().to_string(),
            "source_module": "crates/litd-engine/src/render/upscale.rs",
            "wgsl_module": "crates/litd-engine/src/render/upscale.wgsl",
            "manual_fsv_readback": "read this persisted JSON, its SHA256, and the target GPU adapter report through Synapse/computer control"
        },
        "wgpu_ffx_compatibility_finding": {
            "crate": "cwfitzgerald/wgpu-ffx",
            "license": "MIT",
            "technique": "FSR 3 upscaler, compute-only, no tensor cores",
            "targets_wgpu": 29,
            "workspace_wgpu": 27,
            "drop_in_compatible": false,
            "resolution": "Ported the FSR 3.1 passes to wgpu-27-native WGSL in-repo (upscale.wgsl) rather than forcing a workspace-wide wgpu 27->29 bump. Permitted by the issue: 'vendor/port under its permissive license if needed'."
        },
        "sufficiency_argument": "The happy path renders a deterministic edge-and-gradient scene at 720p and reconstructs 1080p across two frames (first-frame reset then history accumulation), asserting output resolution/format, non-degenerate luma statistics, and a PSNR floor vs a native 1080p reference. The three edges cover the 1x native (no-upscale) path, the maximum 3.0x ultra-performance ratio, and a mid-run output resize where stale history is rejected fail-loud and a reset re-establishes correct output at the new resolution.",
        "happy_path": happy,
        "edges": edges,
        "expected_actuals": {
            "happy_output": [1920, 1080],
            "happy_render": [1280, 720],
            "quality_mode": "quality",
            "scale_factor": 1.5,
            "psnr_floor_db": PSNR_FLOOR_DB,
            "psnr_negative_control_below_floor": true,
            "native_pixel_ratio": 1.0,
            "max_render": [640, 360],
            "resize_stale_history_code": "LITD-RENDER-UPSCALE-INVALID-CONFIG"
        }
    });

    let bytes =
        serde_json::to_vec_pretty(&report).map_err(|e| EngineError::SerializationFailed {
            format: "issue1741-fsr-upscale-json",
            reason: e.to_string(),
        })?;
    fs::write(&report_path, &bytes).map_err(|e| fsv_error("write", e))?;
    let persisted = fs::read(&report_path).map_err(|e| fsv_error("read", e))?;
    if persisted != bytes {
        return Err(EngineError::SerializedFingerprintMismatch {
            expected: format!("{} bytes", bytes.len()),
            actual: format!("{} bytes", persisted.len()),
        });
    }
    let parsed: Value =
        serde_json::from_slice(&persisted).map_err(|e| EngineError::SerializationFailed {
            format: "issue1741-fsr-upscale-readback-json",
            reason: e.to_string(),
        })?;
    assert_report(&parsed)?;
    println!(
        "issue1741 fsr_upscale_fsv report={} bytes={} sha256={} adapter={} psnr_db={} out={}x{} render={}x{} non_degenerate={}",
        report_path.display(),
        persisted.len(),
        sha256_hex(&persisted),
        parsed["happy_path"]["adapter"]["name"],
        parsed["happy_path"]["frame1"]["psnr_db"],
        parsed["happy_path"]["frame1"]["output_width"],
        parsed["happy_path"]["frame1"]["output_height"],
        parsed["happy_path"]["frame1"]["render_width"],
        parsed["happy_path"]["frame1"]["render_height"],
        parsed["happy_path"]["frame1"]["non_degenerate"],
    );
    Ok(())
}

async fn happy_path(context: &GpuContext) -> Result<Value, EngineError> {
    let settings = UpscaleSettings::default(); // enabled, Quality (1.5x), default knobs
    let out_w = 1920;
    let out_h = 1080;
    let (render_w, render_h) =
        litd_engine::render::upscale::render_dimensions(UpscaleQualityMode::Quality, out_w, out_h)?;
    let reference = scene_image(out_w, out_h);

    // Frame 0: first frame, no history.
    let input0 = UpscaleGpuInput {
        render_width: render_w,
        render_height: render_h,
        output_width: out_w,
        output_height: out_h,
        color: scene_color(render_w, render_h),
        motion: vec![[0.0, 0.0]; (render_w * render_h) as usize],
        history: Vec::new(),
        first_frame: true,
    };
    let started0 = Instant::now();
    let frame0 = render_upscale_gpu_with_context(context, settings, &input0).await?;
    let frame0_ns = started0.elapsed().as_nanos();

    // Frame 1: static camera (zero motion), accumulate frame 0's output as history.
    let input1 = UpscaleGpuInput {
        render_width: render_w,
        render_height: render_h,
        output_width: out_w,
        output_height: out_h,
        color: scene_color(render_w, render_h),
        motion: vec![[0.0, 0.0]; (render_w * render_h) as usize],
        history: frame0.output_image.pixels.clone(),
        first_frame: false,
    };
    let started1 = Instant::now();
    let frame1 = render_upscale_gpu_with_context(context, settings, &input1).await?;
    let frame1_ns = started1.elapsed().as_nanos();

    let psnr = litd_engine::render::upscale::psnr_for_readback(&frame1.output_image, &reference)
        .ok_or_else(|| fsv_error("psnr", "reference dimensions mismatch"))?;

    // Discriminating negative control: a deliberately broken reconstruction of the
    // SAME reference must FAIL the floor, proving the PSNR gate has teeth (it is not
    // near-tautological). The broken output applies a uniform per-channel tone
    // offset -- a real class of upscaler bug (wrong color/exposure reconstruction)
    // that no correct upscaler introduces.
    let broken = broken_reconstruction(&reference);
    let broken_psnr = litd_engine::render::upscale::psnr_for_readback(&broken, &reference)
        .ok_or_else(|| fsv_error("psnr", "broken-control dimensions mismatch"))?;
    if broken_psnr >= PSNR_FLOOR_DB {
        return Err(fsv_error(
            "psnr_negative_control",
            format!(
                "broken reconstruction scored {broken_psnr} dB, at/above the {PSNR_FLOOR_DB} dB \
                 floor: the floor does not discriminate a broken upscale (no teeth)"
            ),
        ));
    }

    Ok(json!({
        "adapter": context.adapter_report(),
        "context_id": context.context_id(),
        "wgsl_sha256": sha256_hex(litd_engine::render::upscale::UPSCALE_WGSL.as_bytes()),
        "wgsl_has_reproject": litd_engine::render::upscale::UPSCALE_WGSL.contains("prev_uv"),
        "wgsl_has_rcas": litd_engine::render::upscale::UPSCALE_WGSL.contains("fn rcas"),
        "reference_checksum": reference.checksum(),
        "frame0": frame_json(&frame0.stats, frame0_ns, None),
        "frame1": frame_json(&frame1.stats, frame1_ns, Some(psnr)),
        "psnr_negative_control": {
            "description": "broken reconstruction (uniform +40/255 per-channel tone offset) of the same reference",
            "broken_checksum": broken.checksum(),
            "psnr_db": broken_psnr,
            "floor_db": PSNR_FLOOR_DB,
            "below_floor": broken_psnr < PSNR_FLOOR_DB,
            "real_minus_broken_db": psnr - broken_psnr,
        },
    }))
}

/// A deliberately broken reconstruction of `reference`: every channel is shifted by
/// a constant +40/255 (clamped). A correct upscaler preserves tone; this does not,
/// so it must score far below the PSNR floor. Used as the discriminating negative
/// control that proves the floor is not tautological.
fn broken_reconstruction(reference: &UpscaleImage) -> UpscaleImage {
    let pixels = reference
        .pixels
        .iter()
        .map(|p| {
            [
                p[0].saturating_add(40),
                p[1].saturating_add(40),
                p[2].saturating_add(40),
                p[3],
            ]
        })
        .collect();
    UpscaleImage::from_pixels(reference.width, reference.height, pixels)
}

async fn edge_native(context: &GpuContext) -> Result<Value, EngineError> {
    let settings = UpscaleSettings {
        mode: UpscaleQualityMode::Native,
        ..UpscaleSettings::default()
    };
    let (w, h) = (1280, 720);
    let input = UpscaleGpuInput {
        render_width: w,
        render_height: h,
        output_width: w,
        output_height: h,
        color: scene_color(w, h),
        motion: vec![[0.0, 0.0]; (w * h) as usize],
        history: Vec::new(),
        first_frame: true,
    };
    let frame = render_upscale_gpu_with_context(context, settings, &input).await?;
    Ok(json!({
        "name": "native_1x",
        "before": { "mode": "native", "output": [w, h] },
        "after": {
            "render": [frame.stats.render_width, frame.stats.render_height],
            "output": [frame.stats.output_width, frame.stats.output_height],
            "is_native": frame.stats.is_native,
            "pixel_ratio": frame.stats.pixel_ratio,
            "non_degenerate": frame.stats.output_metrics.is_non_degenerate(),
        }
    }))
}

async fn edge_max_ratio(context: &GpuContext) -> Result<Value, EngineError> {
    let settings = UpscaleSettings {
        mode: UpscaleQualityMode::UltraPerformance,
        ..UpscaleSettings::default()
    };
    let (out_w, out_h) = (1920, 1080);
    let (render_w, render_h) = litd_engine::render::upscale::render_dimensions(
        UpscaleQualityMode::UltraPerformance,
        out_w,
        out_h,
    )?;
    let input = UpscaleGpuInput {
        render_width: render_w,
        render_height: render_h,
        output_width: out_w,
        output_height: out_h,
        color: scene_color(render_w, render_h),
        motion: vec![[0.0, 0.0]; (render_w * render_h) as usize],
        history: Vec::new(),
        first_frame: true,
    };
    let frame = render_upscale_gpu_with_context(context, settings, &input).await?;
    Ok(json!({
        "name": "max_upscale_ratio",
        "before": { "mode": "ultra_performance", "output": [out_w, out_h] },
        "after": {
            "render": [frame.stats.render_width, frame.stats.render_height],
            "output": [frame.stats.output_width, frame.stats.output_height],
            "scale_factor": frame.stats.scale_factor,
            "pixel_ratio": frame.stats.pixel_ratio,
            "non_degenerate": frame.stats.output_metrics.is_non_degenerate(),
        }
    }))
}

async fn edge_resize_mid_run(context: &GpuContext) -> Result<Value, EngineError> {
    let settings = UpscaleSettings::default();
    // Establish a frame at 1920x1080, then resize output to 2560x1440.
    let (r0w, r0h) =
        litd_engine::render::upscale::render_dimensions(UpscaleQualityMode::Quality, 1920, 1080)?;
    let first = UpscaleGpuInput {
        render_width: r0w,
        render_height: r0h,
        output_width: 1920,
        output_height: 1080,
        color: scene_color(r0w, r0h),
        motion: vec![[0.0, 0.0]; (r0w * r0h) as usize],
        history: Vec::new(),
        first_frame: true,
    };
    let frame_a = render_upscale_gpu_with_context(context, settings, &first).await?;

    let (r1w, r1h) =
        litd_engine::render::upscale::render_dimensions(UpscaleQualityMode::Quality, 2560, 1440)?;
    // Reusing stale 1920x1080 history at the new 2560x1440 output must fail loud.
    let stale = UpscaleGpuInput {
        render_width: r1w,
        render_height: r1h,
        output_width: 2560,
        output_height: 1440,
        color: scene_color(r1w, r1h),
        motion: vec![[0.0, 0.0]; (r1w * r1h) as usize],
        history: frame_a.output_image.pixels.clone(), // wrong size on purpose
        first_frame: false,
    };
    let stale_result = render_upscale_gpu_with_context(context, settings, &stale).await;
    let stale_json = match stale_result {
        Ok(_) => json!({ "status": "accepted", "matched_expected_code": false }),
        Err(err) => {
            let report: ErrorReport = err.report();
            json!({
                "status": "rejected",
                "code": report.code,
                "message": report.message,
                "matched_expected_code": report.code == "LITD-RENDER-UPSCALE-INVALID-CONFIG",
            })
        }
    };

    // The correct resize response: reset (first_frame) at the new resolution.
    let reset = UpscaleGpuInput {
        render_width: r1w,
        render_height: r1h,
        output_width: 2560,
        output_height: 1440,
        color: scene_color(r1w, r1h),
        motion: vec![[0.0, 0.0]; (r1w * r1h) as usize],
        history: Vec::new(),
        first_frame: true,
    };
    let frame_b = render_upscale_gpu_with_context(context, settings, &reset).await?;

    Ok(json!({
        "name": "resize_mid_run",
        "before": {
            "initial_output": [1920, 1080],
            "resized_output": [2560, 1440],
        },
        "after": {
            "initial_render": [frame_a.stats.render_width, frame_a.stats.render_height],
            "stale_history_reuse": stale_json,
            "reset_render": [frame_b.stats.render_width, frame_b.stats.render_height],
            "reset_output": [frame_b.stats.output_width, frame_b.stats.output_height],
            "reset_non_degenerate": frame_b.stats.output_metrics.is_non_degenerate(),
        }
    }))
}

fn frame_json(
    stats: &litd_engine::render::upscale::UpscaleGpuStats,
    wall_ns: u128,
    psnr: Option<f32>,
) -> Value {
    json!({
        "render_width": stats.render_width,
        "render_height": stats.render_height,
        "output_width": stats.output_width,
        "output_height": stats.output_height,
        "output_texture_format": stats.output_texture_format,
        "quality_mode": stats.quality_mode,
        "scale_factor": stats.scale_factor,
        "pixel_ratio": stats.pixel_ratio,
        "first_frame": stats.first_frame,
        "pass_labels": stats.pass_labels,
        "output_checksum": stats.output_checksum,
        "mean_luma": stats.output_metrics.mean_luma,
        "luma_variance": stats.output_metrics.luma_variance,
        "distinct_luma_buckets": stats.output_metrics.distinct_luma_buckets,
        "non_degenerate": stats.output_metrics.is_non_degenerate(),
        "psnr_db": psnr,
        "psnr_pass": psnr.map(|v| v >= PSNR_FLOOR_DB),
        "wall_clock_ns": wall_ns,
        "wall_clock_note": "diagnostic only; real per-frame GPU timing requires a PIX/Synapse runtime capture on min-spec hardware",
    })
}

fn assert_report(report: &Value) -> Result<(), EngineError> {
    let happy = &report["happy_path"];
    let f1 = &happy["frame1"];
    let neg = &happy["psnr_negative_control"];
    let native = &report["edges"]["native_1x"]["after"];
    let max_ratio = &report["edges"]["max_upscale_ratio"]["after"];
    let resize = &report["edges"]["resize_mid_run"]["after"];
    let ok = f1["output_width"] == 1920
        && f1["output_height"] == 1080
        && f1["render_width"] == 1280
        && f1["render_height"] == 720
        && f1["output_texture_format"] == "Rgba8Unorm"
        && f1["non_degenerate"] == true
        && f1["psnr_pass"] == true
        && neg["below_floor"] == true
        && neg["psnr_db"].as_f64().unwrap_or(f64::MAX) < f64::from(PSNR_FLOOR_DB)
        && happy["frame0"]["first_frame"] == true
        && native["is_native"] == true
        && (native["pixel_ratio"].as_f64().unwrap_or(0.0) - 1.0).abs() < 1.0e-3
        && native["non_degenerate"] == true
        && max_ratio["render"] == json!([640, 360])
        && max_ratio["non_degenerate"] == true
        && resize["stale_history_reuse"]["matched_expected_code"] == true
        && resize["reset_output"] == json!([2560, 1440])
        && resize["reset_non_degenerate"] == true;
    if ok {
        Ok(())
    } else {
        Err(EngineError::SerializedFingerprintMismatch {
            expected: "issue1741 fsr upscale report with happy path (1080p, PSNR floor, non-degenerate) and three matched edges".to_string(),
            actual: report.to_string(),
        })
    }
}

/// Deterministic synthetic scene: smooth color gradients plus a hard-edged circle
/// and a diagonal band, so downsample -> upscale is a meaningful reconstruction
/// test (edges for RCAS, gradients for temporal accumulation).
fn scene_pixel(u: f32, v: f32) -> [f32; 4] {
    let base_r = 0.15 + 0.7 * u;
    let base_g = 0.20 + 0.6 * v;
    let base_b = 0.30 + 0.5 * (1.0 - u) * (1.0 - v);
    let dx = u - 0.5;
    let dy = v - 0.5;
    let circle = if dx * dx + dy * dy < 0.09 { 0.35 } else { 0.0 };
    let band = if ((u + v) * 8.0).fract() < 0.5 {
        0.10
    } else {
        0.0
    };
    [
        (base_r + circle + band).clamp(0.0, 1.0),
        (base_g + band).clamp(0.0, 1.0),
        (base_b + circle).clamp(0.0, 1.0),
        1.0,
    ]
}

fn scene_color(width: u32, height: u32) -> Vec<[f32; 4]> {
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let u = (x as f32 + 0.5) / width as f32;
            let v = (y as f32 + 0.5) / height as f32;
            pixels.push(scene_pixel(u, v));
        }
    }
    pixels
}

fn scene_image(width: u32, height: u32) -> UpscaleImage {
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let u = (x as f32 + 0.5) / width as f32;
            let v = (y as f32 + 0.5) / height as f32;
            let p = scene_pixel(u, v);
            pixels.push([
                (p[0] * 255.0).round() as u8,
                (p[1] * 255.0).round() as u8,
                (p[2] * 255.0).round() as u8,
                255,
            ]);
        }
    }
    UpscaleImage::from_pixels(width, height, pixels)
}

fn fsv_root() -> Result<PathBuf, EngineError> {
    match env::var("LITD_UPSCALE_FSV_ROOT") {
        Ok(value) if value.trim().is_empty() => {
            Err(fsv_error("root", "LITD_UPSCALE_FSV_ROOT must not be empty"))
        }
        Ok(value) => Ok(PathBuf::from(value)),
        Err(_) => Ok(PathBuf::from("target")
            .join("fsv")
            .join("issue1741_fsr_upscale")
            .join("manual")),
    }
}

fn fsv_error(field: &'static str, error: impl std::fmt::Display) -> EngineError {
    EngineError::UpscaleInvalidConfig {
        field: format!("render.upscale.fsv.{field}"),
        reason: error.to_string(),
    }
}
