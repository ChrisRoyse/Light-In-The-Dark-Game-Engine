//! Text, HUD, floating text, minimap, and fog-of-war render planning.

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Aabb, Mat4, Vec2, Vec3, Vec4},
};

pub use super::font::FontAtlas;

pub const UI_RENDER_WGSL: &str = include_str!("ui_hud_minimap_fog.wgsl");

#[path = "ui_fog.rs"]
mod ui_fog;
#[path = "ui_layout.rs"]
mod ui_layout;
#[path = "ui_map_policy.rs"]
mod ui_map_policy;
#[path = "ui_perf_overlay.rs"]
mod ui_perf_overlay;
#[path = "ui_text.rs"]
mod ui_text;

pub use ui_fog::{FogCell, FogOfWarGrid, FogSummary, VisionSource, reveal_fog, summarize_fog};
pub use ui_layout::{
    HudLayoutPreset, HudReferenceLayout, HudRegion, HudRegionKind, HudStyleTokens, MinimapShape,
    reference_hud_layout,
};
pub use ui_map_policy::{
    MapOverlayMarkerKind, MapOverlayMarkerSource, MapOverlayPolicy, MapOverlayPolicyMode,
    MapOverlayPolicyReadback, validate_map_overlay_policy,
};
pub use ui_perf_overlay::{
    InputLatencyRecord, PerformanceOverlayArtifactReadback, PerformanceOverlayBar,
    PerformanceOverlayImage, PerformanceOverlayPlan, build_performance_overlay_plan,
    render_performance_overlay_image, write_performance_overlay_artifacts,
};
pub use ui_text::{
    SHAPED_TEXT_ATLAS_SCHEMA, ShapedGlyphHit, ShapedTextAtlas, ShapedTextAtlasBuild,
    ShapedTextAtlasGlyph, ShapedTextAtlasImage, ShapedTextLayoutProbe, ShapedTextPreviewImage,
    ShapedTextPreviewReadback, ShapedTextRunPlacement, build_shaped_text_atlas,
    layout_shaped_text_run, measure_shaped_text_layout, render_shaped_text_preview_image,
    verify_shaped_text_atlas_integrity, write_shaped_text_preview_bmp,
};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiRenderSettings {
    pub viewport_width: f32,
    pub viewport_height: f32,
}

impl UiRenderSettings {
    pub fn validate(&self) -> Result<(), EngineError> {
        if !self.viewport_width.is_finite()
            || !self.viewport_height.is_finite()
            || self.viewport_width <= 0.0
            || self.viewport_height <= 0.0
        {
            return invalid_ui("viewport", "width and height must be finite and positive");
        }
        Ok(())
    }

    pub fn viewport_size(&self) -> Vec2 {
        Vec2::new(self.viewport_width, self.viewport_height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiRect {
    pub min: Vec2,
    pub max: Vec2,
}

impl UiRect {
    pub fn new(min: Vec2, max: Vec2) -> Result<Self, EngineError> {
        let rect = Self { min, max };
        rect.validate("rect")?;
        Ok(rect)
    }

    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    pub fn validate(&self, field: impl Into<String>) -> Result<(), EngineError> {
        if !self.min.is_finite()
            || !self.max.is_finite()
            || self.min.x >= self.max.x
            || self.min.y >= self.max.y
        {
            return invalid_ui(
                field,
                "rect coordinates must be finite with positive extents",
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextRun {
    pub id: String,
    pub text: String,
    pub origin_px: Vec2,
    pub font_px: f32,
    pub color: Vec4,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GlyphQuad {
    pub rect_px: UiRect,
    pub uv: UiRect,
    pub color: Vec4,
    pub codepoint: u32,
    pub glyph_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum HudWidgetKind {
    HealthBar { current: f32, max: f32 },
    CommandButton { enabled: bool, cooldown: f32 },
    ResourceCounter { amount: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudWidget {
    pub id: String,
    pub rect_px: UiRect,
    pub kind: HudWidgetKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FloatingText {
    pub text: String,
    pub world_position: Vec3,
    pub age_seconds: f32,
    pub lifetime_seconds: f32,
    pub rise_px_per_second: f32,
    pub color: Vec4,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MinimapConfig {
    pub world_bounds: Aabb,
    pub rect_px: UiRect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MinimapMarker {
    pub id: String,
    pub kind: MapOverlayMarkerKind,
    pub source: MapOverlayMarkerSource,
    pub allowed_policy: MapOverlayPolicyMode,
    pub world_position: Vec3,
    pub color: Vec4,
    pub radius_px: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MinimapMarkerDraw {
    pub position_px: Vec2,
    pub color: Vec4,
    pub radius_px: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MinimapPlan {
    pub markers: Vec<MinimapMarkerDraw>,
    pub viewport_rect_px: Option<UiRect>,
    pub policy: MapOverlayPolicyReadback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiRenderInputs {
    pub text_runs: Vec<TextRun>,
    pub shaped_text_runs: Vec<ShapedTextRunPlacement>,
    pub widgets: Vec<HudWidget>,
    pub floating_texts: Vec<FloatingText>,
    pub minimap: MinimapConfig,
    pub overlay_policy: MapOverlayPolicy,
    pub markers: Vec<MinimapMarker>,
    pub viewport_world_bounds: Aabb,
    pub fog: FogOfWarGrid,
    pub vision_sources: Vec<VisionSource>,
    pub view_projection: Mat4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiPassKind {
    Hud,
    Text,
    FloatingText,
    Minimap,
    FogOfWar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiRenderPlan {
    pub passes: Vec<UiPassKind>,
    pub glyph_quads: Vec<GlyphQuad>,
    pub floating_glyph_quads: Vec<GlyphQuad>,
    pub widget_count: usize,
    pub minimap: MinimapPlan,
    pub fog: FogSummary,
}

pub fn build_ui_render_plan(
    settings: UiRenderSettings,
    atlas: &FontAtlas,
    inputs: UiRenderInputs,
) -> Result<UiRenderPlan, EngineError> {
    settings.validate()?;
    atlas.validate()?;
    let mut glyph_quads = Vec::new();
    for run in &inputs.text_runs {
        glyph_quads.extend(layout_text_run(atlas, run)?);
    }
    for placement in &inputs.shaped_text_runs {
        glyph_quads.extend(layout_shaped_text_run(placement)?);
    }
    for widget in &inputs.widgets {
        validate_widget(widget)?;
    }
    let mut floating_glyph_quads = Vec::new();
    for floating in &inputs.floating_texts {
        if let Some(run) = floating_to_text_run(settings, inputs.view_projection, floating)? {
            floating_glyph_quads.extend(layout_text_run(atlas, &run)?);
        }
    }
    let minimap = build_minimap_plan(
        &inputs.overlay_policy,
        &inputs.minimap,
        &inputs.markers,
        inputs.viewport_world_bounds,
    )?;
    let fog_grid = reveal_fog(&inputs.fog, &inputs.vision_sources)?;
    let fog = summarize_fog(&fog_grid);
    let mut passes = vec![UiPassKind::Hud, UiPassKind::Text, UiPassKind::FloatingText];
    if minimap.policy.surface_visible {
        passes.push(UiPassKind::Minimap);
    }
    passes.push(UiPassKind::FogOfWar);
    Ok(UiRenderPlan {
        passes,
        glyph_quads,
        floating_glyph_quads,
        widget_count: inputs.widgets.len(),
        minimap,
        fog,
    })
}

pub fn layout_text_run(atlas: &FontAtlas, run: &TextRun) -> Result<Vec<GlyphQuad>, EngineError> {
    validate_text_run(run)?;
    // Validate the atlas ONCE per layout pass instead of once per glyph lookup.
    // `glyph_uv`/`glyph_cell_uv` below no longer re-validate, so a text run of N
    // characters over an M-glyph atlas is now O(M + N) rather than O(N x M).
    atlas.validate()?;
    let font_scale = run.font_px / atlas.font_px;
    let mut cursor = run.origin_px;
    let mut quads = Vec::new();
    for ch in run.text.chars() {
        if ch == '\n' {
            cursor.x = run.origin_px.x;
            cursor.y += run.font_px;
            continue;
        }
        let glyph = atlas
            .glyph(ch as u32)
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: format!("text_run.glyph.U+{:04X}", ch as u32),
                reason: "character is not present in the supplied font atlas".to_string(),
            })?;
        // Real per-glyph horizontal advance from the shaped/rasterized font
        // metrics (proportional), scaled from the atlas font size to the run
        // font size. Replaces the old fixed `font_px * 0.6` monospace guess.
        let advance = glyph.advance_width_px * font_scale;
        if !advance.is_finite() || advance <= 0.0 {
            return invalid_ui(
                format!("text_run.advance.U+{:04X}", ch as u32),
                "glyph advance width must be finite and positive",
            );
        }
        let rect = UiRect::new(cursor, cursor + Vec2::new(advance, run.font_px))?;
        quads.push(GlyphQuad {
            rect_px: rect,
            uv: atlas.glyph_cell_uv(glyph)?,
            color: run.color,
            codepoint: ch as u32,
            glyph_id: ch as u32,
        });
        cursor.x += advance;
    }
    Ok(quads)
}

pub fn build_minimap_plan(
    policy: &MapOverlayPolicy,
    config: &MinimapConfig,
    markers: &[MinimapMarker],
    viewport_world_bounds: Aabb,
) -> Result<MinimapPlan, EngineError> {
    let policy_readback = validate_map_overlay_policy(policy, markers)?;
    if !policy_readback.surface_visible {
        return Ok(MinimapPlan {
            markers: Vec::new(),
            viewport_rect_px: None,
            policy: policy_readback,
        });
    }
    validate_aabb("minimap.world_bounds", config.world_bounds)?;
    config.rect_px.validate("minimap.rect_px")?;
    validate_aabb("minimap.viewport_world_bounds", viewport_world_bounds)?;
    let mut marker_draws = Vec::new();
    for marker in markers {
        validate_color("minimap.marker.color", marker.color)?;
        if !marker.world_position.is_finite()
            || !marker.radius_px.is_finite()
            || marker.radius_px <= 0.0
        {
            return invalid_ui("minimap.marker", "position and radius must be finite");
        }
        marker_draws.push(MinimapMarkerDraw {
            position_px: world_to_minimap(config, marker.world_position),
            color: marker.color,
            radius_px: marker.radius_px,
        });
    }
    let viewport_rect_px = if policy_readback.viewport_indicator {
        Some(UiRect::new(
            world_to_minimap(config, viewport_world_bounds.min),
            world_to_minimap(config, viewport_world_bounds.max),
        )?)
    } else {
        None
    };
    Ok(MinimapPlan {
        markers: marker_draws,
        viewport_rect_px,
        policy: policy_readback,
    })
}

fn floating_to_text_run(
    settings: UiRenderSettings,
    view_projection: Mat4,
    floating: &FloatingText,
) -> Result<Option<TextRun>, EngineError> {
    if !floating.world_position.is_finite()
        || !floating.age_seconds.is_finite()
        || !floating.lifetime_seconds.is_finite()
        || !floating.rise_px_per_second.is_finite()
        || floating.age_seconds < 0.0
        || floating.lifetime_seconds <= 0.0
    {
        return invalid_ui(
            "floating_text",
            "position, age, lifetime, and rise must be finite",
        );
    }
    let Some(mut origin) = world_to_screen(settings, view_projection, floating.world_position)
    else {
        return Ok(None);
    };
    let t = (floating.age_seconds / floating.lifetime_seconds).clamp(0.0, 1.0);
    origin.y -= floating.rise_px_per_second * floating.age_seconds;
    let mut color = floating.color;
    color.w *= 1.0 - t;
    Ok(Some(TextRun {
        id: "floating".to_string(),
        text: floating.text.clone(),
        origin_px: origin,
        font_px: 24.0,
        color,
    }))
}

fn world_to_screen(settings: UiRenderSettings, view_projection: Mat4, world: Vec3) -> Option<Vec2> {
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

fn world_to_minimap(config: &MinimapConfig, world: Vec3) -> Vec2 {
    let size = config.world_bounds.max - config.world_bounds.min;
    let uv = Vec2::new(
        (world.x - config.world_bounds.min.x) / size.x,
        (world.z - config.world_bounds.min.z) / size.z,
    )
    .clamp(Vec2::ZERO, Vec2::ONE);
    config.rect_px.min + config.rect_px.size() * uv
}

fn validate_text_run(run: &TextRun) -> Result<(), EngineError> {
    if run.text.is_empty()
        || !run.origin_px.is_finite()
        || !run.font_px.is_finite()
        || run.font_px <= 0.0
    {
        return invalid_ui("text_run", "text, origin, and font size must be valid");
    }
    validate_color("text_run.color", run.color)
}

fn validate_widget(widget: &HudWidget) -> Result<(), EngineError> {
    widget.rect_px.validate("hud.widget.rect")?;
    match widget.kind {
        HudWidgetKind::HealthBar { current, max } => {
            if !current.is_finite() || !max.is_finite() || max <= 0.0 || current < 0.0 {
                return invalid_ui(
                    "hud.health_bar",
                    "current/max must be finite and non-negative",
                );
            }
        }
        HudWidgetKind::CommandButton { cooldown, .. } => {
            if !cooldown.is_finite() || cooldown < 0.0 {
                return invalid_ui(
                    "hud.command_button",
                    "cooldown must be finite and non-negative",
                );
            }
        }
        HudWidgetKind::ResourceCounter { .. } => {}
    }
    Ok(())
}

fn validate_color(field: &'static str, color: Vec4) -> Result<(), EngineError> {
    if !color.is_finite() || color.cmplt(Vec4::ZERO).any() || color.cmpgt(Vec4::ONE).any() {
        return invalid_ui(field, "RGBA must be finite inside 0..=1");
    }
    Ok(())
}

fn validate_aabb(field: &'static str, bounds: Aabb) -> Result<(), EngineError> {
    if !bounds.min.is_finite()
        || !bounds.max.is_finite()
        || bounds.min.x >= bounds.max.x
        || bounds.min.z >= bounds.max.z
    {
        return invalid_ui(field, "bounds must be finite with ordered X/Z extents");
    }
    Ok(())
}

fn invalid_ui<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;
