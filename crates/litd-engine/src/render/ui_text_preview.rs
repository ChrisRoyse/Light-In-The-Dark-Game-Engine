//! Deterministic BMP preview/readback for shaped UI glyph quads.

use std::{
    fs,
    path::{Path, PathBuf},
};

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::{EngineError, math::Vec4};

use super::{GlyphQuad, ShapedTextAtlasImage, UiRect};

const BACKGROUND: [u8; 4] = [8, 10, 14, 255];
const MAX_DIMENSION: u32 = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapedGlyphHit {
    pub glyph_id: u32,
    pub codepoint: u32,
    pub non_background_pixels: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapedTextPreviewReadback {
    pub bmp_path: PathBuf,
    pub bmp_bytes: u64,
    pub bmp_sha256: String,
    pub width: u32,
    pub height: u32,
    pub non_background_pixels: u32,
    pub glyph_rect_hits: Vec<ShapedGlyphHit>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapedTextPreviewImage {
    pub width: u32,
    pub height: u32,
    pixels: Vec<[u8; 4]>,
}

impl ShapedTextPreviewImage {
    pub fn pixels(&self) -> &[[u8; 4]] {
        &self.pixels
    }

    pub fn from_bmp_bytes(bytes: &[u8]) -> Result<Self, EngineError> {
        if bytes.len() < 54 || &bytes[0..2] != b"BM" {
            return invalid_preview("shaped_text_preview.bmp", "BMP header is missing");
        }
        let data_offset = read_u32(bytes, 10)? as usize;
        let width = read_i32(bytes, 18)?;
        let height = read_i32(bytes, 22)?;
        let bits_per_pixel = read_u16(bytes, 28)?;
        if width <= 0 || height <= 0 || bits_per_pixel != 24 || data_offset != 54 {
            return invalid_preview("shaped_text_preview.bmp", "expected positive 24-bit BMP");
        }
        let width = width as u32;
        let height = height as u32;
        validate_dimensions(width, height)?;
        let row_stride = (width as usize * 3).div_ceil(4) * 4;
        let expected_len = data_offset + row_stride * height as usize;
        if bytes.len() < expected_len {
            return invalid_preview("shaped_text_preview.bmp", "BMP payload is truncated");
        }
        let mut pixels = vec![BACKGROUND; width as usize * height as usize];
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

    fn new(width: u32, height: u32) -> Result<Self, EngineError> {
        validate_dimensions(width, height)?;
        Ok(Self {
            width,
            height,
            pixels: vec![BACKGROUND; width as usize * height as usize],
        })
    }

    fn set(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = color;
        }
    }

    fn non_background_pixels(&self) -> u32 {
        self.pixels
            .iter()
            .filter(|pixel| **pixel != BACKGROUND)
            .count() as u32
    }

    fn rect_non_background_pixels(&self, rect: UiRect) -> u32 {
        let (x0, y0, x1, y1) = clamped_rect(rect, self.width, self.height);
        let mut count = 0_u32;
        for y in y0..y1 {
            for x in x0..x1 {
                if self.pixels[(y * self.width + x) as usize] != BACKGROUND {
                    count += 1;
                }
            }
        }
        count
    }
}

pub fn render_shaped_text_preview_image(
    width: u32,
    height: u32,
    quads: &[GlyphQuad],
    atlas: &ShapedTextAtlasImage,
) -> Result<ShapedTextPreviewImage, EngineError> {
    if quads.is_empty() {
        return invalid_preview(
            "shaped_text_preview.quads",
            "at least one glyph quad is required",
        );
    }
    validate_dimensions(width, height)?;
    let mut image = ShapedTextPreviewImage::new(width, height)?;
    for quad in quads {
        draw_quad(&mut image, quad, atlas)?;
    }
    Ok(image)
}

pub fn write_shaped_text_preview_bmp(
    path: impl AsRef<Path>,
    image: &ShapedTextPreviewImage,
    quads: &[GlyphQuad],
) -> Result<ShapedTextPreviewReadback, EngineError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            preview_error(
                "shaped_text_preview.parent",
                format!("failed to create {}: {error}", parent.display()),
            )
        })?;
    }
    fs::write(path, image.bmp_bytes()).map_err(|error| {
        preview_error(
            "shaped_text_preview.write",
            format!("failed to write {}: {error}", path.display()),
        )
    })?;
    let bytes = fs::read(path).map_err(|error| {
        preview_error(
            "shaped_text_preview.readback",
            format!("failed to read {}: {error}", path.display()),
        )
    })?;
    let decoded = ShapedTextPreviewImage::from_bmp_bytes(&bytes)?;
    let glyph_rect_hits = quads
        .iter()
        .map(|quad| ShapedGlyphHit {
            glyph_id: quad.glyph_id,
            codepoint: quad.codepoint,
            non_background_pixels: decoded.rect_non_background_pixels(quad.rect_px),
        })
        .collect::<Vec<_>>();
    Ok(ShapedTextPreviewReadback {
        bmp_path: path.to_path_buf(),
        bmp_bytes: bytes.len() as u64,
        bmp_sha256: sha256_hex(&bytes),
        width: decoded.width,
        height: decoded.height,
        non_background_pixels: decoded.non_background_pixels(),
        glyph_rect_hits,
    })
}

fn draw_quad(
    image: &mut ShapedTextPreviewImage,
    quad: &GlyphQuad,
    atlas: &ShapedTextAtlasImage,
) -> Result<(), EngineError> {
    quad.rect_px.validate("shaped_text_preview.rect")?;
    quad.uv.validate("shaped_text_preview.uv")?;
    let (x0, y0, x1, y1) = clamped_rect(quad.rect_px, image.width, image.height);
    if x0 >= x1 || y0 >= y1 {
        return invalid_preview(
            "shaped_text_preview.rect",
            "glyph rect is outside preview bounds",
        );
    }
    let color = rgba8(quad.color)?;
    let size = quad.rect_px.size();
    let uv_size = quad.uv.size();
    for y in y0..y1 {
        for x in x0..x1 {
            let local_x = ((x as f32 + 0.5) - quad.rect_px.min.x) / size.x;
            let local_y = ((y as f32 + 0.5) - quad.rect_px.min.y) / size.y;
            let uv_x = quad.uv.min.x + local_x.clamp(0.0, 1.0) * uv_size.x;
            let uv_y = quad.uv.min.y + local_y.clamp(0.0, 1.0) * uv_size.y;
            let atlas_x = ((uv_x * atlas.width as f32).floor() as i32)
                .clamp(0, atlas.width as i32 - 1) as u32;
            let atlas_y = ((uv_y * atlas.height as f32).floor() as i32)
                .clamp(0, atlas.height as i32 - 1) as u32;
            let distance = atlas.pixel(atlas_x, atlas_y)? as f32 / 255.0;
            let alpha = smoothstep(0.42, 0.58, distance);
            if alpha > 0.0 {
                image.set(x, y, color);
            }
        }
    }
    Ok(())
}

fn clamped_rect(rect: UiRect, width: u32, height: u32) -> (u32, u32, u32, u32) {
    let x0 = rect.min.x.floor().max(0.0).min(width as f32) as u32;
    let y0 = rect.min.y.floor().max(0.0).min(height as f32) as u32;
    let x1 = rect.max.x.ceil().max(0.0).min(width as f32) as u32;
    let y1 = rect.max.y.ceil().max(0.0).min(height as f32) as u32;
    (x0, y0, x1, y1)
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), EngineError> {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return invalid_preview(
            "shaped_text_preview.dimensions",
            "width and height must be inside 1..=4096",
        );
    }
    Ok(())
}

fn rgba8(color: Vec4) -> Result<[u8; 4], EngineError> {
    if !color.is_finite() || color.cmplt(Vec4::ZERO).any() || color.cmpgt(Vec4::ONE).any() {
        return invalid_preview(
            "shaped_text_preview.color",
            "RGBA must be finite inside 0..=1",
        );
    }
    Ok([
        (color.x * 255.0).round() as u8,
        (color.y * 255.0).round() as u8,
        (color.z * 255.0).round() as u8,
        (color.w * 255.0).round() as u8,
    ])
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, EngineError> {
    bytes
        .get(offset..offset + 2)
        .and_then(|slice| slice.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| preview_error("shaped_text_preview.bmp", "u16 field is truncated"))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, EngineError> {
    bytes
        .get(offset..offset + 4)
        .and_then(|slice| slice.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| preview_error("shaped_text_preview.bmp", "u32 field is truncated"))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, EngineError> {
    bytes
        .get(offset..offset + 4)
        .and_then(|slice| slice.try_into().ok())
        .map(i32::from_le_bytes)
        .ok_or_else(|| preview_error("shaped_text_preview.bmp", "i32 field is truncated"))
}

fn invalid_preview<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(preview_error(field, reason))
}

fn preview_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}
