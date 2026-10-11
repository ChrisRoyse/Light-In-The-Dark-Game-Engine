//! Performance overlay planning and deterministic BMP preview readback.

use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Vec2, Vec4},
    profile::{FrameProfile, ProfileJsonReadback, ProfileSystem, write_profile_json},
};

use super::{TextRun, UiRenderSettings};

const FRAME_BUDGET_NS: u64 = 100_000_000;
const INPUT_BUDGET_NS: u64 = 50_000_000;
const MIN_WIDTH: u32 = 320;
const MIN_HEIGHT: u32 = 120;
const MAX_DIMENSION: u32 = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputLatencyRecord {
    pub action_id: String,
    pub input_frame: u64,
    pub reaction_frame: u64,
    pub input_ns: u64,
    pub reaction_ns: u64,
}

impl InputLatencyRecord {
    pub fn latency_ns(&self) -> Result<u64, EngineError> {
        if self.action_id.trim().is_empty() {
            return invalid_overlay("input_latency.action_id", "action id must be non-empty");
        }
        self.reaction_ns.checked_sub(self.input_ns).ok_or_else(|| {
            ui_error(
                "input_latency.reaction_ns",
                "reaction timestamp must be at or after input timestamp",
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceOverlayBar {
    pub id: String,
    pub label: String,
    pub value_ns: u64,
    pub budget_ns: u64,
    pub filled_px: u32,
    pub max_px: u32,
    pub over_budget: bool,
    pub color_rgba: [u8; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerformanceOverlayPlan {
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub frame_index: u64,
    pub frame_time_ns: u64,
    pub input_latency_ns: u64,
    pub over_100ms: bool,
    pub latency_over_50ms: bool,
    pub systems_present: Vec<ProfileSystem>,
    pub text_runs: Vec<TextRun>,
    pub bars: Vec<PerformanceOverlayBar>,
    pub profile: FrameProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceOverlayArtifactReadback {
    pub bmp_path: PathBuf,
    pub bmp_bytes: u64,
    pub bmp_sha256: String,
    pub export: ProfileJsonReadback,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerformanceOverlayImage {
    pub width: u32,
    pub height: u32,
    pixels: Vec<[u8; 4]>,
}

impl PerformanceOverlayImage {
    pub fn pixels(&self) -> &[[u8; 4]] {
        &self.pixels
    }

    pub fn write_bmp(&self, path: impl AsRef<Path>) -> Result<(), EngineError> {
        fs::write(path.as_ref(), self.bmp_bytes()).map_err(|error| {
            ui_error(
                "performance_overlay.bmp.write",
                format!("failed to write {}: {error}", path.as_ref().display()),
            )
        })
    }

    pub fn bmp_bytes(&self) -> Vec<u8> {
        let row_stride = (self.width as usize * 3).div_ceil(4) * 4;
        let pixel_bytes = row_stride * self.height as usize;
        let file_size = 14 + 40 + pixel_bytes;
        let mut bytes = Vec::with_capacity(file_size);
        bytes.extend_from_slice(b"BM");
        bytes.extend_from_slice(&(file_size as u32).to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(&(54_u32).to_le_bytes());
        bytes.extend_from_slice(&(40_u32).to_le_bytes());
        bytes.extend_from_slice(&(self.width as i32).to_le_bytes());
        bytes.extend_from_slice(&(self.height as i32).to_le_bytes());
        bytes.extend_from_slice(&(1_u16).to_le_bytes());
        bytes.extend_from_slice(&(24_u16).to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        bytes.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
        bytes.extend_from_slice(&(2835_i32).to_le_bytes());
        bytes.extend_from_slice(&(2835_i32).to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        bytes.extend_from_slice(&(0_u32).to_le_bytes());
        let padding = row_stride - self.width as usize * 3;
        for y in (0..self.height).rev() {
            for x in 0..self.width {
                let [r, g, b, _] = self.pixels[(y * self.width + x) as usize];
                bytes.extend_from_slice(&[b, g, r]);
            }
            bytes.extend(std::iter::repeat_n(0, padding));
        }
        bytes
    }

    pub fn from_bmp_bytes(bytes: &[u8]) -> Result<Self, EngineError> {
        if bytes.len() < 54 || &bytes[0..2] != b"BM" {
            return invalid_overlay("performance_overlay.bmp", "BMP header is missing");
        }
        let data_offset = read_u32(bytes, 10)? as usize;
        let width = read_i32(bytes, 18)?;
        let height = read_i32(bytes, 22)?;
        let bits_per_pixel = read_u16(bytes, 28)?;
        if width <= 0 || height <= 0 || bits_per_pixel != 24 {
            return invalid_overlay("performance_overlay.bmp", "expected positive 24-bit BMP");
        }
        let width = width as u32;
        let height = height as u32;
        let row_stride = (width as usize * 3).div_ceil(4) * 4;
        let expected_len = data_offset + row_stride * height as usize;
        if bytes.len() < expected_len {
            return invalid_overlay("performance_overlay.bmp", "BMP payload is truncated");
        }
        let mut pixels = vec![[0, 0, 0, 255]; width as usize * height as usize];
        for file_y in 0..height {
            let y = height - 1 - file_y;
            let row = data_offset + row_stride * file_y as usize;
            for x in 0..width {
                let i = row + x as usize * 3;
                pixels[(y * width + x) as usize] = [bytes[i + 2], bytes[i + 1], bytes[i], 255];
            }
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    fn new(width: u32, height: u32, color: [u8; 4]) -> Self {
        Self {
            width,
            height,
            pixels: vec![color; width as usize * height as usize],
        }
    }

    fn rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
        let max_y = (y + h).min(self.height);
        let max_x = (x + w).min(self.width);
        for py in y.min(self.height)..max_y {
            for px in x.min(self.width)..max_x {
                self.pixels[(py * self.width + px) as usize] = color;
            }
        }
    }
}

pub fn build_performance_overlay_plan(
    settings: UiRenderSettings,
    profile: &FrameProfile,
    latency: &InputLatencyRecord,
) -> Result<PerformanceOverlayPlan, EngineError> {
    settings.validate()?;
    validate_profile(profile)?;
    let latency_ns = latency.latency_ns()?;
    let width = dimension(
        "performance_overlay.viewport_width",
        settings.viewport_width,
    )?;
    let height = dimension(
        "performance_overlay.viewport_height",
        settings.viewport_height,
    )?;
    let bar_max_px = width.saturating_sub(96).max(1);
    let frame_bar = bar(
        "frame",
        "Frame time",
        profile.frame_time_ns,
        FRAME_BUDGET_NS,
        bar_max_px,
        [54, 202, 125, 255],
        [225, 72, 72, 255],
    );
    let latency_bar = bar(
        "input_latency",
        "Input latency",
        latency_ns,
        INPUT_BUDGET_NS,
        bar_max_px,
        [88, 166, 255, 255],
        [232, 173, 65, 255],
    );
    let text_runs = vec![
        overlay_text(
            "frame-time",
            format!(
                "frame {} {}",
                profile.frame_index,
                format_ms(profile.frame_time_ns)
            ),
            16.0,
        ),
        overlay_text(
            "input-latency",
            format!("input {} {}", latency.action_id, format_ms(latency_ns)),
            36.0,
        ),
        overlay_text(
            "systems",
            format!("systems {}", system_list(&profile.systems_present)),
            56.0,
        ),
    ];
    Ok(PerformanceOverlayPlan {
        viewport_width: width,
        viewport_height: height,
        frame_index: profile.frame_index,
        frame_time_ns: profile.frame_time_ns,
        input_latency_ns: latency_ns,
        over_100ms: profile.frame_time_ns > FRAME_BUDGET_NS,
        latency_over_50ms: latency_ns > INPUT_BUDGET_NS,
        systems_present: profile.systems_present.clone(),
        text_runs,
        bars: vec![frame_bar, latency_bar],
        profile: profile.clone(),
    })
}

pub fn render_performance_overlay_image(
    plan: &PerformanceOverlayPlan,
) -> Result<PerformanceOverlayImage, EngineError> {
    if plan.viewport_width < MIN_WIDTH || plan.viewport_height < MIN_HEIGHT {
        return invalid_overlay("performance_overlay.viewport", "viewport is too small");
    }
    let mut image =
        PerformanceOverlayImage::new(plan.viewport_width, plan.viewport_height, [8, 10, 14, 255]);
    image.rect(8, 8, plan.viewport_width - 16, 88, [18, 22, 30, 255]);
    image.rect(8, 8, plan.viewport_width - 16, 2, [181, 143, 74, 255]);
    for (index, bar) in plan.bars.iter().enumerate() {
        let y = 28 + index as u32 * 32;
        image.rect(64, y, bar.max_px, 12, [42, 48, 60, 255]);
        if bar.filled_px > 0 {
            image.rect(64, y, bar.filled_px, 12, bar.color_rgba);
        }
        image.rect(
            64 + bar.max_px.saturating_sub(1),
            y,
            1,
            12,
            [222, 222, 210, 255],
        );
    }
    Ok(image)
}

pub fn write_performance_overlay_artifacts(
    plan: &PerformanceOverlayPlan,
    bmp_path: impl AsRef<Path>,
    export_json_path: impl AsRef<Path>,
) -> Result<PerformanceOverlayArtifactReadback, EngineError> {
    let image = render_performance_overlay_image(plan)?;
    let bmp_path = bmp_path.as_ref();
    if let Some(parent) = bmp_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            ui_error(
                "performance_overlay.bmp.parent",
                format!("failed to create {}: {error}", parent.display()),
            )
        })?;
    }
    image.write_bmp(bmp_path)?;
    let bmp_bytes = fs::read(bmp_path).map_err(|error| {
        ui_error(
            "performance_overlay.bmp.readback",
            format!("failed to read {}: {error}", bmp_path.display()),
        )
    })?;
    let bmp_image = PerformanceOverlayImage::from_bmp_bytes(&bmp_bytes)?;
    if bmp_image != image {
        return invalid_overlay("performance_overlay.bmp", "BMP readback pixels differed");
    }
    let export = write_profile_json(export_json_path, &plan.profile)?;
    Ok(PerformanceOverlayArtifactReadback {
        bmp_path: bmp_path.to_path_buf(),
        bmp_bytes: u64::try_from(bmp_bytes.len())
            .map_err(|_| ui_error_value("performance_overlay.bmp", "BMP length exceeded u64"))?,
        bmp_sha256: sha256_hex(&bmp_bytes),
        export,
    })
}

fn validate_profile(profile: &FrameProfile) -> Result<(), EngineError> {
    if !profile.enabled || profile.cpu_spans.is_empty() {
        return invalid_overlay("performance_overlay.profile", "profile capture is missing");
    }
    for required in [
        ProfileSystem::Sim,
        ProfileSystem::Nav,
        ProfileSystem::Ai,
        ProfileSystem::Render,
        ProfileSystem::Stream,
    ] {
        if !profile.systems_present.contains(&required) {
            return invalid_overlay(
                "performance_overlay.profile.systems_present",
                format!("missing required system {}", required.as_str()),
            );
        }
    }
    Ok(())
}

fn dimension(field: &'static str, value: f32) -> Result<u32, EngineError> {
    let minimum = if field.ends_with("width") {
        MIN_WIDTH
    } else {
        MIN_HEIGHT
    };
    if !value.is_finite() || value < minimum as f32 {
        return invalid_overlay(field, "viewport dimension is below overlay minimum");
    }
    let rounded = value.round();
    if rounded > MAX_DIMENSION as f32 {
        return invalid_overlay(field, "viewport dimension exceeds overlay maximum");
    }
    Ok(rounded as u32)
}

fn bar(
    id: &'static str,
    label: &'static str,
    value_ns: u64,
    budget_ns: u64,
    max_px: u32,
    ok_color: [u8; 4],
    over_color: [u8; 4],
) -> PerformanceOverlayBar {
    let raw = (u128::from(value_ns) * u128::from(max_px)) / u128::from(budget_ns);
    let filled_px = u32::try_from(raw.min(u128::from(max_px))).unwrap_or(max_px);
    let over_budget = value_ns > budget_ns;
    PerformanceOverlayBar {
        id: id.to_string(),
        label: label.to_string(),
        value_ns,
        budget_ns,
        filled_px,
        max_px,
        over_budget,
        color_rgba: if over_budget { over_color } else { ok_color },
    }
}

fn overlay_text(id: &'static str, text: String, y: f32) -> TextRun {
    TextRun {
        id: id.to_string(),
        text,
        origin_px: Vec2::new(16.0, y),
        font_px: 14.0,
        color: Vec4::new(0.92, 0.91, 0.84, 1.0),
    }
}

fn system_list(systems: &[ProfileSystem]) -> String {
    systems
        .iter()
        .map(|system| system.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

fn format_ms(ns: u64) -> String {
    let micros = ns / 1000;
    format!("{}.{:03}ms", micros / 1000, micros % 1000)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, EngineError> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| ui_error_value("performance_overlay.bmp", "BMP header is truncated"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, EngineError> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| ui_error_value("performance_overlay.bmp", "BMP header is truncated"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, EngineError> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| ui_error_value("performance_overlay.bmp", "BMP header is truncated"))?;
    Ok(i32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn invalid_overlay<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(ui_error_value(field, reason))
}

fn ui_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    ui_error_value(field, reason)
}

fn ui_error_value(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "ui_perf_overlay_tests.rs"]
mod tests;
