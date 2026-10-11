//! Shader/material catalog records for reuse and duplicate governance.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    render::{
        material::{PbrMaterial, TextureSlot},
        shader::{ShaderPlan, ShaderVariant, shader_fingerprint},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShaderMaterialCatalogSettings {
    pub near_duplicate_threshold: f32,
}

impl Default for ShaderMaterialCatalogSettings {
    fn default() -> Self {
        Self {
            near_duplicate_threshold: 0.025,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShaderMaterialCatalogInput {
    pub records: Vec<ShaderMaterialRecordInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShaderMaterialRecordInput {
    pub record_id: String,
    pub display_name: String,
    pub shader_id: String,
    pub shader_source_hash: String,
    pub variant_key: String,
    pub material: PbrMaterial,
}

impl ShaderMaterialRecordInput {
    pub fn pbr_variant(
        record_id: impl Into<String>,
        display_name: impl Into<String>,
        shader: &ShaderPlan,
        variant: &ShaderVariant,
        material: PbrMaterial,
    ) -> Self {
        Self {
            record_id: record_id.into(),
            display_name: display_name.into(),
            shader_id: shader.id.clone(),
            shader_source_hash: shader.source_hash.clone(),
            variant_key: variant.key.clone(),
            material,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShaderMaterialCatalog {
    pub schema: String,
    pub source_of_truth: String,
    pub settings: ShaderMaterialCatalogSettings,
    pub record_count: usize,
    pub reuse_group_count: usize,
    pub records: Vec<ShaderMaterialConstellation>,
    pub reuse_groups: Vec<ShaderMaterialReuseGroup>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShaderMaterialConstellation {
    pub record_id: String,
    pub display_name: String,
    pub shader_id: String,
    pub shader_source_hash: String,
    pub variant_key: String,
    pub constellation_id: String,
    pub shader_variant_signature: String,
    pub material_signature: String,
    pub texture_signature: String,
    pub feature_vector: PbrMaterialFeatureVector,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PbrMaterialFeatureVector {
    pub base_color_factor: [f32; 4],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub normal_scale: f32,
    pub occlusion_strength: f32,
    pub emissive_factor: [f32; 3],
    pub texture_mask: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShaderMaterialReuseGroup {
    pub group_id: String,
    pub canonical_record_id: String,
    pub candidates: Vec<ShaderMaterialReuseCandidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShaderMaterialReuseCandidate {
    pub record_id: String,
    pub relationship: ShaderMaterialReuseRelationship,
    pub distance: f32,
    pub can_collapse: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShaderMaterialReuseRelationship {
    ExactDuplicate,
    NearDuplicate,
    ParameterizedVariant,
}

pub fn build_shader_material_catalog(
    settings: ShaderMaterialCatalogSettings,
    input: ShaderMaterialCatalogInput,
) -> Result<ShaderMaterialCatalog, EngineError> {
    validate_settings(settings)?;
    if input.records.is_empty() {
        return invalid_catalog(
            "records",
            "must contain at least one shader/material record",
        );
    }

    let mut seen_ids = BTreeSet::new();
    let mut records = Vec::with_capacity(input.records.len());
    for record in input.records {
        validate_record_input(&record)?;
        if !seen_ids.insert(record.record_id.clone()) {
            return invalid_catalog("records.record_id", "duplicate catalog record id");
        }
        records.push(build_constellation(record)?);
    }
    records.sort_by(|left, right| left.record_id.cmp(&right.record_id));
    let reuse_groups = build_reuse_groups(settings, &records);
    Ok(ShaderMaterialCatalog {
        schema: "litd.shader_material_catalog.v1".to_string(),
        source_of_truth: "shader plans + PBR material configs".to_string(),
        settings,
        record_count: records.len(),
        reuse_group_count: reuse_groups.len(),
        records,
        reuse_groups,
    })
}

fn build_constellation(
    input: ShaderMaterialRecordInput,
) -> Result<ShaderMaterialConstellation, EngineError> {
    let feature_vector = PbrMaterialFeatureVector::from_material(&input.material)?;
    let texture_signature = texture_signature(&input.material)?;
    let material_signature = material_signature(&feature_vector, &texture_signature);
    let shader_variant_signature = format!(
        "shader={};source={};variant={}",
        input.shader_id, input.shader_source_hash, input.variant_key
    );
    let constellation_id = stable_id(&format!(
        "{}|{}|{}",
        shader_variant_signature, material_signature, texture_signature
    ));
    Ok(ShaderMaterialConstellation {
        record_id: input.record_id,
        display_name: input.display_name,
        shader_id: input.shader_id,
        shader_source_hash: input.shader_source_hash,
        variant_key: input.variant_key,
        constellation_id,
        shader_variant_signature,
        material_signature,
        texture_signature,
        feature_vector,
    })
}

fn build_reuse_groups(
    settings: ShaderMaterialCatalogSettings,
    records: &[ShaderMaterialConstellation],
) -> Vec<ShaderMaterialReuseGroup> {
    let mut groups = BTreeMap::<String, ShaderMaterialReuseGroup>::new();
    let mut collapsed_members = BTreeSet::new();
    for left_index in 0..records.len() {
        let left = &records[left_index];
        if collapsed_members.contains(&left.record_id) {
            continue;
        }
        for right in records.iter().skip(left_index + 1) {
            if collapsed_members.contains(&right.record_id) {
                continue;
            }
            let Some(candidate) = classify_candidate(settings, left, right) else {
                continue;
            };
            if candidate.can_collapse {
                collapsed_members.insert(candidate.record_id.clone());
            }
            let group_id = stable_id(&format!("reuse:{}", left.record_id));
            groups
                .entry(left.record_id.clone())
                .or_insert_with(|| ShaderMaterialReuseGroup {
                    group_id,
                    canonical_record_id: left.record_id.clone(),
                    candidates: Vec::new(),
                })
                .candidates
                .push(candidate);
        }
    }
    groups.into_values().collect()
}

fn classify_candidate(
    settings: ShaderMaterialCatalogSettings,
    left: &ShaderMaterialConstellation,
    right: &ShaderMaterialConstellation,
) -> Option<ShaderMaterialReuseCandidate> {
    if left.shader_id != right.shader_id || left.shader_source_hash != right.shader_source_hash {
        return None;
    }
    if left.material_signature == right.material_signature
        && left.shader_variant_signature == right.shader_variant_signature
    {
        return Some(candidate(
            right,
            ShaderMaterialReuseRelationship::ExactDuplicate,
            0.0,
            true,
            "same shader variant and identical canonical material parameters",
        ));
    }
    if left.material_signature == right.material_signature
        && left.shader_variant_signature != right.shader_variant_signature
    {
        return Some(candidate(
            right,
            ShaderMaterialReuseRelationship::ParameterizedVariant,
            0.0,
            false,
            "same material parameters but a different shader variant key",
        ));
    }
    if left.texture_signature == right.texture_signature
        && left.shader_variant_signature == right.shader_variant_signature
    {
        let distance = rounded_distance(left.feature_vector.distance(&right.feature_vector));
        if distance <= settings.near_duplicate_threshold {
            return Some(candidate(
                right,
                ShaderMaterialReuseRelationship::NearDuplicate,
                distance,
                true,
                "same shader variant and material parameters within reuse threshold",
            ));
        }
    }
    None
}

fn candidate(
    record: &ShaderMaterialConstellation,
    relationship: ShaderMaterialReuseRelationship,
    distance: f32,
    can_collapse: bool,
    reason: &str,
) -> ShaderMaterialReuseCandidate {
    ShaderMaterialReuseCandidate {
        record_id: record.record_id.clone(),
        relationship,
        distance,
        can_collapse,
        reason: reason.to_string(),
    }
}

impl PbrMaterialFeatureVector {
    fn from_material(material: &PbrMaterial) -> Result<Self, EngineError> {
        material.validate()?;
        if material
            .emissive_factor
            .to_array()
            .iter()
            .any(|channel| !channel.is_finite() || *channel < 0.0 || *channel > 64.0)
        {
            return invalid_catalog(
                "material.emissive_factor",
                "channels must be finite and inside 0..=64 for catalog comparison",
            );
        }
        Ok(Self {
            base_color_factor: material.base_color_factor.to_array(),
            metallic_factor: material.metallic_factor,
            roughness_factor: material.roughness_factor,
            normal_scale: material.normal_scale,
            occlusion_strength: material.occlusion_strength,
            emissive_factor: material.emissive_factor.to_array(),
            texture_mask: material.texture_mask(),
        })
    }

    fn distance(&self, other: &Self) -> f32 {
        let mut max_delta: f32 = 0.0;
        let left_values = self.normalized_values();
        let right_values = other.normalized_values();
        for (left, right) in left_values.iter().zip(right_values.iter()) {
            max_delta = max_delta.max((left - right).abs());
        }
        max_delta
    }

    fn normalized_values(&self) -> [f32; 11] {
        [
            self.base_color_factor[0],
            self.base_color_factor[1],
            self.base_color_factor[2],
            self.base_color_factor[3],
            self.metallic_factor,
            self.roughness_factor,
            self.normal_scale / 8.0,
            self.occlusion_strength,
            self.emissive_factor[0],
            self.emissive_factor[1],
            self.emissive_factor[2],
        ]
    }
}

fn rounded_distance(value: f32) -> f32 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn validate_settings(settings: ShaderMaterialCatalogSettings) -> Result<(), EngineError> {
    if !settings.near_duplicate_threshold.is_finite()
        || settings.near_duplicate_threshold < 0.0
        || settings.near_duplicate_threshold > 1.0
    {
        return invalid_catalog(
            "settings.near_duplicate_threshold",
            "must be finite and inside 0..=1",
        );
    }
    Ok(())
}

fn validate_record_input(record: &ShaderMaterialRecordInput) -> Result<(), EngineError> {
    for (field, value) in [
        ("record_id", record.record_id.as_str()),
        ("display_name", record.display_name.as_str()),
        ("shader_id", record.shader_id.as_str()),
        ("shader_source_hash", record.shader_source_hash.as_str()),
        ("variant_key", record.variant_key.as_str()),
    ] {
        if value.trim().is_empty() {
            return invalid_catalog(field, "must be non-empty");
        }
    }
    Ok(())
}

fn texture_signature(material: &PbrMaterial) -> Result<String, EngineError> {
    let mut slots = BTreeMap::new();
    for binding in &material.textures {
        if binding.asset_id.trim().is_empty() {
            return invalid_catalog("material.textures.asset_id", "must be non-empty");
        }
        if slots
            .insert(texture_slot_label(binding.slot), binding.asset_id.clone())
            .is_some()
        {
            return invalid_catalog("material.textures.slot", "duplicate texture slot");
        }
    }
    Ok(slots
        .into_iter()
        .map(|(slot, asset)| format!("{slot}={asset}"))
        .collect::<Vec<_>>()
        .join(";"))
}

fn material_signature(vector: &PbrMaterialFeatureVector, texture_signature: &str) -> String {
    format!(
        "base={:.6},{:.6},{:.6},{:.6};metallic={:.6};roughness={:.6};normal={:.6};ao={:.6};emissive={:.6},{:.6},{:.6};mask={};textures={}",
        vector.base_color_factor[0],
        vector.base_color_factor[1],
        vector.base_color_factor[2],
        vector.base_color_factor[3],
        vector.metallic_factor,
        vector.roughness_factor,
        vector.normal_scale,
        vector.occlusion_strength,
        vector.emissive_factor[0],
        vector.emissive_factor[1],
        vector.emissive_factor[2],
        vector.texture_mask,
        texture_signature
    )
}

fn texture_slot_label(slot: TextureSlot) -> &'static str {
    match slot {
        TextureSlot::BaseColor => "base_color",
        TextureSlot::MetallicRoughness => "metallic_roughness",
        TextureSlot::Normal => "normal",
        TextureSlot::AmbientOcclusion => "ambient_occlusion",
        TextureSlot::Emissive => "emissive",
        TextureSlot::Irradiance => "irradiance",
        TextureSlot::PrefilteredEnvironment => "prefiltered_environment",
        TextureSlot::BrdfLut => "brdf_lut",
    }
}

fn stable_id(value: &str) -> String {
    shader_fingerprint(value)
}

fn invalid_catalog<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::Rendering(
        litd_engine::error::RenderingError::MaterialCatalogInvalidConfig {
            field: field.into(),
            reason: reason.into(),
        },
    ))
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
