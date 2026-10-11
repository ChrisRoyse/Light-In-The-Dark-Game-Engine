//! Readback image + non-degenerate output statistics for the FSR upscaler (#1741).

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpscaleImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl UpscaleImage {
    pub fn from_pixels(width: u32, height: u32, pixels: Vec<[u8; 4]>) -> Self {
        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn checksum(&self) -> String {
        let mut bytes = Vec::with_capacity(self.pixels.len() * 4 + 8);
        bytes.extend_from_slice(&self.width.to_le_bytes());
        bytes.extend_from_slice(&self.height.to_le_bytes());
        for pixel in &self.pixels {
            bytes.extend_from_slice(pixel);
        }
        sha256_hex(&bytes)
    }

    pub fn sample_luma(&self, x: u32, y: u32) -> f32 {
        let idx = (y as usize) * (self.width as usize) + (x as usize);
        let p = self.pixels[idx];
        luma(p)
    }
}

/// Output statistics used to prove the upscaled frame is non-degenerate (not a
/// flat/black/saturated buffer) and to compare against a native reference.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UpscaleMetrics {
    pub mean_luma: f32,
    pub luma_variance: f32,
    pub min_luma: f32,
    pub max_luma: f32,
    pub distinct_luma_buckets: u32,
    pub nonzero_ratio: f32,
}

impl UpscaleMetrics {
    /// A non-degenerate image has real dynamic range and multiple luma buckets.
    pub fn is_non_degenerate(&self) -> bool {
        self.luma_variance > 1.0e-5
            && self.distinct_luma_buckets >= 8
            && self.max_luma > self.min_luma
            && self.nonzero_ratio > 0.0
    }
}

pub fn measure(image: &UpscaleImage) -> UpscaleMetrics {
    let count = image.pixels.len().max(1) as f32;
    let mut sum = 0.0_f64;
    let mut sum_sq = 0.0_f64;
    let mut min_luma = f32::INFINITY;
    let mut max_luma = f32::NEG_INFINITY;
    let mut nonzero = 0u32;
    let mut buckets = [false; 64];
    for pixel in &image.pixels {
        let l = luma(*pixel);
        sum += l as f64;
        sum_sq += (l as f64) * (l as f64);
        min_luma = min_luma.min(l);
        max_luma = max_luma.max(l);
        if pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0 {
            nonzero += 1;
        }
        let bucket = ((l * 63.0).round() as i32).clamp(0, 63) as usize;
        buckets[bucket] = true;
    }
    let mean = (sum / count as f64) as f32;
    let variance = ((sum_sq / count as f64) - (mean as f64) * (mean as f64)).max(0.0) as f32;
    let distinct = buckets.iter().filter(|b| **b).count() as u32;
    UpscaleMetrics {
        mean_luma: mean,
        luma_variance: variance,
        min_luma: if min_luma.is_finite() { min_luma } else { 0.0 },
        max_luma: if max_luma.is_finite() { max_luma } else { 0.0 },
        distinct_luma_buckets: distinct,
        nonzero_ratio: nonzero as f32 / count,
    }
}

/// Peak signal-to-noise ratio (dB) between two equal-sized images. Returns
/// `f32::INFINITY` for identical images. Used to gate reconstruction quality
/// against a native-resolution reference.
pub fn psnr(a: &UpscaleImage, b: &UpscaleImage) -> Option<f32> {
    if a.width != b.width || a.height != b.height || a.pixels.len() != b.pixels.len() {
        return None;
    }
    if a.pixels.is_empty() {
        return None;
    }
    let mut mse = 0.0_f64;
    for (pa, pb) in a.pixels.iter().zip(b.pixels.iter()) {
        for c in 0..3 {
            let d = pa[c] as f64 - pb[c] as f64;
            mse += d * d;
        }
    }
    mse /= (a.pixels.len() * 3) as f64;
    if mse <= f64::EPSILON {
        return Some(f32::INFINITY);
    }
    Some((20.0 * (255.0_f64).log10() - 10.0 * mse.log10()) as f32)
}

fn luma(p: [u8; 4]) -> f32 {
    let r = p[0] as f32 / 255.0;
    let g = p[1] as f32 / 255.0;
    let b = p[2] as f32 / 255.0;
    0.2126 * r + 0.7152 * g + 0.0722 * b
}
