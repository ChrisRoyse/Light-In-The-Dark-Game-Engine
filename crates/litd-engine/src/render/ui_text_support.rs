//! Validation, glyph SDF-cell rasterization and hashing helpers for shaped UI text (#1466
//! split of `ui_text.rs`).
//!
//! The placement/atlas request validators, the per-glyph signed-distance-field cell builder,
//! and the FNV hashing helpers. The public atlas/layout API and value types stay in
//! `ui_text.rs`; these helpers are `pub(super)` so it calls them unchanged.

use std::collections::BTreeSet;

use fontdue::Metrics;
use litd_hash::Fnv1a64;

use super::{ShapedTextRunPlacement, verify_shaped_text_atlas_integrity};
use crate::{
    EngineError,
    math::Vec4,
    render::font_sdf,
    render::text_shaping::{ShapedTextRun, verify_shaped_text_run_integrity},
};

pub(super) fn validate_placement(placement: &ShapedTextRunPlacement) -> Result<(), EngineError> {
    if placement.id.trim().is_empty() || !placement.origin_px.is_finite() {
        return invalid_ui("shaped_text.placement", "id and origin must be valid");
    }
    verify_shaped_text_run_integrity(&placement.run)?;
    verify_shaped_text_atlas_integrity(&placement.atlas)?;
    if placement.run.source_font_sha256 != placement.atlas.source_font_sha256
        || (placement.run.font_px - placement.atlas.font_px).abs() > 0.001
    {
        return invalid_ui(
            "shaped_text.placement",
            "run and atlas must use the same font hash and font_px",
        );
    }
    if !placement.color.is_finite()
        || placement.color.cmplt(Vec4::ZERO).any()
        || placement.color.cmpgt(Vec4::ONE).any()
    {
        return invalid_ui("shaped_text.color", "RGBA must be finite inside 0..=1");
    }
    Ok(())
}

pub(super) fn validate_atlas_request(
    cell_px: u32,
    spread_px: u32,
    columns: u32,
) -> Result<(), EngineError> {
    if cell_px == 0 || spread_px == 0 || spread_px.saturating_mul(2) >= cell_px || columns == 0 {
        return invalid_ui(
            "shaped_text_atlas.request",
            "cell_px, spread_px, and columns must be positive and bounded",
        );
    }
    Ok(())
}

pub(super) fn sorted_unique_glyph_ids(run: &ShapedTextRun) -> Vec<u32> {
    run.glyphs
        .iter()
        .map(|glyph| glyph.glyph_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn sdf_cell(
    cell_px: u32,
    spread_px: u32,
    glyph_id: u32,
    metrics: Metrics,
    bitmap: &[u8],
) -> Result<Vec<u8>, EngineError> {
    if bitmap.len() != metrics.width * metrics.height {
        return invalid_ui(
            "shaped_text.glyph.bitmap",
            "bitmap length must match metrics",
        );
    }
    let cell = cell_px as usize;
    let spread = spread_px as usize;
    let available = cell.checked_sub(spread * 2).ok_or_else(|| {
        ui_error(
            "shaped_text_atlas.spread_px",
            "spread leaves no drawable area",
        )
    })?;
    if metrics.width > available || metrics.height > available {
        return invalid_ui(
            "shaped_text.glyph.bitmap",
            format!("glyph id {glyph_id} exceeds cell_px minus spread_px padding"),
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
        spread_px as f32,
    ))
}

pub(super) fn invalid_ui<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(ui_error(field, reason))
}

pub(super) fn ui_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
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
