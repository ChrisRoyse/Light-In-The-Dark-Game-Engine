//! Real TTF/OTF font loading and deterministic SDF atlas generation.

use std::{fs, path::PathBuf};

use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

use crate::{EngineError, math::Vec2};

use super::ui::UiRect;

#[path = "font_store.rs"]
mod font_store;
// #1466: SDF-build/validation/hash helpers extracted to keep font.rs <500 lines.
#[path = "font_build.rs"]
mod font_build;
use font_build::*;

pub use font_store::{
    FontAtlasBmpReadback, FontAtlasJsonReadback, load_font_atlas_bmp, load_font_atlas_json,
    write_font_atlas_bmp, write_font_atlas_json,
};

pub const FONT_ATLAS_SCHEMA: &str = "litd.render.font_atlas.v1";

#[derive(Debug, Clone, PartialEq)]
pub struct FontAtlasBuildRequest {
    pub font_path: PathBuf,
    pub glyphs: Vec<char>,
    pub font_px: f32,
    pub cell_px: u32,
    pub spread_px: u32,
    pub columns: u32,
}

impl FontAtlasBuildRequest {
    pub fn new(
        font_path: impl Into<PathBuf>,
        glyphs: Vec<char>,
        font_px: f32,
        cell_px: u32,
        spread_px: u32,
        columns: u32,
    ) -> Self {
        Self {
            font_path: font_path.into(),
            glyphs,
            font_px,
            cell_px,
            spread_px,
            columns,
        }
    }

    pub fn for_text(
        font_path: impl Into<PathBuf>,
        text: &str,
        font_px: f32,
        cell_px: u32,
        spread_px: u32,
        columns: u32,
    ) -> Self {
        Self::new(
            font_path,
            text.chars().collect(),
            font_px,
            cell_px,
            spread_px,
            columns,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontGlyph {
    pub codepoint: u32,
    pub cell_x: u32,
    pub cell_y: u32,
    pub bitmap_width: u32,
    pub bitmap_height: u32,
    pub xmin: i32,
    pub ymin: i32,
    pub advance_width_px: f32,
    pub advance_height_px: f32,
    pub ink_pixels: u32,
    pub sdf_high_pixels: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontAtlas {
    pub schema: String,
    pub source_font_path: String,
    pub source_font_sha256: String,
    pub atlas_size: Vec2,
    pub glyph_size: Vec2,
    pub columns: u32,
    pub rows: u32,
    pub first_codepoint: u32,
    pub font_px: f32,
    pub spread_px: u32,
    pub glyphs: Vec<FontGlyph>,
    pub fnv1a64: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontAtlasBuild {
    pub atlas: FontAtlas,
    pub image: FontAtlasImage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontAtlasImage {
    pub width: u32,
    pub height: u32,
    pixels: Vec<u8>,
}

impl FontAtlas {
    pub fn from_font_file(request: FontAtlasBuildRequest) -> Result<FontAtlasBuild, EngineError> {
        build_sdf_font_atlas(request)
    }

    pub fn from_ttf_file(request: FontAtlasBuildRequest) -> Result<FontAtlasBuild, EngineError> {
        Self::from_font_file(request)
    }

    /// Build from compile-time font bytes after verifying their pinned source
    /// hash. Packaged hosts use this path so font availability is independent of
    /// machine-local files and tampered bytes fail before parsing.
    pub fn from_verified_font_bytes(
        request: FontAtlasBuildRequest,
        font_bytes: &[u8],
        expected_sha256: &str,
    ) -> Result<FontAtlasBuild, EngineError> {
        build_sdf_font_atlas_from_bytes(request, font_bytes, Some(expected_sha256))
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        verify_font_atlas_integrity(self)
    }

    /// UV rect for a glyph cell.
    ///
    /// The atlas is validated once when it is built (`build_sdf_font_atlas`) and
    /// once when it is loaded from disk (`load_font_atlas_json`/`_bmp`). This
    /// hot-path lookup does NOT re-validate — text layout calls it once per glyph,
    /// so re-scanning every glyph here made layout O(chars x glyphs). Call
    /// [`FontAtlas::validate`] once before a layout pass if the atlas originated
    /// from an untrusted boundary; `layout_text_run` already does.
    pub fn glyph_uv(&self, ch: char) -> Result<UiRect, EngineError> {
        let codepoint = ch as u32;
        let glyph = self.glyph(codepoint).ok_or_else(|| {
            invalid_ui_error(
                "font_atlas.glyph",
                format!("glyph U+{codepoint:04X} is not present in this atlas"),
            )
        })?;
        self.glyph_cell_uv(glyph)
    }

    /// UV rect for an already-resolved glyph cell — no lookup, no validation.
    pub fn glyph_cell_uv(&self, glyph: &FontGlyph) -> Result<UiRect, EngineError> {
        let min = Vec2::new(
            glyph.cell_x as f32 * self.glyph_size.x / self.atlas_size.x,
            glyph.cell_y as f32 * self.glyph_size.y / self.atlas_size.y,
        );
        let max = min + self.glyph_size / self.atlas_size;
        UiRect::new(min, max)
    }

    pub fn glyph(&self, codepoint: u32) -> Option<&FontGlyph> {
        self.glyphs
            .binary_search_by_key(&codepoint, |glyph| glyph.codepoint)
            .ok()
            .map(|index| &self.glyphs[index])
    }
}

impl FontAtlasImage {
    pub fn new(width: u32, height: u32) -> Result<Self, EngineError> {
        if width == 0 || height == 0 {
            return invalid_ui("font_atlas.image", "width and height must be positive");
        }
        let len = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| {
                invalid_ui_error("font_atlas.image", "width * height overflowed usize")
            })?;
        Ok(Self {
            width,
            height,
            pixels: vec![0; len],
        })
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn pixel(&self, x: u32, y: u32) -> Result<u8, EngineError> {
        if x >= self.width || y >= self.height {
            return invalid_ui("font_atlas.pixel", "coordinate is outside the atlas image");
        }
        Ok(self.pixels[(y * self.width + x) as usize])
    }

    pub fn count_cell_pixels_at_or_above(
        &self,
        glyph: &FontGlyph,
        cell_px: u32,
        threshold: u8,
    ) -> u32 {
        let mut count = 0_u32;
        let x0 = glyph.cell_x * cell_px;
        let y0 = glyph.cell_y * cell_px;
        for y in y0..y0 + cell_px {
            for x in x0..x0 + cell_px {
                if self.pixels[(y * self.width + x) as usize] >= threshold {
                    count += 1;
                }
            }
        }
        count
    }

    pub fn bmp_bytes(&self) -> Vec<u8> {
        let row_stride = ((self.width as usize).saturating_mul(3)).div_ceil(4) * 4;
        let pixel_bytes = row_stride
            .checked_mul(self.height as usize)
            .expect("validated FontAtlasImage dimensions must fit BMP bytes");
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
                let value = self.pixels[(y * self.width + x) as usize];
                bytes.extend_from_slice(&[value, value, value]);
            }
            bytes.extend(std::iter::repeat_n(0, padding));
        }
        bytes
    }

    fn write_cell(&mut self, cell_x: u32, cell_y: u32, cell_px: u32, cell: &[u8]) {
        let origin_x = cell_x * cell_px;
        let origin_y = cell_y * cell_px;
        for y in 0..cell_px {
            let dst = ((origin_y + y) * self.width + origin_x) as usize;
            let src = (y * cell_px) as usize;
            self.pixels[dst..dst + cell_px as usize]
                .copy_from_slice(&cell[src..src + cell_px as usize]);
        }
    }
}

pub fn build_sdf_font_atlas(request: FontAtlasBuildRequest) -> Result<FontAtlasBuild, EngineError> {
    let font_bytes = fs::read(&request.font_path).map_err(|error| {
        invalid_ui_error(
            "font.path",
            format!("failed to read {}: {error}", request.font_path.display()),
        )
    })?;
    build_sdf_font_atlas_from_bytes(request, &font_bytes, None)
}

pub fn verify_font_atlas_integrity(atlas: &FontAtlas) -> Result<(), EngineError> {
    if atlas.schema != FONT_ATLAS_SCHEMA {
        return invalid_ui("font_atlas.schema", "must be litd.render.font_atlas.v1");
    }
    if !atlas.atlas_size.is_finite()
        || !atlas.glyph_size.is_finite()
        || atlas.atlas_size.cmple(Vec2::ZERO).any()
        || atlas.glyph_size.cmple(Vec2::ZERO).any()
        || !atlas.font_px.is_finite()
        || atlas.font_px <= 0.0
        || atlas.columns == 0
        || atlas.rows == 0
        || atlas.glyphs.is_empty()
    {
        return invalid_ui(
            "font_atlas",
            "sizes, grid, font_px, and glyphs must be valid",
        );
    }
    let capacity = atlas
        .columns
        .checked_mul(atlas.rows)
        .ok_or_else(|| invalid_ui_error("font_atlas.capacity", "columns * rows overflowed"))?;
    if atlas.glyphs.len() as u32 > capacity {
        return invalid_ui("font_atlas.glyphs", "glyph count exceeds grid capacity");
    }
    let mut previous = None;
    for (index, glyph) in atlas.glyphs.iter().enumerate() {
        if previous.is_some_and(|prev| glyph.codepoint <= prev) {
            return invalid_ui(
                "font_atlas.glyphs",
                "glyph codepoints must be sorted unique",
            );
        }
        if char::from_u32(glyph.codepoint).is_none() {
            return invalid_ui(
                "font_atlas.glyph.codepoint",
                "codepoint must be a valid char",
            );
        }
        if glyph.cell_x != index as u32 % atlas.columns
            || glyph.cell_y != index as u32 / atlas.columns
            || !glyph.advance_width_px.is_finite()
            || !glyph.advance_height_px.is_finite()
        {
            return invalid_ui(
                "font_atlas.glyph",
                "cell positions and advances must be valid",
            );
        }
        previous = Some(glyph.codepoint);
    }
    if atlas.first_codepoint != atlas.glyphs[0].codepoint {
        return invalid_ui(
            "font_atlas.first_codepoint",
            "must match the first sorted glyph",
        );
    }
    let actual = compute_font_atlas_fnv1a64(atlas);
    if actual != atlas.fnv1a64 {
        return Err(EngineError::UiRenderReadbackMismatch {
            field: "font_atlas.fnv1a64".to_string(),
            expected: atlas.fnv1a64.clone(),
            actual,
        });
    }
    Ok(())
}

pub fn compute_font_atlas_fnv1a64(atlas: &FontAtlas) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, &atlas.schema);
    update_str(&mut hasher, &atlas.source_font_sha256);
    hash_f32(&mut hasher, atlas.atlas_size.x);
    hash_f32(&mut hasher, atlas.atlas_size.y);
    hash_f32(&mut hasher, atlas.glyph_size.x);
    hash_f32(&mut hasher, atlas.glyph_size.y);
    hasher.update_u64_le(atlas.columns as u64);
    hasher.update_u64_le(atlas.rows as u64);
    hasher.update_u64_le(atlas.first_codepoint as u64);
    hash_f32(&mut hasher, atlas.font_px);
    hasher.update_u64_le(atlas.spread_px as u64);
    hasher.update_u64_le(atlas.glyphs.len() as u64);
    for glyph in &atlas.glyphs {
        hasher.update_u64_le(glyph.codepoint as u64);
        hasher.update_u64_le(glyph.cell_x as u64);
        hasher.update_u64_le(glyph.cell_y as u64);
        hasher.update_u64_le(glyph.bitmap_width as u64);
        hasher.update_u64_le(glyph.bitmap_height as u64);
        hasher.update_i64_le(i64::from(glyph.xmin));
        hasher.update_i64_le(i64::from(glyph.ymin));
        hash_f32(&mut hasher, glyph.advance_width_px);
        hash_f32(&mut hasher, glyph.advance_height_px);
        hasher.update_u64_le(glyph.ink_pixels as u64);
        hasher.update_u64_le(glyph.sdf_high_pixels as u64);
    }
    hasher.finish_hex()
}

pub fn reference_windows_font_path() -> PathBuf {
    PathBuf::from(r"C:\Windows\Fonts\arial.ttf")
}

pub fn ui_reference_text_atlas_request() -> FontAtlasBuildRequest {
    FontAtlasBuildRequest::for_text(
        reference_windows_font_path(),
        "Ashen Reach245",
        40.0,
        64,
        8,
        8,
    )
}

#[cfg(test)]
#[path = "font_tests.rs"]
mod tests;
