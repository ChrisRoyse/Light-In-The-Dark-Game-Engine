use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::EngineError;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StylizedGpuMetrics {
    pub average_luma: f32,
    pub edge_contrast: f32,
    pub hot_pixel_ratio: f32,
    pub cyan_pixel_ratio: f32,
    pub nonblack_pixel_ratio: f32,
    pub alpha_opaque_ratio: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StylizedGpuImage {
    pub width: u32,
    pub height: u32,
    pixels: Vec<[u8; 4]>,
}

impl StylizedGpuImage {
    pub(crate) fn from_pixels(width: u32, height: u32, pixels: Vec<[u8; 4]>) -> Self {
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
                "stylized_gpu.image.write",
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

pub(super) fn measure(image: &StylizedGpuImage) -> StylizedGpuMetrics {
    measure_region(image, 0, 0, image.width, image.height)
}

pub(super) fn measure_region(
    image: &StylizedGpuImage,
    x0: u32,
    y0: u32,
    width: u32,
    height: u32,
) -> StylizedGpuMetrics {
    let mut luma_sum = 0.0;
    let mut edge_sum = 0.0;
    let mut edge_count = 0_u32;
    let mut hot = 0_u32;
    let mut cyan = 0_u32;
    let mut nonblack = 0_u32;
    let mut opaque = 0_u32;
    for y in y0..(y0 + height).min(image.height) {
        for x in x0..(x0 + width).min(image.width) {
            let [r, g, b, a] = image.pixels[(y * image.width + x) as usize];
            let pixel_luma = luma(r, g, b);
            luma_sum += pixel_luma;
            if r > 210 && g > 80 {
                hot += 1;
            }
            if b > 150 && g > 120 {
                cyan += 1;
            }
            if r > 8 || g > 8 || b > 8 {
                nonblack += 1;
            }
            if a == 255 {
                opaque += 1;
            }
            if x + 1 < (x0 + width).min(image.width) {
                let [nr, ng, nb, _] = image.pixels[(y * image.width + x + 1) as usize];
                edge_sum += (pixel_luma - luma(nr, ng, nb)).abs();
                edge_count += 1;
            }
        }
    }
    let total = (width * height).max(1) as f32;
    StylizedGpuMetrics {
        average_luma: luma_sum / total,
        edge_contrast: edge_sum / edge_count.max(1) as f32,
        hot_pixel_ratio: hot as f32 / total,
        cyan_pixel_ratio: cyan as f32 / total,
        nonblack_pixel_ratio: nonblack as f32 / total,
        alpha_opaque_ratio: opaque as f32 / total,
    }
}

fn luma(r: u8, g: u8, b: u8) -> f32 {
    0.2126 * r as f32 / 255.0 + 0.7152 * g as f32 / 255.0 + 0.0722 * b as f32 / 255.0
}

fn image_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::Rendering(
        litd_engine::error::RenderingError::StylizedShaderInvalidConfig {
            preset: "stylized_gpu".to_string(),
            field: field.into(),
            reason: reason.into(),
        },
    )
}
