use std::{fs, path::Path};

use crate::{
    EngineError,
    math::{Vec2, Vec3},
    render::post::apply_post_process_reference,
};

use super::{ReferenceScenePlan, reference_canvas::Canvas};

const CANONICAL_EDGE_HEIGHT_PX: f32 = 540.0;

#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceSceneImage {
    pub width: u32,
    pub height: u32,
    pixels: Vec<[u8; 4]>,
}

impl ReferenceSceneImage {
    pub(super) fn from_pixels(width: u32, height: u32, pixels: Vec<[u8; 4]>) -> Self {
        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn from_rgba_pixels(
        width: u32,
        height: u32,
        pixels: Vec<[u8; 4]>,
    ) -> Result<Self, EngineError> {
        if width == 0 || height == 0 {
            return Err(EngineError::ReferenceSceneInvalidConfig {
                field: "visual_target.pixels".to_string(),
                reason: "width and height must be non-zero".to_string(),
            });
        }
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| EngineError::ReferenceSceneInvalidConfig {
                field: "visual_target.pixels".to_string(),
                reason: "pixel count overflowed usize".to_string(),
            })?;
        if pixels.len() != expected {
            return Err(EngineError::ReferenceSceneInvalidConfig {
                field: "visual_target.pixels".to_string(),
                reason: format!("expected {expected} pixels, got {}", pixels.len()),
            });
        }
        Ok(Self::from_pixels(width, height, pixels))
    }

    pub fn pixels(&self) -> &[[u8; 4]] {
        &self.pixels
    }

    pub fn checksum(&self) -> String {
        checksum_bytes(&self.bmp_bytes())
    }

    pub fn write_bmp(&self, path: impl AsRef<Path>) -> Result<(), EngineError> {
        fs::write(path, self.bmp_bytes()).map_err(|err| EngineError::ReferenceSceneInvalidConfig {
            field: "visual_target.path".to_string(),
            reason: format!("failed to write BMP: {err}"),
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReferenceSceneMetrics {
    pub average_luma: f32,
    pub warm_highlight_ratio: f32,
    pub cool_shadow_ratio: f32,
    pub emissive_ratio: f32,
    pub edge_contrast: f32,
    pub hud_gold_ratio: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualMetricBand {
    pub name: &'static str,
    pub actual: f32,
    pub min: f32,
    pub max: f32,
    pub pass: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceSceneComparison {
    pub pass: bool,
    pub checksum: String,
    pub metrics: ReferenceSceneMetrics,
    pub bands: Vec<VisualMetricBand>,
}

impl ReferenceSceneComparison {
    pub fn band_summary(&self) -> String {
        self.bands
            .iter()
            .map(|band| {
                format!(
                    "{}={:.4}[{:.4},{:.4}]:{}",
                    band.name, band.actual, band.min, band.max, band.pass
                )
            })
            .collect::<Vec<_>>()
            .join("|")
    }
}

pub fn render_reference_scene_image(
    plan: &ReferenceScenePlan,
) -> Result<ReferenceSceneImage, EngineError> {
    let mut canvas = Canvas::new(plan.settings.width, plan.settings.height, [8, 12, 22, 255]);
    draw_background(&mut canvas);
    draw_world(&mut canvas, plan)?;
    draw_hud(&mut canvas, plan)?;
    Ok(canvas.into_image())
}

pub fn compare_reference_scene_image(image: &ReferenceSceneImage) -> ReferenceSceneComparison {
    let metrics = measure(image);
    let bands = vec![
        band("average_luma", metrics.average_luma, 0.16, 0.58),
        band(
            "warm_highlight_ratio",
            metrics.warm_highlight_ratio,
            0.025,
            0.24,
        ),
        band("cool_shadow_ratio", metrics.cool_shadow_ratio, 0.12, 0.72),
        band("emissive_ratio", metrics.emissive_ratio, 0.006, 0.20),
        band("edge_contrast", metrics.edge_contrast, 0.012, 0.22),
        band("hud_gold_ratio", metrics.hud_gold_ratio, 0.010, 0.18),
    ];
    ReferenceSceneComparison {
        pass: bands.iter().all(|band| band.pass),
        checksum: image.checksum(),
        metrics,
        bands,
    }
}

fn draw_background(canvas: &mut Canvas) {
    let w = canvas.width as f32;
    let h = canvas.height as f32;
    for y in 0..canvas.height {
        let v = y as f32 / h.max(1.0);
        for x in 0..canvas.width {
            let u = x as f32 / w.max(1.0);
            let d = Vec2::new(u - 0.5, v - 0.48).length();
            let fog = (1.0 - d * 1.4).clamp(0.0, 1.0);
            let top = Vec3::new(0.055, 0.058, 0.066).lerp(Vec3::new(0.16, 0.17, 0.16), v);
            let grain = (((x * 17 + y * 31) % 13) as f32 - 6.0) / 700.0;
            let color = top + Vec3::new(0.075, 0.080, 0.070) * fog + Vec3::splat(grain);
            canvas.set(x, y, rgb(color));
        }
    }
}

fn draw_world(canvas: &mut Canvas, plan: &ReferenceScenePlan) -> Result<(), EngineError> {
    let w = canvas.width as f32;
    let h = canvas.height as f32;
    let center = Vec2::new(w * 0.50, h * 0.50);
    let stone = lit(plan, plan.shade_results[0].color * 0.9)?;
    let gold = lit(plan, plan.shade_results[1].color * 0.85)?;
    let ember = lit(plan, plan.shade_results[2].color)?;
    let arcane = lit(plan, plan.shade_results[3].color)?;
    canvas.glow(
        center + Vec2::new(-w * 0.18, -h * 0.02),
        w * 0.18,
        [255, 92, 18, 70],
    );
    canvas.glow(
        center + Vec2::new(w * 0.22, -h * 0.08),
        w * 0.20,
        [42, 166, 255, 72],
    );
    canvas.ellipse(
        center + Vec2::new(0.0, h * 0.05),
        w * 0.36,
        h * 0.25,
        [18, 22, 28, 255],
    );
    canvas.diamond(center + Vec2::new(0.0, h * 0.02), w * 0.34, h * 0.22, stone);
    for i in -4..=4 {
        let offset = i as f32 * w * 0.055;
        canvas.line(
            center + Vec2::new(offset - w * 0.16, -h * 0.11),
            center + Vec2::new(offset + w * 0.12, h * 0.15),
            [116, 124, 118, 210],
            2.0,
        );
        canvas.line(
            center + Vec2::new(offset + w * 0.14, -h * 0.11),
            center + Vec2::new(offset - w * 0.12, h * 0.15),
            [20, 22, 28, 210],
            2.0,
        );
    }
    for x in [0.30, 0.38, 0.64, 0.72] {
        let p = Vec2::new(w * x, h * 0.29);
        canvas.rect(p, Vec2::new(w * 0.035, h * 0.18), [31, 36, 45, 255]);
        canvas.rect(
            p + Vec2::new(w * 0.004, 0.0),
            Vec2::new(w * 0.008, h * 0.16),
            [92, 104, 112, 80],
        );
    }
    canvas.glow(center + Vec2::new(-w * 0.12, h * 0.04), w * 0.07, ember);
    canvas.circle(center + Vec2::new(-w * 0.12, h * 0.04), w * 0.025, ember);
    canvas.glow(center + Vec2::new(w * 0.16, -h * 0.04), w * 0.08, arcane);
    canvas.ring(
        center + Vec2::new(w * 0.16, -h * 0.04),
        w * 0.047,
        w * 0.038,
        arcane,
    );
    let hero = center + Vec2::new(0.0, h * 0.02);
    canvas.ellipse(
        hero + Vec2::new(0.0, h * 0.055),
        w * 0.075,
        h * 0.025,
        [0, 0, 0, 92],
    );
    canvas.circle(hero, w * 0.028, [64, 72, 84, 255]);
    canvas.ring(hero, w * 0.034, w * 0.028, gold);
    canvas.line(
        hero + Vec2::new(w * 0.018, -h * 0.005),
        hero + Vec2::new(w * 0.078, -h * 0.055),
        gold,
        3.0,
    );
    for p in [
        center + Vec2::new(-w * 0.09, h * 0.01),
        center + Vec2::new(-w * 0.04, h * 0.09),
        center + Vec2::new(w * 0.08, h * 0.08),
    ] {
        canvas.circle(p, w * 0.020, [34, 24, 28, 255]);
        canvas.ring(p, w * 0.024, w * 0.020, [180, 42, 35, 210]);
    }
    canvas.rect(
        hero + Vec2::new(-w * 0.03, -h * 0.15),
        Vec2::new(w * 0.07, h * 0.045),
        [255, 218, 48, 230],
    );
    canvas.rect(
        hero + Vec2::new(-w * 0.018, -h * 0.10),
        Vec2::new(w * 0.045, h * 0.030),
        [255, 238, 72, 210],
    );
    Ok(())
}

fn draw_hud(canvas: &mut Canvas, _plan: &ReferenceScenePlan) -> Result<(), EngineError> {
    let w = canvas.width as f32;
    let h = canvas.height as f32;
    let gold = [221, 169, 75, 255];
    canvas.rect(
        Vec2::new(w * 0.27, h * 0.83),
        Vec2::new(w * 0.46, h * 0.15),
        [8, 9, 13, 220],
    );
    canvas.rect(
        Vec2::new(w * 0.27, h * 0.83),
        Vec2::new(w * 0.46, h * 0.010),
        gold,
    );
    canvas.rect(
        Vec2::new(w * 0.27, h * 0.965),
        Vec2::new(w * 0.46, h * 0.010),
        gold,
    );
    canvas.rect(
        Vec2::new(w * 0.27, h * 0.83),
        Vec2::new(w * 0.008, h * 0.145),
        gold,
    );
    canvas.rect(
        Vec2::new(w * 0.722, h * 0.83),
        Vec2::new(w * 0.008, h * 0.145),
        gold,
    );
    canvas.circle(Vec2::new(w * 0.31, h * 0.90), w * 0.050, [145, 18, 18, 245]);
    canvas.circle(Vec2::new(w * 0.69, h * 0.90), w * 0.050, [22, 91, 174, 245]);
    for i in 0..10 {
        let x = w * 0.39 + i as f32 * w * 0.027;
        canvas.rect(
            Vec2::new(x, h * 0.875),
            Vec2::new(w * 0.022, h * 0.050),
            [22, 24, 30, 245],
        );
        canvas.rect(
            Vec2::new(x, h * 0.875),
            Vec2::new(w * 0.022, h * 0.004),
            gold,
        );
        canvas.rect(
            Vec2::new(x, h * 0.922),
            Vec2::new(w * 0.022, h * 0.004),
            gold,
        );
        canvas.rect(
            Vec2::new(x, h * 0.875),
            Vec2::new(w * 0.003, h * 0.050),
            gold,
        );
        canvas.rect(
            Vec2::new(x + w * 0.019, h * 0.875),
            Vec2::new(w * 0.003, h * 0.050),
            gold,
        );
    }
    canvas.rect(
        Vec2::new(w * 0.37, h * 0.854),
        Vec2::new(w * 0.28, h * 0.006),
        gold,
    );
    canvas.rect(
        Vec2::new(w * 0.34, h * 0.03),
        Vec2::new(w * 0.32, h * 0.055),
        [10, 8, 10, 230],
    );
    canvas.rect(
        Vec2::new(w * 0.36, h * 0.047),
        Vec2::new(w * 0.28, h * 0.022),
        [142, 20, 24, 255],
    );
    let mini = Vec2::new(w * 0.865, h * 0.17);
    canvas.circle(mini, w * 0.080, [15, 19, 18, 230]);
    canvas.ring(mini, w * 0.083, w * 0.078, gold);
    canvas.diamond(mini, w * 0.105, h * 0.085, [71, 93, 72, 210]);
    canvas.circle(
        mini + Vec2::new(w * 0.016, -h * 0.010),
        w * 0.007,
        [52, 205, 255, 255],
    );
    canvas.circle(
        mini + Vec2::new(-w * 0.026, h * 0.021),
        w * 0.006,
        [255, 76, 42, 255],
    );
    canvas.rect(
        Vec2::new(w * 0.02, h * 0.63),
        Vec2::new(w * 0.24, h * 0.20),
        [7, 9, 12, 178],
    );
    canvas.rect(
        Vec2::new(w * 0.02, h * 0.63),
        Vec2::new(w * 0.24, h * 0.006),
        gold,
    );
    Ok(())
}

fn lit(plan: &ReferenceScenePlan, hdr: Vec3) -> Result<[u8; 4], EngineError> {
    Ok(rgb(apply_post_process_reference(
        plan.post_plan.settings,
        hdr,
    )?
    .output))
}

fn measure(image: &ReferenceSceneImage) -> ReferenceSceneMetrics {
    let mut luma_sum = 0.0;
    let mut warm = 0;
    let mut cool = 0;
    let mut emissive = 0;
    let mut hud_gold = 0;
    let mut edge_sum = 0.0;
    let mut edge_count = 0;
    let total = image.pixels.len() as f32;
    for y in 0..image.height {
        for x in 0..image.width {
            let p = image.pixels[(y * image.width + x) as usize];
            let r = p[0] as f32 / 255.0;
            let g = p[1] as f32 / 255.0;
            let b = p[2] as f32 / 255.0;
            let l = luma([r, g, b]);
            luma_sum += l;
            if r > 0.55 && g > 0.22 && r > b * 1.35 {
                warm += 1;
            }
            if b > r * 1.12 && l < 0.42 {
                cool += 1;
            }
            if r.max(g).max(b) > 0.86 && (r.max(g).max(b) - r.min(g).min(b)) > 0.25 {
                emissive += 1;
            }
            if y as f32 > image.height as f32 * 0.78 && r > 0.45 && g > 0.30 && b < 0.30 {
                hud_gold += 1;
            }
            if x + 1 < image.width && y + 1 < image.height && x % 4 == 0 && y % 4 == 0 {
                let px = image.pixels[(y * image.width + x + 1) as usize];
                let py = image.pixels[((y + 1) * image.width + x) as usize];
                edge_sum += (l - luma_px(px)).abs() + (l - luma_px(py)).abs();
                edge_count += 2;
            }
        }
    }
    ReferenceSceneMetrics {
        average_luma: luma_sum / total,
        warm_highlight_ratio: warm as f32 / total,
        cool_shadow_ratio: cool as f32 / total,
        emissive_ratio: emissive as f32 / total,
        edge_contrast: normalized_edge_contrast(
            image.width,
            image.height,
            edge_sum / edge_count.max(1) as f32,
        ),
        hud_gold_ratio: hud_gold as f32 / total,
    }
}

fn normalized_edge_contrast(width: u32, height: u32, raw_edge_mean: f32) -> f32 {
    // Perimeter-like edge energy is spread over more sample points as output
    // resolution rises, so normalize high-resolution targets back to the
    // canonical 540px floor without penalizing smaller accepted test targets.
    let shortest_axis = width.min(height).max(1) as f32;
    raw_edge_mean * (shortest_axis / CANONICAL_EDGE_HEIGHT_PX).max(1.0)
}

fn band(name: &'static str, actual: f32, min: f32, max: f32) -> VisualMetricBand {
    VisualMetricBand {
        name,
        actual,
        min,
        max,
        pass: (min..=max).contains(&actual),
    }
}

fn rgb(color: Vec3) -> [u8; 4] {
    [
        (color.x.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.y.clamp(0.0, 1.0) * 255.0).round() as u8,
        (color.z.clamp(0.0, 1.0) * 255.0).round() as u8,
        255,
    ]
}

fn luma(color: [f32; 3]) -> f32 {
    color[0] * 0.2126 + color[1] * 0.7152 + color[2] * 0.0722
}

fn luma_px(pixel: [u8; 4]) -> f32 {
    luma([
        pixel[0] as f32 / 255.0,
        pixel[1] as f32 / 255.0,
        pixel[2] as f32 / 255.0,
    ])
}

fn checksum_bytes(bytes: &[u8]) -> String {
    litd_hash::fnv1a64_hex(bytes)
}
