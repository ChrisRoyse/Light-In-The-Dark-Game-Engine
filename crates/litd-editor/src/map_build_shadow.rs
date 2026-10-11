use std::{collections::BTreeMap, fs, path::Path};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::history::{EditorDocument, stable_fingerprint, to_stable_ron};

use super::{
    SCHEMA_VERSION, SHADOW_BUILDER_FINGERPRINT, SHADOW_BUILDER_ID, SOURCE_ISSUE, invalid_map_build,
    invalid_map_build_error, io_error, sha256_hex, validate_output_extension, write_text_artifact,
};

const MAX_SHADOW_GRID: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StaticShadowBakeSettings {
    pub grid_width: u32,
    pub grid_height: u32,
    pub world_min_x: f32,
    pub world_max_x: f32,
    pub world_min_z: f32,
    pub world_max_z: f32,
    pub shadow_cast_direction_x: i32,
    pub shadow_cast_direction_z: i32,
}

impl Default for StaticShadowBakeSettings {
    fn default() -> Self {
        Self {
            grid_width: 32,
            grid_height: 32,
            world_min_x: -96.0,
            world_max_x: 96.0,
            world_min_z: -96.0,
            world_max_z: 96.0,
            shadow_cast_direction_x: 1,
            shadow_cast_direction_z: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticShadowCaster {
    pub source_id: String,
    pub source_kind: String,
    pub center_cell: [u32; 2],
    pub footprint_tiles: [u32; 2],
    pub height_units: u32,
    pub strength: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StaticShadowBake {
    pub schema_version: u32,
    pub source_issue: u32,
    pub builder_id: String,
    pub builder_fingerprint: String,
    pub calyx_document_id: String,
    pub source_revision_id: String,
    pub document_fingerprint: String,
    pub static_input_fingerprint: String,
    pub settings: StaticShadowBakeSettings,
    pub casters: Vec<StaticShadowCaster>,
    pub cells_sha256: String,
    pub cells: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StaticShadowBakeReceipt {
    pub output_path: String,
    pub artifact_sha256: String,
    pub bake: StaticShadowBake,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedMapBuild {
    pub schema_version: u32,
    pub source_issue: u32,
    pub calyx_document_id: String,
    pub source_revision_id: String,
    pub document_fingerprint: String,
    pub document: EditorDocument,
    pub baked_static_shadows: StaticShadowBake,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapBuildSaveReceipt {
    pub output_path: String,
    pub artifact_sha256: String,
    pub shadow_receipt: StaticShadowBakeReceipt,
    pub saved_map: SavedMapBuild,
}

pub fn bake_static_shadows(
    document: &EditorDocument,
    settings: StaticShadowBakeSettings,
) -> Result<StaticShadowBake, EngineError> {
    document.validate()?;
    settings.validate()?;
    let document_fingerprint = stable_fingerprint(document)?;
    let casters = collect_shadow_casters(document, settings)?;
    let static_input_fingerprint = stable_fingerprint(&(
        SHADOW_BUILDER_FINGERPRINT,
        settings,
        &document.calyx_document_id,
        &document.revision_id,
        &document.terrain_stamps,
        &casters,
    ))?;
    let mut cells = vec![0; settings.cell_count()?];
    for caster in &casters {
        paint_caster(&mut cells, settings, caster)?;
    }
    let bake = StaticShadowBake {
        schema_version: SCHEMA_VERSION,
        source_issue: SOURCE_ISSUE,
        builder_id: SHADOW_BUILDER_ID.to_string(),
        builder_fingerprint: SHADOW_BUILDER_FINGERPRINT.to_string(),
        calyx_document_id: document.calyx_document_id.clone(),
        source_revision_id: document.revision_id.clone(),
        document_fingerprint,
        static_input_fingerprint,
        settings,
        casters,
        cells_sha256: sha256_hex(&cells),
        cells,
    };
    bake.validate()?;
    Ok(bake)
}

pub fn save_static_shadow_bake(
    document: &EditorDocument,
    output_path: &Path,
    settings: StaticShadowBakeSettings,
) -> Result<StaticShadowBakeReceipt, EngineError> {
    validate_output_extension(output_path, "ron", "shadow.output_path")?;
    let bake = bake_static_shadows(document, settings)?;
    let text = to_stable_ron(&bake)?;
    let artifact_sha256 = write_text_artifact(output_path, &text, "shadow")?;
    let readback = load_static_shadow_bake(output_path)?;
    validate_static_shadow_bake_for_document(&readback, document)?;
    if readback != bake {
        return invalid_map_build("shadow.readback", "readback bake differs from written bake");
    }
    Ok(StaticShadowBakeReceipt {
        output_path: output_path.display().to_string(),
        artifact_sha256,
        bake: readback,
    })
}

pub fn calculate_shadows_and_save_map(
    document: &EditorDocument,
    shadow_path: &Path,
    map_path: &Path,
    settings: StaticShadowBakeSettings,
) -> Result<MapBuildSaveReceipt, EngineError> {
    validate_output_extension(map_path, "ron", "map.output_path")?;
    let shadow_receipt = save_static_shadow_bake(document, shadow_path, settings)?;
    let saved_map = SavedMapBuild::new(document.clone(), shadow_receipt.bake.clone())?;
    let text = to_stable_ron(&saved_map)?;
    let artifact_sha256 = write_text_artifact(map_path, &text, "map")?;
    let readback = load_saved_map_build(map_path)?;
    readback.validate()?;
    if readback != saved_map {
        return invalid_map_build("map.readback", "readback map differs from written map");
    }
    Ok(MapBuildSaveReceipt {
        output_path: map_path.display().to_string(),
        artifact_sha256,
        shadow_receipt,
        saved_map: readback,
    })
}

pub fn load_static_shadow_bake(path: &Path) -> Result<StaticShadowBake, EngineError> {
    let text = fs::read_to_string(path).map_err(|error| io_error("shadow.load", error))?;
    let bake: StaticShadowBake =
        ron::from_str(&text).map_err(|error| EngineError::SerializationFailed {
            format: "ron",
            reason: error.to_string(),
        })?;
    bake.validate()?;
    Ok(bake)
}

pub fn load_saved_map_build(path: &Path) -> Result<SavedMapBuild, EngineError> {
    let text = fs::read_to_string(path).map_err(|error| io_error("map.load", error))?;
    let map: SavedMapBuild =
        ron::from_str(&text).map_err(|error| EngineError::SerializationFailed {
            format: "ron",
            reason: error.to_string(),
        })?;
    map.validate()?;
    Ok(map)
}

pub fn validate_static_shadow_bake_for_document(
    bake: &StaticShadowBake,
    document: &EditorDocument,
) -> Result<(), EngineError> {
    bake.validate()?;
    document.validate()?;
    let expected = bake_static_shadows(document, bake.settings)?;
    if bake.calyx_document_id != expected.calyx_document_id
        || bake.source_revision_id != expected.source_revision_id
        || bake.document_fingerprint != expected.document_fingerprint
        || bake.static_input_fingerprint != expected.static_input_fingerprint
        || bake.cells_sha256 != expected.cells_sha256
    {
        return invalid_map_build(
            "shadow.stale",
            "baked static shadows do not match the current editor document",
        );
    }
    Ok(())
}

impl StaticShadowBakeSettings {
    pub fn validate(self) -> Result<(), EngineError> {
        if self.grid_width == 0
            || self.grid_height == 0
            || self.grid_width > MAX_SHADOW_GRID
            || self.grid_height > MAX_SHADOW_GRID
        {
            return invalid_map_build("shadow.grid", "grid dimensions must be 1..=256");
        }
        if !self.world_min_x.is_finite()
            || !self.world_max_x.is_finite()
            || !self.world_min_z.is_finite()
            || !self.world_max_z.is_finite()
            || self.world_min_x >= self.world_max_x
            || self.world_min_z >= self.world_max_z
        {
            return invalid_map_build("shadow.bounds", "world bounds must be finite and ordered");
        }
        if !(-1..=1).contains(&self.shadow_cast_direction_x)
            || !(-1..=1).contains(&self.shadow_cast_direction_z)
            || (self.shadow_cast_direction_x == 0 && self.shadow_cast_direction_z == 0)
        {
            return invalid_map_build(
                "shadow.direction",
                "shadow cast direction must be a non-zero -1..=1 grid vector",
            );
        }
        self.cell_count()?;
        Ok(())
    }

    fn cell_count(self) -> Result<usize, EngineError> {
        self.grid_width
            .checked_mul(self.grid_height)
            .and_then(|count| usize::try_from(count).ok())
            .ok_or_else(|| invalid_map_build_error("shadow.grid", "grid cell count overflowed"))
    }

    fn world_to_cell(self, x: f32, z: f32) -> [u32; 2] {
        let nx =
            ((x - self.world_min_x) / (self.world_max_x - self.world_min_x)).clamp(0.0, 0.999_999);
        let nz =
            ((z - self.world_min_z) / (self.world_max_z - self.world_min_z)).clamp(0.0, 0.999_999);
        [
            (nx * self.grid_width as f32).floor() as u32,
            (nz * self.grid_height as f32).floor() as u32,
        ]
    }
}

impl StaticShadowBake {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema_version != SCHEMA_VERSION
            || self.source_issue != SOURCE_ISSUE
            || self.builder_id != SHADOW_BUILDER_ID
            || self.builder_fingerprint != SHADOW_BUILDER_FINGERPRINT
        {
            return invalid_map_build("shadow.identity", "unexpected static-shadow bake identity");
        }
        self.settings.validate()?;
        if self.cells.len() != self.settings.cell_count()? {
            return invalid_map_build("shadow.cells", "cell count does not match grid dimensions");
        }
        if sha256_hex(&self.cells) != self.cells_sha256 {
            return invalid_map_build("shadow.cells_sha256", "cell data hash mismatch");
        }
        Ok(())
    }

    pub fn nonzero_cell_count(&self) -> usize {
        self.cells.iter().filter(|value| **value != 0).count()
    }
}

impl SavedMapBuild {
    pub fn new(
        document: EditorDocument,
        baked_static_shadows: StaticShadowBake,
    ) -> Result<Self, EngineError> {
        document.validate()?;
        validate_static_shadow_bake_for_document(&baked_static_shadows, &document)?;
        let saved = Self {
            schema_version: SCHEMA_VERSION,
            source_issue: SOURCE_ISSUE,
            calyx_document_id: document.calyx_document_id.clone(),
            source_revision_id: document.revision_id.clone(),
            document_fingerprint: stable_fingerprint(&document)?,
            document,
            baked_static_shadows,
        };
        saved.validate()?;
        Ok(saved)
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema_version != SCHEMA_VERSION || self.source_issue != SOURCE_ISSUE {
            return invalid_map_build("map.identity", "unexpected saved map build identity");
        }
        self.document.validate()?;
        if self.calyx_document_id != self.document.calyx_document_id
            || self.source_revision_id != self.document.revision_id
            || self.document_fingerprint != stable_fingerprint(&self.document)?
        {
            return invalid_map_build("map.document", "saved document identity mismatch");
        }
        validate_static_shadow_bake_for_document(&self.baked_static_shadows, &self.document)
    }
}

fn collect_shadow_casters(
    document: &EditorDocument,
    settings: StaticShadowBakeSettings,
) -> Result<Vec<StaticShadowCaster>, EngineError> {
    let mut casters = Vec::new();
    for stamp in document.terrain_stamps.values() {
        let height_units = (stamp.height_delta.abs() * 2.0).ceil().max(1.0) as u32;
        casters.push(StaticShadowCaster {
            source_id: stamp.id.clone(),
            source_kind: format!("terrain:{}", stamp.kind),
            center_cell: settings
                .world_to_cell(stamp.transform.translation.x, stamp.transform.translation.z),
            footprint_tiles: [
                stamp.width_tiles.min(settings.grid_width),
                stamp.height_tiles.min(settings.grid_height),
            ],
            height_units,
            strength: strength_from_height(56, height_units),
        });
    }
    for object in document.objects.values() {
        let casts = object.kind == "Doodad"
            || object.kind == "Building"
            || object
                .properties
                .get("static_shadow")
                .is_some_and(|value| value == "true");
        if !casts {
            continue;
        }
        let footprint = object_footprint(&object.kind, &object.properties)?;
        let height_units =
            parse_optional_u32(&object.properties, "shadow_height_tiles")?.unwrap_or(3);
        casters.push(StaticShadowCaster {
            source_id: object.id.clone(),
            source_kind: format!("object:{}", object.kind),
            center_cell: settings.world_to_cell(
                object.transform.translation.x,
                object.transform.translation.z,
            ),
            footprint_tiles: [
                footprint[0].min(settings.grid_width),
                footprint[1].min(settings.grid_height),
            ],
            height_units,
            strength: strength_from_height(80, height_units),
        });
    }
    Ok(casters)
}

fn paint_caster(
    cells: &mut [u8],
    settings: StaticShadowBakeSettings,
    caster: &StaticShadowCaster,
) -> Result<(), EngineError> {
    let center_x = i32::try_from(caster.center_cell[0]).unwrap_or(i32::MAX);
    let center_z = i32::try_from(caster.center_cell[1]).unwrap_or(i32::MAX);
    for step in 0..=caster.height_units {
        let strength = caster.strength.saturating_sub((step * 18).min(255) as u8);
        if strength == 0 {
            continue;
        }
        let step_i32 = i32::try_from(step).unwrap_or(0);
        paint_rect(
            cells,
            settings,
            center_x + settings.shadow_cast_direction_x * step_i32,
            center_z + settings.shadow_cast_direction_z * step_i32,
            caster.footprint_tiles,
            strength,
        )?;
    }
    Ok(())
}

fn paint_rect(
    cells: &mut [u8],
    settings: StaticShadowBakeSettings,
    center_x: i32,
    center_z: i32,
    footprint: [u32; 2],
    strength: u8,
) -> Result<(), EngineError> {
    let half_x = i32::try_from(footprint[0] / 2).unwrap_or(0);
    let half_z = i32::try_from(footprint[1] / 2).unwrap_or(0);
    for z in (center_z - half_z)..=(center_z + half_z) {
        for x in (center_x - half_x)..=(center_x + half_x) {
            if x < 0 || z < 0 || x >= settings.grid_width as i32 || z >= settings.grid_height as i32
            {
                continue;
            }
            let index = usize::try_from(z as u32 * settings.grid_width + x as u32)
                .map_err(|_| invalid_map_build_error("shadow.index", "cell index overflowed"))?;
            cells[index] = cells[index].max(strength);
        }
    }
    Ok(())
}

fn object_footprint(
    kind: &str,
    properties: &BTreeMap<String, String>,
) -> Result<[u32; 2], EngineError> {
    if let Some(value) = properties.get("shadow_footprint_tiles") {
        let Some((width, height)) = value.split_once('x') else {
            return invalid_map_build("shadow.footprint", "expected WIDTHxHEIGHT");
        };
        let width = width
            .parse::<u32>()
            .map_err(|_| invalid_map_build_error("shadow.footprint", "width is not u32"))?;
        let height = height
            .parse::<u32>()
            .map_err(|_| invalid_map_build_error("shadow.footprint", "height is not u32"))?;
        if width == 0 || height == 0 {
            return invalid_map_build("shadow.footprint", "footprint dimensions must be positive");
        }
        return Ok([width, height]);
    }
    Ok(match kind {
        "Building" => [4, 4],
        "Doodad" => [2, 2],
        _ => [1, 1],
    })
}

fn parse_optional_u32(
    properties: &BTreeMap<String, String>,
    key: &str,
) -> Result<Option<u32>, EngineError> {
    properties
        .get(key)
        .map(|value| {
            value.parse::<u32>().map_err(|_| {
                invalid_map_build_error("shadow.object_property", format!("{key} must be u32"))
            })
        })
        .transpose()
}

fn strength_from_height(base: u32, height_units: u32) -> u8 {
    base.saturating_add(height_units.saturating_mul(18))
        .min(240) as u8
}
