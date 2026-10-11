use litd_engine::{EngineError, render::editor_world::EditorWorldTextureData};

use crate::asset_vault::AssetVaultRecord;

use super::invalid_composition;

#[path = "asset_scene_composition_atlas_pixels.rs"]
mod pixels;

use pixels::{
    copy_gltf_image_to_tile, copy_manifest_metallic_roughness_to_tile, copy_manifest_rgba_to_tile,
    decode_manifest_rgba, filled_atlas,
};

const TILE_SIZE: u32 = 512;
const ATLAS_COLUMNS: u32 = 8;
const ATLAS_ROWS: u32 = 8;
const ATLAS_SIZE: u32 = TILE_SIZE * ATLAS_COLUMNS;
const MAX_TILES: usize = (ATLAS_COLUMNS * ATLAS_ROWS) as usize;
const DEFAULT_BASE_COLOR_RGBA: [u8; 4] = [255, 255, 255, 255];
const DEFAULT_NORMAL_RGBA: [u8; 4] = [128, 128, 255, 255];
const DEFAULT_METALLIC_ROUGHNESS_RGBA: [u8; 4] = [255, 255, 0, 255];

#[derive(Debug, Clone, Copy)]
pub(super) struct AtlasRegion {
    tile_index: usize,
}

impl AtlasRegion {
    pub(super) fn map_uv(self, uv: [f32; 2]) -> [f32; 2] {
        let col = (self.tile_index as u32) % ATLAS_COLUMNS;
        let row = (self.tile_index as u32) / ATLAS_COLUMNS;
        let margin = 1.0_f32;
        let span = TILE_SIZE as f32 - margin * 2.0;
        let local_u = uv[0].rem_euclid(1.0);
        let local_v = uv[1].rem_euclid(1.0);
        [
            (col as f32 * TILE_SIZE as f32 + margin + local_u * span) / ATLAS_SIZE as f32,
            (row as f32 * TILE_SIZE as f32 + margin + local_v * span) / ATLAS_SIZE as f32,
        ]
    }
}

#[derive(Debug, Clone)]
struct AtlasTile {
    source_path: String,
    region: AtlasRegion,
}

pub(super) struct TextureAtlasData {
    pub(super) base_color: EditorWorldTextureData,
    pub(super) normal: EditorWorldTextureData,
    pub(super) metallic_roughness: EditorWorldTextureData,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum MetallicRoughnessSource {
    Arm,
    RoughnessOnly,
}

pub(super) struct TextureAtlasBuilder {
    base_color_rgba8: Vec<u8>,
    normal_rgba8: Vec<u8>,
    metallic_roughness_rgba8: Vec<u8>,
    tiles: Vec<AtlasTile>,
    gltf_normal_tiles: usize,
    gltf_metallic_roughness_tiles: usize,
    manifest_normal_tiles: usize,
    manifest_metallic_roughness_tiles: usize,
}

impl TextureAtlasBuilder {
    pub(super) fn new() -> Self {
        let mut atlas = Self {
            base_color_rgba8: filled_atlas(DEFAULT_BASE_COLOR_RGBA),
            normal_rgba8: filled_atlas(DEFAULT_NORMAL_RGBA),
            metallic_roughness_rgba8: filled_atlas(DEFAULT_METALLIC_ROUGHNESS_RGBA),
            tiles: Vec::new(),
            gltf_normal_tiles: 0,
            gltf_metallic_roughness_tiles: 0,
            manifest_normal_tiles: 0,
            manifest_metallic_roughness_tiles: 0,
        };
        atlas.tiles.push(AtlasTile {
            source_path: "generated://gltf-spec-default/base-color-white".to_string(),
            region: AtlasRegion { tile_index: 0 },
        });
        atlas
    }

    pub(super) fn region_for_material(
        &mut self,
        material: &gltf::Material<'_>,
        scene_source_relative_path: &str,
        images: &[gltf::image::Data],
    ) -> Result<AtlasRegion, EngineError> {
        let pbr = material.pbr_metallic_roughness();
        let base_image = pbr.base_color_texture().map(|info| info.texture().source());
        let normal_image = material
            .normal_texture()
            .map(|info| info.texture().source());
        let metallic_roughness_image = pbr
            .metallic_roughness_texture()
            .map(|info| info.texture().source());
        if base_image.is_none() && normal_image.is_none() && metallic_roughness_image.is_none() {
            return Ok(self.tiles[0].region);
        }
        let base_source_path = base_image
            .as_ref()
            .map(|image| image_source_relative_path(scene_source_relative_path, image))
            .unwrap_or_else(|| "generated://gltf-spec-default/base-color-white".to_string());
        let normal_source_path = normal_image
            .as_ref()
            .map(|image| image_source_relative_path(scene_source_relative_path, image));
        let metallic_roughness_source_path = metallic_roughness_image
            .as_ref()
            .map(|image| image_source_relative_path(scene_source_relative_path, image));
        let tile_key = material_tile_key(
            &base_source_path,
            normal_source_path.as_deref(),
            metallic_roughness_source_path.as_deref(),
        );
        if let Some(tile) = self.tiles.iter().find(|tile| tile.source_path == tile_key) {
            return Ok(tile.region);
        }
        if self.tiles.len() >= MAX_TILES {
            return invalid_composition(
                "atlas.capacity",
                format!("PBR atlas exceeds {MAX_TILES} texture tiles"),
            );
        }
        let region = AtlasRegion {
            tile_index: self.tiles.len(),
        };
        if let Some(image) = base_image {
            let image_data =
                images
                    .get(image.index())
                    .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                        field: "editor.asset_scene.composition.atlas.image".to_string(),
                        reason: format!(
                            "{} references missing imported image {}",
                            base_source_path,
                            image.index()
                        ),
                    })?;
            copy_gltf_image_to_tile(
                &mut self.base_color_rgba8,
                region,
                image_data,
                &base_source_path,
            )?;
        }
        if let Some(image) = normal_image {
            let source_path = normal_source_path
                .as_deref()
                .expect("normal source path is derived with normal image");
            let image_data =
                images
                    .get(image.index())
                    .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                        field: "editor.asset_scene.composition.atlas.normal_image".to_string(),
                        reason: format!(
                            "{} references missing imported image {}",
                            source_path,
                            image.index()
                        ),
                    })?;
            copy_gltf_image_to_tile(&mut self.normal_rgba8, region, image_data, source_path)?;
            self.gltf_normal_tiles += 1;
        }
        if let Some(image) = metallic_roughness_image {
            let source_path = metallic_roughness_source_path
                .as_deref()
                .expect("metallic-roughness source path is derived with MR image");
            let image_data =
                images
                    .get(image.index())
                    .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                        field: "editor.asset_scene.composition.atlas.image".to_string(),
                        reason: format!(
                            "{} references missing imported image {}",
                            source_path,
                            image.index()
                        ),
                    })?;
            copy_gltf_image_to_tile(
                &mut self.metallic_roughness_rgba8,
                region,
                image_data,
                source_path,
            )?;
            self.gltf_metallic_roughness_tiles += 1;
        }
        self.tiles.push(AtlasTile {
            source_path: tile_key,
            region,
        });
        Ok(region)
    }

    pub(super) fn region_for_manifest_pbr(
        &mut self,
        base_color: &AssetVaultRecord,
        normal: &AssetVaultRecord,
        metallic_roughness: &AssetVaultRecord,
        metallic_roughness_source: MetallicRoughnessSource,
    ) -> Result<AtlasRegion, EngineError> {
        if let Some(tile) = self
            .tiles
            .iter()
            .find(|tile| tile.source_path == base_color.relative_path)
        {
            return Ok(tile.region);
        }
        if self.tiles.len() >= MAX_TILES {
            return invalid_composition(
                "atlas.capacity",
                format!("PBR atlas exceeds {MAX_TILES} texture tiles"),
            );
        }
        let base_color_image = decode_manifest_rgba(base_color)?;
        let normal_image = decode_manifest_rgba(normal)?;
        let metallic_roughness_image = decode_manifest_rgba(metallic_roughness)?;
        let region = AtlasRegion {
            tile_index: self.tiles.len(),
        };
        copy_manifest_rgba_to_tile(
            &mut self.base_color_rgba8,
            region,
            base_color_image.width(),
            base_color_image.height(),
            base_color_image.as_raw(),
            &base_color.relative_path,
        )?;
        copy_manifest_rgba_to_tile(
            &mut self.normal_rgba8,
            region,
            normal_image.width(),
            normal_image.height(),
            normal_image.as_raw(),
            &normal.relative_path,
        )?;
        copy_manifest_metallic_roughness_to_tile(
            &mut self.metallic_roughness_rgba8,
            region,
            metallic_roughness_image.width(),
            metallic_roughness_image.height(),
            metallic_roughness_image.as_raw(),
            &metallic_roughness.relative_path,
            metallic_roughness_source,
        )?;
        self.manifest_normal_tiles += 1;
        self.manifest_metallic_roughness_tiles += 1;
        self.tiles.push(AtlasTile {
            source_path: base_color.relative_path.clone(),
            region,
        });
        Ok(region)
    }

    pub(super) fn into_texture_data(self) -> Result<TextureAtlasData, EngineError> {
        let tile_count = self.tiles.len();
        let base_color = EditorWorldTextureData::new(
            "composition-base-color-atlas",
            format!(
                "generated://texture-atlas/stylized-nature-base-color?tile_size={TILE_SIZE}&tiles={tile_count}"
            ),
            ATLAS_SIZE,
            ATLAS_SIZE,
            self.base_color_rgba8,
        )?;
        let normal = EditorWorldTextureData::new(
            "composition-normal-atlas",
            format!(
                "generated://texture-atlas/stylized-nature-normal?tile_size={TILE_SIZE}&tiles={tile_count}&gltf_normal_tiles={}&manifest_normal_tiles={}",
                self.gltf_normal_tiles, self.manifest_normal_tiles
            ),
            ATLAS_SIZE,
            ATLAS_SIZE,
            self.normal_rgba8,
        )?;
        let metallic_roughness = EditorWorldTextureData::new(
            "composition-metallic-roughness-atlas",
            format!(
                "generated://texture-atlas/stylized-nature-metallic-roughness?tile_size={TILE_SIZE}&tiles={tile_count}&gltf_metallic_roughness_tiles={}&manifest_metallic_roughness_tiles={}",
                self.gltf_metallic_roughness_tiles, self.manifest_metallic_roughness_tiles
            ),
            ATLAS_SIZE,
            ATLAS_SIZE,
            self.metallic_roughness_rgba8,
        )?;
        Ok(TextureAtlasData {
            base_color,
            normal,
            metallic_roughness,
        })
    }
}

fn material_tile_key(
    base_color: &str,
    normal: Option<&str>,
    metallic_roughness: Option<&str>,
) -> String {
    format!(
        "base={};normal={};metallic_roughness={}",
        base_color,
        normal.unwrap_or("generated://gltf-spec-default/normal-flat"),
        metallic_roughness.unwrap_or("generated://gltf-spec-default/metallic-roughness")
    )
}

fn image_source_relative_path(scene_source_relative_path: &str, image: &gltf::Image<'_>) -> String {
    match image.source() {
        gltf::image::Source::Uri { uri, .. } => {
            if uri.starts_with("data:") {
                format!("{}#image[{}]", scene_source_relative_path, image.index())
            } else if let Some((parent, _)) = scene_source_relative_path.rsplit_once('/') {
                format!("{parent}/{uri}").replace('\\', "/")
            } else {
                uri.replace('\\', "/")
            }
        }
        gltf::image::Source::View { .. } => {
            format!("{}#image[{}]", scene_source_relative_path, image.index())
        }
    }
}
