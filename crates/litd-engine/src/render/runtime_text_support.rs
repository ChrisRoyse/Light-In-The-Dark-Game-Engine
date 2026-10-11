//! Helpers for `runtime_text`: world->screen projection, hot-path glyph layout,
//! atlas-cache keying, and plan-integrity hashing. Split out of `runtime_text.rs`
//! to keep both files under the 500-line ratchet (#1466).

use litd_hash::Fnv1a64;

use crate::{
    EngineError,
    math::{Mat4, Vec2, Vec3, Vec4},
    render::{
        font::{FontAtlas, FontAtlasBuildRequest},
        ui::{GlyphQuad, UiRect, UiRenderSettings},
    },
};

use super::{RuntimeTextEntry, RuntimeTextKind, RuntimeTextOutcome, RuntimeTextPlan};

/// Project a world position to viewport pixels, or `None` when it falls outside
/// the clip volume (behind camera or beyond any NDC face). This is the culling
/// gate for off-screen anchors: a label whose anchor projects to `None` renders
/// zero quads. Mirrors the projection `ui::floating_to_text_run` already uses so
/// runtime labels and HUD floating text agree pixel-for-pixel.
pub(super) fn project_world_to_screen(
    settings: UiRenderSettings,
    view_projection: Mat4,
    world: Vec3,
) -> Option<Vec2> {
    let clip = view_projection * world.extend(1.0);
    if clip.w <= f32::EPSILON {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 || ndc.z.abs() > 1.0 {
        return None;
    }
    let viewport = settings.viewport_size();
    Some(Vec2::new(
        (ndc.x * 0.5 + 0.5) * viewport.x,
        (1.0 - (ndc.y * 0.5 + 0.5)) * viewport.y,
    ))
}

/// Lay out one single-line string over an already-validated cached atlas, appending
/// quads to `out` and returning how many were produced.
///
/// Unlike `ui::layout_text_run`, this does NOT re-validate the atlas: the atlas came
/// from `FontAtlasCache`, which validates once at build. Re-validating per label
/// would make a frame of N labels O(N x atlas_glyphs) (#1462 lesson). A missing
/// glyph still fails loud - no silent tofu.
pub(super) fn layout_glyphs_unvalidated(
    atlas: &FontAtlas,
    text: &str,
    origin_px: Vec2,
    font_px: f32,
    color: Vec4,
    out: &mut Vec<GlyphQuad>,
) -> Result<usize, EngineError> {
    let font_scale = font_px / atlas.font_px;
    let mut cursor = origin_px;
    let mut produced = 0_usize;
    for ch in text.chars() {
        let glyph = atlas
            .glyph(ch as u32)
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: format!("runtime_text.glyph.U+{:04X}", ch as u32),
                reason: "character is not present in the cached runtime atlas".to_string(),
            })?;
        let advance = glyph.advance_width_px * font_scale;
        if !advance.is_finite() || advance <= 0.0 {
            return Err(EngineError::UiRenderInvalidConfig {
                field: format!("runtime_text.advance.U+{:04X}", ch as u32),
                reason: "glyph advance width must be finite and positive".to_string(),
            });
        }
        let rect = UiRect::new(cursor, cursor + Vec2::new(advance, font_px))?;
        out.push(GlyphQuad {
            rect_px: rect,
            uv: atlas.glyph_cell_uv(glyph)?,
            color,
            codepoint: ch as u32,
            glyph_id: ch as u32,
        });
        cursor.x += advance;
        produced += 1;
    }
    Ok(produced)
}

/// Deterministic cache key for an atlas build request. Two requests with the same
/// font file, sizing, and glyph set share a cached atlas.
pub(super) fn request_cache_key(request: &FontAtlasBuildRequest) -> String {
    let mut hasher = Fnv1a64::new();
    let path = request.font_path.display().to_string();
    hasher.update_u64_le(path.len() as u64);
    hasher.update(path.as_bytes());
    hasher.update_u64_le(u64::from(request.font_px.to_bits()));
    hasher.update_u64_le(request.cell_px as u64);
    hasher.update_u64_le(request.spread_px as u64);
    hasher.update_u64_le(request.columns as u64);
    hasher.update_u64_le(request.glyphs.len() as u64);
    for ch in &request.glyphs {
        hasher.update_u64_le(*ch as u64);
    }
    hasher.finish_hex()
}

pub(super) fn validate_color(field: &'static str, color: Vec4) -> Result<(), EngineError> {
    if !color.is_finite() || color.cmplt(Vec4::ZERO).any() || color.cmpgt(Vec4::ONE).any() {
        return Err(invalid_runtime_text_error(
            field,
            "RGBA must be finite inside 0..=1",
        ));
    }
    Ok(())
}

pub(super) fn validate_font_px(field: &'static str, font_px: f32) -> Result<(), EngineError> {
    if !font_px.is_finite() || font_px <= 0.0 {
        return Err(invalid_runtime_text_error(
            field,
            "font size must be finite and positive",
        ));
    }
    Ok(())
}

pub(super) fn invalid_runtime_text<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_runtime_text_error(field, reason))
}

pub(super) fn invalid_runtime_text_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    }
}

/// FNV-1a over the whole plan so an independent reader can prove the serialized
/// plan was not tampered between build and readback (FSV integrity, #800).
pub(super) fn compute_runtime_text_plan_fnv1a64(plan: &RuntimeTextPlan) -> String {
    let mut hasher = Fnv1a64::new();
    hasher.update_u64_le(plan.entries.len() as u64);
    for entry in &plan.entries {
        hash_entry(&mut hasher, entry);
    }
    hasher.update_u64_le(plan.quads.len() as u64);
    for quad in &plan.quads {
        hash_quad(&mut hasher, quad);
    }
    hasher.update_u64_le(plan.label_quad_count as u64);
    hasher.update_u64_le(plan.damage_quad_count as u64);
    hasher.update_u64_le(plan.rendered_count as u64);
    hasher.update_u64_le(plan.empty_count as u64);
    hasher.update_u64_le(plan.off_screen_count as u64);
    hasher.finish_hex()
}

fn hash_entry(hasher: &mut Fnv1a64, entry: &RuntimeTextEntry) {
    hasher.update_u64_le(entry.id.len() as u64);
    hasher.update(entry.id.as_bytes());
    hasher.update_u8(match entry.kind {
        RuntimeTextKind::Label => 1,
        RuntimeTextKind::DamageNumber => 2,
    });
    match &entry.outcome {
        RuntimeTextOutcome::Rendered {
            screen_origin_px,
            quad_start,
            quad_count,
        } => {
            hasher.update_u8(1);
            hash_vec2(hasher, *screen_origin_px);
            hasher.update_u64_le(*quad_start as u64);
            hasher.update_u64_le(*quad_count as u64);
        }
        RuntimeTextOutcome::Empty => {
            hasher.update_u8(2);
        }
        RuntimeTextOutcome::OffScreen => {
            hasher.update_u8(3);
        }
    }
}

fn hash_quad(hasher: &mut Fnv1a64, quad: &GlyphQuad) {
    hash_rect(hasher, quad.rect_px);
    hash_rect(hasher, quad.uv);
    hash_vec4(hasher, quad.color);
    hasher.update_u64_le(quad.codepoint as u64);
    hasher.update_u64_le(quad.glyph_id as u64);
}

fn hash_rect(hasher: &mut Fnv1a64, rect: UiRect) {
    hash_vec2(hasher, rect.min);
    hash_vec2(hasher, rect.max);
}

fn hash_vec2(hasher: &mut Fnv1a64, value: Vec2) {
    hasher.update_u64_le(u64::from(value.x.to_bits()));
    hasher.update_u64_le(u64::from(value.y.to_bits()));
}

fn hash_vec4(hasher: &mut Fnv1a64, value: Vec4) {
    hasher.update_u64_le(u64::from(value.x.to_bits()));
    hasher.update_u64_le(u64::from(value.y.to_bits()));
    hasher.update_u64_le(u64::from(value.z.to_bits()));
    hasher.update_u64_le(u64::from(value.w.to_bits()));
}
