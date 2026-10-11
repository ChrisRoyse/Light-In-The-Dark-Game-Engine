//! Shaped UI text placement over real font glyph IDs.

use std::{fs, path::Path};

use fontdue::{Font, FontSettings};
use litd_hash::{Fnv1a64, sha256_hex};
use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Vec2, Vec4},
    render::text_shaping::{ShapedTextRun, TextShapeDirection, verify_shaped_text_run_integrity},
};

use super::{GlyphQuad, UiRect};

#[path = "ui_text_preview.rs"]
mod ui_text_preview;
#[path = "ui_text_support.rs"]
mod ui_text_support;

pub use ui_text_preview::{
    ShapedGlyphHit, ShapedTextPreviewImage, ShapedTextPreviewReadback,
    render_shaped_text_preview_image, write_shaped_text_preview_bmp,
};
use ui_text_support::{
    hash_f32, invalid_ui, sdf_cell, sorted_unique_glyph_ids, ui_error, update_str,
    validate_atlas_request, validate_placement,
};

pub const SHAPED_TEXT_ATLAS_SCHEMA: &str = "litd.render.ui_shaped_text_atlas.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapedTextAtlasGlyph {
    pub glyph_id: u32,
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
pub struct ShapedTextAtlas {
    pub schema: String,
    pub source_font_path: String,
    pub source_font_sha256: String,
    pub font_px: f32,
    pub atlas_size: Vec2,
    pub glyph_size: Vec2,
    pub cell_px: u32,
    pub spread_px: u32,
    pub columns: u32,
    pub rows: u32,
    pub glyphs: Vec<ShapedTextAtlasGlyph>,
    pub fnv1a64: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapedTextAtlasBuild {
    pub atlas: ShapedTextAtlas,
    pub image: ShapedTextAtlasImage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapedTextAtlasImage {
    pub width: u32,
    pub height: u32,
    pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapedTextRunPlacement {
    pub id: String,
    pub run: ShapedTextRun,
    pub atlas: ShapedTextAtlas,
    pub origin_px: Vec2,
    pub color: Vec4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapedTextLayoutProbe {
    pub direction: TextShapeDirection,
    pub max_chars_per_line: usize,
    pub line_count: usize,
    pub max_line_chars: usize,
    pub overflow: bool,
}

impl ShapedTextAtlas {
    pub fn validate(&self) -> Result<(), EngineError> {
        verify_shaped_text_atlas_integrity(self)
    }

    pub fn glyph(&self, glyph_id: u32) -> Option<&ShapedTextAtlasGlyph> {
        self.glyphs
            .binary_search_by_key(&glyph_id, |glyph| glyph.glyph_id)
            .ok()
            .map(|index| &self.glyphs[index])
    }

    /// UV rect for a shaped glyph cell.
    ///
    /// Validated once at build and once in `validate_placement` before layout; this
    /// hot-path lookup does not re-validate (per-glyph revalidation made shaped
    /// layout O(glyphs x atlas_glyphs)). Callers using an atlas from an untrusted
    /// boundary must call [`ShapedTextAtlas::validate`] once first.
    pub fn glyph_uv(&self, glyph_id: u32) -> Result<UiRect, EngineError> {
        let glyph = self.glyph(glyph_id).ok_or_else(|| {
            ui_error(
                "shaped_text_atlas.glyph",
                format!("glyph id {glyph_id} is not present in this atlas"),
            )
        })?;
        let min = Vec2::new(
            glyph.cell_x as f32 * self.glyph_size.x / self.atlas_size.x,
            glyph.cell_y as f32 * self.glyph_size.y / self.atlas_size.y,
        );
        UiRect::new(min, min + self.glyph_size / self.atlas_size)
    }
}

impl ShapedTextAtlasImage {
    pub fn new(width: u32, height: u32) -> Result<Self, EngineError> {
        if width == 0 || height == 0 {
            return invalid_ui(
                "shaped_text_atlas.image",
                "width and height must be positive",
            );
        }
        let len = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| ui_error("shaped_text_atlas.image", "width * height overflowed"))?;
        Ok(Self {
            width,
            height,
            pixels: vec![0; len],
        })
    }

    pub fn pixel(&self, x: u32, y: u32) -> Result<u8, EngineError> {
        if x >= self.width || y >= self.height {
            return invalid_ui(
                "shaped_text_atlas.pixel",
                "coordinate is outside the atlas image",
            );
        }
        Ok(self.pixels[(y * self.width + x) as usize])
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
pub fn build_shaped_text_atlas(
    run: &ShapedTextRun,
    cell_px: u32,
    spread_px: u32,
    columns: u32,
) -> Result<ShapedTextAtlasBuild, EngineError> {
    verify_shaped_text_run_integrity(run)?;
    validate_atlas_request(cell_px, spread_px, columns)?;
    let font_bytes = fs::read(Path::new(&run.source_font_path)).map_err(|error| {
        ui_error(
            "shaped_text.font_path",
            format!("failed to read {}: {error}", run.source_font_path),
        )
    })?;
    let actual_sha = sha256_hex(&font_bytes);
    if actual_sha != run.source_font_sha256 {
        return invalid_ui(
            "shaped_text.font_sha256",
            "font bytes must match the shaped run source font hash",
        );
    }
    let font = Font::from_bytes(
        font_bytes.as_slice(),
        FontSettings {
            scale: run.font_px,
            ..FontSettings::default()
        },
    )
    .map_err(|error| ui_error("shaped_text.font_parse", error))?;
    let glyph_ids = sorted_unique_glyph_ids(run);
    let rows = (glyph_ids.len() as u32).div_ceil(columns);
    let width = columns
        .checked_mul(cell_px)
        .ok_or_else(|| ui_error("shaped_text_atlas.width", "columns * cell_px overflowed"))?;
    let height = rows
        .checked_mul(cell_px)
        .ok_or_else(|| ui_error("shaped_text_atlas.height", "rows * cell_px overflowed"))?;
    let mut image = ShapedTextAtlasImage::new(width, height)?;
    let mut atlas_glyphs = Vec::with_capacity(glyph_ids.len());
    for (index, glyph_id) in glyph_ids.into_iter().enumerate() {
        let glyph_index = u16::try_from(glyph_id)
            .map_err(|_| ui_error("shaped_text.glyph_id", "glyph id exceeds u16"))?;
        if glyph_index >= font.glyph_count() {
            return invalid_ui(
                "shaped_text.glyph_id",
                format!(
                    "glyph id {glyph_id} exceeds font glyph count {}",
                    font.glyph_count()
                ),
            );
        }
        let cell_x = index as u32 % columns;
        let cell_y = index as u32 / columns;
        let (metrics, bitmap) = font.rasterize_indexed(glyph_index, run.font_px);
        let cell = sdf_cell(cell_px, spread_px, glyph_id, metrics, &bitmap)?;
        let sdf_high_pixels = cell.iter().filter(|value| **value >= 128).count() as u32;
        image.write_cell(cell_x, cell_y, cell_px, &cell);
        atlas_glyphs.push(ShapedTextAtlasGlyph {
            glyph_id,
            cell_x,
            cell_y,
            bitmap_width: metrics.width as u32,
            bitmap_height: metrics.height as u32,
            xmin: metrics.xmin,
            ymin: metrics.ymin,
            advance_width_px: metrics.advance_width,
            advance_height_px: metrics.advance_height,
            ink_pixels: bitmap.iter().filter(|value| **value > 0).count() as u32,
            sdf_high_pixels,
        });
    }
    let mut atlas = ShapedTextAtlas {
        schema: SHAPED_TEXT_ATLAS_SCHEMA.to_string(),
        source_font_path: run.source_font_path.clone(),
        source_font_sha256: run.source_font_sha256.clone(),
        font_px: run.font_px,
        atlas_size: Vec2::new(width as f32, height as f32),
        glyph_size: Vec2::splat(cell_px as f32),
        cell_px,
        spread_px,
        columns,
        rows,
        glyphs: atlas_glyphs,
        fnv1a64: String::new(),
    };
    atlas.fnv1a64 = compute_shaped_text_atlas_fnv1a64(&atlas);
    verify_shaped_text_atlas_integrity(&atlas)?;
    Ok(ShapedTextAtlasBuild { atlas, image })
}

pub fn layout_shaped_text_run(
    placement: &ShapedTextRunPlacement,
) -> Result<Vec<GlyphQuad>, EngineError> {
    validate_placement(placement)?;
    let mut quads = Vec::with_capacity(placement.run.glyphs.len());
    for glyph in &placement.run.glyphs {
        let atlas_glyph = placement.atlas.glyph(glyph.glyph_id).ok_or_else(|| {
            ui_error(
                "shaped_text_atlas.glyph",
                format!("glyph id {} is missing from atlas", glyph.glyph_id),
            )
        })?;
        let baseline = placement.origin_px + Vec2::new(glyph.x_px, placement.run.font_px);
        let width = (atlas_glyph.bitmap_width.max(1)) as f32;
        let height = (atlas_glyph.bitmap_height.max(1)) as f32;
        let min = Vec2::new(
            baseline.x + atlas_glyph.xmin as f32,
            baseline.y - atlas_glyph.ymin as f32 - height,
        );
        let rect = UiRect::new(min, min + Vec2::new(width, height))?;
        quads.push(GlyphQuad {
            rect_px: rect,
            uv: placement.atlas.glyph_uv(glyph.glyph_id)?,
            color: placement.color,
            codepoint: glyph.source_text.chars().next().unwrap_or('\0') as u32,
            glyph_id: glyph.glyph_id,
        });
    }
    Ok(quads)
}

pub fn measure_shaped_text_layout(
    run: &ShapedTextRun,
    max_chars_per_line: usize,
) -> Result<ShapedTextLayoutProbe, EngineError> {
    verify_shaped_text_run_integrity(run)?;
    if max_chars_per_line == 0 {
        return invalid_ui(
            "shaped_text_layout.max_chars_per_line",
            "must be greater than zero",
        );
    }
    let mut line_count = 1_usize;
    let mut current = 0_usize;
    let mut max_line_chars = 0_usize;
    for word in run.text.split_whitespace() {
        let width = word.chars().count();
        if current == 0 {
            current = width;
        } else if current + 1 + width <= max_chars_per_line {
            current += 1 + width;
        } else {
            max_line_chars = max_line_chars.max(current);
            line_count += 1;
            current = width;
        }
    }
    max_line_chars = max_line_chars.max(current);
    Ok(ShapedTextLayoutProbe {
        direction: run.resolved_direction,
        max_chars_per_line,
        line_count,
        max_line_chars,
        overflow: max_line_chars > max_chars_per_line,
    })
}

pub fn verify_shaped_text_atlas_integrity(atlas: &ShapedTextAtlas) -> Result<(), EngineError> {
    if atlas.schema != SHAPED_TEXT_ATLAS_SCHEMA {
        return invalid_ui(
            "shaped_text_atlas.schema",
            "must be litd.render.ui_shaped_text_atlas.v1",
        );
    }
    if !atlas.font_px.is_finite()
        || atlas.font_px <= 0.0
        || !atlas.atlas_size.is_finite()
        || !atlas.glyph_size.is_finite()
        || atlas.atlas_size.cmple(Vec2::ZERO).any()
        || atlas.glyph_size.cmple(Vec2::ZERO).any()
        || atlas.cell_px == 0
        || atlas.spread_px == 0
        || atlas.columns == 0
        || atlas.rows == 0
        || atlas.glyphs.is_empty()
    {
        return invalid_ui(
            "shaped_text_atlas",
            "sizes, grid, font_px, and glyphs must be valid",
        );
    }
    let mut previous = None;
    for (index, glyph) in atlas.glyphs.iter().enumerate() {
        if previous.is_some_and(|prev| glyph.glyph_id <= prev) {
            return invalid_ui(
                "shaped_text_atlas.glyphs",
                "glyph ids must be sorted unique",
            );
        }
        if glyph.glyph_id == 0
            || glyph.cell_x != index as u32 % atlas.columns
            || glyph.cell_y != index as u32 / atlas.columns
            || !glyph.advance_width_px.is_finite()
            || !glyph.advance_height_px.is_finite()
        {
            return invalid_ui(
                "shaped_text_atlas.glyph",
                "glyph id, cell positions, and advances must be valid",
            );
        }
        previous = Some(glyph.glyph_id);
    }
    let actual = compute_shaped_text_atlas_fnv1a64(atlas);
    if actual != atlas.fnv1a64 {
        return Err(EngineError::UiRenderReadbackMismatch {
            field: "shaped_text_atlas.fnv1a64".to_string(),
            expected: atlas.fnv1a64.clone(),
            actual,
        });
    }
    Ok(())
}

pub fn compute_shaped_text_atlas_fnv1a64(atlas: &ShapedTextAtlas) -> String {
    let mut hasher = Fnv1a64::new();
    update_str(&mut hasher, &atlas.schema);
    update_str(&mut hasher, &atlas.source_font_sha256);
    hash_f32(&mut hasher, atlas.font_px);
    hasher.update_u64_le(atlas.cell_px as u64);
    hasher.update_u64_le(atlas.spread_px as u64);
    hasher.update_u64_le(atlas.columns as u64);
    hasher.update_u64_le(atlas.rows as u64);
    for glyph in &atlas.glyphs {
        hasher.update_u64_le(glyph.glyph_id as u64);
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
