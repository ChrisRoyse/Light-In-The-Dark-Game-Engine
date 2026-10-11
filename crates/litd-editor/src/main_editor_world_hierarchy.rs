use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Vec2};

use super::{GOLD, PANEL_BG_SOFT, TEXT, TEXT_DIM};

const ICON_BG: Color32 = Color32::from_rgb(25, 33, 46);
const ICON_STROKE: Color32 = Color32::from_rgb(79, 91, 108);
const WATER: Color32 = Color32::from_rgb(64, 153, 214);
const GREEN: Color32 = Color32::from_rgb(83, 151, 83);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorldHierarchyItem {
    Terrain,
    Water,
    Paths,
    Scenery,
    Buildings,
    Characters,
    Areas,
    Cameras,
    Sound,
}

pub(super) const ENVIRONMENT_ITEMS: [WorldHierarchyItem; 5] = [
    WorldHierarchyItem::Terrain,
    WorldHierarchyItem::Water,
    WorldHierarchyItem::Paths,
    WorldHierarchyItem::Scenery,
    WorldHierarchyItem::Buildings,
];

pub(super) const PLACED_ITEMS: [WorldHierarchyItem; 4] = [
    WorldHierarchyItem::Characters,
    WorldHierarchyItem::Areas,
    WorldHierarchyItem::Cameras,
    WorldHierarchyItem::Sound,
];

impl WorldHierarchyItem {
    fn label(self) -> &'static str {
        match self {
            Self::Terrain => "Terrain",
            Self::Water => "Water",
            Self::Paths => "Paths",
            Self::Scenery => "Scenery",
            Self::Buildings => "Buildings",
            Self::Characters => "Characters",
            Self::Areas => "Areas",
            Self::Cameras => "Cameras",
            Self::Sound => "Sound",
        }
    }

    fn help(self) -> &'static str {
        match self {
            Self::Terrain => "Shape the ground and paint its surface.",
            Self::Water => "Add and shape lakes, rivers, and shorelines.",
            Self::Paths => "Draw roads and walking paths.",
            Self::Scenery => "Place trees, rocks, and other scenery.",
            Self::Buildings => "Place and edit structures.",
            Self::Characters => "Characters placed in the world appear here.",
            Self::Areas => "Named areas used by the world appear here.",
            Self::Cameras => "Saved camera views appear here.",
            Self::Sound => "Sounds placed in the world appear here.",
        }
    }
}

pub(super) fn hierarchy_row(ui: &mut egui::Ui, item: WorldHierarchyItem) -> Response {
    let row_size = egui::vec2(ui.available_width(), 25.0);
    let (rect, response) = ui.allocate_exact_size(row_size, Sense::hover());
    if response.hovered() {
        ui.painter().rect_filled(rect, 4, PANEL_BG_SOFT);
    }

    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 13.0, rect.center().y),
        Vec2::splat(20.0),
    );
    draw_icon(ui.painter(), icon_rect, item);
    ui.painter().text(
        Pos2::new(icon_rect.right() + 7.0, rect.center().y),
        Align2::LEFT_CENTER,
        item.label(),
        FontId::proportional(13.0),
        if response.hovered() { TEXT } else { TEXT_DIM },
    );
    response.on_hover_text(item.help())
}

fn draw_icon(painter: &egui::Painter, rect: Rect, item: WorldHierarchyItem) {
    painter.rect_filled(rect, 4, ICON_BG);
    painter.rect_stroke(rect, 4, Stroke::new(1.0, ICON_STROKE), StrokeKind::Inside);
    let center = rect.center();
    match item {
        WorldHierarchyItem::Terrain => draw_terrain(painter, center),
        WorldHierarchyItem::Water => draw_water(painter, center),
        WorldHierarchyItem::Paths => draw_path(painter, center),
        WorldHierarchyItem::Scenery => draw_tree(painter, center),
        WorldHierarchyItem::Buildings => draw_building(painter, center),
        WorldHierarchyItem::Characters => draw_characters(painter, center),
        WorldHierarchyItem::Areas => draw_area(painter, center),
        WorldHierarchyItem::Cameras => draw_camera(painter, center),
        WorldHierarchyItem::Sound => draw_sound(painter, center),
    }
}

fn draw_terrain(painter: &egui::Painter, center: Pos2) {
    painter.add(egui::Shape::convex_polygon(
        vec![
            center + Vec2::new(-7.0, 5.0),
            center + Vec2::new(-1.0, -5.0),
            center + Vec2::new(3.0, 0.0),
            center + Vec2::new(7.0, 5.0),
        ],
        GREEN,
        Stroke::new(1.0, GOLD),
    ));
}

fn draw_water(painter: &egui::Painter, center: Pos2) {
    for y in [-4.0, 0.0, 4.0] {
        painter.line_segment(
            [
                center + Vec2::new(-7.0, y),
                center + Vec2::new(-2.0, y - 1.5),
            ],
            Stroke::new(1.5, WATER),
        );
        painter.line_segment(
            [
                center + Vec2::new(-2.0, y - 1.5),
                center + Vec2::new(3.0, y + 1.5),
            ],
            Stroke::new(1.5, WATER),
        );
        painter.line_segment(
            [center + Vec2::new(3.0, y + 1.5), center + Vec2::new(7.0, y)],
            Stroke::new(1.5, WATER),
        );
    }
}

fn draw_path(painter: &egui::Painter, center: Pos2) {
    painter.line_segment(
        [center + Vec2::new(-5.0, 8.0), center + Vec2::new(4.0, -8.0)],
        Stroke::new(4.0, Color32::from_rgb(154, 117, 70)),
    );
    painter.line_segment(
        [center + Vec2::new(-5.0, 8.0), center + Vec2::new(4.0, -8.0)],
        Stroke::new(1.0, GOLD),
    );
}

fn draw_tree(painter: &egui::Painter, center: Pos2) {
    painter.line_segment(
        [center + Vec2::new(0.0, 1.0), center + Vec2::new(0.0, 8.0)],
        Stroke::new(2.0, Color32::from_rgb(128, 86, 48)),
    );
    painter.circle_filled(center + Vec2::new(0.0, -3.0), 6.0, GREEN);
}

fn draw_building(painter: &egui::Painter, center: Pos2) {
    let body = Rect::from_center_size(center + Vec2::new(0.0, 2.0), Vec2::new(13.0, 12.0));
    painter.rect_filled(body, 1, Color32::from_rgb(124, 129, 137));
    painter.line_segment(
        [
            center + Vec2::new(-8.0, -4.0),
            center + Vec2::new(0.0, -9.0),
        ],
        Stroke::new(2.0, GOLD),
    );
    painter.line_segment(
        [center + Vec2::new(0.0, -9.0), center + Vec2::new(8.0, -4.0)],
        Stroke::new(2.0, GOLD),
    );
}

fn draw_characters(painter: &egui::Painter, center: Pos2) {
    for x in [-4.0, 4.0] {
        painter.circle_filled(center + Vec2::new(x, -4.0), 2.5, GOLD);
        painter.line_segment(
            [center + Vec2::new(x, -1.0), center + Vec2::new(x, 6.0)],
            Stroke::new(2.0, TEXT_DIM),
        );
    }
}

fn draw_area(painter: &egui::Painter, center: Pos2) {
    painter.rect_stroke(
        Rect::from_center_size(center, Vec2::new(14.0, 12.0)),
        2,
        Stroke::new(1.5, WATER),
        StrokeKind::Inside,
    );
    painter.circle_filled(center, 2.0, GOLD);
}

fn draw_camera(painter: &egui::Painter, center: Pos2) {
    painter.rect_stroke(
        Rect::from_center_size(center, Vec2::new(13.0, 10.0)),
        2,
        Stroke::new(1.5, TEXT_DIM),
        StrokeKind::Inside,
    );
    painter.circle_stroke(center, 3.0, Stroke::new(1.5, GOLD));
}

fn draw_sound(painter: &egui::Painter, center: Pos2) {
    painter.line_segment(
        [
            center + Vec2::new(-6.0, -2.0),
            center + Vec2::new(-6.0, 3.0),
        ],
        Stroke::new(2.0, GOLD),
    );
    painter.line_segment(
        [
            center + Vec2::new(-6.0, -2.0),
            center + Vec2::new(-1.0, -6.0),
        ],
        Stroke::new(2.0, GOLD),
    );
    for radius in [4.0, 7.0] {
        painter.circle_stroke(
            center + Vec2::new(-1.0, 0.0),
            radius,
            Stroke::new(1.0, WATER),
        );
    }
    painter.rect_filled(
        Rect::from_min_max(
            center + Vec2::new(-9.0, -9.0),
            center + Vec2::new(-1.0, 9.0),
        ),
        0,
        ICON_BG,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchy_labels_use_plain_language() {
        let labels = ENVIRONMENT_ITEMS
            .into_iter()
            .chain(PLACED_ITEMS)
            .map(WorldHierarchyItem::label)
            .collect::<Vec<_>>();

        assert!(labels.contains(&"Scenery"));
        assert!(labels.contains(&"Characters"));
        assert!(labels.contains(&"Areas"));
        assert!(!labels.contains(&"Doodads"));
        assert!(!labels.contains(&"Units"));
        assert!(!labels.contains(&"Regions"));
    }

    #[test]
    fn every_hierarchy_item_has_task_guidance() {
        for item in ENVIRONMENT_ITEMS.into_iter().chain(PLACED_ITEMS) {
            assert!(item.help().ends_with('.'));
            assert!(item.help().len() >= 25);
        }
    }
}
