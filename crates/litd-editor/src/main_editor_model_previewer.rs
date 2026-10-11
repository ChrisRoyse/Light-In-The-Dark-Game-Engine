use egui::{RichText, Stroke};
use litd_editor::asset_previewer::{
    ModelPreviewCamera, ModelPreviewRequest, load_model_previewer_from_default_catalog,
};

use super::main_editor_asset_entries::compact_asset_label;
use super::main_editor_frame::panel_width;
use super::*;

impl EditorShellRuntime {
    pub(super) fn select_asset(&mut self, index: usize) {
        if self.asset_entries.is_empty() {
            self.model_previewer = None;
            self.model_previewer_error =
                "asset vault not loaded for Objects mode startup".to_string();
            return;
        }
        let next = index.min(self.asset_entries.len().saturating_sub(1));
        if self.selected_asset != next {
            self.selected_asset = next;
            let Some(selected_asset) = self.selected_asset_entry() else {
                self.model_previewer = None;
                self.model_previewer_error = "selected asset is unavailable".to_string();
                return;
            };
            let relative_path = selected_asset.relative_path.clone();
            let camera = self
                .model_previewer
                .as_ref()
                .map(|runtime| runtime.camera)
                .unwrap_or_default();
            self.load_model_previewer(
                relative_path,
                ModelPreviewRequest {
                    selected_animation: None,
                    playback_time_ms: 0,
                    camera,
                },
            );
        }
    }

    pub(super) fn refresh_model_previewer(&mut self) {
        let Some(selected_asset) = self.selected_asset_entry() else {
            self.model_previewer = None;
            self.model_previewer_error =
                "asset vault not loaded for Objects mode startup".to_string();
            return;
        };
        let relative_path = selected_asset.relative_path.clone();
        let request = ModelPreviewRequest {
            selected_animation: None,
            playback_time_ms: 0,
            camera: self
                .model_previewer
                .as_ref()
                .map(|runtime| runtime.camera)
                .unwrap_or_default(),
        };
        self.load_model_previewer(relative_path, request);
    }

    pub(super) fn model_previewer_panel(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Previewer").strong().color(TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("loads {}", self.model_previewer_load_count))
                        .size(11.0)
                        .color(TEXT_DIM),
                );
            });
        });
        if self.model_previewer.is_some() {
            self.loaded_previewer_panel(ui);
        } else {
            ui.label(
                RichText::new(error_summary(&self.model_previewer_error))
                    .size(11.0)
                    .color(Color32::from_rgb(241, 116, 116)),
            );
        }
    }

    fn loaded_previewer_panel(&mut self, ui: &mut egui::Ui) {
        let Some(runtime) = self.model_previewer.as_mut() else {
            return;
        };
        draw_preview_bounds(ui, runtime.bounds_min, runtime.bounds_max);
        ui.label(
            RichText::new(format!(
                "{} / {} tris / {} clips",
                compact_asset_label(&runtime.source_relative_path),
                runtime.triangle_count,
                runtime.animation_clips.len()
            ))
            .size(11.0)
            .color(TEXT),
        );
        ui.label(
            RichText::new(format!("renderer: {}", runtime.renderer_path))
                .size(11.0)
                .color(TEXT_DIM),
        );
        compact_camera_controls(ui, &mut runtime.camera);
        let clip_names = runtime
            .animation_clips
            .iter()
            .map(|clip| clip.name.clone())
            .collect::<Vec<_>>();
        if clip_names.is_empty() {
            ui.label(RichText::new("static").size(11.0).color(TEXT_DIM));
            return;
        }
        let current = runtime.selected_animation.clone();
        let camera = runtime.camera;
        let relative_path = runtime.source_relative_path.clone();
        ui.horizontal_wrapped(|ui| {
            for name in clip_names.into_iter().take(4) {
                let selected = current.as_deref() == Some(name.as_str());
                if ui
                    .selectable_label(selected, RichText::new(name.clone()).size(11.0))
                    .clicked()
                {
                    self.load_model_previewer(
                        relative_path.clone(),
                        ModelPreviewRequest {
                            selected_animation: Some(name),
                            playback_time_ms: 0,
                            camera,
                        },
                    );
                }
            }
        });
    }

    fn load_model_previewer(&mut self, relative_path: String, request: ModelPreviewRequest) {
        match load_model_previewer_from_default_catalog(&relative_path, request) {
            Ok(runtime) => {
                self.model_previewer = Some(runtime);
                self.model_previewer_error.clear();
                self.model_previewer_load_count += 1;
            }
            Err(error) => {
                tracing::warn!(
                    source_path = %relative_path,
                    error = %error,
                    "model previewer failed to load selected asset"
                );
                self.model_previewer = None;
                self.model_previewer_error = error.to_string();
            }
        }
    }
}

fn compact_camera_controls(ui: &mut egui::Ui, camera: &mut ModelPreviewCamera) {
    let width = panel_width(ui);
    ui.add_sized(
        [width, 18.0],
        egui::Slider::new(&mut camera.yaw_degrees, 0.0..=360.0)
            .show_value(false)
            .text("yaw"),
    );
    ui.add_sized(
        [width, 18.0],
        egui::Slider::new(&mut camera.pitch_degrees, -60.0..=20.0)
            .show_value(false)
            .text("pitch"),
    );
    ui.add_sized(
        [width, 18.0],
        egui::Slider::new(&mut camera.distance, 24.0..=240.0)
            .show_value(false)
            .text("zoom"),
    );
}

fn draw_preview_bounds(ui: &mut egui::Ui, min: [f32; 3], max: [f32; 3]) {
    let width = panel_width(ui);
    let height = if width < 240.0 { 86.0 } else { 112.0 };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, PANEL_BG_SOFT);
    painter.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0, GOLD_DEEP),
        egui::StrokeKind::Inside,
    );
    let center = rect.center();
    let scale = 0.74 * rect.width().min(rect.height()) / longest_extent(min, max).max(1.0);
    let points = [
        project_point([min[0], min[1], min[2]], center, scale),
        project_point([max[0], min[1], min[2]], center, scale),
        project_point([max[0], max[1], min[2]], center, scale),
        project_point([min[0], max[1], min[2]], center, scale),
        project_point([min[0], min[1], max[2]], center, scale),
        project_point([max[0], min[1], max[2]], center, scale),
        project_point([max[0], max[1], max[2]], center, scale),
        project_point([min[0], max[1], max[2]], center, scale),
    ];
    for (a, b) in [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ] {
        painter.line_segment([points[a], points[b]], Stroke::new(1.0, GOLD));
    }
}

fn project_point(point: [f32; 3], center: egui::Pos2, scale: f32) -> egui::Pos2 {
    egui::pos2(
        center.x + (point[0] - point[2]) * 0.42 * scale,
        center.y - point[1] * 0.54 * scale + (point[0] + point[2]) * 0.12 * scale,
    )
}

fn longest_extent(min: [f32; 3], max: [f32; 3]) -> f32 {
    let x = max[0] - min[0];
    let y = max[1] - min[1];
    let z = max[2] - min[2];
    x.max(y).max(z)
}

fn error_summary(error: &str) -> String {
    const LIMIT: usize = 156;
    if error.chars().count() <= LIMIT {
        error.to_string()
    } else {
        let mut value = error.chars().take(LIMIT - 3).collect::<String>();
        value.push_str("...");
        value
    }
}
