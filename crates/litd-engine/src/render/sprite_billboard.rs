//! Content-addressed sprite textures, materials, and scene billboards.

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::EngineError;

#[path = "sprite_billboard_defaults.rs"]
mod sprite_billboard_defaults;
#[path = "sprite_billboard_gpu.rs"]
mod sprite_billboard_gpu;
#[path = "sprite_billboard_search.rs"]
mod sprite_billboard_search;
#[path = "sprite_billboard_validate.rs"]
mod sprite_billboard_validate;

pub use sprite_billboard_gpu::{
    SpriteBillboardGpuBillboardReadback, SpriteBillboardGpuFrame, SpriteBillboardGpuScene,
    SpriteBillboardGpuStats, render_sprite_billboard_library_gpu,
    render_sprite_billboard_library_gpu_with_context,
};
use sprite_billboard_search::build_search_rows;
use sprite_billboard_validate::{ValidatedSpriteBillboardLibrary, frame_uv, validate_library};

pub const SPRITE_BILLBOARD_SCHEMA: &str = "litd.render.sprite_billboard_library.v1";
const MAX_TEXTURES: usize = 128;
const MAX_MATERIALS: usize = 256;
const MAX_SHEETS: usize = 128;
const MAX_BILLBOARDS: usize = 2048;
/// Row alignment (px) the GPU atlas builder rounds the combined width up to.
/// Must match the alignment used by `build_atlas`.
const ATLAS_ROW_ALIGNMENT: u32 = 64;
/// Maximum allowed combined-atlas dimension (px). Bounded to the wgpu/WebGPU
/// baseline `max_texture_dimension_2d` (8192) so a validated library always
/// fits a 2D texture on the #928 min-spec target and can never overflow the
/// u32 arithmetic in `build_atlas`.
const MAX_ATLAS_DIMENSION: u32 = 8192;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteTextureRecord {
    pub texture_id: String,
    pub source_relative_path: String,
    pub width: u32,
    pub height: u32,
    pub rgba8_sha256: String,
    pub rgba8: Vec<u8>,
    pub tags: Vec<String>,
}

impl SpriteTextureRecord {
    pub fn content_address(&self) -> String {
        format!("sha256:{}", self.rgba8_sha256)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpriteAlphaMode {
    Opaque,
    Masked,
    Blended,
    Additive,
}

impl SpriteAlphaMode {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Opaque => "opaque",
            Self::Masked => "masked",
            Self::Blended => "blended",
            Self::Additive => "additive",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteMaterialRecord {
    pub material_id: String,
    pub texture_id: String,
    pub tint: [f32; 4],
    pub alpha_mode: SpriteAlphaMode,
    pub alpha_cutoff: f32,
    pub emissive_rgb: [f32; 3],
    pub unlit: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteSheetRecord {
    pub sheet_id: String,
    pub texture_id: String,
    pub columns: u32,
    pub rows: u32,
    pub frame_count: u32,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillboardFacingMode {
    CameraFacing,
    AxisLockedY,
    ScreenAligned,
}

impl BillboardFacingMode {
    pub const fn id(self) -> &'static str {
        match self {
            Self::CameraFacing => "camera_facing",
            Self::AxisLockedY => "axis_locked_y",
            Self::ScreenAligned => "screen_aligned",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteBillboardInstance {
    pub billboard_id: String,
    pub material_id: String,
    pub sheet_id: Option<String>,
    pub frame_index: u32,
    pub center: [f32; 3],
    pub size: [f32; 2],
    pub facing: BillboardFacingMode,
    pub sort_layer: i32,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteBillboardLibrarySelection {
    pub schema: String,
    pub library_id: String,
    pub selected_billboard_id: String,
    pub textures: Vec<SpriteTextureRecord>,
    pub materials: Vec<SpriteMaterialRecord>,
    pub sheets: Vec<SpriteSheetRecord>,
    pub billboards: Vec<SpriteBillboardInstance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteBillboardLibraryReadback {
    pub schema: String,
    pub library_id: String,
    pub source_of_truth: String,
    pub selected_billboard_id: String,
    pub texture_count: usize,
    pub material_count: usize,
    pub sheet_count: usize,
    pub billboard_count: usize,
    pub texture_content_addresses: Vec<String>,
    pub material_texture_content_addresses: Vec<String>,
    pub search_row_count: usize,
    pub search_rows: Vec<SpriteBillboardSearchRow>,
    pub total_vertex_count: usize,
    pub total_index_count: usize,
    pub triangle_count: usize,
    pub meshes: Vec<SpriteBillboardMeshReadback>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteBillboardSearchRow {
    pub row_id: String,
    pub entity_kind: String,
    pub primary_id: String,
    pub texture_id: Option<String>,
    pub material_id: Option<String>,
    pub content_address: Option<String>,
    pub terms: Vec<String>,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Pod, Zeroable)]
pub struct SpriteBillboardVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteBillboardMeshReadback {
    pub billboard_id: String,
    pub material_id: String,
    pub texture_id: String,
    pub texture_content_address: String,
    pub sheet_id: Option<String>,
    pub frame_index: u32,
    pub frame_uv: [f32; 4],
    pub facing: String,
    pub vertex_count: usize,
    pub index_count: usize,
    pub triangle_count: usize,
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub vertices: Vec<SpriteBillboardVertex>,
    pub indices: Vec<u32>,
}

impl SpriteBillboardLibrarySelection {
    pub fn issue868_defaults() -> Result<Self, EngineError> {
        sprite_billboard_defaults::issue868_defaults()
    }
}

pub fn build_sprite_billboard_library_readback(
    library: &SpriteBillboardLibrarySelection,
) -> Result<SpriteBillboardLibraryReadback, EngineError> {
    let validated = validate_library(library)?;
    let mut meshes = Vec::with_capacity(library.billboards.len());
    for billboard in &library.billboards {
        meshes.push(mesh_readback(billboard, &validated)?);
    }
    let texture_content_addresses = library
        .textures
        .iter()
        .map(SpriteTextureRecord::content_address)
        .collect::<Vec<_>>();
    let material_texture_content_addresses = library
        .materials
        .iter()
        .map(|material| validated.textures[material.texture_id.as_str()].content_address())
        .collect::<Vec<_>>();
    let search_rows = build_search_rows(library, &validated);
    let total_vertex_count = meshes.iter().map(|mesh| mesh.vertex_count).sum();
    let total_index_count = meshes.iter().map(|mesh| mesh.index_count).sum();
    Ok(SpriteBillboardLibraryReadback {
        schema: SPRITE_BILLBOARD_SCHEMA.to_string(),
        library_id: library.library_id.clone(),
        source_of_truth: "Rust render::sprite_billboard content-addressed RGBA8 textures, material records, sheets, and billboard mesh readback".to_string(),
        selected_billboard_id: library.selected_billboard_id.clone(),
        texture_count: library.textures.len(),
        material_count: library.materials.len(),
        sheet_count: library.sheets.len(),
        billboard_count: library.billboards.len(),
        texture_content_addresses,
        material_texture_content_addresses,
        search_row_count: search_rows.len(),
        search_rows,
        total_vertex_count,
        total_index_count,
        triangle_count: total_index_count / 3,
        meshes,
    })
}

fn mesh_readback(
    billboard: &SpriteBillboardInstance,
    validated: &ValidatedSpriteBillboardLibrary<'_>,
) -> Result<SpriteBillboardMeshReadback, EngineError> {
    let material = validated.materials[billboard.material_id.as_str()];
    let texture = validated.textures[material.texture_id.as_str()];
    let frame_uv = if let Some(sheet_id) = &billboard.sheet_id {
        let sheet = validated.sheets[sheet_id.as_str()];
        if sheet.texture_id != material.texture_id {
            return invalid_sprite_billboard(
                "billboard.sheet_id",
                "sheet texture must match material texture",
            );
        }
        frame_uv(sheet, billboard.frame_index)?
    } else {
        if billboard.frame_index != 0 {
            return invalid_sprite_billboard(
                "billboard.frame_index",
                "non-sheet billboards must use frame_index 0",
            );
        }
        [0.0, 0.0, 1.0, 1.0]
    };
    let [cx, cy, cz] = billboard.center;
    let half_w = billboard.size[0] * 0.5;
    let half_h = billboard.size[1] * 0.5;
    let color = material.tint;
    let vertices = vec![
        SpriteBillboardVertex {
            position: [cx - half_w, cy - half_h, cz],
            uv: [frame_uv[0], frame_uv[3]],
            color,
        },
        SpriteBillboardVertex {
            position: [cx + half_w, cy - half_h, cz],
            uv: [frame_uv[2], frame_uv[3]],
            color,
        },
        SpriteBillboardVertex {
            position: [cx + half_w, cy + half_h, cz],
            uv: [frame_uv[2], frame_uv[1]],
            color,
        },
        SpriteBillboardVertex {
            position: [cx - half_w, cy + half_h, cz],
            uv: [frame_uv[0], frame_uv[1]],
            color,
        },
    ];
    Ok(SpriteBillboardMeshReadback {
        billboard_id: billboard.billboard_id.clone(),
        material_id: material.material_id.clone(),
        texture_id: texture.texture_id.clone(),
        texture_content_address: texture.content_address(),
        sheet_id: billboard.sheet_id.clone(),
        frame_index: billboard.frame_index,
        frame_uv,
        facing: billboard.facing.id().to_string(),
        vertex_count: 4,
        index_count: 6,
        triangle_count: 2,
        bounds_min: [cx - half_w, cy - half_h, cz],
        bounds_max: [cx + half_w, cy + half_h, cz],
        vertices,
        indices: vec![0, 1, 2, 0, 2, 3],
    })
}

pub(crate) fn invalid_sprite_billboard<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(sprite_billboard_error(field, reason))
}

pub(crate) fn sprite_billboard_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("render.sprite_billboard.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "sprite_billboard_tests.rs"]
mod tests;
