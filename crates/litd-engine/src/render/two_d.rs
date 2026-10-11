//! Deterministic 2D reference pipeline for sprites, tilemaps, and 2D lighting.

use serde::{Deserialize, Serialize};

use crate::{EngineError, render::reference::ReferenceSceneImage};

/// UV rect of one sprite-sheet frame: (top-left uv, bottom-right uv). Named so the
/// per-sprite UV list reads as a list of frame rects rather than a nested array pair.
pub type SpriteFrameUv = ([f32; 2], [f32; 2]);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteSheet2d {
    pub id: String,
    pub columns: u32,
    pub rows: u32,
    pub frame_width_px: u32,
    pub frame_height_px: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tile2d {
    pub frame_index: u32,
    pub material: TileMaterial2d,
    pub normal: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TileMaterial2d {
    Basalt,
    Moss,
    Water,
    GoldTrim,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileMap2d {
    pub columns: u32,
    pub rows: u32,
    pub tile_size_px: u32,
    pub tiles: Vec<Tile2d>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteInstance2d {
    pub id: String,
    pub frame_index: u32,
    pub position_px: [f32; 2],
    pub size_px: [f32; 2],
    pub tint: [f32; 4],
    pub normal: [f32; 3],
    pub emissive: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Light2d {
    pub id: String,
    pub position_px: [f32; 2],
    pub radius_px: f32,
    pub color: [f32; 3],
    pub intensity: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Render2dSettings {
    pub output_width: u32,
    pub output_height: u32,
    pub target_fps: u32,
    pub max_draw_calls: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Render2dPipelinePlan {
    pub issue: u32,
    pub settings: Render2dSettings,
    pub sprite_sheet: SpriteSheet2d,
    pub tilemap: TileMap2d,
    pub sprites: Vec<SpriteInstance2d>,
    pub lights: Vec<Light2d>,
    pub normal_map_contract: String,
    pub reference_images: Vec<String>,
    pub draw_calls: u32,
}

impl SpriteSheet2d {
    pub fn frame_count(&self) -> u32 {
        self.columns.saturating_mul(self.rows)
    }

    pub fn frame_uv(&self, frame_index: u32) -> Result<([f32; 2], [f32; 2]), EngineError> {
        if self.columns == 0 || self.rows == 0 || frame_index >= self.frame_count() {
            return invalid_2d(
                "sprite_sheet.frame_index",
                "frame index is outside the sheet",
            );
        }
        let col = frame_index % self.columns;
        let row = frame_index / self.columns;
        let inv_cols = 1.0 / self.columns as f32;
        let inv_rows = 1.0 / self.rows as f32;
        Ok((
            [col as f32 * inv_cols, row as f32 * inv_rows],
            [(col + 1) as f32 * inv_cols, (row + 1) as f32 * inv_rows],
        ))
    }
}

impl Render2dPipelinePlan {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.issue != 1154 {
            return invalid_2d("issue", "2D pipeline plan must be bound to #1154");
        }
        if self.settings.output_width < 320 || self.settings.output_height < 180 {
            return invalid_2d("settings.output", "2D output must be at least 320x180");
        }
        if self.settings.target_fps < 60 {
            return invalid_2d(
                "settings.target_fps",
                "2D pipeline must target at least 60fps",
            );
        }
        if self.draw_calls > self.settings.max_draw_calls {
            return invalid_2d("draw_calls", "2D pipeline exceeded draw-call budget");
        }
        if self.tilemap.tiles.len()
            != self.tilemap.columns.saturating_mul(self.tilemap.rows) as usize
        {
            return invalid_2d("tilemap.tiles", "tile count must match tilemap dimensions");
        }
        if self.tilemap.tile_size_px == 0 {
            return invalid_2d("tilemap.tile_size_px", "tile size must be non-zero");
        }
        if self.lights.is_empty() {
            return invalid_2d("lights", "2D normal lighting requires at least one light");
        }
        for tile in &self.tilemap.tiles {
            self.sprite_sheet.frame_uv(tile.frame_index)?;
            validate_normal(tile.normal)?;
        }
        for sprite in &self.sprites {
            self.sprite_sheet.frame_uv(sprite.frame_index)?;
            validate_normal(sprite.normal)?;
            if sprite.size_px[0] <= 0.0 || sprite.size_px[1] <= 0.0 {
                return invalid_2d("sprite.size_px", "sprite size must be positive");
            }
        }
        for light in &self.lights {
            if light.radius_px <= 0.0 || light.intensity <= 0.0 {
                return invalid_2d("light", "light radius and intensity must be positive");
            }
        }
        Ok(())
    }

    pub fn sprite_frame_uvs(&self) -> Result<Vec<SpriteFrameUv>, EngineError> {
        self.sprites
            .iter()
            .map(|sprite| self.sprite_sheet.frame_uv(sprite.frame_index))
            .collect()
    }

    pub fn normal_mapped_tile_count(&self) -> usize {
        self.tilemap
            .tiles
            .iter()
            .filter(|tile| tile.normal != [0.0, 0.0, 1.0])
            .count()
    }
}

pub fn build_issue1154_2d_pipeline_plan(
    settings: Render2dSettings,
) -> Result<Render2dPipelinePlan, EngineError> {
    let sprite_sheet = SpriteSheet2d {
        id: "litd.issue1154.sprite_sheet".to_string(),
        columns: 4,
        rows: 4,
        frame_width_px: 32,
        frame_height_px: 32,
    };
    let tilemap = reference_tilemap();
    let sprites = vec![
        sprite(
            "hero",
            5,
            [470.0, 288.0],
            [44.0, 58.0],
            [0.95, 0.72, 0.24, 1.0],
            0.12,
        ),
        sprite(
            "ranger",
            6,
            [628.0, 248.0],
            [40.0, 54.0],
            [0.26, 0.73, 0.95, 1.0],
            0.20,
        ),
        sprite(
            "ember_imp",
            9,
            [380.0, 315.0],
            [42.0, 42.0],
            [1.0, 0.25, 0.08, 1.0],
            0.75,
        ),
        sprite(
            "ward",
            12,
            [552.0, 190.0],
            [36.0, 48.0],
            [0.70, 0.95, 1.0, 1.0],
            0.45,
        ),
    ];
    let lights = vec![
        Light2d {
            id: "torch_warm".to_string(),
            position_px: [420.0, 280.0],
            radius_px: 220.0,
            color: [1.0, 0.46, 0.12],
            intensity: 1.15,
        },
        Light2d {
            id: "arcane_blue".to_string(),
            position_px: [650.0, 240.0],
            radius_px: 250.0,
            color: [0.18, 0.68, 1.0],
            intensity: 0.95,
        },
    ];
    let plan = Render2dPipelinePlan {
        issue: 1154,
        settings,
        sprite_sheet,
        tilemap,
        sprites,
        lights,
        normal_map_contract:
            "per-tile and per-sprite tangent-space normals modulate 2D light falloff".to_string(),
        reference_images: vec![
            "docs/readme/ED12-play-topdown.png".to_string(),
            "docs/readme/GP03-combat.png".to_string(),
            "docs/readme/ED11-world-view.png".to_string(),
        ],
        draw_calls: 5,
    };
    plan.validate()?;
    Ok(plan)
}

pub fn render_2d_pipeline_reference(
    plan: &Render2dPipelinePlan,
) -> Result<ReferenceSceneImage, EngineError> {
    super::two_d_image::render_reference_image(plan)
}

pub fn issue1154_settings() -> Render2dSettings {
    Render2dSettings {
        output_width: 960,
        output_height: 540,
        target_fps: 60,
        max_draw_calls: 8,
    }
}

fn reference_tilemap() -> TileMap2d {
    let columns = 18;
    let rows = 10;
    let tiles = (0..rows)
        .flat_map(|y| {
            (0..columns).map(move |x| {
                let material = if x < 3 {
                    TileMaterial2d::Water
                } else if (x + y) % 7 == 0 {
                    TileMaterial2d::GoldTrim
                } else if y > 6 || x > 13 {
                    TileMaterial2d::Moss
                } else {
                    TileMaterial2d::Basalt
                };
                Tile2d {
                    frame_index: ((x + y * 2) % 12),
                    material,
                    normal: match (x + y) % 4 {
                        0 => [0.12, -0.04, 0.99],
                        1 => [-0.10, 0.08, 0.99],
                        2 => [0.04, 0.14, 0.98],
                        _ => [0.0, 0.0, 1.0],
                    },
                }
            })
        })
        .collect();
    TileMap2d {
        columns,
        rows,
        tile_size_px: 38,
        tiles,
    }
}

fn sprite(
    id: &str,
    frame_index: u32,
    position_px: [f32; 2],
    size_px: [f32; 2],
    tint: [f32; 4],
    emissive: f32,
) -> SpriteInstance2d {
    SpriteInstance2d {
        id: id.to_string(),
        frame_index,
        position_px,
        size_px,
        tint,
        normal: [0.0, -0.12, 0.99],
        emissive,
    }
}

fn validate_normal(normal: [f32; 3]) -> Result<(), EngineError> {
    let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if !(0.8..=1.2).contains(&len) || normal[2] < 0.2 {
        return invalid_2d(
            "normal",
            "2D normal-map vectors must be unit-ish and face the camera",
        );
    }
    Ok(())
}

fn invalid_2d<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::ReferenceSceneInvalidConfig {
        field: format!("render.two_d.{}", field.into()),
        reason: reason.into(),
    })
}
