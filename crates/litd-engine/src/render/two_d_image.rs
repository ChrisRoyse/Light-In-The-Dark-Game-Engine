use crate::{
    EngineError,
    render::{
        reference::ReferenceSceneImage,
        two_d::{Render2dPipelinePlan, TileMaterial2d},
    },
};

pub(super) fn render_reference_image(
    plan: &Render2dPipelinePlan,
) -> Result<ReferenceSceneImage, EngineError> {
    plan.validate()?;
    let width = plan.settings.output_width;
    let height = plan.settings.output_height;
    let mut pixels = vec![[7, 11, 20, 255]; (width * height) as usize];
    draw_background(width, height, &mut pixels);
    draw_tilemap(plan, width, height, &mut pixels);
    draw_sprites(plan, width, height, &mut pixels);
    draw_hud(width, height, &mut pixels);
    ReferenceSceneImage::from_rgba_pixels(width, height, pixels)
}

fn draw_background(width: u32, height: u32, pixels: &mut [[u8; 4]]) {
    for y in 0..height {
        let v = y as f32 / height as f32;
        for x in 0..width {
            let u = x as f32 / width as f32;
            let fog =
                (1.0 - ((u - 0.52).powi(2) + (v - 0.48).powi(2)).sqrt() * 1.5).clamp(0.0, 1.0);
            let grain = if (x + y) % 2 == 0 { 0.034 } else { -0.014 };
            set_px(
                pixels,
                width,
                x,
                y,
                rgb([
                    0.125 + 0.085 * fog + grain,
                    0.122 + 0.075 * fog + grain * 0.6,
                    0.130 + 0.065 * fog + grain * 0.3,
                ]),
            );
        }
    }
}

fn draw_tilemap(plan: &Render2dPipelinePlan, width: u32, height: u32, pixels: &mut [[u8; 4]]) {
    let start_x = width as i32 / 2 - (plan.tilemap.columns as i32 * 38) / 2;
    let start_y = height as i32 / 2 - (plan.tilemap.rows as i32 * 30) / 2;
    for row in 0..plan.tilemap.rows {
        for col in 0..plan.tilemap.columns {
            let tile = &plan.tilemap.tiles[(row * plan.tilemap.columns + col) as usize];
            let x = start_x + col as i32 * 38 + (row % 2) as i32 * 18;
            let y = start_y + row as i32 * 30;
            let color = lit_color(
                material_color(tile.material),
                tile.normal,
                [x as f32, y as f32],
                plan,
            );
            rect(RectArgs {
                pixels,
                width,
                height,
                x,
                y,
                w: 36,
                h: 28,
                color,
            });
            rect(RectArgs {
                pixels,
                width,
                height,
                x,
                y,
                w: 36,
                h: 2,
                color: [9, 13, 22, 115],
            });
            if matches!(tile.material, TileMaterial2d::GoldTrim) {
                rect(RectArgs {
                    pixels,
                    width,
                    height,
                    x: x + 8,
                    y: y + 10,
                    w: 20,
                    h: 5,
                    color: [248, 189, 44, 210],
                });
            }
        }
    }
}

fn draw_sprites(plan: &Render2dPipelinePlan, width: u32, height: u32, pixels: &mut [[u8; 4]]) {
    for sprite in &plan.sprites {
        let light = lit_color(
            [sprite.tint[0], sprite.tint[1], sprite.tint[2]],
            sprite.normal,
            sprite.position_px,
            plan,
        );
        let x = (sprite.position_px[0] - sprite.size_px[0] / 2.0) as i32;
        let y = (sprite.position_px[1] - sprite.size_px[1] / 2.0) as i32;
        glow(
            pixels,
            width,
            height,
            sprite.position_px,
            sprite.size_px[0].max(sprite.size_px[1]) * 0.9,
            rgb([
                sprite.tint[0] * sprite.emissive,
                sprite.tint[1] * sprite.emissive,
                sprite.tint[2] * sprite.emissive,
            ]),
        );
        ellipse(
            pixels,
            width,
            height,
            sprite.position_px,
            sprite.size_px[0] * 0.52,
            sprite.size_px[1] * 0.50,
            light,
        );
        rect(RectArgs {
            pixels,
            width,
            height,
            x,
            y: y + sprite.size_px[1] as i32 - 5,
            w: sprite.size_px[0] as i32,
            h: 4,
            color: [8, 10, 16, 180],
        });
    }
}

fn draw_hud(width: u32, height: u32, pixels: &mut [[u8; 4]]) {
    rect(RectArgs {
        pixels,
        width,
        height,
        x: 0,
        y: 0,
        w: width as i32,
        h: 42,
        color: [8, 13, 24, 235],
    });
    rect(RectArgs {
        pixels,
        width,
        height,
        x: 0,
        y: height as i32 - 58,
        w: width as i32,
        h: 58,
        color: [10, 13, 20, 240],
    });
    rect(RectArgs {
        pixels,
        width,
        height,
        x: 118,
        y: height as i32 - 42,
        w: 262,
        h: 18,
        color: [209, 44, 34, 230],
    });
    rect(RectArgs {
        pixels,
        width,
        height,
        x: 580,
        y: height as i32 - 42,
        w: 262,
        h: 18,
        color: [27, 128, 210, 230],
    });
    for i in 0..12 {
        rect(RectArgs {
            pixels,
            width,
            height,
            x: 372 + i * 18,
            y: height as i32 - 50,
            w: 16,
            h: 34,
            color: [235, 176, 36, 230],
        });
    }
    ring(
        pixels,
        width,
        height,
        [width as f32 - 88.0, 82.0],
        48.0,
        44.0,
        [228, 176, 65, 230],
    );
}

fn material_color(material: TileMaterial2d) -> [f32; 3] {
    match material {
        TileMaterial2d::Basalt => [0.27, 0.26, 0.24],
        TileMaterial2d::Moss => [0.22, 0.34, 0.22],
        TileMaterial2d::Water => [0.08, 0.22, 0.32],
        TileMaterial2d::GoldTrim => [0.82, 0.56, 0.15],
    }
}

fn lit_color(
    base: [f32; 3],
    normal: [f32; 3],
    pos: [f32; 2],
    plan: &Render2dPipelinePlan,
) -> [u8; 4] {
    let n = normalize(normal);
    let mut out = [base[0] * 0.68, base[1] * 0.66, base[2] * 0.62];
    for light in &plan.lights {
        let dx = light.position_px[0] - pos[0];
        let dy = light.position_px[1] - pos[1];
        let dist = (dx * dx + dy * dy).sqrt();
        let falloff = (1.0 - dist / light.radius_px).clamp(0.0, 1.0).powf(1.7);
        let l = normalize([dx / light.radius_px, dy / light.radius_px, 0.75]);
        let ndotl = dot(n, l).max(0.12);
        for i in 0..3 {
            out[i] += base[i] * light.color[i] * light.intensity * falloff * ndotl;
        }
    }
    rgb(out)
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(0.0001);
    [v[0] / len, v[1] / len, v[2] / len]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn rgb(color: [f32; 3]) -> [u8; 4] {
    [
        (color[0].clamp(0.0, 1.0) * 255.0) as u8,
        (color[1].clamp(0.0, 1.0) * 255.0) as u8,
        (color[2].clamp(0.0, 1.0) * 255.0) as u8,
        255,
    ]
}

fn set_px(pixels: &mut [[u8; 4]], width: u32, x: u32, y: u32, color: [u8; 4]) {
    pixels[(y * width + x) as usize] = color;
}

fn blend(pixels: &mut [[u8; 4]], width: u32, height: u32, x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return;
    }
    let idx = (y as u32 * width + x as u32) as usize;
    let alpha = color[3] as f32 / 255.0;
    for (dst, src) in pixels[idx][0..3].iter_mut().zip(color[0..3].iter()) {
        *dst = (*dst as f32 * (1.0 - alpha) + *src as f32 * alpha).round() as u8;
    }
}

struct RectArgs<'a> {
    pixels: &'a mut [[u8; 4]],
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: [u8; 4],
}

fn rect(args: RectArgs<'_>) {
    let RectArgs {
        pixels,
        width,
        height,
        x,
        y,
        w,
        h,
        color,
    } = args;
    for yy in y..y + h {
        for xx in x..x + w {
            blend(pixels, width, height, xx, yy, color);
        }
    }
}

fn ellipse(
    pixels: &mut [[u8; 4]],
    width: u32,
    height: u32,
    center: [f32; 2],
    rx: f32,
    ry: f32,
    color: [u8; 4],
) {
    for y in (center[1] - ry) as i32..=(center[1] + ry) as i32 {
        for x in (center[0] - rx) as i32..=(center[0] + rx) as i32 {
            let dx = (x as f32 - center[0]) / rx;
            let dy = (y as f32 - center[1]) / ry;
            if dx * dx + dy * dy <= 1.0 {
                blend(pixels, width, height, x, y, color);
            }
        }
    }
}

fn ring(
    pixels: &mut [[u8; 4]],
    width: u32,
    height: u32,
    center: [f32; 2],
    outer: f32,
    inner: f32,
    color: [u8; 4],
) {
    for y in (center[1] - outer) as i32..=(center[1] + outer) as i32 {
        for x in (center[0] - outer) as i32..=(center[0] + outer) as i32 {
            let dx = x as f32 - center[0];
            let dy = y as f32 - center[1];
            let d = dx * dx + dy * dy;
            if d <= outer * outer && d >= inner * inner {
                blend(pixels, width, height, x, y, color);
            }
        }
    }
}

fn glow(
    pixels: &mut [[u8; 4]],
    width: u32,
    height: u32,
    center: [f32; 2],
    radius: f32,
    mut color: [u8; 4],
) {
    for y in (center[1] - radius) as i32..=(center[1] + radius) as i32 {
        for x in (center[0] - radius) as i32..=(center[0] + radius) as i32 {
            let dx = x as f32 - center[0];
            let dy = y as f32 - center[1];
            let d = (dx * dx + dy * dy).sqrt() / radius;
            if d <= 1.0 {
                color[3] = (100.0 * (1.0 - d).powf(1.8)) as u8;
                blend(pixels, width, height, x, y, color);
            }
        }
    }
}
