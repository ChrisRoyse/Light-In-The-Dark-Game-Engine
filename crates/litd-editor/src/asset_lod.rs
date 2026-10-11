use litd_hash::sha256_hex;
use std::{fs, path::Path};

use litd_engine::EngineError;
use meshopt::{SimplifyOptions, optimize_overdraw_in_place_decoder, optimize_vertex_cache};
use serde::{Deserialize, Serialize};

use crate::{
    asset_products::{AssetProduct, AssetProductKind},
    asset_scene::load_editor_world_asset_scene,
};

pub const ASSET_LOD_SCHEMA: &str = "litd.asset_lod_product.v1";
const BUILDER_ID: &str = "litd.asset.meshopt_lod_builder";
const BUILDER_FINGERPRINT: &str = "meshopt-0.6.2:index-cache-overdraw-simplify";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetLodSettings {
    pub target_index_ratios: Vec<f32>,
    pub target_error: f32,
    pub overdraw_threshold: f32,
}

impl Default for AssetLodSettings {
    fn default() -> Self {
        Self {
            target_index_ratios: vec![1.0, 0.5, 0.25],
            target_error: 0.02,
            overdraw_threshold: 1.05,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetLodProduct {
    pub schema: String,
    pub builder_id: String,
    pub builder_fingerprint: String,
    pub product_id: String,
    pub source_relative_path: String,
    pub source_sha256: String,
    pub source_vertex_count: usize,
    pub source_index_count: usize,
    pub source_triangle_count: usize,
    pub source_mesh_primitives: u32,
    pub levels: Vec<AssetLodLevel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetLodLevel {
    pub level_index: usize,
    pub level_id: String,
    pub target_index_ratio: f32,
    pub target_index_count: usize,
    pub output_index_count: usize,
    pub output_triangle_count: usize,
    pub result_error: f32,
    pub index_buffer_path: String,
    pub index_buffer_bytes: u64,
    pub index_buffer_sha256: String,
}

pub fn build_asset_lod_product(
    product: &AssetProduct,
    settings: AssetLodSettings,
    output_dir: &Path,
) -> Result<AssetLodProduct, EngineError> {
    validate_settings(&settings)?;
    product.validate()?;
    if product.product_kind != AssetProductKind::Mesh {
        return invalid_lod("product.kind", "LOD generation requires a mesh product");
    }
    let loaded = load_editor_world_asset_scene(product)?;
    let vertices = loaded.scene.vertices();
    let source_indices = loaded.scene.indices();
    validate_source_mesh(vertices.len(), source_indices)?;
    fs::create_dir_all(output_dir).map_err(|error| lod_error("output_dir", error.to_string()))?;
    let positions = vertices
        .iter()
        .map(|vertex| vertex.position)
        .collect::<Vec<[f32; 3]>>();
    let mut levels = Vec::with_capacity(settings.target_index_ratios.len());
    for (level_index, ratio) in settings.target_index_ratios.iter().copied().enumerate() {
        let target_index_count = target_index_count(source_indices.len(), ratio)?;
        let mut result_error = 0.0_f32;
        let mut indices = if level_index == 0 && ratio == 1.0 {
            source_indices.to_vec()
        } else {
            meshopt::simplify_decoder(
                source_indices,
                &positions,
                target_index_count,
                settings.target_error,
                SimplifyOptions::LockBorder,
                Some(&mut result_error),
            )
        };
        validate_level_indices(
            "simplify",
            source_indices.len(),
            vertices.len(),
            ratio,
            &indices,
        )?;
        indices = optimize_vertex_cache(&indices, vertices.len());
        optimize_overdraw_in_place_decoder(&mut indices, &positions, settings.overdraw_threshold);
        validate_level_indices(
            "optimize",
            source_indices.len(),
            vertices.len(),
            ratio,
            &indices,
        )?;
        let level_id = format!("{}.lod{}", product.product_id, level_index);
        let path = output_dir.join(format!("lod{level_index}.indices.u32le"));
        let bytes = u32le_bytes(&indices);
        fs::write(&path, &bytes).map_err(|error| {
            lod_error(
                "index_buffer.write",
                format!("failed to write {}: {error}", path.display()),
            )
        })?;
        let readback = fs::read(&path).map_err(|error| {
            lod_error(
                "index_buffer.readback",
                format!("failed to read {}: {error}", path.display()),
            )
        })?;
        if readback != bytes {
            return invalid_lod(
                "index_buffer.readback",
                "index buffer readback changed bytes",
            );
        }
        levels.push(AssetLodLevel {
            level_index,
            level_id,
            target_index_ratio: ratio,
            target_index_count,
            output_index_count: indices.len(),
            output_triangle_count: indices.len() / 3,
            result_error,
            index_buffer_path: path.display().to_string(),
            index_buffer_bytes: readback.len() as u64,
            index_buffer_sha256: sha256_hex(&readback),
        });
    }
    let lod_product = AssetLodProduct {
        schema: ASSET_LOD_SCHEMA.to_string(),
        builder_id: BUILDER_ID.to_string(),
        builder_fingerprint: BUILDER_FINGERPRINT.to_string(),
        product_id: product.product_id.clone(),
        source_relative_path: product.source_relative_path.clone(),
        source_sha256: product.source_sha256.clone(),
        source_vertex_count: vertices.len(),
        source_index_count: source_indices.len(),
        source_triangle_count: source_indices.len() / 3,
        source_mesh_primitives: loaded.report.mesh_primitives,
        levels,
    };
    validate_asset_lod_product(&lod_product)?;
    tracing::info!(
        product_id = %lod_product.product_id,
        lod_levels = lod_product.levels.len(),
        source_triangles = lod_product.source_triangle_count,
        code = "LITD-ASSET-LOD-BUILT",
        "asset LOD product built"
    );
    Ok(lod_product)
}

pub fn write_asset_lod_product(
    path: &Path,
    product: &AssetLodProduct,
) -> Result<Vec<u8>, EngineError> {
    validate_asset_lod_product(product)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| lod_error("manifest.dir", error.to_string()))?;
    }
    let bytes = serde_json::to_vec_pretty(product)
        .map_err(|error| lod_error("manifest.serialize", error.to_string()))?;
    fs::write(path, &bytes).map_err(|error| {
        lod_error(
            "manifest.write",
            format!("failed to write {}: {error}", path.display()),
        )
    })?;
    let readback = fs::read(path).map_err(|error| {
        lod_error(
            "manifest.readback",
            format!("failed to read {}: {error}", path.display()),
        )
    })?;
    if readback != bytes {
        return invalid_lod("manifest.readback", "manifest readback changed bytes");
    }
    Ok(readback)
}

pub fn read_asset_lod_product(path: &Path) -> Result<AssetLodProduct, EngineError> {
    let bytes = fs::read(path).map_err(|error| {
        lod_error(
            "manifest.read",
            format!("failed to read {}: {error}", path.display()),
        )
    })?;
    let product = serde_json::from_slice::<AssetLodProduct>(&bytes)
        .map_err(|error| lod_error("manifest.json", error.to_string()))?;
    validate_asset_lod_product(&product)?;
    Ok(product)
}

pub fn validate_asset_lod_product(product: &AssetLodProduct) -> Result<(), EngineError> {
    if product.schema != ASSET_LOD_SCHEMA
        || product.builder_id != BUILDER_ID
        || product.builder_fingerprint != BUILDER_FINGERPRINT
        || product.product_id.trim().is_empty()
        || product.source_relative_path.trim().is_empty()
        || product.source_sha256.len() != 64
        || product.source_vertex_count == 0
        || product.source_index_count == 0
        || !product.source_index_count.is_multiple_of(3)
        || product.levels.is_empty()
    {
        return invalid_lod("product", "required LOD product fields are invalid");
    }
    for (index, level) in product.levels.iter().enumerate() {
        validate_level(product, index, level)?;
    }
    Ok(())
}

fn validate_level(
    product: &AssetLodProduct,
    expected_index: usize,
    level: &AssetLodLevel,
) -> Result<(), EngineError> {
    if level.level_index != expected_index
        || level.level_id != format!("{}.lod{expected_index}", product.product_id)
        || !level.target_index_ratio.is_finite()
        || level.target_index_ratio <= 0.0
        || level.target_index_ratio > 1.0
        || level.target_index_count == 0
        || !level.target_index_count.is_multiple_of(3)
        || level.target_index_count > product.source_index_count
        || level.output_index_count == 0
        || !level.output_index_count.is_multiple_of(3)
        || level.output_index_count > product.source_index_count
        || level.output_index_count > level.target_index_count
        || level.output_triangle_count != level.output_index_count / 3
        || level.index_buffer_bytes != (level.output_index_count as u64) * 4
        || level.index_buffer_sha256.len() != 64
    {
        return invalid_lod("level", "LOD level metadata is invalid");
    }
    let bytes = fs::read(&level.index_buffer_path).map_err(|error| {
        lod_error(
            "level.index_buffer",
            format!("failed to read {}: {error}", level.index_buffer_path),
        )
    })?;
    if bytes.len() as u64 != level.index_buffer_bytes
        || sha256_hex(&bytes) != level.index_buffer_sha256
    {
        return invalid_lod(
            "level.index_buffer",
            "index buffer bytes do not match metadata",
        );
    }
    Ok(())
}

fn validate_settings(settings: &AssetLodSettings) -> Result<(), EngineError> {
    if settings.target_index_ratios.is_empty() || settings.target_index_ratios.len() > 8 {
        return invalid_lod("settings.ratios", "LOD ratio count must be 1..=8");
    }
    if !settings.target_error.is_finite()
        || settings.target_error <= 0.0
        || settings.target_error > 1.0
        || !settings.overdraw_threshold.is_finite()
        || settings.overdraw_threshold < 1.0
        || settings.overdraw_threshold > 2.0
    {
        return invalid_lod(
            "settings",
            "target_error and overdraw_threshold are out of range",
        );
    }
    let mut previous = 1.1_f32;
    for ratio in &settings.target_index_ratios {
        if !ratio.is_finite() || *ratio <= 0.0 || *ratio > 1.0 || *ratio >= previous {
            return invalid_lod(
                "settings.ratios",
                "LOD ratios must be finite, in (0,1], and strictly descending",
            );
        }
        previous = *ratio;
    }
    Ok(())
}

fn validate_source_mesh(vertex_count: usize, indices: &[u32]) -> Result<(), EngineError> {
    if vertex_count == 0 || indices.is_empty() || !indices.len().is_multiple_of(3) {
        return invalid_lod("source", "source mesh must contain indexed triangles");
    }
    if indices.iter().any(|index| *index as usize >= vertex_count) {
        return invalid_lod("source.indices", "source index exceeds vertex count");
    }
    Ok(())
}

fn validate_level_indices(
    stage: &str,
    source_index_count: usize,
    vertex_count: usize,
    ratio: f32,
    indices: &[u32],
) -> Result<(), EngineError> {
    if indices.is_empty() || !indices.len().is_multiple_of(3) {
        return invalid_lod(stage, "LOD index buffer must contain complete triangles");
    }
    if indices.len() > source_index_count
        || indices.iter().any(|index| *index as usize >= vertex_count)
    {
        return invalid_lod(stage, "LOD index buffer is outside source mesh bounds");
    }
    if ratio < 1.0 && indices.len() >= source_index_count {
        return invalid_lod(stage, "reduced LOD failed to reduce the source index count");
    }
    Ok(())
}

fn target_index_count(source_index_count: usize, ratio: f32) -> Result<usize, EngineError> {
    let raw = (source_index_count as f32 * ratio).floor() as usize;
    let target = raw - (raw % 3);
    if target < 3 {
        return invalid_lod(
            "target_index_count",
            "LOD target would contain zero triangles",
        );
    }
    Ok(target)
}

fn u32le_bytes(indices: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(indices.len() * 4);
    for index in indices {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    bytes
}

fn invalid_lod<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(lod_error(field, reason.into()))
}

fn lod_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    let field = format!("editor.asset_lod.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-ASSET-LOD-INVALID",
        "asset LOD product rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

#[cfg(test)]
#[path = "asset_lod_tests.rs"]
mod tests;
