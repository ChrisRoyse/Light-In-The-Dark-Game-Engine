pub fn apply_editor_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = egui::Color32::from_rgb(18, 24, 35);
    style.visuals.window_fill = egui::Color32::from_rgb(20, 26, 36);
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(7, 10, 16);
    style.visuals.selection.bg_fill = egui::Color32::from_rgb(108, 78, 35);
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(34, 43, 58);
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(58, 63, 75);
    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(116, 84, 38);
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    ctx.set_style(style);
}
