use crate::math::Vec2;

use super::reference_image::ReferenceSceneImage;

pub(super) struct Canvas {
    pub(super) width: u32,
    pub(super) height: u32,
    pixels: Vec<[u8; 4]>,
}

impl Canvas {
    pub(super) fn new(width: u32, height: u32, color: [u8; 4]) -> Self {
        Self {
            width,
            height,
            pixels: vec![color; (width * height) as usize],
        }
    }

    pub(super) fn into_image(self) -> ReferenceSceneImage {
        ReferenceSceneImage::from_pixels(self.width, self.height, self.pixels)
    }

    pub(super) fn set(&mut self, x: u32, y: u32, color: [u8; 4]) {
        self.pixels[(y * self.width + x) as usize] = color;
    }

    pub(super) fn blend(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let i = (y as u32 * self.width + x as u32) as usize;
        let a = color[3] as f32 / 255.0;
        for (dst, src) in self.pixels[i][0..3].iter_mut().zip(color[0..3].iter()) {
            *dst = ((*dst as f32 * (1.0 - a)) + (*src as f32 * a)).round() as u8;
        }
    }

    pub(super) fn rect(&mut self, min: Vec2, size: Vec2, color: [u8; 4]) {
        for y in min.y as i32..(min.y + size.y) as i32 {
            for x in min.x as i32..(min.x + size.x) as i32 {
                self.blend(x, y, color);
            }
        }
    }

    pub(super) fn circle(&mut self, center: Vec2, radius: f32, color: [u8; 4]) {
        let r2 = radius * radius;
        for y in (center.y - radius) as i32..=(center.y + radius) as i32 {
            for x in (center.x - radius) as i32..=(center.x + radius) as i32 {
                if Vec2::new(x as f32, y as f32).distance_squared(center) <= r2 {
                    self.blend(x, y, color);
                }
            }
        }
    }

    pub(super) fn ring(&mut self, center: Vec2, outer: f32, inner: f32, color: [u8; 4]) {
        let outer2 = outer * outer;
        let inner2 = inner * inner;
        for y in (center.y - outer) as i32..=(center.y + outer) as i32 {
            for x in (center.x - outer) as i32..=(center.x + outer) as i32 {
                let d = Vec2::new(x as f32, y as f32).distance_squared(center);
                if d <= outer2 && d >= inner2 {
                    self.blend(x, y, color);
                }
            }
        }
    }

    pub(super) fn ellipse(&mut self, center: Vec2, rx: f32, ry: f32, color: [u8; 4]) {
        for y in (center.y - ry) as i32..=(center.y + ry) as i32 {
            for x in (center.x - rx) as i32..=(center.x + rx) as i32 {
                let p = Vec2::new((x as f32 - center.x) / rx, (y as f32 - center.y) / ry);
                if p.length_squared() <= 1.0 {
                    self.blend(x, y, color);
                }
            }
        }
    }

    pub(super) fn diamond(&mut self, center: Vec2, rx: f32, ry: f32, color: [u8; 4]) {
        for y in (center.y - ry) as i32..=(center.y + ry) as i32 {
            for x in (center.x - rx) as i32..=(center.x + rx) as i32 {
                let d = ((x as f32 - center.x).abs() / rx) + ((y as f32 - center.y).abs() / ry);
                if d <= 1.0 {
                    self.blend(x, y, color);
                }
            }
        }
    }

    pub(super) fn glow(&mut self, center: Vec2, radius: f32, color: [u8; 4]) {
        for y in (center.y - radius) as i32..=(center.y + radius) as i32 {
            for x in (center.x - radius) as i32..=(center.x + radius) as i32 {
                let d = Vec2::new(x as f32, y as f32).distance(center) / radius;
                if d <= 1.0 {
                    let mut c = color;
                    c[3] = (color[3] as f32 * (1.0 - d).powf(1.8)) as u8;
                    self.blend(x, y, c);
                }
            }
        }
    }

    pub(super) fn line(&mut self, a: Vec2, b: Vec2, color: [u8; 4], width: f32) {
        let steps = a.distance(b).max(1.0) as i32;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.circle(a.lerp(b, t), width, color);
        }
    }
}
