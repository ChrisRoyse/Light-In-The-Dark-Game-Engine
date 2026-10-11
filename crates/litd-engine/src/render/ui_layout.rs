use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    math::{Vec2, Vec4},
};

use super::{UiRect, UiRenderSettings};

const REFERENCE_VIEWPORT: Vec2 = Vec2::new(1920.0, 1080.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HudLayoutPreset {
    EditorWorldView,
    PlayTopDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MinimapShape {
    Rectangular,
    Circular,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HudRegionKind {
    TopModeBar,
    LeftHierarchy,
    RightInspector,
    ContentBrowser,
    CommandBar,
    ChatPanel,
    QuestTracker,
    BossBar,
    Minimap,
    StatusStrip,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HudRegion {
    pub kind: HudRegionKind,
    pub rect_px: UiRect,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HudStyleTokens {
    pub panel_color: Vec4,
    pub border_color: Vec4,
    pub accent_color: Vec4,
    pub text_primary_color: Vec4,
}

impl HudStyleTokens {
    pub fn dark_gold() -> Self {
        Self {
            panel_color: Vec4::new(0.02, 0.03, 0.06, 0.84),
            border_color: Vec4::new(0.87, 0.67, 0.32, 1.0),
            accent_color: Vec4::new(1.0, 0.76, 0.26, 1.0),
            text_primary_color: Vec4::new(0.95, 0.91, 0.82, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HudReferenceLayout {
    pub preset: HudLayoutPreset,
    pub minimap_shape: MinimapShape,
    pub minimap_rect_px: UiRect,
    pub command_slot_count: u32,
    pub style: HudStyleTokens,
    pub regions: Vec<HudRegion>,
    pub reference_images: Vec<String>,
}

impl HudReferenceLayout {
    pub fn region_rect(&self, kind: HudRegionKind) -> Option<UiRect> {
        self.regions
            .iter()
            .find(|region| region.kind == kind)
            .map(|region| region.rect_px)
    }

    pub fn reference_image_list(&self) -> String {
        self.reference_images.join("|")
    }
}

pub fn reference_hud_layout(
    settings: UiRenderSettings,
    preset: HudLayoutPreset,
) -> Result<HudReferenceLayout, EngineError> {
    settings.validate()?;
    match preset {
        HudLayoutPreset::EditorWorldView => editor_world_view(settings),
        HudLayoutPreset::PlayTopDown => play_top_down(settings),
    }
}

fn editor_world_view(settings: UiRenderSettings) -> Result<HudReferenceLayout, EngineError> {
    let viewport = settings.viewport_size();
    let chrome_scale = editor_world_chrome_scale(settings)?;
    let top_height = 74.0 * chrome_scale;
    let left_width = 410.0 * chrome_scale;
    let right_width = 372.0 * chrome_scale;
    let content_height = 222.0 * chrome_scale;
    let status_height = 38.0 * chrome_scale;
    let minimap_width = 256.0 * chrome_scale;
    let minimap_height = 272.0 * chrome_scale;
    let right_edge_gap = 20.0 * chrome_scale;
    let minimap_status_gap = 24.0 * chrome_scale;

    let right_min_x = viewport.x - right_width;
    let status_min_y = viewport.y - status_height;
    let content_min_y = status_min_y - content_height;
    let minimap_max_y = status_min_y - minimap_status_gap;
    let minimap_min_y = minimap_max_y - minimap_height;
    let minimap_min_x = viewport.x - right_edge_gap - minimap_width;
    if right_min_x <= left_width || content_min_y <= top_height || minimap_min_y <= top_height {
        return invalid_layout(
            "editor_world_view.regions",
            format!(
                "viewport {:.1}x{:.1} is too small for docked editor chrome at scale {:.3}",
                viewport.x, viewport.y, chrome_scale
            ),
        );
    }

    let regions = vec![
        region_rect(
            HudRegionKind::TopModeBar,
            Vec2::new(0.0, 0.0),
            Vec2::new(viewport.x, top_height),
        )?,
        region_rect(
            HudRegionKind::LeftHierarchy,
            Vec2::new(0.0, top_height),
            Vec2::new(left_width, status_min_y),
        )?,
        region_rect(
            HudRegionKind::RightInspector,
            Vec2::new(right_min_x, top_height),
            Vec2::new(viewport.x, minimap_min_y),
        )?,
        region_rect(
            HudRegionKind::ContentBrowser,
            Vec2::new(left_width, content_min_y),
            Vec2::new(right_min_x, status_min_y),
        )?,
        region_rect(
            HudRegionKind::Minimap,
            Vec2::new(minimap_min_x, minimap_min_y),
            Vec2::new(minimap_min_x + minimap_width, minimap_max_y),
        )?,
        region_rect(
            HudRegionKind::StatusStrip,
            Vec2::new(0.0, status_min_y),
            Vec2::new(viewport.x, viewport.y),
        )?,
    ];
    Ok(HudReferenceLayout {
        preset: HudLayoutPreset::EditorWorldView,
        minimap_shape: MinimapShape::Rectangular,
        minimap_rect_px: UiRect::new(
            Vec2::new(minimap_min_x, minimap_min_y),
            Vec2::new(minimap_min_x + minimap_width, minimap_max_y),
        )?,
        command_slot_count: 0,
        style: HudStyleTokens::dark_gold(),
        regions,
        reference_images: vec![
            "docs/readme/ED11-world-view.png".to_string(),
            "docs/readme/ED01-editor-shell.png".to_string(),
        ],
    })
}

fn editor_world_chrome_scale(settings: UiRenderSettings) -> Result<f32, EngineError> {
    let viewport = settings.viewport_size();
    let scale = (viewport.x / REFERENCE_VIEWPORT.x)
        .min(viewport.y / REFERENCE_VIEWPORT.y)
        .min(1.0);
    if !scale.is_finite() || scale <= 0.0 {
        return invalid_layout(
            "editor_world_view.chrome_scale",
            "computed chrome scale must be finite and positive",
        );
    }
    Ok(scale)
}

fn play_top_down(settings: UiRenderSettings) -> Result<HudReferenceLayout, EngineError> {
    let regions = vec![
        region(
            settings,
            HudRegionKind::TopModeBar,
            [0.0, 0.0],
            [1920.0, 92.0],
        )?,
        region(
            settings,
            HudRegionKind::BossBar,
            [600.0, 24.0],
            [1320.0, 92.0],
        )?,
        region(
            settings,
            HudRegionKind::QuestTracker,
            [24.0, 170.0],
            [450.0, 410.0],
        )?,
        region(
            settings,
            HudRegionKind::ChatPanel,
            [24.0, 688.0],
            [486.0, 956.0],
        )?,
        region(
            settings,
            HudRegionKind::CommandBar,
            [560.0, 912.0],
            [1360.0, 1068.0],
        )?,
        region(
            settings,
            HudRegionKind::Minimap,
            [1648.0, 48.0],
            [1896.0, 296.0],
        )?,
        region(
            settings,
            HudRegionKind::StatusStrip,
            [0.0, 1038.0],
            [1920.0, 1080.0],
        )?,
    ];
    Ok(HudReferenceLayout {
        preset: HudLayoutPreset::PlayTopDown,
        minimap_shape: MinimapShape::Circular,
        minimap_rect_px: rect(settings, [1648.0, 48.0], [1896.0, 296.0])?,
        command_slot_count: 10,
        style: HudStyleTokens::dark_gold(),
        regions,
        reference_images: vec![
            "docs/readme/ED12-play-topdown.png".to_string(),
            "docs/readme/GP03-combat.png".to_string(),
            "docs/readme/GP09-inventory.png".to_string(),
            "docs/readme/GP11-quest.png".to_string(),
        ],
    })
}

fn region(
    settings: UiRenderSettings,
    kind: HudRegionKind,
    min: [f32; 2],
    max: [f32; 2],
) -> Result<HudRegion, EngineError> {
    let rect = rect(settings, min, max)?;
    region_rect(kind, rect.min, rect.max)
}

fn region_rect(kind: HudRegionKind, min: Vec2, max: Vec2) -> Result<HudRegion, EngineError> {
    Ok(HudRegion {
        kind,
        rect_px: UiRect::new(min, max)?,
    })
}

fn rect(settings: UiRenderSettings, min: [f32; 2], max: [f32; 2]) -> Result<UiRect, EngineError> {
    let scale = settings.viewport_size() / REFERENCE_VIEWPORT;
    UiRect::new(
        Vec2::new(min[0] * scale.x, min[1] * scale.y),
        Vec2::new(max[0] * scale.x, max[1] * scale.y),
    )
}

fn invalid_layout<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}
