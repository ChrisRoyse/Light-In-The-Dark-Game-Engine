//! SDF-atlas build helpers extracted from `font.rs` for the #1466 / #800 sec.6 <500-line doctrine.
//!
//! Rasterize-to-SDF cell packing, request validation, deterministic glyph ordering, the
//! UiRenderInvalidConfig error constructors, and the FNV-1a hashing primitives. `FontAtlas`,
//! `build_sdf_font_atlas`, and `verify_font_atlas_integrity` stay in `font`; these are the
//! crate-private (`pub(super)`) helpers it calls, brought back into `font` via `use font_build::*`.

use std::collections::BTreeSet;

use fontdue::{Font, FontSettings, Metrics};
use litd_hash::{Fnv1a64, sha256_hex};

use super::{
    FONT_ATLAS_SCHEMA, FontAtlas, FontAtlasBuild, FontAtlasBuildRequest, FontAtlasImage, FontGlyph,
    compute_font_atlas_fnv1a64, verify_font_atlas_integrity,
};
use crate::EngineError;
use crate::math::Vec2;
use crate::render::font_sdf;

pub(super) fn build_sdf_font_atlas_from_bytes(
    request: FontAtlasBuildRequest,
    font_bytes: &[u8],
    expected_sha256: Option<&str>,
) -> Result<FontAtlasBuild, EngineError> {
    validate_request(&request)?;
    let glyphs = sorted_unique_glyphs(&request)?;
    if font_bytes.is_empty() {
        return invalid_ui("font.bytes", "font bytes must be non-empty");
    }
    let source_font_sha256 = sha256_hex(font_bytes);
    if let Some(expected) = expected_sha256
        && source_font_sha256 != expected
    {
        return Err(EngineError::UiRenderReadbackMismatch {
            field: "font.source_sha256".to_string(),
            expected: expected.to_string(),
            actual: source_font_sha256,
        });
    }
    let settings = FontSettings {
        scale: request.font_px,
        ..FontSettings::default()
    };
    let font = Font::from_bytes(font_bytes, settings)
        .map_err(|error| invalid_ui_error("font.parse", error))?;
    for ch in &glyphs {
        if !font.has_glyph(*ch) {
            return invalid_ui(
                format!("font.glyph.U+{:04X}", *ch as u32),
                "requested glyph is absent; choose a font with coverage instead of using tofu",
            );
        }
    }

    let rows = (glyphs.len() as u32).div_ceil(request.columns);
    let width = request
        .columns
        .checked_mul(request.cell_px)
        .ok_or_else(|| invalid_ui_error("font_atlas.width", "columns * cell_px overflowed u32"))?;
    let height = rows
        .checked_mul(request.cell_px)
        .ok_or_else(|| invalid_ui_error("font_atlas.height", "rows * cell_px overflowed u32"))?;
    let mut image = FontAtlasImage::new(width, height)?;
    let mut atlas_glyphs = Vec::with_capacity(glyphs.len());
    for (index, ch) in glyphs.iter().copied().enumerate() {
        let cell_x = index as u32 % request.columns;
        let cell_y = index as u32 / request.columns;
        let (metrics, bitmap) = font.rasterize(ch, request.font_px);
        let cell = raster_to_sdf_cell(&request, ch, metrics, &bitmap)?;
        let sdf_high_pixels = cell.iter().filter(|value| **value >= 128).count() as u32;
        image.write_cell(cell_x, cell_y, request.cell_px, &cell);
        atlas_glyphs.push(FontGlyph {
            codepoint: ch as u32,
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

    let mut atlas = FontAtlas {
        schema: FONT_ATLAS_SCHEMA.to_string(),
        source_font_path: request.font_path.display().to_string(),
        source_font_sha256,
        atlas_size: Vec2::new(width as f32, height as f32),
        glyph_size: Vec2::splat(request.cell_px as f32),
        columns: request.columns,
        rows,
        first_codepoint: atlas_glyphs
            .first()
            .map(|glyph| glyph.codepoint)
            .unwrap_or(0),
        font_px: request.font_px,
        spread_px: request.spread_px,
        glyphs: atlas_glyphs,
        fnv1a64: String::new(),
    };
    atlas.fnv1a64 = compute_font_atlas_fnv1a64(&atlas);
    verify_font_atlas_integrity(&atlas)?;
    Ok(FontAtlasBuild { atlas, image })
}

pub(super) fn raster_to_sdf_cell(
    request: &FontAtlasBuildRequest,
    ch: char,
    metrics: Metrics,
    bitmap: &[u8],
) -> Result<Vec<u8>, EngineError> {
    let cell = request.cell_px as usize;
    let spread = request.spread_px as usize;
    let available = cell.checked_sub(spread * 2).ok_or_else(|| {
        invalid_ui_error(
            "font_atlas.spread_px",
            "spread leaves no drawable cell area",
        )
    })?;
    if metrics.width > available || metrics.height > available {
        return invalid_ui(
            format!("font.glyph.U+{:04X}", ch as u32),
            "rasterized glyph exceeds cell_px minus spread_px padding",
        );
    }
    if bitmap.len() != metrics.width * metrics.height {
        return invalid_ui(
            "font.glyph.bitmap",
            "bitmap length must match glyph metrics",
        );
    }
    let mut coverage = vec![0_u8; cell * cell];
    let offset_x = spread + (available - metrics.width) / 2;
    let offset_y = spread + (available - metrics.height) / 2;
    for y in 0..metrics.height {
        for x in 0..metrics.width {
            coverage[(offset_y + y) * cell + offset_x + x] = bitmap[y * metrics.width + x];
        }
    }
    Ok(font_sdf::sdf_from_coverage(
        &coverage,
        cell,
        cell,
        request.spread_px as f32,
    ))
}

pub(super) fn validate_request(request: &FontAtlasBuildRequest) -> Result<(), EngineError> {
    if request.font_path.as_os_str().is_empty() {
        return invalid_ui("font.path", "font path must be non-empty");
    }
    if request.glyphs.is_empty() {
        return invalid_ui("font.glyphs", "at least one glyph must be requested");
    }
    if !request.font_px.is_finite()
        || request.font_px <= 0.0
        || request.cell_px == 0
        || request.columns == 0
        || request.spread_px == 0
        || request.spread_px.saturating_mul(2) >= request.cell_px
    {
        return invalid_ui(
            "font_atlas.request",
            "font_px, cell_px, spread_px, and columns must be positive and bounded",
        );
    }
    Ok(())
}

pub(super) fn sorted_unique_glyphs(
    request: &FontAtlasBuildRequest,
) -> Result<Vec<char>, EngineError> {
    let glyphs = request.glyphs.iter().copied().collect::<BTreeSet<_>>();
    if glyphs.len() > u32::MAX as usize {
        return invalid_ui("font.glyphs", "glyph count exceeds u32 atlas indexing");
    }
    Ok(glyphs.into_iter().collect())
}

pub(super) fn invalid_ui<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_ui_error(field, reason))
}

pub(super) fn invalid_ui_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}

pub(super) fn update_str(hasher: &mut Fnv1a64, value: &str) {
    hasher.update_u64_le(value.len() as u64);
    hasher.update(value.as_bytes());
}

pub(super) fn hash_f32(hasher: &mut Fnv1a64, value: f32) {
    hasher.update_u64_le(u64::from(value.to_bits()));
}
