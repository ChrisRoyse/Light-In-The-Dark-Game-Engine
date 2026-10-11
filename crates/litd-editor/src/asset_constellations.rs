#[path = "asset_constellation_media.rs"]
mod asset_constellation_media;

use std::collections::{BTreeMap, BTreeSet};

use litd_engine::{EngineError, render::shader::shader_fingerprint};
use serde::{Deserialize, Serialize};

use crate::{
    asset_products::{AssetProductKind, product_from_vault_record},
    asset_scene::load_editor_world_asset_scene,
    asset_vault::{AssetSemanticProfile, AssetVaultRecord, AssetVaultSemanticIndex},
    gltf_validation::validate_gltf_source,
    sound_editor::sound_library_entry_from_vault_record,
};
use asset_constellation_media::{
    image_info, image_or_thumbnail_path, is_document_image, is_image_path, is_mesh_path,
    relative_to_vault,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetConstellationIndex {
    pub schema: String,
    pub source_of_truth: String,
    pub constellation_count: usize,
    pub modality_counts: BTreeMap<AssetModality, usize>,
    pub constellations: Vec<AssetMultimodalConstellation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetMultimodalConstellation {
    pub record_id: String,
    pub constellation_id: String,
    pub relative_path: String,
    pub source: String,
    pub asset_id: String,
    pub asset_type: String,
    pub title: String,
    pub sha256: String,
    pub roles: Vec<String>,
    pub tags: Vec<String>,
    pub slots: Vec<AssetModalitySlot>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AssetModality {
    Text,
    Image,
    DocumentImage,
    Audio,
    MeshStructured,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetModalitySlot {
    pub modality: AssetModality,
    pub state: AssetSlotState,
    pub source_path: Option<String>,
    pub feature_hash: Option<String>,
    pub evidence: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetSlotState {
    Populated,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSimilarityHit {
    pub relative_path: String,
    pub score: u32,
    pub shared_roles: Vec<String>,
    pub shared_tags: Vec<String>,
    pub shared_modalities: Vec<AssetModality>,
    pub reason: String,
}

pub fn build_asset_constellation_index(
    records: &[AssetVaultRecord],
) -> Result<AssetConstellationIndex, EngineError> {
    if records.is_empty() {
        return invalid_constellation("records", "must contain at least one real asset record");
    }
    let semantic_index = AssetVaultSemanticIndex::from_records(records)?;
    let mut seen = BTreeSet::new();
    let mut constellations = Vec::with_capacity(records.len());
    for record in records {
        let key = format!("{}|{}", record.relative_path, record.sha256);
        if !seen.insert(key) {
            return invalid_constellation("records.duplicate", "duplicate asset record input");
        }
        let profile = semantic_index
            .profile_for_path(&record.relative_path)
            .ok_or_else(|| invalid_constellation_error("semantic.profile", "missing profile"))?;
        constellations.push(build_constellation(record, profile)?);
    }
    constellations.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let modality_counts = count_modalities(&constellations);
    Ok(AssetConstellationIndex {
        schema: "litd.asset_multimodal_constellations.v1".to_string(),
        source_of_truth: "real AssetVaultRecord files + semantic profiles + media metadata"
            .to_string(),
        constellation_count: constellations.len(),
        modality_counts,
        constellations,
    })
}

pub fn find_assets_like_this(
    index: &AssetConstellationIndex,
    source_relative_path: &str,
    limit: usize,
) -> Result<Vec<AssetSimilarityHit>, EngineError> {
    if limit == 0 {
        return invalid_constellation("query.limit", "must be greater than zero");
    }
    let source = index
        .constellations
        .iter()
        .find(|asset| asset.relative_path == source_relative_path)
        .ok_or_else(|| invalid_constellation_error("query.source", "source asset not indexed"))?;
    let mut hits = index
        .constellations
        .iter()
        .filter(|candidate| candidate.relative_path != source.relative_path)
        .map(|candidate| similarity_hit(source, candidate))
        .filter(|hit| hit.score > 0)
        .collect::<Vec<_>>();
    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    hits.truncate(limit);
    Ok(hits)
}

fn build_constellation(
    record: &AssetVaultRecord,
    profile: &AssetSemanticProfile,
) -> Result<AssetMultimodalConstellation, EngineError> {
    let mut slots = vec![
        text_slot(record, profile),
        image_slot(record)?,
        document_image_slot(record)?,
        audio_slot(record)?,
        mesh_structured_slot(record)?,
    ];
    slots.sort_by(|left, right| left.modality.cmp(&right.modality));
    let record_id = stable_id(&format!("{}|{}", record.relative_path, record.sha256));
    let populated = slots
        .iter()
        .filter(|slot| slot.state == AssetSlotState::Populated)
        .map(|slot| format!("{:?}", slot.modality))
        .collect::<Vec<_>>()
        .join("|");
    let constellation_id = stable_id(&format!(
        "{}|{}|{}|{}",
        record_id,
        profile.roles.join("|"),
        profile.tags.join("|"),
        populated
    ));
    Ok(AssetMultimodalConstellation {
        record_id,
        constellation_id,
        relative_path: record.relative_path.clone(),
        source: record.source.clone(),
        asset_id: record.asset_id.clone(),
        asset_type: record.asset_type.clone(),
        title: record.title.clone(),
        sha256: record.sha256.clone(),
        roles: profile.roles.clone(),
        tags: profile.tags.clone(),
        slots,
    })
}

fn text_slot(record: &AssetVaultRecord, profile: &AssetSemanticProfile) -> AssetModalitySlot {
    let text = format!(
        "{} {} {} {} {} {} {}",
        record.source,
        record.asset_id,
        record.asset_type,
        record.title,
        record.relative_path,
        profile.roles.join(" "),
        profile.tags.join(" ")
    );
    slot(
        AssetModality::Text,
        AssetSlotState::Populated,
        Some(record.relative_path.clone()),
        [("terms", text), ("bytes", record.bytes_on_disk.to_string())],
    )
}

fn image_slot(record: &AssetVaultRecord) -> Result<AssetModalitySlot, EngineError> {
    let Some(path) = image_or_thumbnail_path(record)? else {
        return Ok(missing_slot(
            AssetModality::Image,
            "no image or thumbnail found",
        ));
    };
    let info = image_info(&path)?;
    Ok(slot(
        AssetModality::Image,
        AssetSlotState::Populated,
        Some(relative_to_vault(record, &path)?),
        [
            ("width", info.width.to_string()),
            ("height", info.height.to_string()),
            ("color_type", info.color_type),
        ],
    ))
}

fn document_image_slot(record: &AssetVaultRecord) -> Result<AssetModalitySlot, EngineError> {
    if !is_image_path(&record.absolute_path) || !is_document_image(record) {
        return Ok(missing_slot(
            AssetModality::DocumentImage,
            "asset is not a texture/sheet/document image",
        ));
    }
    let info = image_info(&record.absolute_path)?;
    Ok(slot(
        AssetModality::DocumentImage,
        AssetSlotState::Populated,
        Some(record.relative_path.clone()),
        [
            ("width", info.width.to_string()),
            ("height", info.height.to_string()),
            ("kind", "texture_or_sheet".to_string()),
        ],
    ))
}

fn audio_slot(record: &AssetVaultRecord) -> Result<AssetModalitySlot, EngineError> {
    if record.asset_type != "audio" {
        return Ok(missing_slot(AssetModality::Audio, "asset is not audio"));
    }
    let entry = sound_library_entry_from_vault_record(record)?;
    Ok(slot(
        AssetModality::Audio,
        AssetSlotState::Populated,
        Some(record.relative_path.clone()),
        [
            ("codec", format!("{:?}", entry.codec)),
            ("length_ms", entry.length_ms.to_string()),
            ("sample_rate_hz", entry.sample_rate_hz.to_string()),
            ("channels", entry.channels.to_string()),
        ],
    ))
}

fn mesh_structured_slot(record: &AssetVaultRecord) -> Result<AssetModalitySlot, EngineError> {
    if !is_mesh_path(&record.absolute_path) {
        return Ok(missing_slot(
            AssetModality::MeshStructured,
            "asset is not glTF/GLB mesh",
        ));
    }
    let product = product_from_vault_record(record)?;
    if product.product_kind != AssetProductKind::Mesh {
        return invalid_constellation("mesh.product_kind", "glTF/GLB record is not a mesh product");
    }
    let validation = validate_gltf_source(&record.absolute_path, &record.relative_path)?;
    let scene = load_editor_world_asset_scene(&product)?;
    Ok(slot(
        AssetModality::MeshStructured,
        AssetSlotState::Populated,
        Some(record.relative_path.clone()),
        [
            ("mesh_count", validation.mesh_count.to_string()),
            ("primitive_count", validation.primitive_count.to_string()),
            ("vertex_count", scene.report.vertex_count.to_string()),
            ("triangle_count", scene.report.triangle_count.to_string()),
            ("material_count", scene.report.material_count.to_string()),
            ("image_count", scene.report.image_count.to_string()),
        ],
    ))
}

fn similarity_hit(
    source: &AssetMultimodalConstellation,
    candidate: &AssetMultimodalConstellation,
) -> AssetSimilarityHit {
    let shared_roles = intersection(&source.roles, &candidate.roles);
    let shared_tags = intersection(&source.tags, &candidate.tags);
    let shared_modalities = source
        .populated_modalities()
        .intersection(&candidate.populated_modalities())
        .cloned()
        .collect::<Vec<_>>();
    let mut score = 0_u32;
    if source.asset_id == candidate.asset_id {
        score += 80;
    }
    if source.source == candidate.source {
        score += 25;
    }
    if source.asset_type == candidate.asset_type {
        score += 25;
    }
    score += (shared_roles.len() as u32) * 20;
    score += (shared_tags.len() as u32) * 5;
    score += (shared_modalities.len() as u32) * 10;
    AssetSimilarityHit {
        relative_path: candidate.relative_path.clone(),
        score,
        shared_roles,
        shared_tags,
        shared_modalities,
        reason: "shared source/type/tags/roles/modalities".to_string(),
    }
}

impl AssetMultimodalConstellation {
    pub fn slot(&self, modality: AssetModality) -> Option<&AssetModalitySlot> {
        self.slots.iter().find(|slot| slot.modality == modality)
    }

    fn populated_modalities(&self) -> BTreeSet<AssetModality> {
        self.slots
            .iter()
            .filter(|slot| slot.state == AssetSlotState::Populated)
            .map(|slot| slot.modality.clone())
            .collect()
    }
}

fn count_modalities(
    constellations: &[AssetMultimodalConstellation],
) -> BTreeMap<AssetModality, usize> {
    let mut counts = BTreeMap::new();
    for constellation in constellations {
        for slot in constellation
            .slots
            .iter()
            .filter(|slot| slot.state == AssetSlotState::Populated)
        {
            *counts.entry(slot.modality.clone()).or_insert(0) += 1;
        }
    }
    counts
}

fn slot(
    modality: AssetModality,
    state: AssetSlotState,
    source_path: Option<String>,
    values: impl IntoIterator<Item = (&'static str, String)>,
) -> AssetModalitySlot {
    let evidence = values
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect::<BTreeMap<_, _>>();
    let feature_hash = (state == AssetSlotState::Populated)
        .then(|| stable_id(&format!("{modality:?}|{source_path:?}|{evidence:?}")));
    AssetModalitySlot {
        modality,
        state,
        source_path,
        feature_hash,
        evidence,
    }
}

fn missing_slot(modality: AssetModality, reason: &str) -> AssetModalitySlot {
    slot(
        modality,
        AssetSlotState::Missing,
        None,
        [("reason", reason.to_string())],
    )
}

fn search_key(record: &AssetVaultRecord) -> String {
    format!(
        "{} {} {} {} {}",
        record.source, record.asset_id, record.asset_type, record.title, record.relative_path
    )
    .to_ascii_lowercase()
}

fn intersection(left: &[String], right: &[String]) -> Vec<String> {
    let right = right.iter().collect::<BTreeSet<_>>();
    left.iter()
        .filter(|item| right.contains(item))
        .cloned()
        .collect()
}

fn stable_id(value: &str) -> String {
    shader_fingerprint(value)
}

fn invalid_constellation<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_constellation_error(field, reason))
}

pub(crate) fn invalid_constellation_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_constellations.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "asset_constellations_tests.rs"]
mod tests;
