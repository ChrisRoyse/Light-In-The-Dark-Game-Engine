use egui::{Color32, Rect, Stroke, StrokeKind, Vec2};

const GOLD: Color32 = Color32::from_rgb(222, 179, 98);
const BLUE_FIRE: Color32 = Color32::from_rgb(54, 184, 255);
const LAVA: Color32 = Color32::from_rgb(244, 96, 43);
const STONE: Color32 = Color32::from_rgb(96, 98, 93);

#[derive(Debug, Clone, Copy)]
pub enum AssetIcon {
    Knight,
    Wizard,
    Tower,
    Tombstone,
    Fire,
    Rune,
    Tree,
    Rocks,
}

pub fn draw_asset_icon(painter: &egui::Painter, rect: Rect, icon: AssetIcon, selected: bool) {
    let icon_painter = painter.with_clip_rect(rect);
    let fill = if selected {
        Color32::from_rgb(58, 45, 26)
    } else {
        Color32::from_rgb(30, 36, 45)
    };
    icon_painter.rect_filled(rect, 4, fill);
    icon_painter.rect_stroke(
        rect,
        4,
        Stroke::new(
            1.0,
            if selected {
                GOLD
            } else {
                Color32::from_rgb(78, 70, 52)
            },
        ),
        StrokeKind::Inside,
    );
    let icon_rect = rect.shrink(4.0);
    match icon {
        AssetIcon::Knight => icon_knight(&icon_painter, icon_rect),
        AssetIcon::Wizard => icon_wizard(&icon_painter, icon_rect),
        AssetIcon::Tower => icon_tower(&icon_painter, icon_rect),
        AssetIcon::Tombstone => icon_tombstone(&icon_painter, icon_rect),
        AssetIcon::Fire => icon_fire(&icon_painter, icon_rect),
        AssetIcon::Rune => icon_rune(&icon_painter, icon_rect),
        AssetIcon::Tree => icon_tree(&icon_painter, icon_rect),
        AssetIcon::Rocks => icon_rocks(&icon_painter, icon_rect),
    }
}

fn icon_knight(p: &egui::Painter, r: Rect) {
    let c = r.center();
    p.circle_filled(
        c + Vec2::new(0.0, -14.0),
        7.0,
        Color32::from_rgb(178, 184, 185),
    );
    p.rect_filled(
        Rect::from_center_size(c + Vec2::new(0.0, 6.0), Vec2::new(18.0, 28.0)),
        2,
        Color32::from_rgb(98, 106, 112),
    );
    p.line_segment(
        [c + Vec2::new(-15.0, -2.0), c + Vec2::new(16.0, 18.0)],
        Stroke::new(2.0, GOLD),
    );
}

fn icon_wizard(p: &egui::Painter, r: Rect) {
    let c = r.center();
    p.add(egui::Shape::convex_polygon(
        vec![
            c + Vec2::new(-12.0, 20.0),
            c + Vec2::new(12.0, 20.0),
            c + Vec2::new(0.0, -18.0),
        ],
        Color32::from_rgb(45, 64, 109),
        Stroke::new(1.0, BLUE_FIRE),
    ));
    p.circle_filled(c + Vec2::new(16.0, -10.0), 6.0, BLUE_FIRE);
}

fn icon_tower(p: &egui::Painter, r: Rect) {
    let c = r.center();
    let body = Rect::from_center_size(c, Vec2::new(24.0, 38.0));
    p.rect_filled(body, 1, STONE);
    p.rect_stroke(body, 1, Stroke::new(1.0, GOLD), StrokeKind::Inside);
}

fn icon_tombstone(p: &egui::Painter, r: Rect) {
    let c = r.center();
    p.circle_filled(
        c + Vec2::new(0.0, -8.0),
        13.0,
        Color32::from_rgb(110, 112, 104),
    );
    p.rect_filled(
        Rect::from_center_size(c + Vec2::new(0.0, 8.0), Vec2::new(26.0, 30.0)),
        2,
        STONE,
    );
}

fn icon_fire(p: &egui::Painter, r: Rect) {
    let c = r.center();
    p.circle_filled(c, 17.0, Color32::from_rgba_premultiplied(255, 94, 32, 80));
    p.add(egui::Shape::convex_polygon(
        vec![
            c + Vec2::new(-10.0, 14.0),
            c + Vec2::new(10.0, 14.0),
            c + Vec2::new(2.0, -20.0),
        ],
        LAVA,
        Stroke::new(1.0, GOLD),
    ));
}

fn icon_rune(p: &egui::Painter, r: Rect) {
    let c = r.center();
    p.circle_stroke(c, 20.0, Stroke::new(2.0, BLUE_FIRE));
    p.circle_stroke(c, 12.0, Stroke::new(1.0, Color32::from_rgb(92, 234, 255)));
    p.line_segment(
        [c + Vec2::new(-12.0, 0.0), c + Vec2::new(12.0, 0.0)],
        Stroke::new(1.0, BLUE_FIRE),
    );
}

fn icon_tree(p: &egui::Painter, r: Rect) {
    let c = r.center();
    for (dy, w) in [(-15.0, 24.0), (-4.0, 34.0), (8.0, 42.0)] {
        p.add(egui::Shape::convex_polygon(
            vec![
                c + Vec2::new(-w * 0.5, dy + 14.0),
                c + Vec2::new(w * 0.5, dy + 14.0),
                c + Vec2::new(0.0, dy - 14.0),
            ],
            Color32::from_rgb(58, 117, 66),
            Stroke::new(1.0, Color32::from_rgb(31, 63, 38)),
        ));
    }
}

fn icon_rocks(p: &egui::Painter, r: Rect) {
    let c = r.center();
    for (dx, dy, radius) in [(-10.0, 6.0, 12.0), (5.0, -2.0, 16.0), (16.0, 10.0, 10.0)] {
        let center = c + Vec2::new(dx, dy);
        p.circle_filled(center, radius, Color32::from_rgb(119, 116, 105));
        p.circle_stroke(
            center,
            radius,
            Stroke::new(1.0, Color32::from_rgb(61, 64, 63)),
        );
    }
}
