use std::collections::BTreeSet;

use litd_engine::{
    EngineError,
    math::{Aabb, Mat4, Vec2, Vec3, Vec4},
    render::{
        font::{FontAtlasBuildRequest, reference_windows_font_path},
        ui::{
            FloatingText, FogOfWarGrid, FontAtlas, HudLayoutPreset, HudRegionKind, HudWidget,
            HudWidgetKind, MapOverlayMarkerKind, MapOverlayMarkerSource, MapOverlayPolicy,
            MapOverlayPolicyMode, MinimapConfig, MinimapMarker, MinimapShape, TextRun, UiRect,
            UiRenderInputs, UiRenderSettings, VisionSource, build_ui_render_plan,
            reference_hud_layout,
        },
    },
};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

pub const HUD_UI_BUILDER_SCHEMA: &str = "litd.editor.hud_ui_builder.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudUiBuilderSelection {
    pub builder_id: String,
    pub preset: HudLayoutPreset,
    pub viewport_width_px: u32,
    pub viewport_height_px: u32,
    pub widgets: Vec<HudUiWidgetSpec>,
    pub text_runs: Vec<HudUiTextSpec>,
    pub overlay_policy: MapOverlayPolicy,
    pub minimap_markers: Vec<HudUiMinimapMarkerSpec>,
    pub floating_texts: Vec<FloatingText>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HudNormalizedRect {
    pub min: Vec2,
    pub max: Vec2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudUiWidgetSpec {
    pub widget_id: String,
    pub region: HudRegionKind,
    pub rect: HudNormalizedRect,
    pub kind: HudWidgetKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudUiTextSpec {
    pub text_id: String,
    pub region: HudRegionKind,
    pub offset_px: Vec2,
    pub font_px: f32,
    pub text: String,
    pub color: Vec4,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudUiMinimapMarkerSpec {
    pub marker_id: String,
    pub kind: MapOverlayMarkerKind,
    pub source: MapOverlayMarkerSource,
    pub allowed_policy: MapOverlayPolicyMode,
    pub world_position: Vec3,
    pub color: Vec4,
    pub radius_px: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudUiBuilderReadback {
    pub schema: String,
    pub builder_id: String,
    pub selection_sha256: String,
    pub preset: HudLayoutPreset,
    pub viewport_width_px: u32,
    pub viewport_height_px: u32,
    pub reference_images: Vec<String>,
    pub region_count: usize,
    pub command_slot_count: u32,
    pub minimap_shape: MinimapShape,
    pub overlay_policy_mode: MapOverlayPolicyMode,
    pub map_surface_visible: bool,
    pub player_position_indicator: bool,
    pub viewport_indicator: bool,
    pub quest_markers: bool,
    pub route_lines: bool,
    pub widget_count: usize,
    pub text_run_count: usize,
    pub minimap_marker_count: usize,
    pub pass_count: usize,
    pub glyph_quad_count: usize,
    pub floating_glyph_quad_count: usize,
    pub fog_hidden: usize,
    pub fog_explored: usize,
    pub fog_visible: usize,
    pub plan_sha256: String,
}

pub fn build_hud_ui_builder_readback(
    selection: &HudUiBuilderSelection,
) -> Result<HudUiBuilderReadback, EngineError> {
    validate_selection(selection)?;
    let settings = settings(selection)?;
    let layout = reference_hud_layout(settings, selection.preset)?;
    let world = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(96.0))?;
    let atlas = build_font_atlas(selection)?;
    let inputs = UiRenderInputs {
        text_runs: build_text_runs(selection, &layout)?,
        shaped_text_runs: Vec::new(),
        widgets: build_widgets(selection, &layout)?,
        floating_texts: selection.floating_texts.clone(),
        minimap: MinimapConfig {
            world_bounds: world,
            rect_px: layout.minimap_rect_px,
        },
        overlay_policy: selection.overlay_policy.clone(),
        markers: build_markers(selection)?,
        viewport_world_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(24.0))?,
        fog: FogOfWarGrid::new(8, 8, world)?,
        vision_sources: vec![VisionSource {
            world_position: Vec3::ZERO,
            radius: 34.0,
        }],
        view_projection: Mat4::IDENTITY,
    };
    let plan = build_ui_render_plan(settings, &atlas, inputs)?;
    let plan_bytes =
        serde_json::to_vec(&plan).map_err(|error| EngineError::SerializationFailed {
            format: "hud-ui-render-plan-json",
            reason: format!("serialize HUD UI render plan: {error}"),
        })?;
    let selection_bytes =
        serde_json::to_vec(selection).map_err(|error| EngineError::SerializationFailed {
            format: "hud-ui-builder-selection-json",
            reason: format!("serialize HUD UI builder selection: {error}"),
        })?;
    Ok(HudUiBuilderReadback {
        schema: HUD_UI_BUILDER_SCHEMA.to_string(),
        builder_id: selection.builder_id.clone(),
        selection_sha256: sha256_hex(&selection_bytes),
        preset: selection.preset,
        viewport_width_px: selection.viewport_width_px,
        viewport_height_px: selection.viewport_height_px,
        reference_images: layout.reference_images,
        region_count: layout.regions.len(),
        command_slot_count: layout.command_slot_count,
        minimap_shape: layout.minimap_shape,
        overlay_policy_mode: plan.minimap.policy.mode,
        map_surface_visible: plan.minimap.policy.surface_visible,
        player_position_indicator: plan.minimap.policy.player_position_indicator,
        viewport_indicator: plan.minimap.policy.viewport_indicator,
        quest_markers: plan.minimap.policy.quest_markers,
        route_lines: plan.minimap.policy.route_lines,
        widget_count: selection.widgets.len(),
        text_run_count: selection.text_runs.len(),
        minimap_marker_count: selection.minimap_markers.len(),
        pass_count: plan.passes.len(),
        glyph_quad_count: plan.glyph_quads.len(),
        floating_glyph_quad_count: plan.floating_glyph_quads.len(),
        fog_hidden: plan.fog.hidden,
        fog_explored: plan.fog.explored,
        fog_visible: plan.fog.visible,
        plan_sha256: sha256_hex(&plan_bytes),
    })
}

fn validate_selection(selection: &HudUiBuilderSelection) -> Result<(), EngineError> {
    validate_single_line("builder_id", &selection.builder_id)?;
    if selection.widgets.is_empty() {
        return invalid_builder("widgets", "at least one HUD widget is required");
    }
    let mut ids = BTreeSet::new();
    for widget in &selection.widgets {
        validate_single_line("widget_id", &widget.widget_id)?;
        if !ids.insert(widget.widget_id.as_str()) {
            return invalid_builder("widget_id", format!("duplicate {}", widget.widget_id));
        }
        validate_rect(widget.rect)?;
    }
    ids.clear();
    for text in &selection.text_runs {
        validate_single_line("text_id", &text.text_id)?;
        if !ids.insert(text.text_id.as_str()) {
            return invalid_builder("text_id", format!("duplicate {}", text.text_id));
        }
        validate_single_line("text", &text.text)?;
        if !text.offset_px.is_finite() || !text.font_px.is_finite() || text.font_px <= 0.0 {
            return invalid_builder("text", "offset and font size must be finite and positive");
        }
    }
    ids.clear();
    for marker in &selection.minimap_markers {
        validate_single_line("marker_id", &marker.marker_id)?;
        if !ids.insert(marker.marker_id.as_str()) {
            return invalid_builder("marker_id", format!("duplicate {}", marker.marker_id));
        }
    }
    Ok(())
}

fn build_widgets(
    selection: &HudUiBuilderSelection,
    layout: &litd_engine::render::ui::HudReferenceLayout,
) -> Result<Vec<HudWidget>, EngineError> {
    selection
        .widgets
        .iter()
        .map(|spec| {
            let region = required_region(layout, spec.region)?;
            Ok(HudWidget {
                id: spec.widget_id.clone(),
                rect_px: rect_in_region(region, spec.rect)?,
                kind: spec.kind,
            })
        })
        .collect()
}

fn build_text_runs(
    selection: &HudUiBuilderSelection,
    layout: &litd_engine::render::ui::HudReferenceLayout,
) -> Result<Vec<TextRun>, EngineError> {
    selection
        .text_runs
        .iter()
        .map(|spec| {
            let region = required_region(layout, spec.region)?;
            Ok(TextRun {
                id: spec.text_id.clone(),
                text: spec.text.clone(),
                origin_px: region.min + spec.offset_px,
                font_px: spec.font_px,
                color: spec.color,
            })
        })
        .collect()
}

fn build_markers(selection: &HudUiBuilderSelection) -> Result<Vec<MinimapMarker>, EngineError> {
    selection
        .minimap_markers
        .iter()
        .map(|marker| {
            Ok(MinimapMarker {
                id: marker.marker_id.clone(),
                kind: marker.kind,
                source: marker.source,
                allowed_policy: marker.allowed_policy,
                world_position: marker.world_position,
                color: marker.color,
                radius_px: marker.radius_px,
            })
        })
        .collect()
}

fn build_font_atlas(selection: &HudUiBuilderSelection) -> Result<FontAtlas, EngineError> {
    let mut text = String::new();
    for run in &selection.text_runs {
        text.push_str(&run.text);
    }
    for floating in &selection.floating_texts {
        text.push_str(&floating.text);
    }
    if text.is_empty() {
        text.push_str("HUD");
    }
    Ok(FontAtlas::from_font_file(FontAtlasBuildRequest::for_text(
        reference_windows_font_path(),
        &text,
        40.0,
        64,
        8,
        8,
    ))?
    .atlas)
}

fn settings(selection: &HudUiBuilderSelection) -> Result<UiRenderSettings, EngineError> {
    let settings = UiRenderSettings {
        viewport_width: selection.viewport_width_px as f32,
        viewport_height: selection.viewport_height_px as f32,
    };
    settings.validate()?;
    Ok(settings)
}

fn required_region(
    layout: &litd_engine::render::ui::HudReferenceLayout,
    kind: HudRegionKind,
) -> Result<UiRect, EngineError> {
    match layout.region_rect(kind) {
        Some(rect) => Ok(rect),
        None => invalid_builder("region", format!("{kind:?} is not in preset")),
    }
}

fn rect_in_region(region: UiRect, rect: HudNormalizedRect) -> Result<UiRect, EngineError> {
    let size = region.size();
    UiRect::new(region.min + size * rect.min, region.min + size * rect.max)
}

fn validate_rect(rect: HudNormalizedRect) -> Result<(), EngineError> {
    if !rect.min.is_finite()
        || !rect.max.is_finite()
        || rect.min.cmplt(Vec2::ZERO).any()
        || rect.max.cmpgt(Vec2::ONE).any()
        || rect.min.x >= rect.max.x
        || rect.min.y >= rect.max.y
    {
        return invalid_builder(
            "rect",
            "normalized rect must be finite, ordered, and inside 0..=1",
        );
    }
    Ok(())
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_builder(field, "value must be a non-empty single line");
    }
    Ok(())
}

fn invalid_builder<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.hud_ui_builder.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "hud_ui_builder_tests.rs"]
mod tests;
