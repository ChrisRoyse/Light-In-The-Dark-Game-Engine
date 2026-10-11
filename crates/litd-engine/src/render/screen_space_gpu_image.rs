use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::EngineError;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScreenSpaceGpuMetrics {
    pub average_luma: f32,
    pub edge_contrast: f32,
    pub dark_pixel_ratio: f32,
    pub reflection_pixel_ratio: f32,
    pub alpha_opaque_ratio: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenSpaceGpuImage {
    pub width: u32,
    pub height: u32,
    pixels: Vec<[u8; 4]>,
}

impl ScreenSpaceGpuImage {
    pub(crate) fn from_pixels(width: u32, height: u32, pixels: Vec<[u8; 4]>) -> Self {
        Self {
            width,
            height,
            pixels,
        }
    }

    pub(crate) fn from_float_pixels(width: u32, height: u32, pixels: &[[f32; 4]]) -> Self {
        let pixels = pixels
            .iter()
            .map(|pixel| {
                [
                    unit_to_u8(pixel[0]),
                    unit_to_u8(pixel[1]),
                    unit_to_u8(pixel[2]),
                    unit_to_u8(pixel[3]),
                ]
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
                "screen_space_gpu.image.write",
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

pub(super) fn measure(image: &ScreenSpaceGpuImage) -> ScreenSpaceGpuMetrics {
    let mut luma_sum = 0.0;
    let mut dark = 0_u32;
    let mut reflection = 0_u32;
    let mut opaque = 0_u32;
    let mut edge_sum = 0.0;
    let mut edge_count = 0_u32;
    for y in 0..image.height {
        for x in 0..image.width {
            let [r, g, b, a] = image.pixels[(y * image.width + x) as usize];
            let pixel_luma = luma(r, g, b);
            luma_sum += pixel_luma;
            if pixel_luma < 0.18 {
                dark += 1;
            }
            if b > r.saturating_add(10) && b > 90 {
                reflection += 1;
            }
            if a == 255 {
                opaque += 1;
            }
            if x + 1 < image.width {
                let [nr, ng, nb, _] = image.pixels[(y * image.width + x + 1) as usize];
                edge_sum += (pixel_luma - luma(nr, ng, nb)).abs();
                edge_count += 1;
            }
        }
    }
    let total = image.pixels.len().max(1) as f32;
    ScreenSpaceGpuMetrics {
        average_luma: luma_sum / total,
        edge_contrast: edge_sum / edge_count.max(1) as f32,
        dark_pixel_ratio: dark as f32 / total,
        reflection_pixel_ratio: reflection as f32 / total,
        alpha_opaque_ratio: opaque as f32 / total,
    }
}

fn luma(r: u8, g: u8, b: u8) -> f32 {
    0.2126 * r as f32 / 255.0 + 0.7152 * g as f32 / 255.0 + 0.0722 * b as f32 / 255.0
}

fn unit_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn image_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::ScreenSpaceInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}
