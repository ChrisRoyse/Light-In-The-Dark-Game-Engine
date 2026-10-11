//! Runtime text: world-anchored labels + screen-space damage numbers over a
//! cross-frame-cached SDF font atlas.
//!
//! The editor draws its own UI with egui; produced GAMES have no egui and render
//! this plan instead (#882, presentation tier of #873). Design decisions:
//!
//! * Technique = the SDF glyph atlas the engine already builds with fontdue
//!   (rasterize) + rustybuzz (shape), NOT glyphon/cosmic-text. Both stacks are
//!   permissive (#1073), but glyphon pulls a larger tree (cosmic-text + swash +
//!   etagere) and re-rasterizes per pixel size, whereas one SDF atlas is scale-
//!   free: the same texture serves a nameplate near and far and a damage number
//!   scaling in, with no per-size rebuild - the right call for the $500-PC game
//!   budget (#928).
//! * Atlas caching = [`FontAtlasCache`] builds the atlas once and reuses it every
//!   frame. Rebuilding an SDF atlas per frame would blow the frame budget; the
//!   cache turns a per-frame cost into a one-time load cost.
//!
//! Edge behavior (each recorded per entry, never a silent skip):
//! * empty string  -> [`RuntimeTextOutcome::Empty`], zero quads, no error (an
//!   empty label is a valid "no nameplate" state, not a failure).
//! * off-screen anchor -> [`RuntimeTextOutcome::OffScreen`], zero quads (culled by
//!   projection).
//! * very long string -> laid out in full; quad count equals the character count.

use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Mat4, Vec2, Vec3, Vec4},
    render::{
        font::{FontAtlas, FontAtlasBuild, FontAtlasBuildRequest},
        ui::{GlyphQuad, UiRenderSettings},
    },
};

#[path = "runtime_text_support.rs"]
mod runtime_text_support;

use runtime_text_support::{
    compute_runtime_text_plan_fnv1a64, invalid_runtime_text, layout_glyphs_unvalidated,
    project_world_to_screen, request_cache_key, validate_color, validate_font_px,
};

pub const RUNTIME_TEXT_PLAN_SCHEMA: &str = "litd.render.runtime_text_plan.v1";

/// Build-once/reuse cache of SDF font atlases, keyed by the full build request
/// (font file, sizing, glyph set). A produced game builds its runtime glyph atlas
/// at load and every frame reuses it via [`FontAtlasCache::get_or_build`].
#[derive(Debug, Default)]
pub struct FontAtlasCache {
    entries: HashMap<String, Arc<FontAtlasBuild>>,
    hits: u64,
    misses: u64,
}

impl FontAtlasCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the cached atlas for `request`, building (and validating) it on the
    /// first miss. The returned `Arc` shares the atlas + image with no clone of the
    /// (potentially large) atlas bitmap.
    pub fn get_or_build(
        &mut self,
        request: FontAtlasBuildRequest,
    ) -> Result<Arc<FontAtlasBuild>, EngineError> {
        let key = request_cache_key(&request);
        if let Some(build) = self.entries.get(&key) {
            self.hits += 1;
            return Ok(build.clone());
        }
        let build = Arc::new(FontAtlas::from_font_file(request)?);
        self.misses += 1;
        self.entries.insert(key, build.clone());
        Ok(build)
    }

    /// Seed the cache from a font atlas already built from verified embedded
    /// bytes. The source id must match the request exactly so later plan builds
    /// cannot silently cross-wire a different font.
    pub fn insert_prebuilt(
        &mut self,
        request: FontAtlasBuildRequest,
        build: FontAtlasBuild,
    ) -> Result<Arc<FontAtlasBuild>, EngineError> {
        build.atlas.validate()?;
        let expected_source = request.font_path.display().to_string();
        if build.atlas.source_font_path != expected_source {
            return invalid_runtime_text(
                "runtime_text.font_source",
                format!(
                    "prebuilt atlas source {} does not match request {expected_source}",
                    build.atlas.source_font_path
                ),
            );
        }
        let key = request_cache_key(&request);
        if self.entries.contains_key(&key) {
            return invalid_runtime_text(
                "runtime_text.font_cache",
                "prebuilt atlas request is already cached",
            );
        }
        let build = Arc::new(build);
        self.entries.insert(key, build.clone());
        self.misses += 1;
        Ok(build)
    }

    pub fn hits(&self) -> u64 {
        self.hits
    }

    pub fn misses(&self) -> u64 {
        self.misses
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// A persistent, world-anchored label (e.g. a unit nameplate). Projected to the
/// screen every frame and culled when its anchor leaves the view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldLabel {
    pub id: String,
    pub text: String,
    pub world_position: Vec3,
    /// World-space offset from the anchor (e.g. lift the nameplate above a unit).
    pub world_offset: Vec3,
    pub font_px: f32,
    pub color: Vec4,
}

/// A screen-space damage number: spawned at a world position, it rises in pixels
/// and fades over its lifetime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DamageNumber {
    pub id: String,
    pub text: String,
    pub world_position: Vec3,
    pub age_seconds: f32,
    pub lifetime_seconds: f32,
    pub rise_px_per_second: f32,
    pub font_px: f32,
    pub color: Vec4,
}

/// Everything the runtime wants drawn as text this frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeTextScene {
    pub settings: UiRenderSettings,
    pub view_projection: Mat4,
    pub labels: Vec<WorldLabel>,
    pub damage_numbers: Vec<DamageNumber>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeTextKind {
    Label,
    DamageNumber,
}

/// What happened to one scene item. Every item produces exactly one entry so the
/// full-state readback can prove nothing was silently dropped.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RuntimeTextOutcome {
    Rendered {
        screen_origin_px: Vec2,
        quad_start: usize,
        quad_count: usize,
    },
    Empty,
    OffScreen,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeTextEntry {
    pub id: String,
    pub kind: RuntimeTextKind,
    pub outcome: RuntimeTextOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeTextPlan {
    pub schema: String,
    pub quads: Vec<GlyphQuad>,
    pub entries: Vec<RuntimeTextEntry>,
    pub label_quad_count: usize,
    pub damage_quad_count: usize,
    pub rendered_count: usize,
    pub empty_count: usize,
    pub off_screen_count: usize,
    pub fnv1a64: String,
}

/// Build the frame's text plan: project each world-anchored label and damage
/// number, animate damage numbers, and lay out visible glyphs over the cached
/// atlas. `atlas_request` selects (and, on the first frame, builds) the runtime
/// glyph atlas; every subsequent frame is a cache hit.
pub fn build_runtime_text_plan(
    cache: &mut FontAtlasCache,
    atlas_request: &FontAtlasBuildRequest,
    scene: &RuntimeTextScene,
) -> Result<RuntimeTextPlan, EngineError> {
    scene.settings.validate()?;
    if !scene
        .view_projection
        .to_cols_array()
        .iter()
        .all(|value| value.is_finite())
    {
        return invalid_runtime_text(
            "runtime_text.view_projection",
            "view-projection matrix must be finite",
        );
    }
    let build = cache.get_or_build(atlas_request.clone())?;
    let atlas = &build.atlas;

    let mut quads = Vec::new();
    let mut entries = Vec::with_capacity(scene.labels.len() + scene.damage_numbers.len());
    let mut label_quad_count = 0_usize;
    let mut damage_quad_count = 0_usize;

    for label in &scene.labels {
        validate_font_px("runtime_text.label.font_px", label.font_px)?;
        validate_color("runtime_text.label.color", label.color)?;
        if !label.world_position.is_finite() || !label.world_offset.is_finite() {
            return invalid_runtime_text(
                "runtime_text.label.anchor",
                "label world position and offset must be finite",
            );
        }
        let outcome = if label.text.is_empty() {
            RuntimeTextOutcome::Empty
        } else if let Some(origin) = project_world_to_screen(
            scene.settings,
            scene.view_projection,
            label.world_position + label.world_offset,
        ) {
            let start = quads.len();
            let count = layout_glyphs_unvalidated(
                atlas,
                &label.text,
                origin,
                label.font_px,
                label.color,
                &mut quads,
            )?;
            label_quad_count += count;
            RuntimeTextOutcome::Rendered {
                screen_origin_px: origin,
                quad_start: start,
                quad_count: count,
            }
        } else {
            RuntimeTextOutcome::OffScreen
        };
        entries.push(RuntimeTextEntry {
            id: label.id.clone(),
            kind: RuntimeTextKind::Label,
            outcome,
        });
    }

    for number in &scene.damage_numbers {
        validate_font_px("runtime_text.damage.font_px", number.font_px)?;
        validate_color("runtime_text.damage.color", number.color)?;
        if !number.world_position.is_finite()
            || !number.age_seconds.is_finite()
            || !number.lifetime_seconds.is_finite()
            || !number.rise_px_per_second.is_finite()
            || number.age_seconds < 0.0
            || number.lifetime_seconds <= 0.0
        {
            return invalid_runtime_text(
                "runtime_text.damage.animation",
                "age, lifetime, rise, and position must be finite (lifetime > 0, age >= 0)",
            );
        }
        let outcome = if number.text.is_empty() {
            RuntimeTextOutcome::Empty
        } else if let Some(mut origin) =
            project_world_to_screen(scene.settings, scene.view_projection, number.world_position)
        {
            let t = (number.age_seconds / number.lifetime_seconds).clamp(0.0, 1.0);
            origin.y -= number.rise_px_per_second * number.age_seconds;
            let mut color = number.color;
            color.w *= 1.0 - t;
            let start = quads.len();
            let count = layout_glyphs_unvalidated(
                atlas,
                &number.text,
                origin,
                number.font_px,
                color,
                &mut quads,
            )?;
            damage_quad_count += count;
            RuntimeTextOutcome::Rendered {
                screen_origin_px: origin,
                quad_start: start,
                quad_count: count,
            }
        } else {
            RuntimeTextOutcome::OffScreen
        };
        entries.push(RuntimeTextEntry {
            id: number.id.clone(),
            kind: RuntimeTextKind::DamageNumber,
            outcome,
        });
    }

    let rendered_count = entries
        .iter()
        .filter(|entry| matches!(entry.outcome, RuntimeTextOutcome::Rendered { .. }))
        .count();
    let empty_count = entries
        .iter()
        .filter(|entry| matches!(entry.outcome, RuntimeTextOutcome::Empty))
        .count();
    let off_screen_count = entries
        .iter()
        .filter(|entry| matches!(entry.outcome, RuntimeTextOutcome::OffScreen))
        .count();

    let mut plan = RuntimeTextPlan {
        schema: RUNTIME_TEXT_PLAN_SCHEMA.to_string(),
        quads,
        entries,
        label_quad_count,
        damage_quad_count,
        rendered_count,
        empty_count,
        off_screen_count,
        fnv1a64: String::new(),
    };
    plan.fnv1a64 = compute_runtime_text_plan_fnv1a64(&plan);
    verify_runtime_text_plan_integrity(&plan)?;
    Ok(plan)
}

/// Independent re-derivation of the plan hash + structural invariants. A reader
/// that deserializes a plan calls this to prove it was not tampered.
pub fn verify_runtime_text_plan_integrity(plan: &RuntimeTextPlan) -> Result<(), EngineError> {
    if plan.schema != RUNTIME_TEXT_PLAN_SCHEMA {
        return invalid_runtime_text("runtime_text_plan.schema", "must be the v1 schema");
    }
    if plan.label_quad_count + plan.damage_quad_count != plan.quads.len() {
        return invalid_runtime_text(
            "runtime_text_plan.quad_count",
            "label + damage quad counts must equal total quads",
        );
    }
    let mut rendered_quads = 0_usize;
    let mut rendered = 0_usize;
    let mut empty = 0_usize;
    let mut off_screen = 0_usize;
    for entry in &plan.entries {
        match &entry.outcome {
            RuntimeTextOutcome::Rendered {
                quad_start,
                quad_count,
                ..
            } => {
                let end = quad_start
                    .checked_add(*quad_count)
                    .ok_or_else(|| plan_error("quad range overflow"))?;
                if *quad_count == 0 || end > plan.quads.len() {
                    return invalid_runtime_text(
                        "runtime_text_plan.entry",
                        "a rendered entry must map to a non-empty in-bounds quad range",
                    );
                }
                rendered_quads += quad_count;
                rendered += 1;
            }
            RuntimeTextOutcome::Empty => empty += 1,
            RuntimeTextOutcome::OffScreen => off_screen += 1,
        }
    }
    if rendered_quads != plan.quads.len()
        || rendered != plan.rendered_count
        || empty != plan.empty_count
        || off_screen != plan.off_screen_count
    {
        return invalid_runtime_text(
            "runtime_text_plan.tallies",
            "entry outcome tallies must match the plan summary counts",
        );
    }
    let actual = compute_runtime_text_plan_fnv1a64(plan);
    if actual != plan.fnv1a64 {
        return Err(EngineError::UiRenderReadbackMismatch {
            field: "runtime_text_plan.fnv1a64".to_string(),
            expected: plan.fnv1a64.clone(),
            actual,
        });
    }
    Ok(())
}

fn plan_error(reason: &str) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: "runtime_text_plan.entry".to_string(),
        reason: reason.to_string(),
    }
}

#[cfg(test)]
#[path = "runtime_text_tests.rs"]
mod tests;
