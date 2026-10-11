use std::{fs, path::Path};

use image::{ColorType, ImageFormat};
use litd_engine::{
    EngineError,
    render::reference::{
        ReferenceSceneSettings, build_reference_scene_plan, render_reference_scene_image,
    },
};
use litd_hash::sha256_hex;

use super::{
    HarnessScreenshotDiffRecord, ScreenshotDiffHarnessArgs,
    mcp_harness_tools_validation::invalid_harness,
};

pub(super) fn build_screenshot_diff_record(
    args: &ScreenshotDiffHarnessArgs,
) -> Result<HarnessScreenshotDiffRecord, EngineError> {
    if args.render_reference_scene {
        render_reference_scene_png(args)?;
    }
    let baseline = read_png_rgba("baseline", &args.baseline_path)?;
    let candidate = read_png_rgba("candidate", &args.candidate_path)?;
    let metrics = diff_metrics(&baseline, &candidate)?;
    let verdict = if metrics.changed_bytes <= args.max_changed_bytes
        && metrics.mean_abs_error <= args.max_mean_abs_error
    {
        "passed"
    } else {
        "failed"
    };
    let record = HarnessScreenshotDiffRecord {
        diff_id: args.diff_id.clone(),
        baseline_path: args.baseline_path.clone(),
        candidate_path: args.candidate_path.clone(),
        source_of_truth: "decoded RGBA pixels from PNG screenshot artifacts".to_string(),
        baseline_sha256: sha256_hex(&baseline.bytes),
        candidate_sha256: sha256_hex(&candidate.bytes),
        baseline_bytes: baseline.bytes.len() as u64,
        candidate_bytes: candidate.bytes.len() as u64,
        width: baseline.width,
        height: baseline.height,
        pixel_count: u64::from(baseline.width) * u64::from(baseline.height),
        changed_bytes: metrics.changed_bytes,
        changed_pixels: metrics.changed_pixels,
        max_changed_bytes: args.max_changed_bytes,
        mean_abs_error: metrics.mean_abs_error,
        mean_luma_error: metrics.mean_luma_error,
        rms_error: metrics.rms_error,
        max_channel_delta: metrics.max_channel_delta,
        max_mean_abs_error: args.max_mean_abs_error,
        render_reference_scene: args.render_reference_scene,
        verdict: verdict.to_string(),
    };
    validate_screenshot_diff_record(&record)?;
    Ok(record)
}

pub(super) fn validate_screenshot_diff_record(
    record: &HarnessScreenshotDiffRecord,
) -> Result<(), EngineError> {
    if record.diff_id.trim().is_empty()
        || record.baseline_path.trim().is_empty()
        || record.candidate_path.trim().is_empty()
        || record.source_of_truth.trim().is_empty()
        || record.baseline_sha256.len() != 64
        || record.candidate_sha256.len() != 64
        || record.baseline_bytes == 0
        || record.candidate_bytes == 0
        || record.width == 0
        || record.height == 0
        || record.pixel_count != u64::from(record.width) * u64::from(record.height)
    {
        return invalid_harness(
            "screenshot.record",
            "screenshot diff identity/readback is invalid",
        );
    }
    let max_changed = record.pixel_count.checked_mul(4).ok_or_else(|| {
        invalid_harness::<()>("screenshot.record.size", "pixel count overflow").unwrap_err()
    })?;
    if record.changed_bytes > max_changed || record.changed_pixels > record.pixel_count {
        return invalid_harness("screenshot.record.metrics", "diff counts exceed image size");
    }
    if !record.mean_abs_error.is_finite()
        || !record.mean_luma_error.is_finite()
        || !record.rms_error.is_finite()
        || !record.max_mean_abs_error.is_finite()
        || record.mean_abs_error < 0.0
        || record.mean_luma_error < 0.0
        || record.rms_error < 0.0
        || record.max_mean_abs_error < 0.0
    {
        return invalid_harness("screenshot.record.metrics", "diff metrics are invalid");
    }
    let expected = if record.changed_bytes <= record.max_changed_bytes
        && record.mean_abs_error <= record.max_mean_abs_error
    {
        "passed"
    } else {
        "failed"
    };
    if record.verdict != expected {
        return invalid_harness("screenshot.record.verdict", "screenshot verdict is invalid");
    }
    Ok(())
}

fn render_reference_scene_png(args: &ScreenshotDiffHarnessArgs) -> Result<(), EngineError> {
    let width = match args.render_width {
        Some(width) => width,
        None => return invalid_harness("screenshot.render_width", "render width is required"),
    };
    let height = match args.render_height {
        Some(height) => height,
        None => return invalid_harness("screenshot.render_height", "render height is required"),
    };
    let settings = ReferenceSceneSettings { width, height };
    let plan = build_reference_scene_plan(settings)?;
    let image = render_reference_scene_image(&plan)?;
    let mut rgba = Vec::with_capacity(image.pixels().len() * 4);
    for pixel in image.pixels() {
        rgba.extend_from_slice(pixel);
    }
    image::save_buffer_with_format(
        Path::new(&args.candidate_path),
        &rgba,
        image.width,
        image.height,
        ColorType::Rgba8,
        ImageFormat::Png,
    )
    .map_err(|error| {
        invalid_harness::<()>("screenshot.candidate.render", error.to_string()).unwrap_err()
    })
}

struct DecodedPng {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn read_png_rgba(label: &str, path: &str) -> Result<DecodedPng, EngineError> {
    let bytes = fs::read(path).map_err(|error| {
        invalid_harness::<()>(format!("screenshot.{label}.read"), error.to_string()).unwrap_err()
    })?;
    let format = image::guess_format(&bytes).map_err(|error| {
        invalid_harness::<()>(format!("screenshot.{label}.format"), error.to_string()).unwrap_err()
    })?;
    if format != ImageFormat::Png {
        return invalid_harness(format!("screenshot.{label}.format"), "file is not a PNG");
    }
    let image = image::load_from_memory_with_format(&bytes, ImageFormat::Png)
        .map_err(|error| {
            invalid_harness::<()>(format!("screenshot.{label}.decode"), error.to_string())
                .unwrap_err()
        })?
        .to_rgba8();
    Ok(DecodedPng {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
        bytes,
    })
}

#[derive(Clone, Copy)]
struct DiffMetrics {
    changed_bytes: u64,
    changed_pixels: u64,
    mean_abs_error: f32,
    mean_luma_error: f32,
    rms_error: f32,
    max_channel_delta: u8,
}

fn diff_metrics(left: &DecodedPng, right: &DecodedPng) -> Result<DiffMetrics, EngineError> {
    if left.width != right.width || left.height != right.height {
        return invalid_harness(
            "screenshot.dimensions",
            format!(
                "baseline {}x{} differs from candidate {}x{}",
                left.width, left.height, right.width, right.height
            ),
        );
    }
    let mut changed_bytes = 0_u64;
    let mut changed_pixels = 0_u64;
    let mut sum_abs = 0_f64;
    let mut sum_luma = 0_f64;
    let mut sum_square = 0_f64;
    let mut max_channel_delta = 0_u8;
    for (left_px, right_px) in left.rgba.chunks_exact(4).zip(right.rgba.chunks_exact(4)) {
        let mut pixel_changed = false;
        let mut luma_delta = 0_f64;
        for channel in 0..4 {
            let delta = left_px[channel].abs_diff(right_px[channel]);
            if delta > 0 {
                changed_bytes += 1;
                pixel_changed = true;
            }
            max_channel_delta = max_channel_delta.max(delta);
            let delta_f = f64::from(delta);
            sum_abs += delta_f;
            sum_square += delta_f * delta_f;
            luma_delta += match channel {
                0 => 0.2126 * delta_f,
                1 => 0.7152 * delta_f,
                2 => 0.0722 * delta_f,
                _ => 0.0,
            };
        }
        if pixel_changed {
            changed_pixels += 1;
        }
        sum_luma += luma_delta;
    }
    let channels = left.rgba.len() as f64;
    let pixels = channels / 4.0;
    Ok(DiffMetrics {
        changed_bytes,
        changed_pixels,
        mean_abs_error: (sum_abs / (channels * 255.0)) as f32,
        mean_luma_error: (sum_luma / (pixels * 255.0)) as f32,
        rms_error: ((sum_square / channels).sqrt() / 255.0) as f32,
        max_channel_delta,
    })
}
