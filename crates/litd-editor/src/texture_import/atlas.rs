use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use super::{
    ProcessedTextureAsset, TextureRole, rgba_len, sha256_hex, texture_import_error,
    validate_rgba_len,
};

const ATLAS_EDGE_PADDING: u32 = 1;
const DEFAULT_ATLAS_TILE_SIZE: u32 = 512;
const DEFAULT_ATLAS_COLUMNS: u32 = 8;
const DEFAULT_MAX_ATLAS_SIZE: u32 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureAtlasSettings {
    pub tile_size: u32,
    pub columns: u32,
    pub max_size: u32,
}

impl Default for TextureAtlasSettings {
    fn default() -> Self {
        Self {
            tile_size: DEFAULT_ATLAS_TILE_SIZE,
            columns: DEFAULT_ATLAS_COLUMNS,
            max_size: DEFAULT_MAX_ATLAS_SIZE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextureAtlasPlacement {
    pub source_relative_path: String,
    pub role: TextureRole,
    pub source_sha256: String,
    pub decoded_rgba8_sha256: String,
    pub tile_index: usize,
    pub tile_x: u32,
    pub tile_y: u32,
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextureAtlasReceipt {
    pub atlas_id: String,
    pub width: u32,
    pub height: u32,
    pub tile_size: u32,
    pub columns: u32,
    pub rows: u32,
    pub texture_count: usize,
    pub rgba8_bytes: usize,
    pub rgba8_sha256: String,
    pub placements: Vec<TextureAtlasPlacement>,
}

#[derive(Debug, Clone)]
pub struct TextureAtlasBuild {
    pub receipt: TextureAtlasReceipt,
    pub rgba8: Vec<u8>,
}

pub fn build_texture_atlas(
    textures: &[ProcessedTextureAsset],
    settings: TextureAtlasSettings,
) -> Result<TextureAtlasBuild, EngineError> {
    validate_atlas_settings(settings)?;
    if textures.is_empty() {
        return texture_import_error("atlas.inputs", "at least one texture is required");
    }
    let rows = (textures.len() as u32).div_ceil(settings.columns);
    let width = settings
        .columns
        .checked_mul(settings.tile_size)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.texture_import.atlas.size".to_string(),
            reason: "atlas width overflow".to_string(),
        })?;
    let height =
        rows.checked_mul(settings.tile_size)
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.texture_import.atlas.size".to_string(),
                reason: "atlas height overflow".to_string(),
            })?;
    if width > settings.max_size || height > settings.max_size {
        return texture_import_error(
            "atlas.capacity",
            format!(
                "atlas {width}x{height} exceeds max {}x{}",
                settings.max_size, settings.max_size
            ),
        );
    }
    let mut rgba8 = vec![0_u8; rgba_len(width, height, "generated://texture-atlas")?];
    let mut placements = Vec::with_capacity(textures.len());
    for (index, texture) in textures.iter().enumerate() {
        let tile_x = (index as u32 % settings.columns) * settings.tile_size;
        let tile_y = (index as u32 / settings.columns) * settings.tile_size;
        copy_resampled_tile(
            &mut rgba8,
            width,
            height,
            tile_x,
            tile_y,
            settings.tile_size,
            texture,
        )?;
        let min_u = (tile_x + ATLAS_EDGE_PADDING) as f32 / width as f32;
        let min_v = (tile_y + ATLAS_EDGE_PADDING) as f32 / height as f32;
        let max_u = (tile_x + settings.tile_size - ATLAS_EDGE_PADDING) as f32 / width as f32;
        let max_v = (tile_y + settings.tile_size - ATLAS_EDGE_PADDING) as f32 / height as f32;
        placements.push(TextureAtlasPlacement {
            source_relative_path: texture.receipt.source_relative_path.clone(),
            role: texture.receipt.role,
            source_sha256: texture.receipt.source_sha256.clone(),
            decoded_rgba8_sha256: texture.receipt.decoded_rgba8_sha256.clone(),
            tile_index: index,
            tile_x,
            tile_y,
            uv_min: [min_u, min_v],
            uv_max: [max_u, max_v],
        });
    }
    let rgba8_sha256 = sha256_hex(&rgba8);
    let atlas_id = format!("litd.texture_atlas.rgba8.{}", &rgba8_sha256[..16]);
    Ok(TextureAtlasBuild {
        receipt: TextureAtlasReceipt {
            atlas_id,
            width,
            height,
            tile_size: settings.tile_size,
            columns: settings.columns,
            rows,
            texture_count: textures.len(),
            rgba8_bytes: rgba8.len(),
            rgba8_sha256,
            placements,
        },
        rgba8,
    })
}

fn validate_atlas_settings(settings: TextureAtlasSettings) -> Result<(), EngineError> {
    if settings.tile_size <= ATLAS_EDGE_PADDING * 2 {
        return texture_import_error("atlas.tile_size", "tile_size must leave room for padding");
    }
    if settings.columns == 0 || settings.max_size == 0 {
        return texture_import_error("atlas.settings", "columns and max_size must be non-zero");
    }
    if settings.tile_size > settings.max_size {
        return texture_import_error("atlas.tile_size", "tile_size exceeds max atlas size");
    }
    Ok(())
}

fn copy_resampled_tile(
    target: &mut [u8],
    atlas_width: u32,
    atlas_height: u32,
    tile_x: u32,
    tile_y: u32,
    tile_size: u32,
    texture: &ProcessedTextureAsset,
) -> Result<(), EngineError> {
    let source = &texture.receipt.source_relative_path;
    validate_rgba_len(
        texture.receipt.width,
        texture.receipt.height,
        &texture.decoded_rgba8,
        source,
    )?;
    for y in 0..tile_size {
        for x in 0..tile_size {
            let inner_x = x.clamp(ATLAS_EDGE_PADDING, tile_size - ATLAS_EDGE_PADDING - 1)
                - ATLAS_EDGE_PADDING;
            let inner_y = y.clamp(ATLAS_EDGE_PADDING, tile_size - ATLAS_EDGE_PADDING - 1)
                - ATLAS_EDGE_PADDING;
            let inner_span = tile_size - ATLAS_EDGE_PADDING * 2;
            let src_x = inner_x * texture.receipt.width / inner_span;
            let src_y = inner_y * texture.receipt.height / inner_span;
            let pixel = read_rgba_pixel(
                &texture.decoded_rgba8,
                texture.receipt.width,
                texture.receipt.height,
                src_x.min(texture.receipt.width - 1),
                src_y.min(texture.receipt.height - 1),
                source,
            )?;
            write_rgba_pixel(
                target,
                atlas_width,
                atlas_height,
                tile_x + x,
                tile_y + y,
                pixel,
                source,
            )?;
        }
    }
    Ok(())
}

fn read_rgba_pixel(
    rgba8: &[u8],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    source: &str,
) -> Result<[u8; 4], EngineError> {
    if x >= width || y >= height {
        return texture_import_error(
            "atlas.sample",
            format!("{source} sample coordinate {x},{y} exceeds {width}x{height}"),
        );
    }
    let offset = ((y as usize * width as usize) + x as usize) * 4;
    Ok([
        rgba8[offset],
        rgba8[offset + 1],
        rgba8[offset + 2],
        rgba8[offset + 3],
    ])
}

fn write_rgba_pixel(
    target: &mut [u8],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    pixel: [u8; 4],
    source: &str,
) -> Result<(), EngineError> {
    if x >= width || y >= height {
        return texture_import_error(
            "atlas.write",
            format!("{source} atlas coordinate {x},{y} exceeds {width}x{height}"),
        );
    }
    let offset = ((y as usize * width as usize) + x as usize) * 4;
    target[offset..offset + 4].copy_from_slice(&pixel);
    Ok(())
}
