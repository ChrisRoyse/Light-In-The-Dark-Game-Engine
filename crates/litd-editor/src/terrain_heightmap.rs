use std::{collections::BTreeMap, fs, path::PathBuf};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::{
    history::{EditorDocument, stable_fingerprint},
    terrain_bulk_ops::terrain_bulk_summary,
};

#[path = "terrain_heightmap_io.rs"]
mod io;

use io::{
    read_heightmap_samples, sample_count, sample_index, sample_to_height, samples_from_document,
    sha256_file, write_heightmap,
};

const HEIGHT_EPSILON: f32 = 0.000_1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeightmapFileFormat {
    Png,
    Raw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeightmapBitDepth {
    U8,
    U16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeightmapByteOrder {
    LittleEndian,
    BigEndian,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeightmapImportSpec {
    pub path: PathBuf,
    pub format: HeightmapFileFormat,
    pub raw_bit_depth: Option<HeightmapBitDepth>,
    pub raw_byte_order: HeightmapByteOrder,
    pub min_height: f32,
    pub max_height: f32,
    pub flip_vertical: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeightmapExportSpec {
    pub path: PathBuf,
    pub format: HeightmapFileFormat,
    pub bit_depth: HeightmapBitDepth,
    pub raw_byte_order: HeightmapByteOrder,
    pub min_height: f32,
    pub max_height: f32,
    pub flip_vertical: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainHeightmapImportReport {
    pub operation: String,
    pub source_path: String,
    pub source_sha256: String,
    pub format: HeightmapFileFormat,
    pub bit_depth: HeightmapBitDepth,
    pub width: u32,
    pub height: u32,
    pub min_height: f32,
    pub max_height: f32,
    pub changed_count: usize,
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub before_summary: String,
    pub after_summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainHeightmapExportReport {
    pub operation: String,
    pub output_path: String,
    pub output_sha256: String,
    pub format: HeightmapFileFormat,
    pub bit_depth: HeightmapBitDepth,
    pub width: u32,
    pub height: u32,
    pub min_height: f32,
    pub max_height: f32,
    pub sample_count: usize,
    pub document_fingerprint: String,
    pub document_summary: String,
}

#[derive(Debug, Clone)]
pub(super) struct TerrainGrid {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) ids_by_coord: BTreeMap<(i32, i32), String>,
}

impl HeightmapImportSpec {
    pub fn png(path: impl Into<PathBuf>, min_height: f32, max_height: f32) -> Self {
        Self {
            path: path.into(),
            format: HeightmapFileFormat::Png,
            raw_bit_depth: None,
            raw_byte_order: HeightmapByteOrder::LittleEndian,
            min_height,
            max_height,
            flip_vertical: false,
        }
    }

    pub fn raw(
        path: impl Into<PathBuf>,
        bit_depth: HeightmapBitDepth,
        byte_order: HeightmapByteOrder,
        min_height: f32,
        max_height: f32,
    ) -> Self {
        Self {
            path: path.into(),
            format: HeightmapFileFormat::Raw,
            raw_bit_depth: Some(bit_depth),
            raw_byte_order: byte_order,
            min_height,
            max_height,
            flip_vertical: false,
        }
    }
}

impl HeightmapExportSpec {
    pub fn png(
        path: impl Into<PathBuf>,
        bit_depth: HeightmapBitDepth,
        min_height: f32,
        max_height: f32,
    ) -> Self {
        Self {
            path: path.into(),
            format: HeightmapFileFormat::Png,
            bit_depth,
            raw_byte_order: HeightmapByteOrder::LittleEndian,
            min_height,
            max_height,
            flip_vertical: false,
        }
    }

    pub fn raw(
        path: impl Into<PathBuf>,
        bit_depth: HeightmapBitDepth,
        byte_order: HeightmapByteOrder,
        min_height: f32,
        max_height: f32,
    ) -> Self {
        Self {
            path: path.into(),
            format: HeightmapFileFormat::Raw,
            bit_depth,
            raw_byte_order: byte_order,
            min_height,
            max_height,
            flip_vertical: false,
        }
    }
}

pub fn import_heightmap(
    document: &mut EditorDocument,
    spec: &HeightmapImportSpec,
) -> Result<TerrainHeightmapImportReport, EngineError> {
    validate_height_range(spec.min_height, spec.max_height)?;
    document.validate()?;
    let grid = terrain_grid(document)?;
    let source_sha256 = sha256_file(&spec.path)?;
    let samples = read_heightmap_samples(spec, grid.width, grid.height)?;
    let before = document.clone();
    let mut after = before.clone();
    let mut changed_count = 0;
    for z in 0..grid.height {
        for x in 0..grid.width {
            let sample_z = if spec.flip_vertical {
                grid.height - 1 - z
            } else {
                z
            };
            let index = sample_index(grid.width, x, sample_z)?;
            let height = sample_to_height(
                samples.values[index],
                samples.bit_depth,
                spec.min_height,
                spec.max_height,
            )?;
            let id = grid
                .ids_by_coord
                .get(&(x as i32, z as i32))
                .ok_or_else(|| heightmap_error("grid.coord", format!("missing cell {x},{z}")))?;
            let cell = after
                .terrain_cells
                .get_mut(id)
                .ok_or_else(|| heightmap_error("grid.cell", format!("missing cell {id}")))?;
            if (cell.height - height).abs() > HEIGHT_EPSILON {
                cell.height = height;
                changed_count += 1;
            }
        }
    }
    if changed_count == 0 {
        return invalid_heightmap(
            "import.no_change",
            "heightmap import changed no terrain cells",
        );
    }
    after.validate()?;
    let before_fingerprint = stable_fingerprint(&before)?;
    let after_fingerprint = stable_fingerprint(&after)?;
    if before_fingerprint == after_fingerprint {
        return invalid_heightmap(
            "import.fingerprint",
            "heightmap import did not mutate state",
        );
    }
    let report = TerrainHeightmapImportReport {
        operation: "heightmap.import".to_string(),
        source_path: spec.path.display().to_string(),
        source_sha256,
        format: spec.format,
        bit_depth: samples.bit_depth,
        width: samples.width,
        height: samples.height,
        min_height: spec.min_height,
        max_height: spec.max_height,
        changed_count,
        before_fingerprint,
        after_fingerprint,
        before_summary: terrain_bulk_summary(&before),
        after_summary: terrain_bulk_summary(&after),
    };
    *document = after;
    Ok(report)
}

pub fn export_heightmap(
    document: &EditorDocument,
    spec: &HeightmapExportSpec,
) -> Result<TerrainHeightmapExportReport, EngineError> {
    validate_height_range(spec.min_height, spec.max_height)?;
    document.validate()?;
    let grid = terrain_grid(document)?;
    let samples = samples_from_document(document, spec, &grid)?;
    if let Some(parent) = spec.path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            heightmap_error(
                "export.parent",
                format!("failed to create {}: {err}", parent.display()),
            )
        })?;
    }
    write_heightmap(
        &spec.path,
        spec.format,
        spec.bit_depth,
        spec.raw_byte_order,
        grid.width,
        grid.height,
        &samples,
    )?;
    let output_sha256 = sha256_file(&spec.path)?;
    Ok(TerrainHeightmapExportReport {
        operation: "heightmap.export".to_string(),
        output_path: spec.path.display().to_string(),
        output_sha256,
        format: spec.format,
        bit_depth: spec.bit_depth,
        width: grid.width,
        height: grid.height,
        min_height: spec.min_height,
        max_height: spec.max_height,
        sample_count: samples.len(),
        document_fingerprint: stable_fingerprint(document)?,
        document_summary: terrain_bulk_summary(document),
    })
}

pub fn heightmap_samples_from_document(
    document: &EditorDocument,
    spec: &HeightmapExportSpec,
) -> Result<Vec<u16>, EngineError> {
    validate_height_range(spec.min_height, spec.max_height)?;
    document.validate()?;
    let grid = terrain_grid(document)?;
    samples_from_document(document, spec, &grid)
}

fn terrain_grid(document: &EditorDocument) -> Result<TerrainGrid, EngineError> {
    let width = document.world_properties.width_tiles;
    let height = document.world_properties.height_tiles;
    if width == 0 || height == 0 {
        return invalid_heightmap("grid.dimensions", "world dimensions must be non-zero");
    }
    let mut ids_by_coord = BTreeMap::new();
    for cell in document.terrain_cells.values() {
        ids_by_coord.insert((cell.tile_x, cell.tile_z), cell.id.clone());
    }
    for z in 0..height {
        for x in 0..width {
            if !ids_by_coord.contains_key(&(x as i32, z as i32)) {
                return invalid_heightmap("grid.coord", format!("missing terrain cell {x},{z}"));
            }
        }
    }
    let expected = sample_count(width, height)?;
    if ids_by_coord.len() != expected {
        return invalid_heightmap(
            "grid.extra_cells",
            format!(
                "terrain grid has {} cells, expected {expected}",
                ids_by_coord.len()
            ),
        );
    }
    Ok(TerrainGrid {
        width,
        height,
        ids_by_coord,
    })
}

fn validate_height_range(min_height: f32, max_height: f32) -> Result<(), EngineError> {
    if !min_height.is_finite() || !max_height.is_finite() || max_height <= min_height {
        return invalid_heightmap(
            "height.range",
            "min_height and max_height must be finite and strictly increasing",
        );
    }
    Ok(())
}

pub(crate) fn invalid_heightmap<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(heightmap_error(field, reason))
}

pub(crate) fn heightmap_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.terrain_heightmap.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "terrain_heightmap_tests.rs"]
mod tests;
