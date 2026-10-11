use egui::RichText;

use super::main_editor_asset_entries::compact_asset_label;
use super::main_editor_frame::fixed_panel;
use super::*;

pub(super) const CONTENT_BROWSER_THUMBNAIL_WARM_TARGET: usize = 24;

#[derive(Debug, Clone)]
pub(super) struct ContentBrowserDragPayload {
    pub(super) asset_indices: Vec<usize>,
}

impl EditorShellRuntime {
    pub(super) fn show_content_browser(&mut self, ctx: &egui::Context) {
        let Some(rect) = self.panel_rect(DockPanelKind::ContentBrowser) else {
            return;
        };
        self.thumbnail_cache.preload_first(
            ctx,
            self.asset_entries.iter().map(|asset| &asset.thumbnail),
            CONTENT_BROWSER_THUMBNAIL_WARM_TARGET,
        );
        fixed_panel(ctx, "editor.content_browser", rect, 3, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Content Browser").strong().color(TEXT));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} assets", self.asset_entries.len())).color(GOLD),
                    );
                });
            });
            let available_height = ui.available_height().max(0.0);
            let tile = content_browser_tile_metrics(available_height);
            // #1461: the click is collected here and applied AFTER the draw loop so each tile can
            // BORROW its AssetEntry instead of cloning the whole struct (8+ Strings) every frame —
            // select_asset needs &mut self, which would otherwise conflict with the borrow.
            let mut clicked_index: Option<usize> = None;
            egui::ScrollArea::horizontal()
                .auto_shrink([false, false])
                .max_height(available_height)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for index in 0..self.asset_entries.len() {
                            // Borrow, not clone (#1461). thumbnail_cache is a disjoint &mut field.
                            let asset = &self.asset_entries[index];
                            let selected = self.selected_asset == index;
                            let thumbnail_cache = &mut self.thumbnail_cache;
                            ui.vertical(|ui| {
                                ui.set_width(tile.item_width);
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(tile.icon_size, tile.icon_size),
                                    egui::Sense::click_and_drag(),
                                );
                                response.dnd_set_drag_payload(ContentBrowserDragPayload {
                                    asset_indices: vec![index],
                                });
                                thumbnail_cache.draw_thumbnail_or_error(
                                    ctx,
                                    ui.painter(),
                                    rect,
                                    &asset.thumbnail,
                                    asset.icon,
                                    selected,
                                );
                                // #1461: build the 7-line detail string ONLY when hovered, via a
                                // lazy closure — not eagerly for every tile every frame.
                                let response = response.on_hover_ui(|ui| {
                                    ui.label(format!(
                                        "{}\n{}\n{}\nlicense: {}\nstatus: {}\nsha256: {}\nthumbnail: {}",
                                        asset.source,
                                        asset.asset_type,
                                        asset.relative_path,
                                        asset.license,
                                        asset.status,
                                        asset.sha256,
                                        asset.thumbnail.relative_path(),
                                    ));
                                });
                                if response.clicked() {
                                    clicked_index = Some(index);
                                }
                                if tile.label_height > 0.0 {
                                    ui.add_sized(
                                        [tile.item_width, tile.label_height],
                                        egui::Label::new(
                                            RichText::new(compact_asset_label(&asset.label))
                                                .size(tile.label_font)
                                                .color(TEXT),
                                        )
                                        .truncate(),
                                    );
                                }
                            });
                            ui.add_space(tile.item_spacing);
                        }
                    });
                });
            if let Some(index) = clicked_index {
                self.select_asset(index);
            }
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ContentBrowserTileMetrics {
    pub icon_size: f32,
    pub item_width: f32,
    pub item_spacing: f32,
    pub label_font: f32,
    pub label_height: f32,
}

pub(super) fn content_browser_tile_metrics(available_height: f32) -> ContentBrowserTileMetrics {
    let available_height = available_height.max(0.0);
    let label_height = if available_height < 72.0 {
        0.0
    } else if available_height < 126.0 {
        16.0
    } else {
        32.0
    };
    let gap = if label_height > 0.0 { 4.0 } else { 0.0 };
    let icon_room = (available_height - label_height - gap).max(0.0);
    let icon_size = if icon_room < 42.0 {
        icon_room
    } else {
        icon_room.min(78.0)
    };
    let compact = available_height < 126.0;
    ContentBrowserTileMetrics {
        icon_size,
        item_width: if compact { icon_size.max(58.0) } else { 86.0 },
        item_spacing: if compact { 10.0 } else { 18.0 },
        label_font: if compact { 11.0 } else { 12.0 },
        label_height,
    }
}
