use egui::{Stroke, UiBuilder};

use super::*;

pub(super) fn fixed_vscroll_panel(
    ctx: &egui::Context,
    id: &'static str,
    rect: UiRect,
    radius: u8,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    fixed_panel(ctx, id, rect, radius, |ui| {
        let max_size = ui.available_size();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(max_size.y)
            .show(ui, |ui| {
                ui.set_min_width(max_size.x);
                ui.set_max_width(max_size.x);
                add_contents(ui);
            });
    });
}

pub(super) fn fixed_panel(
    ctx: &egui::Context,
    id: &'static str,
    rect: UiRect,
    radius: u8,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Area::new(egui::Id::new(id))
        .fixed_pos(pos(rect.min))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let panel_size = size(rect);
            let panel_rect = egui::Rect::from_min_size(ui.min_rect().min, panel_size);
            ui.shrink_clip_rect(panel_rect);
            ui.scope_builder(UiBuilder::new().max_rect(panel_rect), |ui| {
                ui.set_min_size(panel_size);
                ui.set_max_size(panel_size);
                ui.shrink_clip_rect(panel_rect);
                let margin = panel_margin(panel_size);
                egui::Frame::default()
                    .fill(PANEL_BG)
                    .stroke(Stroke::new(1.0, GOLD_DEEP))
                    .corner_radius(egui::CornerRadius::same(radius))
                    .inner_margin(egui::Margin::same(margin))
                    .show(ui, |ui| {
                        ui.shrink_clip_rect(panel_rect);
                        let inset = f32::from(margin) * 2.0;
                        let inner_size =
                            (panel_size - egui::vec2(inset, inset)).max(egui::Vec2::ZERO);
                        ui.set_min_size(inner_size);
                        ui.set_max_size(inner_size);
                        add_contents(ui);
                    });
            });
        });
}

pub(super) fn pos(point: EngineVec2) -> egui::Pos2 {
    egui::pos2(point.x, point.y)
}

pub(super) fn size(rect: UiRect) -> egui::Vec2 {
    egui::vec2(rect.size().x, rect.size().y)
}

pub(super) fn panel_width(ui: &egui::Ui) -> f32 {
    ui.available_width().clamp(64.0, 4096.0)
}

fn panel_margin(panel_size: egui::Vec2) -> i8 {
    if panel_size.y < 36.0 || panel_size.x < 150.0 {
        3
    } else if panel_size.y < 64.0 || panel_size.x < 220.0 {
        5
    } else {
        8
    }
}
