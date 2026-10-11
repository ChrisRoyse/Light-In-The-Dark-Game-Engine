use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::EngineError;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PostGpuMetrics {
    pub average_luma: f32,
    pub lit_pixel_ratio: f32,
    pub hot_pixel_ratio: f32,
    pub blue_pixel_ratio: f32,
    pub alpha_opaque_ratio: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostGpuImage {
    pub width: u32,
    pub height: u32,
    pixels: Vec<[u8; 4]>,
}

impl PostGpuImage {
    pub(crate) fn from_pixels(width: u32, height: u32, pixels: Vec<[u8; 4]>) -> Self {
        Self {
            width,
            height,
            pixels,
        }
    }

    pub(crate) fn from_hdr_visualization(width: u32, height: u32, pixels: &[[f32; 4]]) -> Self {
        let pixels = pixels
            .iter()
            .map(|pixel| {
                let r = hdr_to_u8(pixel[0]);
                let g = hdr_to_u8(pixel[1]);
                let b = hdr_to_u8(pixel[2]);
                [r, g, b, 255]
            })
            .collect();
        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn pixels(&self) -> &[[u8; 4]] {
        &self.pixels
    }

    pub fn checksum(&self) -> String {
        litd_hash::fnv1a64_hex(&self.bmp_bytes())
    }

    pub fn write_bmp(&self, path: impl AsRef<Path>) -> Result<(), EngineError> {
        fs::write(path.as_ref(), self.bmp_bytes()).map_err(|error| {
            image_error(
                "post_gpu.image.write",
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
}

pub(super) fn measure(image: &PostGpuImage) -> PostGpuMetrics {
    let mut luma_sum = 0.0;
    let mut lit = 0_u32;
    let mut hot = 0_u32;
    let mut blue = 0_u32;
    let mut opaque = 0_u32;
    for &[r, g, b, a] in image.pixels() {
        let rf = r as f32 / 255.0;
        let gf = g as f32 / 255.0;
        let bf = b as f32 / 255.0;
        let luma = 0.2126 * rf + 0.7152 * gf + 0.0722 * bf;
        luma_sum += luma;
        if luma > 0.05 {
            lit += 1;
        }
        if rf > 0.55 && gf > 0.18 {
            hot += 1;
        }
        if bf > 0.45 && bf > rf {
            blue += 1;
        }
        if a == 255 {
            opaque += 1;
        }
    }
    let total = image.pixels().len().max(1) as f32;
    PostGpuMetrics {
        average_luma: luma_sum / total,
        lit_pixel_ratio: lit as f32 / total,
        hot_pixel_ratio: hot as f32 / total,
        blue_pixel_ratio: blue as f32 / total,
        alpha_opaque_ratio: opaque as f32 / total,
    }
}

fn hdr_to_u8(value: f32) -> u8 {
    let mapped = value.max(0.0) / (1.0 + value.max(0.0));
    (mapped.powf(1.0 / 2.2) * 255.0).round() as u8
}

fn image_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::PostProcessInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}
