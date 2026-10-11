use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::EngineError;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ForwardPlusGpuMetrics {
    pub average_luma: f32,
    pub lit_pixel_ratio: f32,
    pub hot_pixel_ratio: f32,
    pub blue_pixel_ratio: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForwardPlusGpuImage {
    pub width: u32,
    pub height: u32,
    pixels: Vec<[u8; 4]>,
}

impl ForwardPlusGpuImage {
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
                "forward_plus.image.write",
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

pub(super) fn measure(image: &ForwardPlusGpuImage) -> ForwardPlusGpuMetrics {
    let mut luma_sum = 0.0;
    let mut lit = 0_u32;
    let mut hot = 0_u32;
    let mut blue = 0_u32;
    for &[r, g, b, _] in image.pixels() {
        let rf = r as f32 / 255.0;
        let gf = g as f32 / 255.0;
        let bf = b as f32 / 255.0;
        let luma = 0.2126 * rf + 0.7152 * gf + 0.0722 * bf;
        luma_sum += luma;
        if luma > 0.05 {
            lit += 1;
        }
        if rf > 0.38 && gf > 0.10 {
            hot += 1;
        }
        if bf > 0.28 && bf > rf {
            blue += 1;
        }
    }
    let total = image.pixels().len().max(1) as f32;
    ForwardPlusGpuMetrics {
        average_luma: luma_sum / total,
        lit_pixel_ratio: lit as f32 / total,
        hot_pixel_ratio: hot as f32 / total,
        blue_pixel_ratio: blue as f32 / total,
    }
}

fn image_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::Rendering(
        litd_engine::error::RenderingError::ClusteredLightingInvalidConfig {
            field: field.into(),
            reason: reason.into(),
        },
    )
}
