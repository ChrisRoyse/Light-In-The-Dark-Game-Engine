use std::{
    collections::BTreeSet,
    env,
    path::{Path, PathBuf},
};

use litd_engine::{
    EngineError,
    calyx_json_store::{
        CalyxJsonStoreReadback, CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes,
    },
};
use serde::{Deserialize, Serialize};

use crate::asset_scene_foliage_config::{
    EditorFoliageConfig, FoliagePatchDefinition, FoliagePrototypeDefinition,
};

const SCHEMA_VERSION: u32 = 1;
const RECORD_KIND: &str = "litd.editor.foliage_brush_state";
const SOURCE_ISSUE: u32 = 1271;
const BRUSH_ENV: &str = "LITD_EDITOR_FOLIAGE_BRUSH_STATE";
const DEFAULT_PROTOTYPE_ID: &str = "ed11_grass_clump";
const BRUSH_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA6";
const BRUSH_VAULT_PURPOSE: &[u8] = b"litd-editor-foliage-brush-issue1359";
const BRUSH_COLLECTION: &str = "litd_editor_foliage_brush";
const BRUSH_ROW_KEY: &[u8] = b"foliage/brush/latest";
const BRUSH_COLUMN: &[u8] = b"brush_json";

#[derive(Debug, Clone, PartialEq)]
pub struct FoliageBrushState {
    pub source_path: PathBuf,
    pub source_relative_path: String,
    pub source_sha256: String,
    pub present_on_disk: bool,
    pub stamps: Vec<FoliageBrushStamp>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FoliageBrushApplyStats {
    pub state_present: bool,
    pub state_source_path: String,
    pub state_sha256: String,
    pub stamp_count: usize,
    pub add_stamp_count: usize,
    pub erase_stamp_count: usize,
    pub added_patch_count: usize,
    pub erased_patch_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoliageBrushOperation {
    Add,
    Erase,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoliageBrushStamp {
    pub stamp_id: String,
    pub operation: FoliageBrushOperation,
    pub prototype_id: String,
    pub center_xz: [f32; 2],
    pub radius: f32,
    pub density_percent: f32,
    pub target_clumps: u32,
    pub yaw_degrees: f32,
    pub height: f32,
    pub color: [f32; 4],
}

#[derive(Debug, Serialize, Deserialize)]
struct RawFoliageBrushState {
    schema_version: u32,
    record_kind: String,
    source_issue: u32,
    stamps: Vec<FoliageBrushStamp>,
}

#[derive(Debug, Clone)]
pub struct FoliageBrushStore {
    path: PathBuf,
}

impl FoliageBrushStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn from_env() -> Result<Self, EngineError> {
        let path = match env::var(BRUSH_ENV) {
            Ok(value) if value.trim().is_empty() => {
                return brush_error("path.env", "LITD_EDITOR_FOLIAGE_BRUSH_STATE is empty");
            }
            Ok(value) => absolute_or_workspace_relative(PathBuf::from(value)),
            Err(env::VarError::NotPresent) => default_brush_state_path(),
            Err(error) => {
                return brush_error("path.env", format!("{BRUSH_ENV}: {error}"));
            }
        };
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn add_density_brush(
        &self,
        config: &EditorFoliageConfig,
        center_xz: [f32; 2],
        radius: f32,
        density_percent: f32,
    ) -> Result<FoliageBrushState, EngineError> {
        let prototype = prototype_for(config, DEFAULT_PROTOTYPE_ID)?;
        validate_brush_center_radius(center_xz, radius)?;
        if !density_percent.is_finite() || density_percent <= 0.0 || density_percent > 100.0 {
            return brush_error(
                "stamp.density_percent",
                "add brush density must be finite in 0..100",
            );
        }
        let mut state = load_foliage_brush_state(&self.path)?;
        let target_clumps =
            target_clumps_for_brush(radius, density_percent, config.density_scale, prototype)?;
        let ordinal = state.stamps.len() + 1;
        state.stamps.push(FoliageBrushStamp {
            stamp_id: format!("foliage-brush-{ordinal:04}"),
            operation: FoliageBrushOperation::Add,
            prototype_id: DEFAULT_PROTOTYPE_ID.to_string(),
            center_xz,
            radius,
            density_percent,
            target_clumps,
            yaw_degrees: (center_xz[0] * 0.37 + center_xz[1] * 0.19).rem_euclid(360.0),
            height: (prototype.min_height + prototype.max_height) * 0.5,
            color: [0.12, 0.17, 0.13, 0.0],
        });
        save_foliage_brush_state(&self.path, &state.stamps)?;
        load_foliage_brush_state(&self.path)
    }

    pub fn erase_density_brush(
        &self,
        center_xz: [f32; 2],
        radius: f32,
    ) -> Result<FoliageBrushState, EngineError> {
        validate_brush_center_radius(center_xz, radius)?;
        let mut state = load_foliage_brush_state(&self.path)?;
        let ordinal = state.stamps.len() + 1;
        state.stamps.push(FoliageBrushStamp {
            stamp_id: format!("foliage-brush-{ordinal:04}"),
            operation: FoliageBrushOperation::Erase,
            prototype_id: "erase".to_string(),
            center_xz,
            radius,
            density_percent: 0.0,
            target_clumps: 0,
            yaw_degrees: 0.0,
            height: 0.0,
            color: [0.0, 0.0, 0.0, 0.0],
        });
        save_foliage_brush_state(&self.path, &state.stamps)?;
        load_foliage_brush_state(&self.path)
    }
}

impl Default for FoliageBrushApplyStats {
    fn default() -> Self {
        Self {
            state_present: false,
            state_source_path: "none".to_string(),
            state_sha256: "none".to_string(),
            stamp_count: 0,
            add_stamp_count: 0,
            erase_stamp_count: 0,
            added_patch_count: 0,
            erased_patch_count: 0,
        }
    }
}

pub fn load_foliage_brush_state(path: impl AsRef<Path>) -> Result<FoliageBrushState, EngineError> {
    let path = path.as_ref();
    let source_relative_path = relative_to_workspace(path);
    if !path.exists() {
        return Ok(FoliageBrushState {
            source_path: path.to_path_buf(),
            source_relative_path,
            source_sha256: "missing".to_string(),
            present_on_disk: false,
            stamps: Vec::new(),
        });
    }
    let readback = read_calyx_json_bytes(&brush_spec(path)).map_err(|error| {
        persistence_error(
            "read",
            format!(
                "read Calyx Aster foliage brush state {}: {error}",
                path.display()
            ),
        )
    })?;
    let readback = readback.ok_or_else(|| {
        persistence_error(
            "read",
            format!(
                "missing Calyx Aster foliage brush state row at {}",
                path.display()
            ),
        )
    })?;
    let raw = parse_raw_state_bytes(&readback.bytes, path)?;
    validate_raw_state(&raw, path)?;
    Ok(FoliageBrushState {
        source_path: path.to_path_buf(),
        source_relative_path: format!("calyx-aster:{source_relative_path}"),
        source_sha256: readback.sha256,
        present_on_disk: true,
        stamps: raw.stamps,
    })
}

pub fn apply_foliage_brush_state(
    mut config: EditorFoliageConfig,
    brush_state: &FoliageBrushState,
) -> Result<EditorFoliageConfig, EngineError> {
    let mut patches = std::mem::take(&mut config.patches);
    let mut stats = FoliageBrushApplyStats {
        state_present: brush_state.present_on_disk,
        state_source_path: brush_state.source_relative_path.clone(),
        state_sha256: brush_state.source_sha256.clone(),
        stamp_count: brush_state.stamps.len(),
        add_stamp_count: brush_state
            .stamps
            .iter()
            .filter(|stamp| stamp.operation == FoliageBrushOperation::Add)
            .count(),
        erase_stamp_count: brush_state
            .stamps
            .iter()
            .filter(|stamp| stamp.operation == FoliageBrushOperation::Erase)
            .count(),
        added_patch_count: 0,
        erased_patch_count: 0,
    };
    for stamp in &brush_state.stamps {
        match stamp.operation {
            FoliageBrushOperation::Add => {
                prototype_for(&config, &stamp.prototype_id)?;
                patches.push(FoliagePatchDefinition {
                    prototype_id: stamp.prototype_id.clone(),
                    center_xz: stamp.center_xz,
                    radius: stamp.radius,
                    target_clumps: stamp.target_clumps,
                    yaw_degrees: stamp.yaw_degrees,
                    height: stamp.height,
                    color: stamp.color,
                });
                stats.added_patch_count += 1;
            }
            FoliageBrushOperation::Erase => {
                let before = patches.len();
                patches.retain(|patch| {
                    !point_in_brush(patch.center_xz, stamp.center_xz, stamp.radius)
                });
                stats.erased_patch_count += before - patches.len();
            }
        }
    }
    if patches.is_empty() {
        return brush_error(
            "apply",
            "foliage brush state erased every foliage patch; at least one patch must remain",
        );
    }
    config.patches = patches;
    config.brush = stats;
    Ok(config)
}

fn save_foliage_brush_state(path: &Path, stamps: &[FoliageBrushStamp]) -> Result<(), EngineError> {
    let raw = RawFoliageBrushState {
        schema_version: SCHEMA_VERSION,
        record_kind: RECORD_KIND.to_string(),
        source_issue: SOURCE_ISSUE,
        stamps: stamps.to_vec(),
    };
    validate_raw_state(&raw, path)?;
    let json =
        serde_json::to_vec_pretty(&raw).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize {}: {error}", path.display()),
        })?;
    write_foliage_brush_state_bytes(path, &json)?;
    Ok(())
}

fn write_foliage_brush_state_bytes(
    path: &Path,
    bytes: &[u8],
) -> Result<CalyxJsonStoreReadback, EngineError> {
    let raw = parse_raw_state_bytes(bytes, path)?;
    validate_raw_state(&raw, path)?;
    write_calyx_json_bytes(&brush_spec(path), bytes).map_err(|error| {
        persistence_error(
            "write",
            format!(
                "write Calyx Aster foliage brush state {}: {error}",
                path.display()
            ),
        )
    })
}

fn parse_raw_state_bytes(bytes: &[u8], path: &Path) -> Result<RawFoliageBrushState, EngineError> {
    if bytes.iter().all(|byte| byte.is_ascii_whitespace()) {
        return brush_error(
            "records",
            format!("foliage brush state is empty: {}", path.display()),
        );
    }
    serde_json::from_slice::<RawFoliageBrushState>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!(
                "parse Calyx Aster foliage brush state {}: {error}",
                path.display()
            ),
        }
    })
}

fn validate_raw_state(raw: &RawFoliageBrushState, path: &Path) -> Result<(), EngineError> {
    if raw.schema_version != SCHEMA_VERSION {
        return brush_error(
            "schema_version",
            format!(
                "{} has schema_version {}, expected {SCHEMA_VERSION}",
                path.display(),
                raw.schema_version
            ),
        );
    }
    if raw.record_kind != RECORD_KIND {
        return brush_error(
            "record_kind",
            format!(
                "{} has record_kind '{}', expected '{RECORD_KIND}'",
                path.display(),
                raw.record_kind
            ),
        );
    }
    if raw.source_issue != SOURCE_ISSUE {
        return brush_error("source_issue", "foliage brush state must bind to #1271");
    }
    let mut ids = BTreeSet::new();
    for stamp in &raw.stamps {
        validate_stamp(stamp)?;
        if !ids.insert(stamp.stamp_id.clone()) {
            return brush_error(
                "stamp.stamp_id",
                format!("duplicate foliage brush stamp '{}'", stamp.stamp_id),
            );
        }
    }
    Ok(())
}

fn validate_stamp(stamp: &FoliageBrushStamp) -> Result<(), EngineError> {
    if stamp.stamp_id.trim().is_empty() || stamp.prototype_id.trim().is_empty() {
        return brush_error("stamp.id", "stamp_id and prototype_id are required");
    }
    validate_brush_center_radius(stamp.center_xz, stamp.radius)?;
    if !stamp.yaw_degrees.is_finite()
        || !stamp.height.is_finite()
        || stamp.color.iter().any(|value| !value.is_finite())
    {
        return brush_error(
            "stamp.values",
            "yaw, height, and color channels must be finite",
        );
    }
    match stamp.operation {
        FoliageBrushOperation::Add => {
            if !stamp.density_percent.is_finite()
                || stamp.density_percent <= 0.0
                || stamp.density_percent > 100.0
                || stamp.target_clumps == 0
                || stamp.target_clumps > 32
                || stamp.height <= 0.0
            {
                return brush_error(
                    "stamp.add",
                    "add stamps require density 0..100, target_clumps 1..=32, and positive height",
                );
            }
        }
        FoliageBrushOperation::Erase => {
            if stamp.density_percent != 0.0 || stamp.target_clumps != 0 {
                return brush_error(
                    "stamp.erase",
                    "erase stamps must carry density 0 and target_clumps 0",
                );
            }
        }
    }
    Ok(())
}

fn validate_brush_center_radius(center_xz: [f32; 2], radius: f32) -> Result<(), EngineError> {
    if center_xz.iter().any(|value| !value.is_finite())
        || !radius.is_finite()
        || !(2.0..=128.0).contains(&radius)
    {
        return brush_error(
            "stamp.brush",
            "brush center must be finite and radius must be in 2..=128",
        );
    }
    Ok(())
}

fn target_clumps_for_brush(
    radius: f32,
    density_percent: f32,
    density_scale: f32,
    prototype: &FoliagePrototypeDefinition,
) -> Result<u32, EngineError> {
    let area = std::f32::consts::PI * radius * radius;
    let clumps =
        area * prototype.density_per_1000_units * density_scale * density_percent / 100_000.0;
    if !clumps.is_finite() || clumps <= 0.0 {
        return brush_error("stamp.target_clumps", "brush produced no foliage clumps");
    }
    Ok((clumps.round() as u32).clamp(1, 32))
}

fn prototype_for<'a>(
    config: &'a EditorFoliageConfig,
    prototype_id: &str,
) -> Result<&'a FoliagePrototypeDefinition, EngineError> {
    config
        .prototypes
        .iter()
        .find(|prototype| prototype.prototype_id == prototype_id)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.asset_scene.foliage_brush.prototype_id".to_string(),
            reason: format!("unknown foliage prototype '{prototype_id}'"),
        })
}

fn point_in_brush(xz: [f32; 2], center_xz: [f32; 2], radius: f32) -> bool {
    let dx = xz[0] - center_xz[0];
    let dz = xz[1] - center_xz[1];
    dx * dx + dz * dz <= radius * radius
}

#[path = "asset_scene_foliage_brush_support.rs"]
mod foliage_brush_support;
use foliage_brush_support::*;

#[cfg(test)]
#[path = "asset_scene_foliage_brush_tests.rs"]
mod tests;
