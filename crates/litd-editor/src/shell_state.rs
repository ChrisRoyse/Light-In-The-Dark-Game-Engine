use litd_engine::{
    EngineError,
    math::{Vec2, Vec3},
    render::ui::{UiRect, UiRenderSettings},
};
use serde::{Deserialize, Serialize};

use super::{DockPanelKind, EditorMode, EditorShellPlan, build_editor_shell_plan, invalid_shell};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorldViewCameraState {
    pub target_world: Vec3,
    pub distance: f32,
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
}

impl WorldViewCameraState {
    pub fn validate(&self) -> Result<(), EngineError> {
        if !self.target_world.is_finite()
            || !self.distance.is_finite()
            || !self.yaw_degrees.is_finite()
            || !self.pitch_degrees.is_finite()
            || self.distance <= 0.0
            || self.pitch_degrees < -89.0
            || self.pitch_degrees > 89.0
        {
            return invalid_shell(
                "camera",
                "camera target, positive distance, yaw, and pitch -89..89 must be finite and valid",
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInspectorState {
    pub mode: EditorMode,
    pub title: String,
    pub tool_id: String,
    pub operation_namespace: String,
    pub primary_controls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DockPanelPersistence {
    pub kind: DockPanelKind,
    pub rect_px: UiRect,
    pub floating_rect_px: Option<UiRect>,
    pub docked: bool,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorShellPersistence {
    pub active_mode: EditorMode,
    pub camera: WorldViewCameraState,
    pub panels: Vec<DockPanelPersistence>,
}

pub fn switch_editor_mode(
    plan: &EditorShellPlan,
    mode: EditorMode,
) -> Result<EditorShellPlan, EngineError> {
    if !plan.mode_tabs.contains(&mode) {
        return invalid_shell(
            "mode_tabs.switch",
            format!("mode {:?} is not available", mode),
        );
    }
    let mut next = plan.clone();
    next.active_mode = mode;
    next.active_inspector = inspector_for_mode(mode);
    next.validate()?;
    Ok(next)
}

pub fn pop_out_panel(
    plan: &EditorShellPlan,
    kind: DockPanelKind,
    floating_rect_px: UiRect,
) -> Result<EditorShellPlan, EngineError> {
    if matches!(kind, DockPanelKind::TopModeBar | DockPanelKind::StatusStrip) {
        return invalid_shell(
            "dock_panels.popout",
            "top mode bar and status strip are fixed world-view chrome",
        );
    }
    let mut next = plan.clone();
    let panel = next
        .dock_panels
        .iter_mut()
        .find(|panel| panel.kind == kind)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.shell.dock_panels.popout".to_string(),
            reason: format!("panel {:?} is missing", kind),
        })?;
    panel.docked = false;
    panel.visible = true;
    panel.floating_rect_px = Some(floating_rect_px);
    next.validate()?;
    Ok(next)
}

pub fn reattach_panel(
    plan: &EditorShellPlan,
    kind: DockPanelKind,
) -> Result<EditorShellPlan, EngineError> {
    let mut next = plan.clone();
    let panel = next
        .dock_panels
        .iter_mut()
        .find(|panel| panel.kind == kind)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.shell.dock_panels.reattach".to_string(),
            reason: format!("panel {:?} is missing", kind),
        })?;
    panel.docked = true;
    panel.visible = true;
    panel.floating_rect_px = None;
    next.validate()?;
    Ok(next)
}

pub fn persist_shell_state(plan: &EditorShellPlan) -> Result<EditorShellPersistence, EngineError> {
    plan.validate()?;
    Ok(EditorShellPersistence {
        active_mode: plan.active_mode,
        camera: plan.viewport.camera,
        panels: plan
            .dock_panels
            .iter()
            .map(|panel| DockPanelPersistence {
                kind: panel.kind,
                rect_px: panel.rect_px,
                floating_rect_px: panel.floating_rect_px,
                docked: panel.docked,
                visible: panel.visible,
            })
            .collect(),
    })
}

pub fn restore_shell_state(
    settings: UiRenderSettings,
    persistence: &EditorShellPersistence,
) -> Result<EditorShellPlan, EngineError> {
    let mut plan = build_editor_shell_plan(settings)?;
    plan = switch_editor_mode(&plan, persistence.active_mode)?;
    plan.viewport.camera = persistence.camera;
    for stored in &persistence.panels {
        let Some(panel) = plan
            .dock_panels
            .iter_mut()
            .find(|panel| panel.kind == stored.kind)
        else {
            return invalid_shell(
                "dock_persistence.panel",
                format!("stored panel {:?} is unknown", stored.kind),
            );
        };
        panel.docked = stored.docked;
        panel.visible = stored.visible;
        if stored.docked {
            panel.floating_rect_px = None;
        } else {
            panel.floating_rect_px = Some(clamp_floating_rect(
                stored.floating_rect_px.unwrap_or(stored.rect_px),
                settings,
            )?);
        }
    }
    plan.validate()?;
    Ok(plan)
}

fn clamp_floating_rect(rect: UiRect, settings: UiRenderSettings) -> Result<UiRect, EngineError> {
    settings.validate()?;
    let margin = 12.0;
    let viewport = settings.viewport_size();
    let available = Vec2::new(
        (viewport.x - margin * 2.0).max(1.0),
        (viewport.y - margin * 2.0).max(1.0),
    );
    let size = rect.size().min(available).max(Vec2::ONE);
    let max_min = Vec2::new(viewport.x - margin - size.x, viewport.y - margin - size.y)
        .max(Vec2::splat(margin));
    let min = Vec2::new(
        rect.min.x.clamp(margin, max_min.x),
        rect.min.y.clamp(margin, max_min.y),
    );
    UiRect::new(min, min + size)
}

pub(super) fn inspector_for_mode(mode: EditorMode) -> ToolInspectorState {
    let (title, tool_id, controls): (&str, &str, &[&str]) = match mode {
        EditorMode::Terrain => (
            "Doodad Brush",
            "editor.tool.terrain.brush",
            &[
                "brush_radius",
                "falloff",
                "placement",
                "scale_min_max",
                "density",
            ],
        ),
        EditorMode::Objects => (
            "Object Data",
            "editor.tool.objects.data",
            &["unit_stats", "abilities", "model", "collision", "loot"],
        ),
        EditorMode::Triggers => (
            "Trigger ECA",
            "editor.tool.triggers.eca",
            &[
                "events",
                "conditions",
                "actions",
                "variables",
                "test_trigger",
            ],
        ),
        EditorMode::Cinematics => (
            "Cinematic Director",
            "editor.tool.cinematics.director",
            &["shot_list", "camera", "lighting", "timeline", "preview"],
        ),
        EditorMode::Vfx => (
            "VFX Graph",
            "editor.tool.vfx.graph",
            &["emitter", "curve", "color_ramp", "bounds", "preview"],
        ),
        EditorMode::AiCoAuthor => (
            "AI Co-Author",
            "editor.tool.ai.coauthor",
            &["prompt", "grounding", "proposal", "lowering", "apply"],
        ),
    };
    ToolInspectorState {
        mode,
        title: title.to_string(),
        tool_id: tool_id.to_string(),
        operation_namespace: format!(
            "editor.{}",
            mode.label().to_ascii_lowercase().replace(' ', "_")
        ),
        primary_controls: controls.iter().map(|control| control.to_string()).collect(),
    }
}
