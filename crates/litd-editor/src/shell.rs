//! Egui-ready editor shell layout: docked panels, live viewport, gizmo, and snapping.

use serde::{Deserialize, Serialize};

use litd_engine::{
    EngineError,
    math::{Aabb, Vec2, Vec3},
    render::ui::{HudLayoutPreset, HudRegionKind, UiRect, UiRenderSettings, reference_hud_layout},
};

#[path = "shell_camera.rs"]
mod shell_camera;
#[path = "shell_state.rs"]
mod shell_state;

pub use shell_camera::{
    ED11_BIRD_EYE_CAMERA_PROFILE_ID, WORLD_VIEW_CAMERA_BOOKMARK_SLOT_COUNT,
    WorldViewCameraBookmark, WorldViewCameraControlInput, WorldViewFrameSelection,
    apply_world_view_camera_control, clamp_world_view_camera_to_profile,
    ed11_bird_eye_default_camera, frame_world_view_camera_on_selection,
    previous_schema_eight_stock_camera, restore_world_view_camera_bookmark,
    save_world_view_camera_bookmark, set_world_view_camera_target_xz,
    validate_world_view_camera_bookmarks, world_view_camera_bookmark_summary,
};
pub use shell_state::{
    DockPanelPersistence, EditorShellPersistence, ToolInspectorState, WorldViewCameraState,
    persist_shell_state, pop_out_panel, reattach_panel, restore_shell_state, switch_editor_mode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorMode {
    Terrain,
    Objects,
    Triggers,
    Cinematics,
    Vfx,
    AiCoAuthor,
}

impl EditorMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Terrain => "Terrain",
            Self::Objects => "Objects",
            Self::Triggers => "Triggers",
            Self::Cinematics => "Cinematics",
            Self::Vfx => "VFX",
            Self::AiCoAuthor => "AI Co-Author",
        }
    }

    pub const fn compact_label(self) -> &'static str {
        match self {
            Self::Terrain => "Terrain",
            Self::Objects => "Obj",
            Self::Triggers => "Trig",
            Self::Cinematics => "Cine",
            Self::Vfx => "VFX",
            Self::AiCoAuthor => "AI",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DockPanelKind {
    TopModeBar,
    LeftHierarchy,
    RightInspector,
    ContentBrowser,
    Minimap,
    StatusStrip,
}

impl DockPanelKind {
    pub const fn egui_suffix(self) -> &'static str {
        match self {
            Self::TopModeBar => "top_mode_bar",
            Self::LeftHierarchy => "left_hierarchy",
            Self::RightInspector => "right_inspector",
            Self::ContentBrowser => "content_browser",
            Self::Minimap => "minimap",
            Self::StatusStrip => "status_strip",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DockPanel {
    pub kind: DockPanelKind,
    pub title: String,
    pub egui_id: String,
    pub rect_px: UiRect,
    pub floating_rect_px: Option<UiRect>,
    pub docked: bool,
    pub visible: bool,
}

impl DockPanel {
    pub fn egui_rect(&self) -> egui::Rect {
        rect_to_egui(self.rect_px)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransformGizmoMode {
    Translate,
    Rotate,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SnapSettings {
    pub grid_enabled: bool,
    pub grid_size: f32,
    pub rotation_enabled: bool,
    pub rotation_degrees: f32,
    pub scale_enabled: bool,
    pub scale_step: f32,
}

impl SnapSettings {
    pub fn validate(&self) -> Result<(), EngineError> {
        if !self.grid_size.is_finite() || self.grid_size <= 0.0 {
            return invalid_shell("snap.grid_size", "must be finite and positive");
        }
        if !self.rotation_degrees.is_finite() || self.rotation_degrees <= 0.0 {
            return invalid_shell("snap.rotation_degrees", "must be finite and positive");
        }
        if !self.scale_step.is_finite() || self.scale_step <= 0.0 {
            return invalid_shell("snap.scale_step", "must be finite and positive");
        }
        Ok(())
    }

    pub fn snap_translation_delta(&self, delta: Vec3) -> Vec3 {
        if !self.grid_enabled {
            return delta;
        }
        Vec3::new(
            snap_value(delta.x, self.grid_size),
            snap_value(delta.y, self.grid_size),
            snap_value(delta.z, self.grid_size),
        )
    }

    pub fn snap_rotation_degrees(&self, degrees: f32) -> f32 {
        if self.rotation_enabled {
            snap_value(degrees, self.rotation_degrees)
        } else {
            degrees
        }
    }

    pub fn snap_scale_factor(&self, scale: f32) -> f32 {
        if self.scale_enabled {
            snap_value(scale, self.scale_step)
        } else {
            scale
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransformGizmoState {
    pub visible: bool,
    pub mode: TransformGizmoMode,
    pub selected_entities: u32,
    pub pivot_world: Vec3,
    pub anchor_px: Vec2,
    pub snap: SnapSettings,
}

impl TransformGizmoState {
    pub fn validate(&self) -> Result<(), EngineError> {
        if !self.pivot_world.is_finite() || !self.anchor_px.is_finite() {
            return invalid_shell("gizmo", "pivot and anchor must be finite");
        }
        self.snap.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorViewportState {
    pub egui_id: String,
    pub rect_px: UiRect,
    pub world_bounds: Aabb,
    pub camera: WorldViewCameraState,
    pub live_engine_view: bool,
}

impl EditorViewportState {
    pub fn egui_rect(&self) -> egui::Rect {
        rect_to_egui(self.rect_px)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EguiShellIds {
    pub root_id: String,
    pub dock_area_id: String,
    pub central_viewport_id: String,
    pub gizmo_layer_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorShellPlan {
    pub settings: UiRenderSettings,
    pub active_mode: EditorMode,
    pub mode_tabs: Vec<EditorMode>,
    pub egui: EguiShellIds,
    pub dock_panels: Vec<DockPanel>,
    pub viewport: EditorViewportState,
    pub active_inspector: ToolInspectorState,
    pub gizmo: TransformGizmoState,
    pub reference_images: Vec<String>,
}

impl EditorShellPlan {
    pub fn panel(&self, kind: DockPanelKind) -> Option<&DockPanel> {
        self.dock_panels.iter().find(|panel| panel.kind == kind)
    }

    pub fn dock_panel_summary(&self) -> String {
        self.dock_panels
            .iter()
            .map(|panel| format!("{:?}:{}", panel.kind, rect_summary(panel.rect_px)))
            .collect::<Vec<_>>()
            .join("|")
    }

    pub fn mode_tab_summary(&self) -> String {
        self.mode_tabs
            .iter()
            .map(|mode| mode.label())
            .collect::<Vec<_>>()
            .join("|")
    }

    pub fn reference_image_list(&self) -> String {
        self.reference_images.join("|")
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        self.settings.validate()?;
        if self.mode_tabs.is_empty() {
            return invalid_shell("mode_tabs", "must contain at least one mode");
        }
        if self.dock_panels.len() < 6 {
            return invalid_shell("dock_panels", "must contain the fixed world-view docks");
        }
        if self.reference_images.is_empty() {
            return invalid_shell("reference_images", "must name README visual references");
        }
        self.viewport.rect_px.validate("editor.viewport")?;
        validate_aabb("editor.viewport.world_bounds", self.viewport.world_bounds)?;
        if !self.viewport.live_engine_view {
            return invalid_shell(
                "viewport.live_engine_view",
                "must reserve the live engine view",
            );
        }
        self.viewport.camera.validate()?;
        if self.active_inspector.mode != self.active_mode {
            return invalid_shell(
                "active_inspector",
                "active inspector must match the active mode tab",
            );
        }
        if self.active_inspector.primary_controls.is_empty() {
            return invalid_shell(
                "active_inspector.controls",
                "active inspector must expose mode controls",
            );
        }
        for panel in &self.dock_panels {
            panel
                .rect_px
                .validate(format!("editor.panel.{:?}", panel.kind))?;
            if let Some(rect) = panel.floating_rect_px {
                rect.validate(format!("editor.panel.{:?}.floating", panel.kind))?;
            }
            if !panel.visible && !panel.docked {
                return invalid_shell(
                    "dock_panels.visibility",
                    "floating panels must remain visible or be explicitly re-docked first",
                );
            }
        }
        self.gizmo.validate()
    }
}

pub fn build_editor_shell_plan(settings: UiRenderSettings) -> Result<EditorShellPlan, EngineError> {
    settings.validate()?;
    let reference = reference_hud_layout(settings, HudLayoutPreset::EditorWorldView)?;
    let top = required_rect(&reference, HudRegionKind::TopModeBar)?;
    let left = required_rect(&reference, HudRegionKind::LeftHierarchy)?;
    let right = required_rect(&reference, HudRegionKind::RightInspector)?;
    let content = required_rect(&reference, HudRegionKind::ContentBrowser)?;
    let minimap = required_rect(&reference, HudRegionKind::Minimap)?;
    let status = required_rect(&reference, HudRegionKind::StatusStrip)?;
    let viewport_rect = UiRect::new(
        Vec2::new(left.max.x, top.max.y),
        Vec2::new(right.min.x, content.min.y),
    )?;
    let viewport_center = viewport_rect.min + viewport_rect.size() * 0.5;

    let plan = EditorShellPlan {
        settings,
        active_mode: EditorMode::Terrain,
        mode_tabs: vec![
            EditorMode::Terrain,
            EditorMode::Objects,
            EditorMode::Triggers,
            EditorMode::Cinematics,
            EditorMode::Vfx,
            EditorMode::AiCoAuthor,
        ],
        egui: EguiShellIds {
            root_id: "editor.shell.root".to_string(),
            dock_area_id: "editor.shell.dock_area".to_string(),
            central_viewport_id: "editor.viewport.world".to_string(),
            gizmo_layer_id: "editor.viewport.gizmo".to_string(),
        },
        dock_panels: vec![
            panel(DockPanelKind::TopModeBar, "Mode Tabs", top),
            panel(DockPanelKind::LeftHierarchy, "World Hierarchy", left),
            panel(DockPanelKind::RightInspector, "Tool Inspector", right),
            panel(DockPanelKind::ContentBrowser, "Content Browser", content),
            panel(DockPanelKind::Minimap, "World Map", minimap),
            panel(DockPanelKind::StatusStrip, "Status", status),
        ],
        viewport: EditorViewportState {
            egui_id: "editor.viewport.world".to_string(),
            rect_px: viewport_rect,
            world_bounds: Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(128.0))?,
            camera: shell_camera::ed11_bird_eye_default_camera(),
            live_engine_view: true,
        },
        active_inspector: shell_state::inspector_for_mode(EditorMode::Terrain),
        gizmo: TransformGizmoState {
            visible: true,
            mode: TransformGizmoMode::Translate,
            selected_entities: 1,
            pivot_world: Vec3::ZERO,
            anchor_px: viewport_center,
            snap: SnapSettings {
                grid_enabled: true,
                grid_size: 1.0,
                rotation_enabled: true,
                rotation_degrees: 15.0,
                scale_enabled: true,
                scale_step: 0.1,
            },
        },
        reference_images: reference.reference_images,
    };
    plan.validate()?;
    Ok(plan)
}

pub fn rect_summary(rect: UiRect) -> String {
    format!(
        "{:.1},{:.1}->{:.1},{:.1}",
        rect.min.x, rect.min.y, rect.max.x, rect.max.y
    )
}

fn panel(kind: DockPanelKind, title: &str, rect_px: UiRect) -> DockPanel {
    DockPanel {
        kind,
        title: title.to_string(),
        egui_id: format!("editor.panel.{}", kind.egui_suffix()),
        rect_px,
        floating_rect_px: None,
        docked: true,
        visible: true,
    }
}

fn required_rect(
    reference: &litd_engine::render::ui::HudReferenceLayout,
    kind: HudRegionKind,
) -> Result<UiRect, EngineError> {
    reference
        .region_rect(kind)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: format!("editor.reference.{kind:?}"),
            reason: "required editor shell region is missing".to_string(),
        })
}

fn rect_to_egui(rect: UiRect) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(rect.min.x, rect.min.y),
        egui::pos2(rect.max.x, rect.max.y),
    )
}

fn validate_aabb(field: &'static str, bounds: Aabb) -> Result<(), EngineError> {
    if !bounds.min.is_finite()
        || !bounds.max.is_finite()
        || bounds.min.x >= bounds.max.x
        || bounds.min.y >= bounds.max.y
        || bounds.min.z >= bounds.max.z
    {
        return invalid_shell(field, "bounds must be finite with ordered extents");
    }
    Ok(())
}

fn snap_value(value: f32, step: f32) -> f32 {
    (value / step).round() * step
}

pub(super) fn invalid_shell<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.shell.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "shell_tests.rs"]
mod tests;
