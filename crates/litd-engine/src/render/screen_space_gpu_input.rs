use serde::{Deserialize, Serialize};

use crate::EngineError;

use super::{screen_space_gpu::invalid_screen_gpu, screen_space_gpu_image::ScreenSpaceGpuImage};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenSpaceGpuInputImage {
    pub width: u32,
    pub height: u32,
    color_pixels: Vec<[f32; 4]>,
    history_pixels: Vec<[f32; 4]>,
    depth_pixels: Vec<f32>,
    motion_pixels: Vec<[f32; 2]>,
}

impl ScreenSpaceGpuInputImage {
    pub fn from_scene_buffers(
        width: u32,
        height: u32,
        color_pixels: Vec<[f32; 4]>,
        history_pixels: Vec<[f32; 4]>,
        depth_pixels: Vec<f32>,
        motion_pixels: Vec<[f32; 2]>,
    ) -> Result<Self, EngineError> {
        let input = Self {
            width,
            height,
            color_pixels,
            history_pixels,
            depth_pixels,
            motion_pixels,
        };
        super::screen_space_gpu::validate_input(&input)?;
        Ok(input)
    }

    pub fn synthetic_scene(width: u32, height: u32) -> Result<Self, EngineError> {
        if width == 0 || height == 0 {
            return invalid_screen_gpu(
                "screen_space_gpu.synthetic_size",
                "width and height must be non-zero",
            );
        }
        let mut color_pixels = Vec::with_capacity((width * height) as usize);
        let mut history_pixels = Vec::with_capacity((width * height) as usize);
        let mut depth_pixels = Vec::with_capacity((width * height) as usize);
        let mut motion_pixels = Vec::with_capacity((width * height) as usize);
        let hero_center = (width as f32 * 0.46, height as f32 * 0.52);
        let pool_center = (width as f32 * 0.72, height as f32 * 0.66);
        for y in 0..height {
            for x in 0..width {
                let nx = x as f32 / (width - 1).max(1) as f32;
                let ny = y as f32 / (height - 1).max(1) as f32;
                let wall = if x > width / 2 { 0.34 } else { 0.0 };
                let hero = radial((x as f32, y as f32), hero_center, width as f32 * 0.13);
                let pool = radial((x as f32, y as f32), pool_center, width as f32 * 0.18);
                let color = [
                    (0.08 + nx * 0.15 + wall + hero * 0.38).clamp(0.0, 1.0),
                    (0.11 + ny * 0.14 + wall * 0.62 + hero * 0.23 + pool * 0.12).clamp(0.0, 1.0),
                    (0.16 + (1.0 - nx) * 0.10 + wall * 0.20 + pool * 0.42).clamp(0.0, 1.0),
                    1.0,
                ];
                color_pixels.push(color);
                history_pixels.push([
                    (color[0] * 0.91 + 0.025).clamp(0.0, 1.0),
                    (color[1] * 0.95 + 0.010).clamp(0.0, 1.0),
                    (color[2] * 0.98 + 0.015).clamp(0.0, 1.0),
                    1.0,
                ]);
                depth_pixels.push((0.72 - hero * 0.34 - wall * 0.20 + pool * 0.08).clamp(0.0, 1.0));
                motion_pixels.push([
                    ((nx - 0.5) * 0.045 + hero * 0.020).clamp(-1.0, 1.0),
                    ((0.5 - ny) * 0.030 - hero * 0.015).clamp(-1.0, 1.0),
                ]);
            }
        }
        Self::from_scene_buffers(
            width,
            height,
            color_pixels,
            history_pixels,
            depth_pixels,
            motion_pixels,
        )
    }

    pub fn color_pixels(&self) -> &[[f32; 4]] {
        &self.color_pixels
    }

    pub fn history_pixels(&self) -> &[[f32; 4]] {
        &self.history_pixels
    }

    pub fn depth_pixels(&self) -> &[f32] {
        &self.depth_pixels
    }

    pub fn motion_pixels(&self) -> &[[f32; 2]] {
        &self.motion_pixels
    }

    pub fn write_color_bmp(&self, path: impl AsRef<std::path::Path>) -> Result<(), EngineError> {
        ScreenSpaceGpuImage::from_float_pixels(self.width, self.height, &self.color_pixels)
            .write_bmp(path)
    }
}

fn radial(point: (f32, f32), center: (f32, f32), radius: f32) -> f32 {
    let dx = point.0 - center.0;
    let dy = point.1 - center.1;
    (1.0 - (dx * dx + dy * dy).sqrt() / radius).clamp(0.0, 1.0)
}
