use egui::{Color32, Pos2, Rect, RichText, Stroke, StrokeKind, Vec2};
use litd_editor::asset_scene::{EditorAssetSceneReport, EditorWorldMapReport};

use super::main_editor_frame::{fixed_panel, panel_width};
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MinimapLayerVisibility {
    pub(super) areas: bool,
    pub(super) markers: bool,
    pub(super) rings: bool,
}

impl Default for MinimapLayerVisibility {
    fn default() -> Self {
        Self {
            areas: true,
            markers: true,
            rings: true,
        }
    }
}

impl MinimapLayerVisibility {
    pub(super) fn summary(self) -> String {
        format!(
            "areas={};markers={};rings={}",
            self.areas, self.markers, self.rings
        )
    }
}

#[derive(Debug, Clone)]
pub(super) struct EditorMinimapModel {
    source_id: String,
    source_path: String,
    map: EditorWorldMapReport,
}

impl EditorMinimapModel {
    pub(super) fn from_scene_report(report: &EditorAssetSceneReport) -> Result<Self, EngineError> {
        let map = EditorWorldMapReport::new(
            report.world_map.source_kind.clone(),
            report.world_map.bounds_min,
            report.world_map.bounds_max,
            report.world_map.outline.clone(),
            report.world_map.areas.clone(),
            report.world_map.points.clone(),
            report.world_map.rings.clone(),
        )?;
        Ok(Self {
            source_id: report.product_id.clone(),
            source_path: report.source_relative_path.clone(),
            map,
        })
    }

    pub(super) fn source_id(&self) -> &str {
        &self.source_id
    }

    pub(super) fn source_path(&self) -> &str {
        &self.source_path
    }

    pub(super) fn source_kind(&self) -> &str {
        &self.map.source_kind
    }

    pub(super) fn world_bounds_summary(&self) -> String {
        format!(
            "{:.1},{:.1}->{:.1},{:.1}",
            self.map.bounds_min[0],
            self.map.bounds_min[1],
            self.map.bounds_max[0],
            self.map.bounds_max[1]
        )
    }

    pub(super) fn feature_count(&self) -> usize {
        self.map.feature_count()
    }

    pub(super) fn area_count(&self) -> usize {
        self.map.areas.len()
    }

    pub(super) fn area_count_by_kind(&self, kind: &str) -> usize {
        self.map
            .areas
            .iter()
            .filter(|area| area.kind == kind)
            .count()
    }

    pub(super) fn marker_count(&self) -> usize {
        self.map.points.len()
    }

    pub(super) fn marker_count_by_kind(&self, kind: &str) -> usize {
        self.map
            .points
            .iter()
            .filter(|point| point.kind == kind)
            .count()
    }

    pub(super) fn ring_count(&self) -> usize {
        self.map.rings.len()
    }

    pub(super) fn viewport_world_rect_summary(
        &self,
        camera_target_xz: [f32; 2],
        camera_distance: f32,
        viewport_aspect: f32,
    ) -> Result<String, EngineError> {
        let rect = minimap_viewport_world_rect(camera_target_xz, camera_distance, viewport_aspect)?;
        Ok(format!(
            "{:.1},{:.1}->{:.1},{:.1}",
            rect.min_xz[0], rect.min_xz[1], rect.max_xz[0], rect.max_xz[1]
        ))
    }

    #[cfg(test)]
    pub(super) fn world_center_xz(&self) -> [f32; 2] {
        [
            (self.map.bounds_min[0] + self.map.bounds_max[0]) * 0.5,
            (self.map.bounds_min[1] + self.map.bounds_max[1]) * 0.5,
        ]
    }
}

impl EditorShellRuntime {
    pub(super) fn show_minimap(&mut self, ctx: &egui::Context) {
        let Some(rect) = self.panel_rect(DockPanelKind::Minimap) else {
            return;
        };
        fixed_panel(ctx, "editor.minimap", rect, 3, |ui| {
            ui.label(RichText::new("World Map").color(TEXT).strong());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.checkbox(&mut self.minimap_layers.areas, "A")
                    .on_hover_text("Area overlays");
                ui.checkbox(&mut self.minimap_layers.markers, "M")
                    .on_hover_text("Placement markers");
                ui.checkbox(&mut self.minimap_layers.rings, "R")
                    .on_hover_text("Selection rings");
            });
            let (map_rect, response) = ui.allocate_exact_size(
                egui::vec2(panel_width(ui), (ui.available_height() - 4.0).max(64.0)),
                egui::Sense::click_and_drag(),
            );
            if (response.clicked() || response.dragged())
                && let Some(pointer) = response.interact_pointer_pos()
            {
                match self.navigate_minimap_from_screen(map_rect, pointer) {
                    Ok(true) | Ok(false) => {}
                    Err(err) => tracing::error!(error = %err, "minimap navigation failed"),
                }
            }
            let camera = self.plan.viewport.camera;
            let viewport_aspect = (self.plan.viewport.rect_px.size().x
                / self.plan.viewport.rect_px.size().y)
                .max(0.1);
            draw_scene_minimap(
                ui,
                map_rect,
                &self.minimap,
                [camera.target_world.x, camera.target_world.z],
                camera.distance,
                viewport_aspect,
                self.minimap_layers,
            );
        });
    }

    pub(super) fn navigate_minimap_from_screen(
        &mut self,
        map_rect: Rect,
        pointer: Pos2,
    ) -> Result<bool, EngineError> {
        let plot = minimap_plot_rect(map_rect)?;
        if !plot.contains(pointer) {
            return Ok(false);
        }
        let projection = MapProjection::new(&self.minimap.map, plot)?;
        let xz = projection.unproject(pointer)?;
        self.set_world_view_camera_target_xz(xz)?;
        self.minimap_navigation_count += 1;
        self.last_minimap_navigation_xz = Some(xz);
        Ok(true)
    }
}

fn draw_scene_minimap(
    ui: &mut egui::Ui,
    rect: Rect,
    model: &EditorMinimapModel,
    camera_target_xz: [f32; 2],
    camera_distance: f32,
    viewport_aspect: f32,
    layers: MinimapLayerVisibility,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2, Color32::from_rgb(8, 11, 16));
    painter.rect_stroke(rect, 2, Stroke::new(1.0, GOLD), StrokeKind::Inside);
    let Ok(plot) = minimap_plot_rect(rect) else {
        return;
    };
    let Ok(projection) = MapProjection::new(&model.map, plot) else {
        return;
    };

    let outline = model
        .map
        .outline
        .iter()
        .map(|point| projection.project(*point))
        .collect::<Vec<_>>();
    painter.add(egui::Shape::convex_polygon(
        outline,
        Color32::from_rgb(58, 77, 82),
        Stroke::new(1.0, Color32::from_rgb(111, 116, 100)),
    ));

    if layers.areas {
        for area in &model.map.areas {
            let points = area
                .points
                .iter()
                .map(|point| projection.project(*point))
                .collect::<Vec<_>>();
            let (fill, stroke) = area_style(&area.kind);
            painter.add(egui::Shape::convex_polygon(points, fill, stroke));
        }
    }

    if layers.markers {
        for point in &model.map.points {
            let radius = (point.radius * projection.scale * 0.38).clamp(2.0, 6.0);
            painter.circle_filled(
                projection.project(point.world_xz),
                radius,
                marker_color(&point.kind),
            );
        }
    }

    if layers.rings {
        for ring in &model.map.rings {
            painter.circle_stroke(
                projection.project(ring.center_xz),
                projection.length(ring.radius).max(2.0),
                Stroke::new(2.0, GOLD),
            );
        }
    }

    if let Ok(viewport) =
        minimap_viewport_world_rect(camera_target_xz, camera_distance, viewport_aspect)
    {
        let viewport_points = viewport
            .corners()
            .into_iter()
            .map(|point| projection.project(point))
            .collect::<Vec<_>>();
        painter.add(egui::Shape::closed_line(
            viewport_points,
            Stroke::new(2.0, GOLD),
        ));
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MinimapViewportWorldRect {
    min_xz: [f32; 2],
    max_xz: [f32; 2],
}

impl MinimapViewportWorldRect {
    fn corners(self) -> [[f32; 2]; 4] {
        [
            [self.min_xz[0], self.min_xz[1]],
            [self.max_xz[0], self.min_xz[1]],
            [self.max_xz[0], self.max_xz[1]],
            [self.min_xz[0], self.max_xz[1]],
        ]
    }
}

fn minimap_viewport_world_rect(
    camera_target_xz: [f32; 2],
    camera_distance: f32,
    viewport_aspect: f32,
) -> Result<MinimapViewportWorldRect, EngineError> {
    if camera_target_xz.iter().any(|value| !value.is_finite())
        || !camera_distance.is_finite()
        || camera_distance <= 0.0
        || !viewport_aspect.is_finite()
        || viewport_aspect <= 0.0
    {
        return invalid_shell_runtime(
            "minimap.viewport_world_rect",
            "camera target, distance, and viewport aspect must be finite and positive",
        );
    }
    let view_half_y = (camera_distance * 0.16).clamp(16.0, 54.0);
    let view_half_x = (view_half_y * viewport_aspect).clamp(20.0, 72.0);
    Ok(MinimapViewportWorldRect {
        min_xz: [
            camera_target_xz[0] - view_half_x,
            camera_target_xz[1] - view_half_y,
        ],
        max_xz: [
            camera_target_xz[0] + view_half_x,
            camera_target_xz[1] + view_half_y,
        ],
    })
}

fn minimap_plot_rect(rect: Rect) -> Result<Rect, EngineError> {
    if !rect.min.x.is_finite()
        || !rect.min.y.is_finite()
        || !rect.max.x.is_finite()
        || !rect.max.y.is_finite()
    {
        return invalid_shell_runtime("minimap.rect", "minimap rectangle must be finite");
    }
    let plot = rect.shrink(8.0);
    if plot.width() <= 1.0 || plot.height() <= 1.0 {
        return invalid_shell_runtime(
            "minimap.plot",
            "minimap plot area must remain larger than 1 point after padding",
        );
    }
    Ok(plot)
}

fn area_style(kind: &str) -> (Color32, Stroke) {
    match kind {
        "path" => (
            Color32::from_rgba_unmultiplied(148, 118, 76, 190),
            Stroke::new(0.8, Color32::from_rgb(94, 78, 48)),
        ),
        "rift_blue" => (
            Color32::from_rgba_unmultiplied(36, 170, 245, 210),
            Stroke::new(0.8, Color32::from_rgb(54, 184, 255)),
        ),
        "rift_lava" => (
            Color32::from_rgba_unmultiplied(244, 96, 43, 210),
            Stroke::new(0.8, Color32::from_rgb(255, 142, 73)),
        ),
        "ed11_blue_branch_fissure" => (
            Color32::from_rgba_unmultiplied(42, 188, 255, 205),
            Stroke::new(0.9, Color32::from_rgb(96, 218, 255)),
        ),
        "ed11_lava_branch_fissure" => (
            Color32::from_rgba_unmultiplied(255, 93, 34, 210),
            Stroke::new(0.9, Color32::from_rgb(255, 150, 64)),
        ),
        "polyhaven_ruins_surface" => (
            Color32::from_rgba_unmultiplied(132, 126, 116, 150),
            Stroke::new(0.7, Color32::from_rgb(170, 156, 128)),
        ),
        "polyhaven_rocky_surface" => (
            Color32::from_rgba_unmultiplied(86, 104, 112, 145),
            Stroke::new(0.7, Color32::from_rgb(128, 148, 154)),
        ),
        "polyhaven_mossy_surface" => (
            Color32::from_rgba_unmultiplied(66, 112, 86, 135),
            Stroke::new(0.7, Color32::from_rgb(102, 152, 110)),
        ),
        _ => (
            Color32::from_rgba_unmultiplied(94, 108, 96, 160),
            Stroke::new(0.8, Color32::from_rgb(120, 128, 112)),
        ),
    }
}

fn marker_color(kind: &str) -> Color32 {
    match kind {
        "tree" => Color32::from_rgb(68, 150, 88),
        "rock" => Color32::from_rgb(150, 154, 148),
        "mushroom" => Color32::from_rgb(226, 108, 112),
        "plant" => Color32::from_rgb(86, 208, 151),
        "structure" => Color32::from_rgb(198, 170, 122),
        "foundation" => Color32::from_rgb(118, 118, 132),
        "wall" => Color32::from_rgb(160, 148, 128),
        "tower" => Color32::from_rgb(192, 170, 148),
        "roof" => Color32::from_rgb(152, 74, 62),
        "stair" => Color32::from_rgb(144, 134, 118),
        "prop" => Color32::from_rgb(150, 128, 104),
        "debris" => Color32::from_rgb(126, 111, 92),
        "warm_light_pool" => Color32::from_rgb(255, 171, 82),
        "cool_light_pool" => Color32::from_rgb(58, 188, 255),
        "atmospheric_fog_veil" => Color32::from_rgb(114, 159, 196),
        "ed11_depth_fog_card" => Color32::from_rgb(126, 178, 214),
        "ed11_selection_halo" => Color32::from_rgb(255, 211, 92),
        "ed11_foliage_patch" => Color32::from_rgb(113, 150, 88),
        "ed11_blue_rift_beacon" => Color32::from_rgb(48, 205, 255),
        "ed11_lava_rift_beacon" => Color32::from_rgb(255, 112, 48),
        _ => Color32::from_rgb(184, 151, 92),
    }
}

struct MapProjection {
    min: [f32; 2],
    max_z: f32,
    origin: Pos2,
    scale: f32,
}

impl MapProjection {
    fn new(map: &EditorWorldMapReport, rect: Rect) -> Result<Self, EngineError> {
        let world_width = map.bounds_max[0] - map.bounds_min[0];
        let world_height = map.bounds_max[1] - map.bounds_min[1];
        if !world_width.is_finite()
            || !world_height.is_finite()
            || world_width <= 0.0
            || world_height <= 0.0
        {
            return invalid_shell_runtime(
                "minimap.world_bounds",
                "minimap world bounds must be finite and ordered",
            );
        }
        let scale = (rect.width() / world_width).min(rect.height() / world_height);
        if !scale.is_finite() || scale <= 0.0 {
            return invalid_shell_runtime("minimap.scale", "minimap scale must be positive");
        }
        let map_size = Vec2::new(world_width * scale, world_height * scale);
        let origin = rect.center() - map_size * 0.5;
        Ok(Self {
            min: map.bounds_min,
            max_z: map.bounds_max[1],
            origin,
            scale,
        })
    }

    fn project(&self, point: [f32; 2]) -> Pos2 {
        Pos2::new(
            self.origin.x + (point[0] - self.min[0]) * self.scale,
            self.origin.y + (self.max_z - point[1]) * self.scale,
        )
    }

    fn length(&self, world_length: f32) -> f32 {
        world_length * self.scale
    }

    fn unproject(&self, point: Pos2) -> Result<[f32; 2], EngineError> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return invalid_shell_runtime(
                "minimap.pointer",
                "minimap pointer position must be finite",
            );
        }
        Ok([
            self.min[0] + (point.x - self.origin.x) / self.scale,
            self.max_z - (point.y - self.origin.y) / self.scale,
        ])
    }
}
