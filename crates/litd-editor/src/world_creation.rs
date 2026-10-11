//! New-world creation records and deterministic initial terrain generation.

use std::collections::BTreeMap;

use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use litd_hash::{FNV1A64_OFFSET, FNV1A64_PRIME};
use serde::{Deserialize, Serialize};

use crate::{
    history::{EditorDocument, EditorObject, EditorRegion, stable_fingerprint},
    terrain_bulk_ops::EditorTerrainCell,
};

#[path = "world_creation_store.rs"]
mod store;

pub use store::{
    WORLD_ASTER_CHUNK_SCHEMA, WORLD_ASTER_CHUNK_SIZE, WORLD_ASTER_COLLECTION, WORLD_ASTER_COLUMN,
    WORLD_ASTER_ENV, WORLD_ASTER_MANIFEST_COLUMN, WORLD_ASTER_ROW_PREFIX, WorldAsterReadback,
    aster_dir_from_env, default_aster_dir, parse_world_record, read_world_record,
    write_world_record,
};

pub const WORLD_CREATION_SCHEMA_VERSION: u32 = 1;
pub const WORLD_CREATION_SOURCE_ISSUE: u32 = 1210;
pub const WORLD_RECORD_KIND: &str = "litd.editor.world_record";
pub const WORLD_CREATE_OPERATION_ID: &str = "editor.world.create";
pub const WORLD_READBACK_TOOL_NAME: &str = "litd.editor.world.readback";
pub const MIN_WORLD_DIMENSION_TILES: u32 = 4;
pub const MAX_WORLD_DIMENSION_TILES: u32 = 128;
pub const MIN_WORLD_LEVEL: i16 = -16;
pub const MAX_WORLD_LEVEL: i16 = 16;
pub const DEFAULT_WORLD_TILESET_ID: &str = "tileset.ashen_grass";
pub const DEFAULT_WORLD_TILE_ID: &str = "tile.ashen_grass.default";
pub const DEFAULT_WORLD_CLIFF_TYPE: &str = "cliff.natural";
pub const DEFAULT_WORLD_LOADING_SCREEN: &str = "docs/readme/ED01-editor-shell.png";
pub const DEFAULT_WORLD_PREVIEW: &str = "docs/readme/ED11-world-view.png";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorPlayableBounds {
    pub min_tile_x: i32,
    pub min_tile_z: i32,
    pub max_tile_x_exclusive: i32,
    pub max_tile_z_exclusive: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorWorldProperties {
    pub display_name: String,
    pub description: String,
    pub width_tiles: u32,
    pub height_tiles: u32,
    pub tileset_id: String,
    pub initial_tile_id: String,
    pub initial_cliff_type: String,
    pub initial_cliff_level: i16,
    pub water_level: i16,
    pub random_height_seed: Option<u64>,
    pub playable_bounds: EditorPlayableBounds,
    pub loading_screen_asset_ref: String,
    pub preview_asset_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorWorldCreationSpec {
    pub calyx_document_id: String,
    pub revision_id: String,
    pub properties: EditorWorldProperties,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorWorldRecord {
    pub schema_version: u32,
    pub source_issue: u32,
    pub record_kind: String,
    pub capability_operation_id: String,
    pub readback_tool_name: String,
    pub spec: EditorWorldCreationSpec,
    pub document: EditorDocument,
    pub document_fingerprint: String,
    pub terrain_cell_count: usize,
    pub default_tile_count: usize,
    pub playable_region_id: String,
    pub summary: String,
}

impl Default for EditorPlayableBounds {
    fn default() -> Self {
        Self {
            min_tile_x: 0,
            min_tile_z: 0,
            max_tile_x_exclusive: 16,
            max_tile_z_exclusive: 12,
        }
    }
}

impl Default for EditorWorldProperties {
    fn default() -> Self {
        Self {
            display_name: "Untitled World".to_string(),
            description: "New Light in the Dark world".to_string(),
            width_tiles: 16,
            height_tiles: 12,
            tileset_id: DEFAULT_WORLD_TILESET_ID.to_string(),
            initial_tile_id: DEFAULT_WORLD_TILE_ID.to_string(),
            initial_cliff_type: DEFAULT_WORLD_CLIFF_TYPE.to_string(),
            initial_cliff_level: 0,
            water_level: -2,
            random_height_seed: None,
            playable_bounds: EditorPlayableBounds::default(),
            loading_screen_asset_ref: DEFAULT_WORLD_LOADING_SCREEN.to_string(),
            preview_asset_ref: DEFAULT_WORLD_PREVIEW.to_string(),
        }
    }
}

impl EditorWorldCreationSpec {
    pub fn issue1210_default() -> Self {
        Self {
            calyx_document_id: "calyx://world/issue1210-new-world".to_string(),
            revision_id: "issue1210-new-world-rev-1".to_string(),
            properties: EditorWorldProperties {
                display_name: "Issue 1210 Ashen Test World".to_string(),
                description: "Known world creation settings for issue 1210 FSV".to_string(),
                random_height_seed: Some(1210),
                ..EditorWorldProperties::default()
            },
        }
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.calyx_document_id.trim().is_empty() || self.revision_id.trim().is_empty() {
            return invalid_world_creation(
                "spec.identity",
                "calyx_document_id and revision_id must be non-empty",
            );
        }
        self.properties.validate()
    }
}

impl EditorPlayableBounds {
    pub fn validate(&self, width_tiles: u32, height_tiles: u32) -> Result<(), EngineError> {
        if self.min_tile_x < 0 || self.min_tile_z < 0 {
            return invalid_world_creation(
                "properties.playable_bounds.min",
                "playable bounds minimum must be non-negative",
            );
        }
        if self.max_tile_x_exclusive <= self.min_tile_x
            || self.max_tile_z_exclusive <= self.min_tile_z
        {
            return invalid_world_creation(
                "properties.playable_bounds.extent",
                "playable bounds must have positive width and height",
            );
        }
        if self.max_tile_x_exclusive > width_tiles as i32
            || self.max_tile_z_exclusive > height_tiles as i32
        {
            return invalid_world_creation(
                "properties.playable_bounds.world",
                "playable bounds must stay inside world dimensions",
            );
        }
        Ok(())
    }

    fn center(&self) -> Vec3 {
        Vec3::new(
            (self.min_tile_x + self.max_tile_x_exclusive) as f32 * 0.5,
            0.0,
            (self.min_tile_z + self.max_tile_z_exclusive) as f32 * 0.5,
        )
    }

    fn half_extents(&self) -> Vec3 {
        Vec3::new(
            (self.max_tile_x_exclusive - self.min_tile_x) as f32 * 0.5,
            16.0,
            (self.max_tile_z_exclusive - self.min_tile_z) as f32 * 0.5,
        )
    }
}

impl EditorWorldProperties {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.display_name.trim().is_empty()
            || self.tileset_id.trim().is_empty()
            || self.initial_tile_id.trim().is_empty()
            || self.initial_cliff_type.trim().is_empty()
            || self.loading_screen_asset_ref.trim().is_empty()
            || self.preview_asset_ref.trim().is_empty()
        {
            return invalid_world_creation(
                "properties.required",
                "display name, tileset, tile, cliff type, loading screen, and preview are required",
            );
        }
        validate_dimension("width_tiles", self.width_tiles)?;
        validate_dimension("height_tiles", self.height_tiles)?;
        validate_world_level("initial_cliff_level", self.initial_cliff_level)?;
        validate_world_level("water_level", self.water_level)?;
        self.playable_bounds
            .validate(self.width_tiles, self.height_tiles)
    }
}

pub fn create_editor_world_record(
    spec: EditorWorldCreationSpec,
) -> Result<EditorWorldRecord, EngineError> {
    spec.validate()?;
    let mut document = EditorDocument::new(&spec.calyx_document_id, &spec.revision_id)?;
    document.world_properties = spec.properties.clone();
    document.upsert_object(root_world_object(&spec.properties))?;
    document.upsert_region(playable_region(&spec.properties.playable_bounds))?;
    fill_default_terrain_cells(&mut document, &spec.properties)?;
    document.validate()?;
    let document_fingerprint = stable_fingerprint(&document)?;
    let terrain_cell_count = document.terrain_cells.len();
    let default_tile_count = document
        .terrain_cells
        .values()
        .filter(|cell| cell.tile_id == spec.properties.initial_tile_id)
        .count();
    let summary = world_record_summary(
        &spec.properties,
        terrain_cell_count,
        default_tile_count,
        &document_fingerprint,
    );
    Ok(EditorWorldRecord {
        schema_version: WORLD_CREATION_SCHEMA_VERSION,
        source_issue: WORLD_CREATION_SOURCE_ISSUE,
        record_kind: WORLD_RECORD_KIND.to_string(),
        capability_operation_id: WORLD_CREATE_OPERATION_ID.to_string(),
        readback_tool_name: WORLD_READBACK_TOOL_NAME.to_string(),
        spec,
        document,
        document_fingerprint,
        terrain_cell_count,
        default_tile_count,
        playable_region_id: "playable_bounds".to_string(),
        summary,
    })
}

pub fn validate_world_record(record: &EditorWorldRecord) -> Result<(), EngineError> {
    if record.schema_version != WORLD_CREATION_SCHEMA_VERSION {
        return invalid_world_creation(
            "record.schema_version",
            format!(
                "expected {}, got {}",
                WORLD_CREATION_SCHEMA_VERSION, record.schema_version
            ),
        );
    }
    if record.record_kind != WORLD_RECORD_KIND {
        return invalid_world_creation(
            "record.kind",
            format!("expected {WORLD_RECORD_KIND}, got {}", record.record_kind),
        );
    }
    if record.capability_operation_id != WORLD_CREATE_OPERATION_ID
        || record.readback_tool_name != WORLD_READBACK_TOOL_NAME
    {
        return invalid_world_creation(
            "record.capability",
            "world creation record capability route changed",
        );
    }
    record.spec.validate()?;
    record.document.validate()?;
    let fingerprint = stable_fingerprint(&record.document)?;
    if fingerprint != record.document_fingerprint {
        return invalid_world_creation(
            "record.fingerprint",
            "document fingerprint does not match persisted world record",
        );
    }
    if record.terrain_cell_count != record.document.terrain_cells.len() {
        return invalid_world_creation("record.terrain_cell_count", "terrain cell count drifted");
    }
    if record.default_tile_count
        != record
            .document
            .terrain_cells
            .values()
            .filter(|cell| cell.tile_id == record.spec.properties.initial_tile_id)
            .count()
    {
        return invalid_world_creation("record.default_tile_count", "default tile count drifted");
    }
    Ok(())
}

fn root_world_object(properties: &EditorWorldProperties) -> EditorObject {
    EditorObject::new("world", "World", None, Transform::IDENTITY)
        .with_property("display_name", properties.display_name.clone())
        .with_property("description", properties.description.clone())
        .with_property("width_tiles", properties.width_tiles.to_string())
        .with_property("height_tiles", properties.height_tiles.to_string())
        .with_property("tileset_id", properties.tileset_id.clone())
        .with_property("initial_tile_id", properties.initial_tile_id.clone())
        .with_property("water_level", properties.water_level.to_string())
        .with_property(
            "loading_screen_asset_ref",
            properties.loading_screen_asset_ref.clone(),
        )
        .with_property("preview_asset_ref", properties.preview_asset_ref.clone())
}

fn playable_region(bounds: &EditorPlayableBounds) -> EditorRegion {
    EditorRegion {
        id: "playable_bounds".to_string(),
        name: "Playable Bounds".to_string(),
        center: bounds.center(),
        half_extents: bounds.half_extents(),
        properties: BTreeMap::from([
            (
                "source_issue".to_string(),
                WORLD_CREATION_SOURCE_ISSUE.to_string(),
            ),
            ("kind".to_string(), "playable_bounds".to_string()),
        ]),
    }
}

fn fill_default_terrain_cells(
    document: &mut EditorDocument,
    properties: &EditorWorldProperties,
) -> Result<(), EngineError> {
    for z in 0..properties.height_tiles {
        for x in 0..properties.width_tiles {
            let id = format!("terrain_cell_{x}_{z}");
            let height = terrain_height_for_cell(x, z, properties);
            document.upsert_terrain_cell(EditorTerrainCell::new(
                id,
                x as i32,
                z as i32,
                properties.initial_tile_id.clone(),
                properties.initial_cliff_type.clone(),
                properties.initial_cliff_level,
                height,
            ))?;
        }
    }
    Ok(())
}

fn terrain_height_for_cell(x: u32, z: u32, properties: &EditorWorldProperties) -> f32 {
    let base = f32::from(properties.initial_cliff_level);
    let Some(seed) = properties.random_height_seed else {
        return base;
    };
    let hash = stable_noise(seed, x, z);
    base + ((hash % 1001) as f32 / 1000.0 - 0.5) * 0.5
}

fn stable_noise(seed: u64, x: u32, z: u32) -> u64 {
    let mut value = seed ^ FNV1A64_OFFSET;
    for next in [u64::from(x), u64::from(z), seed.rotate_left(17)] {
        value ^= next;
        value = value.wrapping_mul(FNV1A64_PRIME);
    }
    value
}

fn validate_dimension(field: &str, value: u32) -> Result<(), EngineError> {
    if !(MIN_WORLD_DIMENSION_TILES..=MAX_WORLD_DIMENSION_TILES).contains(&value) {
        return invalid_world_creation(
            format!("properties.{field}"),
            format!("{field} must be {MIN_WORLD_DIMENSION_TILES}..={MAX_WORLD_DIMENSION_TILES}"),
        );
    }
    Ok(())
}

fn validate_world_level(field: &str, value: i16) -> Result<(), EngineError> {
    if !(MIN_WORLD_LEVEL..=MAX_WORLD_LEVEL).contains(&value) {
        return invalid_world_creation(
            format!("properties.{field}"),
            format!("{field} must be {MIN_WORLD_LEVEL}..={MAX_WORLD_LEVEL}"),
        );
    }
    Ok(())
}

fn world_record_summary(
    properties: &EditorWorldProperties,
    cells: usize,
    default_tiles: usize,
    fingerprint: &str,
) -> String {
    format!(
        "world={} dims={}x{} tile={} cells={} default_tiles={} bounds={},{},{}x{} fingerprint={}",
        properties.display_name,
        properties.width_tiles,
        properties.height_tiles,
        properties.initial_tile_id,
        cells,
        default_tiles,
        properties.playable_bounds.min_tile_x,
        properties.playable_bounds.min_tile_z,
        properties.playable_bounds.max_tile_x_exclusive,
        properties.playable_bounds.max_tile_z_exclusive,
        fingerprint
    )
}

pub(crate) fn invalid_world_creation<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.world_creation.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "world_creation_tests.rs"]
mod tests;
