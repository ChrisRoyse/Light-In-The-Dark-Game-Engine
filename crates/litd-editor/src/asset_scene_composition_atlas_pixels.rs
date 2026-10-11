use image::{ImageBuffer, Rgba};
use litd_engine::EngineError;

use crate::asset_vault::AssetVaultRecord;

use super::{ATLAS_COLUMNS, ATLAS_SIZE, AtlasRegion, MetallicRoughnessSource, TILE_SIZE};

pub(super) fn filled_atlas(pixel: [u8; 4]) -> Vec<u8> {
    let mut rgba8 = Vec::with_capacity((ATLAS_SIZE * ATLAS_SIZE * 4) as usize);
    for _ in 0..(ATLAS_SIZE * ATLAS_SIZE) {
        rgba8.extend_from_slice(&pixel);
    }
    rgba8
}

pub(super) fn decode_manifest_rgba(
    record: &AssetVaultRecord,
) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>, EngineError> {
    let image = image::ImageReader::open(&record.absolute_path)
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.atlas.open_image".to_string(),
            reason: format!(
                "{} failed image open: {error}",
                record.absolute_path.display()
            ),
        })?
        .decode()
        .map_err(|error| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.atlas.decode_image".to_string(),
            reason: format!(
                "{} failed image decode: {error}",
                record.absolute_path.display()
            ),
        })?
        .to_rgba8();
    validate_rgba_image(
        image.width(),
        image.height(),
        image.as_raw(),
        &record.relative_path,
    )?;
    Ok(image)
}

pub(super) fn copy_gltf_image_to_tile(
    target: &mut [u8],
    region: AtlasRegion,
    image: &gltf::image::Data,
    source_path: &str,
) -> Result<(), EngineError> {
    validate_gltf_image(image, source_path)?;
    let tile_col = region.tile_index as u32 % ATLAS_COLUMNS;
    let tile_row = region.tile_index as u32 / ATLAS_COLUMNS;
    for y in 0..TILE_SIZE {
        let src_y = y * image.height / TILE_SIZE;
        for x in 0..TILE_SIZE {
            let src_x = x * image.width / TILE_SIZE;
            let rgba = rgba_pixel(image, src_x, src_y, source_path)?;
            write_tile_pixel(target, tile_col, tile_row, x, y, rgba);
        }
    }
    Ok(())
}

pub(super) fn copy_manifest_rgba_to_tile(
    target: &mut [u8],
    region: AtlasRegion,
    width: u32,
    height: u32,
    pixels: &[u8],
    source_path: &str,
) -> Result<(), EngineError> {
    let tile_col = region.tile_index as u32 % ATLAS_COLUMNS;
    let tile_row = region.tile_index as u32 / ATLAS_COLUMNS;
    for y in 0..TILE_SIZE {
        let src_y = y * height / TILE_SIZE;
        for x in 0..TILE_SIZE {
            let src_x = x * width / TILE_SIZE;
            let rgba = rgba_manifest_pixel(width, height, pixels, src_x, src_y, source_path)?;
            write_tile_pixel(target, tile_col, tile_row, x, y, rgba);
        }
    }
    Ok(())
}

pub(super) fn copy_manifest_metallic_roughness_to_tile(
    target: &mut [u8],
    region: AtlasRegion,
    width: u32,
    height: u32,
    pixels: &[u8],
    source_path: &str,
    source: MetallicRoughnessSource,
) -> Result<(), EngineError> {
    let tile_col = region.tile_index as u32 % ATLAS_COLUMNS;
    let tile_row = region.tile_index as u32 / ATLAS_COLUMNS;
    for y in 0..TILE_SIZE {
        let src_y = y * height / TILE_SIZE;
        for x in 0..TILE_SIZE {
            let src_x = x * width / TILE_SIZE;
            let rgba = rgba_manifest_pixel(width, height, pixels, src_x, src_y, source_path)?;
            let packed = match source {
                MetallicRoughnessSource::Arm => rgba,
                MetallicRoughnessSource::RoughnessOnly => [255, rgba[1], 0, 255],
            };
            write_tile_pixel(target, tile_col, tile_row, x, y, packed);
        }
    }
    Ok(())
}

fn write_tile_pixel(
    target: &mut [u8],
    tile_col: u32,
    tile_row: u32,
    x: u32,
    y: u32,
    rgba: [u8; 4],
) {
    let dst_x = tile_col * TILE_SIZE + x;
    let dst_y = tile_row * TILE_SIZE + y;
    let dst = ((dst_y * ATLAS_SIZE + dst_x) * 4) as usize;
    target[dst..dst + 4].copy_from_slice(&rgba);
}

fn validate_rgba_image(
    width: u32,
    height: u32,
    pixels: &[u8],
    source_path: &str,
) -> Result<(), EngineError> {
    if width == 0 || height == 0 {
        return invalid_atlas("atlas.image", format!("{source_path} has zero dimensions"));
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|count| count.checked_mul(4))
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.atlas.image.bytes".to_string(),
            reason: format!("{source_path} dimensions overflow byte count"),
        })?;
    if pixels.len() != expected {
        return invalid_atlas(
            "atlas.image.bytes",
            format!(
                "{source_path} decoded RGBA byte count {} != expected {expected}",
                pixels.len()
            ),
        );
    }
    Ok(())
}

fn rgba_manifest_pixel(
    width: u32,
    height: u32,
    pixels: &[u8],
    x: u32,
    y: u32,
    source_path: &str,
) -> Result<[u8; 4], EngineError> {
    if x >= width || y >= height {
        return invalid_atlas(
            "atlas.image.coordinates",
            format!("{source_path} sample coordinate {x},{y} exceeds image dimensions"),
        );
    }
    let offset = ((y as usize * width as usize) + x as usize) * 4;
    Ok([
        pixels[offset],
        pixels[offset + 1],
        pixels[offset + 2],
        pixels[offset + 3],
    ])
}

fn validate_gltf_image(image: &gltf::image::Data, source_path: &str) -> Result<(), EngineError> {
    if image.width == 0 || image.height == 0 {
        return invalid_atlas("atlas.image", format!("{source_path} has zero dimensions"));
    }
    let pixel_count = (image.width as usize)
        .checked_mul(image.height as usize)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.composition.atlas.image.size".to_string(),
            reason: format!("{source_path} dimensions overflow pixel count"),
        })?;
    let channels = match image.format {
        gltf::image::Format::R8G8B8A8 => 4,
        gltf::image::Format::R8G8B8 => 3,
        format => {
            return invalid_atlas(
                "atlas.image.format",
                format!("{source_path} uses unsupported glTF texture format {format:?}"),
            );
        }
    };
    let expected =
        pixel_count
            .checked_mul(channels)
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.asset_scene.composition.atlas.image.bytes".to_string(),
                reason: format!("{source_path} dimensions overflow byte count"),
            })?;
    if image.pixels.len() != expected {
        return invalid_atlas(
            "atlas.image.bytes",
            format!(
                "{source_path} decoded byte count {} != expected {expected}",
                image.pixels.len()
            ),
        );
    }
    Ok(())
}

fn rgba_pixel(
    image: &gltf::image::Data,
    x: u32,
    y: u32,
    source_path: &str,
) -> Result<[u8; 4], EngineError> {
    if x >= image.width || y >= image.height {
        return invalid_atlas(
            "atlas.image.coordinates",
            format!("{source_path} sample coordinate {x},{y} exceeds image dimensions"),
        );
    }
    let pixel = (y as usize * image.width as usize) + x as usize;
    match image.format {
        gltf::image::Format::R8G8B8A8 => {
            let offset = pixel * 4;
            Ok([
                image.pixels[offset],
                image.pixels[offset + 1],
                image.pixels[offset + 2],
                image.pixels[offset + 3],
            ])
        }
        gltf::image::Format::R8G8B8 => {
            let offset = pixel * 3;
            Ok([
                image.pixels[offset],
                image.pixels[offset + 1],
                image.pixels[offset + 2],
                255,
            ])
        }
        format => invalid_atlas(
            "atlas.image.format",
            format!("{source_path} uses unsupported glTF texture format {format:?}"),
        ),
    }
}

fn invalid_atlas<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_scene.composition.{}", field.into()),
        reason: reason.into(),
    })
}
