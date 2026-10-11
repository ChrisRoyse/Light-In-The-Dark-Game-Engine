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
use sha2::{Digest, Sha256};

use crate::asset_scene::EditorFoliageReport;
use crate::asset_scene_foliage_brush::FoliageBrushApplyStats;

const RECORD_KIND: &str = "litd.editor.foliage_tileset_config";
const SCHEMA_VERSION: u32 = 1;
const DEFAULT_CONFIG_RELATIVE_PATH: &str = "tmp/litd-editor-foliage-config-aster";
const CONFIG_ENV: &str = "LITD_EDITOR_FOLIAGE_CONFIG";
const CONFIG_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA5";
const CONFIG_VAULT_PURPOSE: &[u8] = b"litd-editor-foliage-config-issue1359";
const CONFIG_COLLECTION: &str = "litd_editor_foliage_config";
const CONFIG_ROW_KEY: &[u8] = b"foliage/config/default";
const CONFIG_COLUMN: &[u8] = b"config_json";
const DEFAULT_CONFIG_JSON: &str = include_str!("../assets/foliage/stylized_nature_ed11_v1.json");

#[derive(Debug, Clone)]
pub struct EditorFoliageConfig {
    pub schema_version: u32,
    pub config_id: String,
    pub tileset_id: String,
    pub source_issue: u32,
    pub density_scale: f32,
    pub source_relative_path: String,
    pub source_sha256: String,
    pub brush: FoliageBrushApplyStats,
    pub prototypes: Vec<FoliagePrototypeDefinition>,
    pub patches: Vec<FoliagePatchDefinition>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FoliagePrototypeDefinition {
    pub prototype_id: String,
    pub render_mode: String,
    pub mesh_asset: String,
    pub material_id: String,
    pub use_gpu_instancing: bool,
    pub density_per_1000_units: f32,
    pub minimum_spacing: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub min_width: f32,
    pub max_width: f32,
    pub align_to_ground: bool,
    pub wind_strength: f32,
    pub cull_start: f32,
    pub cull_end: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FoliagePatchDefinition {
    pub prototype_id: String,
    pub center_xz: [f32; 2],
    pub radius: f32,
    pub target_clumps: u32,
    pub yaw_degrees: f32,
    pub height: f32,
    pub color: [f32; 4],
}

#[derive(Debug, Deserialize, Serialize)]
struct RawFoliageConfig {
    schema_version: u32,
    record_kind: String,
    config_id: String,
    tileset_id: String,
    source_issue: u32,
    density_scale: f32,
    prototypes: Vec<FoliagePrototypeDefinition>,
    patches: Vec<RawFoliagePatchDefinition>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RawFoliagePatchDefinition {
    prototype_id: String,
    center_xz: [f32; 2],
    radius: f32,
    target_clumps: u32,
    yaw_degrees: f32,
    height: f32,
    color: [f32; 4],
}

impl EditorFoliageConfig {
    pub fn target_clump_count(&self) -> u32 {
        self.patches.iter().map(|patch| patch.target_clumps).sum()
    }

    pub fn gpu_instancing_required(&self) -> bool {
        self.prototypes
            .iter()
            .all(|prototype| prototype.use_gpu_instancing)
    }

    pub fn render_mode_summary(&self) -> String {
        self.prototypes
            .iter()
            .map(|prototype| format!("{}:{}", prototype.prototype_id, prototype.render_mode))
            .collect::<Vec<_>>()
            .join("|")
    }

    pub fn cull_range_summary(&self) -> String {
        let start = self
            .prototypes
            .iter()
            .map(|prototype| prototype.cull_start)
            .fold(f32::INFINITY, f32::min);
        let end = self
            .prototypes
            .iter()
            .map(|prototype| prototype.cull_end)
            .fold(f32::NEG_INFINITY, f32::max);
        format!("{start:.1}..{end:.1}")
    }

    pub fn to_report(
        &self,
        placed_clump_count: u32,
        foliage_index_count: u32,
    ) -> EditorFoliageReport {
        EditorFoliageReport {
            config_id: self.config_id.clone(),
            tileset_id: self.tileset_id.clone(),
            source_relative_path: self.source_relative_path.clone(),
            source_sha256: self.source_sha256.clone(),
            brush_state_present: self.brush.state_present,
            brush_state_source_path: self.brush.state_source_path.clone(),
            brush_state_sha256: self.brush.state_sha256.clone(),
            brush_stamp_count: self.brush.stamp_count,
            brush_add_stamp_count: self.brush.add_stamp_count,
            brush_erase_stamp_count: self.brush.erase_stamp_count,
            brush_added_patch_count: self.brush.added_patch_count,
            brush_erased_patch_count: self.brush.erased_patch_count,
            schema_version: self.schema_version,
            source_issue: self.source_issue,
            prototype_count: self.prototypes.len(),
            patch_count: self.patches.len(),
            target_clump_count: self.target_clump_count(),
            placed_clump_count,
            foliage_index_count,
            density_scale: self.density_scale,
            gpu_instancing_required: self.gpu_instancing_required(),
            render_mode_summary: self.render_mode_summary(),
            cull_range_summary: self.cull_range_summary(),
        }
    }
}

pub fn load_default_foliage_config() -> Result<EditorFoliageConfig, EngineError> {
    let path = match env::var(CONFIG_ENV) {
        Ok(value) if value.trim().is_empty() => {
            return foliage_error("path.env", "LITD_EDITOR_FOLIAGE_CONFIG is empty");
        }
        Ok(value) => absolute_or_workspace_relative(PathBuf::from(value)),
        Err(env::VarError::NotPresent) => {
            let path = default_foliage_config_path();
            write_foliage_config_bytes(&path, DEFAULT_CONFIG_JSON.as_bytes())?;
            path
        }
        Err(error) => {
            return foliage_error("path.env", format!("LITD_EDITOR_FOLIAGE_CONFIG: {error}"));
        }
    };
    load_foliage_config(path)
}

pub fn default_foliage_config_path() -> PathBuf {
    workspace_root().join(DEFAULT_CONFIG_RELATIVE_PATH)
}

pub fn load_foliage_config(path: impl AsRef<Path>) -> Result<EditorFoliageConfig, EngineError> {
    let path = path.as_ref();
    let readback = read_calyx_json_bytes(&config_spec(path)).map_err(|error| {
        foliage_error_value(
            "read",
            format!(
                "read Calyx Aster foliage config {}: {error}",
                path.display()
            ),
        )
    })?;
    let readback = readback.ok_or_else(|| {
        foliage_error_value(
            "read",
            format!(
                "missing Calyx Aster foliage config row at {}",
                path.display()
            ),
        )
    })?;
    let raw = parse_raw_config(&readback.bytes, path)?;
    build_config(
        raw,
        format!("calyx-aster:{}", relative_to_workspace(path)),
        readback.sha256,
    )
}

fn write_foliage_config_bytes(
    path: &Path,
    bytes: &[u8],
) -> Result<CalyxJsonStoreReadback, EngineError> {
    let raw = parse_raw_config(bytes, path)?;
    let source_sha256 = format!("{:x}", Sha256::digest(bytes));
    build_config(
        raw,
        format!("calyx-aster:{}", relative_to_workspace(path)),
        source_sha256.clone(),
    )?;
    let readback = write_calyx_json_bytes(&config_spec(path), bytes).map_err(|error| {
        foliage_error_value(
            "write",
            format!(
                "write Calyx Aster foliage config {}: {error}",
                path.display()
            ),
        )
    })?;
    if readback.sha256 != source_sha256 {
        return foliage_error(
            "write.hash",
            format!(
                "Calyx Aster readback hash {} did not match parsed source hash {} for {}",
                readback.sha256,
                source_sha256,
                path.display()
            ),
        );
    }
    Ok(readback)
}

#[cfg(test)]
pub(crate) fn seed_default_foliage_config(
    path: &Path,
) -> Result<CalyxJsonStoreReadback, EngineError> {
    write_foliage_config_bytes(path, DEFAULT_CONFIG_JSON.as_bytes())
}

/// Golden SHA-256 of the committed default foliage-config asset
/// (`assets/foliage/stylized_nature_ed11_v1.json`).
///
/// #1794 review: this is the CONTENT-DRIFT guard. `default_foliage_config_source_sha256()` derives
/// the expected hash from the very bytes the loader ingests, so on its own it can only prove the
/// Aster round-trip preserved those bytes -- it has zero power to detect that the asset FILE was
/// changed (a change moves the derived value AND the loader readback together). This frozen literal
/// restores that power: tests assert the derived value EQUALS this literal, so an accidental asset
/// edit fails loud with a clear expected-vs-actual diff, while a legitimate edit updates exactly
/// ONE literal here. To refresh after an intentional edit:
/// `sha256sum crates/litd-editor/assets/foliage/stylized_nature_ed11_v1.json`.
#[cfg(test)]
pub(crate) const DEFAULT_FOLIAGE_CONFIG_GOLDEN_SHA256: &str =
    "3598cf406d6069f907fd7091eb26b8ea918b6a5f5a1009df9a05e04a12b9274d";

/// SHA-256 of the committed default foliage-config asset bytes (`DEFAULT_CONFIG_JSON`, the
/// `include_str!` of `assets/foliage/stylized_nature_ed11_v1.json`), computed from the real file
/// at test time.
///
/// #1761/#1765: the loader's `source_sha256` is the Aster readback hash of exactly these bytes
/// (`write_foliage_config_bytes` already fails loud if the round-trip hash differs). Deriving the
/// expected value from the real asset proves the loader ingested the exact committed bytes intact
/// and gives a good expected-vs-actual error message. It does NOT by itself detect asset content
/// drift (see `DEFAULT_FOLIAGE_CONFIG_GOLDEN_SHA256`); the callers pin the derived value against the
/// golden literal for that. The semantic assertions (prototype/patch/clump counts, render modes,
/// cull range) still pin the config's meaning, so a content-changing edit is not silently masked.
#[cfg(test)]
pub(crate) fn default_foliage_config_source_sha256() -> String {
    format!("{:x}", Sha256::digest(DEFAULT_CONFIG_JSON.as_bytes()))
}

fn parse_raw_config(bytes: &[u8], path: &Path) -> Result<RawFoliageConfig, EngineError> {
    if bytes.iter().all(|byte| byte.is_ascii_whitespace()) {
        return foliage_error(
            "records",
            format!("foliage config is empty: {}", path.display()),
        );
    }
    serde_json::from_slice::<RawFoliageConfig>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!(
                "parse Calyx Aster foliage config {}: {error}",
                path.display()
            ),
        }
    })
}

fn config_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: CONFIG_VAULT_ID,
        purpose: CONFIG_VAULT_PURPOSE,
        collection: CONFIG_COLLECTION,
        row_key: CONFIG_ROW_KEY,
        column: CONFIG_COLUMN,
    }
}

fn build_config(
    raw: RawFoliageConfig,
    source_relative_path: String,
    source_sha256: String,
) -> Result<EditorFoliageConfig, EngineError> {
    if raw.schema_version != SCHEMA_VERSION {
        return foliage_error(
            "schema_version",
            format!(
                "{} has schema_version {}, expected {SCHEMA_VERSION}",
                source_relative_path, raw.schema_version
            ),
        );
    }
    if raw.record_kind != RECORD_KIND {
        return foliage_error(
            "record_kind",
            format!(
                "{} has record_kind '{}', expected '{RECORD_KIND}'",
                source_relative_path, raw.record_kind
            ),
        );
    }
    validate_id("config_id", &raw.config_id)?;
    validate_id("tileset_id", &raw.tileset_id)?;
    if raw.source_issue != 1271 {
        return foliage_error("source_issue", "foliage config must bind to #1271");
    }
    validate_positive("density_scale", raw.density_scale)?;
    if raw.prototypes.is_empty() || raw.patches.is_empty() {
        return foliage_error("records", "foliage config requires prototypes and patches");
    }
    let prototype_ids = validate_prototypes(&raw.prototypes)?;
    let patches = validate_patches(raw.patches, &prototype_ids, &raw.prototypes)?;
    Ok(EditorFoliageConfig {
        schema_version: raw.schema_version,
        config_id: raw.config_id,
        tileset_id: raw.tileset_id,
        source_issue: raw.source_issue,
        density_scale: raw.density_scale,
        source_relative_path,
        source_sha256,
        brush: FoliageBrushApplyStats::default(),
        prototypes: raw.prototypes,
        patches,
    })
}

fn validate_prototypes(
    prototypes: &[FoliagePrototypeDefinition],
) -> Result<BTreeSet<String>, EngineError> {
    let mut ids = BTreeSet::new();
    for prototype in prototypes {
        validate_id("prototype.prototype_id", &prototype.prototype_id)?;
        if !ids.insert(prototype.prototype_id.clone()) {
            return foliage_error(
                "prototype.prototype_id",
                format!("duplicate foliage prototype '{}'", prototype.prototype_id),
            );
        }
        validate_id("prototype.render_mode", &prototype.render_mode)?;
        if prototype.render_mode != "instanced_billboard_cross"
            && prototype.render_mode != "instanced_mesh"
        {
            return foliage_error(
                "prototype.render_mode",
                format!(
                    "{} uses unsupported render_mode '{}'",
                    prototype.prototype_id, prototype.render_mode
                ),
            );
        }
        validate_id("prototype.mesh_asset", &prototype.mesh_asset)?;
        validate_id("prototype.material_id", &prototype.material_id)?;
        validate_positive(
            "prototype.density_per_1000_units",
            prototype.density_per_1000_units,
        )?;
        validate_positive("prototype.minimum_spacing", prototype.minimum_spacing)?;
        validate_ordered_pair(
            "prototype.height",
            prototype.min_height,
            prototype.max_height,
        )?;
        validate_ordered_pair("prototype.width", prototype.min_width, prototype.max_width)?;
        validate_finite("prototype.wind_strength", prototype.wind_strength)?;
        validate_ordered_pair("prototype.cull", prototype.cull_start, prototype.cull_end)?;
        if !prototype.use_gpu_instancing {
            return foliage_error(
                "prototype.use_gpu_instancing",
                format!("{} must require GPU instancing", prototype.prototype_id),
            );
        }
        if !prototype.align_to_ground {
            return foliage_error(
                "prototype.align_to_ground",
                format!("{} must align to terrain normals", prototype.prototype_id),
            );
        }
    }
    Ok(ids)
}

fn validate_patches(
    patches: Vec<RawFoliagePatchDefinition>,
    prototype_ids: &BTreeSet<String>,
    prototypes: &[FoliagePrototypeDefinition],
) -> Result<Vec<FoliagePatchDefinition>, EngineError> {
    let mut validated = Vec::with_capacity(patches.len());
    for patch in patches {
        if !prototype_ids.contains(&patch.prototype_id) {
            return foliage_error(
                "patch.prototype_id",
                format!(
                    "patch references unknown prototype '{}'",
                    patch.prototype_id
                ),
            );
        }
        validate_xz("patch.center_xz", patch.center_xz)?;
        validate_positive("patch.radius", patch.radius)?;
        if patch.target_clumps == 0 || patch.target_clumps > 32 {
            return foliage_error(
                "patch.target_clumps",
                "patch target_clumps must be in 1..=32",
            );
        }
        validate_finite("patch.yaw_degrees", patch.yaw_degrees)?;
        let prototype = prototypes
            .iter()
            .find(|prototype| prototype.prototype_id == patch.prototype_id)
            .expect("prototype existence was checked above");
        if patch.height < prototype.min_height || patch.height > prototype.max_height {
            return foliage_error(
                "patch.height",
                format!(
                    "{} height {:.2} is outside prototype range {:.2}..{:.2}",
                    patch.prototype_id, patch.height, prototype.min_height, prototype.max_height
                ),
            );
        }
        if patch
            .color
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0 || *value > 1.0)
        {
            return foliage_error("patch.color", "patch color must be finite 0..1 RGBA");
        }
        validated.push(FoliagePatchDefinition {
            prototype_id: patch.prototype_id,
            center_xz: patch.center_xz,
            radius: patch.radius,
            target_clumps: patch.target_clumps,
            yaw_degrees: patch.yaw_degrees,
            height: patch.height,
            color: patch.color,
        });
    }
    Ok(validated)
}

#[path = "asset_scene_foliage_config_support.rs"]
mod foliage_config_support;
use foliage_config_support::*;

#[cfg(test)]
#[path = "asset_scene_foliage_config_tests.rs"]
mod tests;
