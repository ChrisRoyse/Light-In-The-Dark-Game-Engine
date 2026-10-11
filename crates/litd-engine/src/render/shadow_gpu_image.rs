use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::EngineError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShadowGpuImage {
    pub width: u32,
    pub height: u32,
    pixels: Vec<[u8; 4]>,
}

impl ShadowGpuImage {
    pub(crate) fn from_depths(width: u32, height: u32, depths: &[f32]) -> Self {
        let pixels = depths
            .iter()
            .map(|depth| {
                let gray = if *depth >= 0.999 {
                    255
                } else {
                    (depth.clamp(0.0, 1.0) * 255.0) as u8
                };
                [gray, gray, gray, 255]
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
            shadow_image_error(
                "shadow_gpu.image.write",
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

fn shadow_image_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::ShadowInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}
