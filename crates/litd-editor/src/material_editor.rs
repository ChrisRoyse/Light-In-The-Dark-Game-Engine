//! No-code PBR material authoring capability (#862).
//!
//! This is the single shared capability layer (#915) behind BOTH the egui
//! material-inspector GUI ([`material_editor_panel`]) and the material MCP tools
//! ([`material_editor_mcp`]). It builds a validated metallic-roughness
//! [`PbrMaterial`] from author-facing factor + map fields, persists it through
//! the REAL Calyx content path (`put_content` + lowering -- pending_measurement
//! safe with no ORT, #1791), and reports the fingerprinted lowered artifact the
//! renderer consumes plus the preview uniform fingerprint.

use std::path::Path;

use litd_engine::{
    EngineError,
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentMeasurementConfig,
        EngineContentRecord, EngineContentSource, resolve_engine_content_panel_vault,
    },
    calyx_content_measurement_absent::addon_absent_content_measurement,
    calyx_lowering::EngineLoweringValve,
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    math::{Vec3, Vec4},
    render::material::{PbrMaterial, TextureBinding, TextureSlot},
};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PBR_MATERIAL_CONTENT_SCHEMA: &str = "litd.editor.pbr_material.v1";
pub const PBR_MATERIAL_SURFACE_KIND: &str = "pbr_material";

/// Author-facing texture slots (the subset a no-code editor assigns). Maps to the
/// engine [`TextureSlot`]; IBL/BRDF slots are studio-lighting concerns, not
/// per-material authoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialMapSlot {
    BaseColor,
    MetallicRoughness,
    Normal,
    AmbientOcclusion,
    Emissive,
}

impl MaterialMapSlot {
    fn engine_slot(self) -> TextureSlot {
        match self {
            Self::BaseColor => TextureSlot::BaseColor,
            Self::MetallicRoughness => TextureSlot::MetallicRoughness,
            Self::Normal => TextureSlot::Normal,
            Self::AmbientOcclusion => TextureSlot::AmbientOcclusion,
            Self::Emissive => TextureSlot::Emissive,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialMapAssignment {
    pub slot: MaterialMapSlot,
    pub asset_id: String,
}

/// The inspector form model: exactly the fields a user edits in the no-code
/// panel or an agent sends over MCP. Deterministically lowers to a [`PbrMaterial`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PbrMaterialAuthorRequest {
    pub material_id: String,
    pub display_name: String,
    pub author: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub normal_scale: f32,
    pub occlusion: f32,
    pub emissive: [f32; 3],
    #[serde(default)]
    pub maps: Vec<MaterialMapAssignment>,
}

impl PbrMaterialAuthorRequest {
    /// Build and validate the engine material this request describes.
    pub fn to_material(&self) -> Result<PbrMaterial, EngineError> {
        if self.display_name.trim().is_empty() {
            return Err(material_error(
                "display_name",
                "display name must be non-empty",
            ));
        }
        let mut material = PbrMaterial::new(self.display_name.clone());
        material.base_color_factor = Vec4::from_array(self.base_color);
        material.metallic_factor = self.metallic;
        material.roughness_factor = self.roughness;
        material.normal_scale = self.normal_scale;
        material.occlusion_strength = self.occlusion;
        material.emissive_factor = Vec3::from_array(self.emissive);
        let mut seen = Vec::new();
        for assignment in &self.maps {
            if assignment.asset_id.trim().is_empty() {
                return Err(material_error(
                    "maps.asset_id",
                    "assigned map asset id must be non-empty",
                ));
            }
            let slot = assignment.slot.engine_slot();
            if seen.contains(&slot) {
                return Err(material_error(
                    "maps.slot",
                    "a texture slot may only be assigned one map",
                ));
            }
            seen.push(slot);
            material =
                material.with_texture(TextureBinding::new(slot, assignment.asset_id.clone()));
        }
        material.validate()?;
        Ok(material)
    }
}

/// Persisted readback: the authored record's identity + the two frozen
/// fingerprints (Calyx content lowering artifact + renderer preview uniform).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PbrMaterialAuthorReadback {
    pub material_id: String,
    pub display_name: String,
    pub source_path: String,
    pub vault_path: String,
    pub cx_id: String,
    pub content_input_sha256: String,
    pub lowered_artifact_id: String,
    pub lowered_fingerprint: String,
    pub lowered_field_count: usize,
    pub lowered_package_fingerprint: String,
    pub measurement_state: String,
    pub pending_learned_lens_count: usize,
    pub preview_uniform_fingerprint: String,
    pub map_count: usize,
    pub texture_mask: u32,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub normal_scale: f32,
    pub occlusion: f32,
    pub emissive: [f32; 3],
}

/// Author a PBR material into the Calyx content store at `vault_dir` and return
/// the persisted readback. Persistence goes through `put_content` (the real
/// path); the record is then lowered and verified so the renderer-facing
/// artifact is proven present.
pub fn author_pbr_material(
    vault_dir: impl AsRef<Path>,
    request: &PbrMaterialAuthorRequest,
) -> Result<PbrMaterialAuthorReadback, EngineError> {
    let vault_dir = vault_dir.as_ref();
    validate_material_id(&request.material_id)?;
    if request.author.trim().is_empty() {
        return Err(material_error("author", "author must be non-empty"));
    }
    let material = request.to_material()?;
    let payload = material_payload(request, &material);
    let source_hash = sha256_hex(canonical_input(&payload).as_bytes());
    let record = EngineContentRecord {
        kind: EngineContentKind::Asset,
        id: request.material_id.clone(),
        display_name: request.display_name.clone(),
        payload,
        source: EngineContentSource {
            path: format!("editor://pbr-material/{}", request.material_id),
            author: request.author.clone(),
            source_hash,
        },
        anchors: Vec::new(),
    };

    let store = open_store(vault_dir)?;
    let content = store
        .put_content(&record)
        .map_err(|error| material_error("put_content", error.to_string()))?;
    let package = EngineLoweringValve::lower_content_for_sim(&content)
        .map_err(|error| material_error("lower", error.to_string()))?;
    let lowered = EngineLoweringValve::verify_lowered_package(&package)
        .map_err(|error| material_error("verify_lowered", error.to_string()))?;

    Ok(PbrMaterialAuthorReadback {
        material_id: content.id.clone(),
        display_name: content.display_name.clone(),
        source_path: content.source_path.clone(),
        vault_path: vault_dir.display().to_string(),
        cx_id: content.cx_id.clone(),
        content_input_sha256: content.input_sha256.clone(),
        lowered_artifact_id: content.lowering.artifact_id.clone(),
        lowered_fingerprint: content.lowering.fingerprint.clone(),
        lowered_field_count: lowered.field_count,
        lowered_package_fingerprint: lowered.package_fingerprint.clone(),
        measurement_state: content.measurement.measurement_state.clone(),
        pending_learned_lens_count: content.measurement.pending_learned_lenses.len(),
        preview_uniform_fingerprint: material.preview_fingerprint(),
        map_count: request.maps.len(),
        texture_mask: material.texture_mask(),
        base_color: material.base_color_factor.to_array(),
        metallic: material.metallic_factor,
        roughness: material.roughness_factor,
        normal_scale: material.normal_scale,
        occlusion: material.occlusion_strength,
        emissive: material.emissive_factor.to_array(),
    })
}

/// Read a persisted PBR material back from the Calyx content store and rebuild
/// the authored material for round-trip verification.
pub fn read_pbr_material(
    vault_dir: impl AsRef<Path>,
    material_id: &str,
) -> Result<PbrMaterialReadback, EngineError> {
    validate_material_id(material_id)?;
    let store = open_store(vault_dir.as_ref())?;
    let content = store
        .get_content_for_editor(EngineContentKind::Asset, material_id)
        .map_err(|error| material_error("get_content", error.to_string()))?;
    let material = parse_material_payload(&content.payload)?;
    Ok(PbrMaterialReadback {
        material_id: content.id.clone(),
        display_name: content.display_name.clone(),
        cx_id: content.cx_id.clone(),
        content_input_sha256: content.input_sha256.clone(),
        lowered_fingerprint: content.lowering.fingerprint.clone(),
        measurement_state: content.measurement.measurement_state.clone(),
        preview_uniform_fingerprint: material.preview_fingerprint(),
        texture_mask: material.texture_mask(),
        base_color: material.base_color_factor.to_array(),
        metallic: material.metallic_factor,
        roughness: material.roughness_factor,
        normal_scale: material.normal_scale,
        occlusion: material.occlusion_strength,
        emissive: material.emissive_factor.to_array(),
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PbrMaterialReadback {
    pub material_id: String,
    pub display_name: String,
    pub cx_id: String,
    pub content_input_sha256: String,
    pub lowered_fingerprint: String,
    pub measurement_state: String,
    pub preview_uniform_fingerprint: String,
    pub texture_mask: u32,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub normal_scale: f32,
    pub occlusion: f32,
    pub emissive: [f32; 3],
}

fn material_payload(request: &PbrMaterialAuthorRequest, material: &PbrMaterial) -> Value {
    // The lowering valve accepts INTEGER-only JSON (floats are forbidden so a
    // lowered sim artifact is bit-deterministic). PBR factors are arbitrary f32,
    // so each is stored as its lossless IEEE-754 bit pattern (a u32, widened to a
    // JSON integer) and decoded back on readback -- an exact, deterministic
    // round-trip with no fixed-point precision loss.
    json!({
        "schema": PBR_MATERIAL_CONTENT_SCHEMA,
        "surface_kind": PBR_MATERIAL_SURFACE_KIND,
        "float_encoding": "ieee754_f32_bits",
        "material_id": request.material_id,
        "display_name": request.display_name,
        "base_color_bits": f32x4_bits(material.base_color_factor.to_array()),
        "metallic_bits": f32_bits(material.metallic_factor),
        "roughness_bits": f32_bits(material.roughness_factor),
        "normal_scale_bits": f32_bits(material.normal_scale),
        "occlusion_bits": f32_bits(material.occlusion_strength),
        "emissive_bits": f32x3_bits(material.emissive_factor.to_array()),
        "texture_mask": material.texture_mask(),
        "maps": request.maps,
    })
}

fn parse_material_payload(payload: &Value) -> Result<PbrMaterial, EngineError> {
    let object = payload
        .as_object()
        .ok_or_else(|| material_error("payload", "material payload must be a JSON object"))?;
    let request = PbrMaterialAuthorRequest {
        material_id: string_field(object, "material_id")?,
        display_name: string_field(object, "display_name")?,
        author: "readback".to_string(),
        base_color: f32x4_from_bits(object, "base_color_bits")?,
        metallic: f32_from_bits(object, "metallic_bits")?,
        roughness: f32_from_bits(object, "roughness_bits")?,
        normal_scale: f32_from_bits(object, "normal_scale_bits")?,
        occlusion: f32_from_bits(object, "occlusion_bits")?,
        emissive: f32x3_from_bits(object, "emissive_bits")?,
        maps: match object.get("maps") {
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|error| material_error("payload.maps", error.to_string()))?,
            None => Vec::new(),
        },
    };
    request.to_material()
}

fn canonical_input(payload: &Value) -> String {
    payload.to_string()
}

fn open_store(vault_dir: &Path) -> Result<EngineCalyxContentStore, EngineError> {
    let db = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(vault_dir))
        .map_err(|error| material_error("open_vault", error.to_string()))?;
    Ok(EngineCalyxContentStore::with_measurement_config(
        db,
        material_measurement_config(),
    ))
}

/// Choose the content-measurement panel. The REAL canonical CPU-explicit panel is
/// used whenever it can be surfaced (the AI addon is present and the canonical
/// panel vault is materialized). When it cannot -- addon absent / panel vault not
/// materialized on this host -- authoring must still succeed and record a
/// `pending_measurement` row (#1791), so we degrade LOUDLY (a warn + a labelled
/// source) to the addon-absent panel rather than hard-failing. Same code path as
/// the engine's existing ORT-absent degradation; the outcome is environment-
/// determined and always recorded in `measurement_state`.
fn material_measurement_config() -> EngineContentMeasurementConfig {
    match resolve_engine_content_panel_vault()
        .map_err(|error| error.to_string())
        .and_then(|vault| {
            EngineContentMeasurementConfig::from_loaded_cpu_explicit_panel_vault(vault)
                .map_err(|error| error.to_string())
        }) {
        Ok(config) => config,
        Err(reason) => {
            tracing::warn!(
                target: "litd_editor::material_editor",
                reason = %reason,
                "canonical CPU-explicit content panel unavailable; authoring with addon-absent panel -> pending_measurement (#1791)"
            );
            addon_absent_content_measurement(
                "addon_absent:material_editor(canonical_cpu_explicit_panel_unavailable)",
            )
        }
    }
}

fn validate_material_id(id: &str) -> Result<(), EngineError> {
    if id.trim().is_empty() {
        return Err(material_error(
            "material_id",
            "material id must be non-empty",
        ));
    }
    if id.chars().any(char::is_control) {
        return Err(material_error(
            "material_id",
            "material id must not contain control characters",
        ));
    }
    Ok(())
}

fn string_field(
    object: &serde_json::Map<String, Value>,
    key: &'static str,
) -> Result<String, EngineError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| material_error(key, "missing string field in persisted material"))
}

fn f32_bits(value: f32) -> u64 {
    u64::from(value.to_bits())
}

fn f32x3_bits(values: [f32; 3]) -> [u64; 3] {
    [
        f32_bits(values[0]),
        f32_bits(values[1]),
        f32_bits(values[2]),
    ]
}

fn f32x4_bits(values: [f32; 4]) -> [u64; 4] {
    [
        f32_bits(values[0]),
        f32_bits(values[1]),
        f32_bits(values[2]),
        f32_bits(values[3]),
    ]
}

fn f32_from_bits(
    object: &serde_json::Map<String, Value>,
    key: &'static str,
) -> Result<f32, EngineError> {
    let bits = object
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| material_error(key, "missing or oversized f32-bits field"))?;
    Ok(f32::from_bits(bits))
}

fn bits_array(
    object: &serde_json::Map<String, Value>,
    key: &'static str,
    expected: usize,
) -> Result<Vec<f32>, EngineError> {
    let array = object
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| material_error(key, "missing f32-bits array field"))?;
    if array.len() != expected {
        return Err(material_error(key, "unexpected f32-bits component count"));
    }
    array
        .iter()
        .map(|value| {
            value
                .as_u64()
                .and_then(|bits| u32::try_from(bits).ok())
                .map(f32::from_bits)
                .ok_or_else(|| material_error(key, "f32-bits component is missing or oversized"))
        })
        .collect()
}

fn f32x3_from_bits(
    object: &serde_json::Map<String, Value>,
    key: &'static str,
) -> Result<[f32; 3], EngineError> {
    let array = bits_array(object, key, 3)?;
    Ok([array[0], array[1], array[2]])
}

fn f32x4_from_bits(
    object: &serde_json::Map<String, Value>,
    key: &'static str,
) -> Result<[f32; 4], EngineError> {
    let array = bits_array(object, key, 4)?;
    Ok([array[0], array[1], array[2], array[3]])
}

fn material_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::Rendering(
        litd_engine::error::RenderingError::MaterialCatalogInvalidConfig {
            field: format!("editor.material_editor.{}", field.into()),
            reason: reason.into(),
        },
    )
}

#[cfg(test)]
#[path = "material_editor_tests.rs"]
mod tests;
