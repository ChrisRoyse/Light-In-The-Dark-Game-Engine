use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use serde::{Deserialize, Serialize};

use crate::{
    asset_vault::AssetVaultRecord,
    history::{EditorDocument, EditorObject, stable_fingerprint},
    world_creation::{EditorWorldRecord, validate_world_record},
};

pub const PLACED_ASSET_SOURCE_PROPERTY: &str = "placement.source";
pub const PLACED_ASSET_REQUEST_PROPERTY: &str = "placement.request_id";
pub const PLACED_ASSET_INDEX_PROPERTY: &str = "placement.index";
pub const PLACED_ASSET_RELATIVE_PATH_PROPERTY: &str = "asset.relative_path";
pub const PLACED_ASSET_SHA256_PROPERTY: &str = "asset.sha256";
pub const PLACED_ASSET_ID_PROPERTY: &str = "asset.id";
pub const PLACED_ASSET_TYPE_PROPERTY: &str = "asset.type";

const MAX_DRAG_ASSETS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentBrowserAssetRef {
    pub source: String,
    pub asset_id: String,
    pub asset_type: String,
    pub title: String,
    pub relative_path: String,
    pub bytes_on_disk: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetDropRequest {
    pub request_id: String,
    pub assets: Vec<ContentBrowserAssetRef>,
    pub drop_world: Vec3,
    pub surface_valid: bool,
    pub snap_grid: Option<f32>,
    pub clamp_to_playable_bounds: bool,
    pub multi_asset_spacing: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetPlacementReport {
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub placed_object_ids: Vec<String>,
    pub object_count: usize,
    pub snapped: bool,
    pub clamped: bool,
    pub readback: Vec<AssetPlacementReadback>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetPlacementReadback {
    pub object_id: String,
    pub asset_relative_path: String,
    pub asset_sha256: String,
    pub translation: [f32; 3],
}

impl ContentBrowserAssetRef {
    pub fn from_vault_record(record: &AssetVaultRecord) -> Self {
        Self {
            source: record.source.clone(),
            asset_id: record.asset_id.clone(),
            asset_type: record.asset_type.clone(),
            title: record.title.clone(),
            relative_path: record.relative_path.clone(),
            bytes_on_disk: record.bytes_on_disk,
            sha256: record.sha256.clone(),
        }
    }

    pub fn is_placeable_model(&self) -> bool {
        let path = self.relative_path.to_ascii_lowercase();
        path.ends_with(".gltf") || path.ends_with(".glb")
    }
}

impl AssetDropRequest {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.request_id.trim().is_empty() {
            return invalid_asset_placement("request_id", "request id must be non-empty");
        }
        if self.assets.is_empty() {
            return invalid_asset_placement("assets", "drop must contain at least one asset");
        }
        if self.assets.len() > MAX_DRAG_ASSETS {
            return invalid_asset_placement(
                "assets",
                format!(
                    "drop contains {} assets, max {MAX_DRAG_ASSETS}",
                    self.assets.len()
                ),
            );
        }
        if !self.surface_valid {
            return invalid_asset_placement("surface", "drop target surface is invalid");
        }
        if !self.drop_world.is_finite() {
            return invalid_asset_placement("drop_world", "drop position must be finite");
        }
        if let Some(grid) = self.snap_grid
            && (!grid.is_finite() || grid <= 0.0)
        {
            return invalid_asset_placement("snap_grid", "snap grid must be positive and finite");
        }
        if !self.multi_asset_spacing.is_finite() || self.multi_asset_spacing <= 0.0 {
            return invalid_asset_placement(
                "multi_asset_spacing",
                "multi asset spacing must be positive and finite",
            );
        }
        for asset in &self.assets {
            validate_asset(asset)?;
        }
        Ok(())
    }
}

pub fn place_assets_in_world_record(
    record: &mut EditorWorldRecord,
    request: &AssetDropRequest,
) -> Result<AssetPlacementReport, EngineError> {
    let report = place_assets_in_document(&mut record.document, request)?;
    record.document_fingerprint = stable_fingerprint(&record.document)?;
    record.summary = format!(
        "world={} placed_assets={} fingerprint={}",
        record.spec.properties.display_name,
        report.placed_object_ids.len(),
        record.document_fingerprint
    );
    validate_world_record(record)?;
    Ok(report)
}

pub fn place_assets_in_document(
    document: &mut EditorDocument,
    request: &AssetDropRequest,
) -> Result<AssetPlacementReport, EngineError> {
    request.validate()?;
    document.validate()?;
    if !document.objects.contains_key("world") {
        return invalid_asset_placement(
            "document.world",
            "document must contain root object world",
        );
    }
    let before_fingerprint = stable_fingerprint(document)?;
    let mut next = document.clone();
    let mut placed_object_ids = Vec::new();
    let mut readback = Vec::new();
    let mut any_clamped = false;

    for (index, asset) in request.assets.iter().enumerate() {
        let position = target_position(document, request, index)?;
        any_clamped |= position.clamped;
        let object_id = placed_object_id(&request.request_id, index, asset)?;
        if next.objects.contains_key(&object_id) {
            return invalid_asset_placement(
                "document.objects",
                format!("placed object id collision {object_id}"),
            );
        }
        let mut object = EditorObject::new(
            object_id.clone(),
            "Doodad",
            Some("world".to_string()),
            Transform::from_translation(position.translation),
        );
        object.properties.insert(
            PLACED_ASSET_SOURCE_PROPERTY.to_string(),
            "content_browser_drag_drop".to_string(),
        );
        object.properties.insert(
            PLACED_ASSET_REQUEST_PROPERTY.to_string(),
            request.request_id.clone(),
        );
        object
            .properties
            .insert(PLACED_ASSET_INDEX_PROPERTY.to_string(), index.to_string());
        object.properties.insert(
            PLACED_ASSET_RELATIVE_PATH_PROPERTY.to_string(),
            asset.relative_path.clone(),
        );
        object.properties.insert(
            PLACED_ASSET_SHA256_PROPERTY.to_string(),
            asset.sha256.clone(),
        );
        object
            .properties
            .insert(PLACED_ASSET_ID_PROPERTY.to_string(), asset.asset_id.clone());
        object.properties.insert(
            PLACED_ASSET_TYPE_PROPERTY.to_string(),
            asset.asset_type.clone(),
        );
        next.upsert_object(object)?;
        let stored = next.objects.get(&object_id).ok_or_else(|| {
            invalid_asset_placement_error(
                "document.readback",
                format!("placed object {object_id} missing immediately after upsert"),
            )
        })?;
        readback.push(AssetPlacementReadback {
            object_id: object_id.clone(),
            asset_relative_path: stored
                .properties
                .get(PLACED_ASSET_RELATIVE_PATH_PROPERTY)
                .cloned()
                .unwrap_or_default(),
            asset_sha256: stored
                .properties
                .get(PLACED_ASSET_SHA256_PROPERTY)
                .cloned()
                .unwrap_or_default(),
            translation: stored.transform.translation.to_array(),
        });
        placed_object_ids.push(object_id);
    }

    next.validate()?;
    let after_fingerprint = stable_fingerprint(&next)?;
    *document = next;
    Ok(AssetPlacementReport {
        before_fingerprint,
        after_fingerprint,
        placed_object_ids,
        object_count: document.objects.len(),
        snapped: request.snap_grid.is_some(),
        clamped: any_clamped,
        readback,
    })
}

#[derive(Debug, Clone, Copy)]
struct PositionedDrop {
    translation: Vec3,
    clamped: bool,
}

fn target_position(
    document: &EditorDocument,
    request: &AssetDropRequest,
    index: usize,
) -> Result<PositionedDrop, EngineError> {
    let properties = &document.world_properties;
    let mut translation = request.drop_world;
    translation.x += index as f32 * request.multi_asset_spacing;
    if let Some(grid) = request.snap_grid {
        translation.x = (translation.x / grid).round() * grid;
        translation.z = (translation.z / grid).round() * grid;
    }
    let min_x = properties.playable_bounds.min_tile_x as f32;
    let min_z = properties.playable_bounds.min_tile_z as f32;
    let max_x = properties.playable_bounds.max_tile_x_exclusive as f32;
    let max_z = properties.playable_bounds.max_tile_z_exclusive as f32;
    let before = translation;
    if request.clamp_to_playable_bounds {
        translation.x = translation.x.clamp(min_x, max_x);
        translation.z = translation.z.clamp(min_z, max_z);
    } else if translation.x < min_x
        || translation.x > max_x
        || translation.z < min_z
        || translation.z > max_z
    {
        return invalid_asset_placement(
            "drop_world",
            format!(
                "drop {},{} is outside playable bounds {},{}..{},{}",
                translation.x, translation.z, min_x, min_z, max_x, max_z
            ),
        );
    }
    translation.y = 0.0;
    Ok(PositionedDrop {
        translation,
        clamped: translation.x != before.x || translation.z != before.z,
    })
}

fn placed_object_id(
    request_id: &str,
    index: usize,
    asset: &ContentBrowserAssetRef,
) -> Result<String, EngineError> {
    Ok(format!(
        "placed_{}_{}_{}",
        sanitize_id("request_id", request_id)?,
        index,
        sanitize_id("asset_id", &asset.asset_id)?
    ))
}

fn validate_asset(asset: &ContentBrowserAssetRef) -> Result<(), EngineError> {
    if asset.relative_path.trim().is_empty()
        || asset.asset_id.trim().is_empty()
        || asset.sha256.trim().is_empty()
        || asset.bytes_on_disk == 0
    {
        return invalid_asset_placement(
            "asset",
            "asset id, relative path, sha256, and non-zero bytes are required",
        );
    }
    if !asset.is_placeable_model() {
        return invalid_asset_placement(
            "asset.relative_path",
            format!("{} is not a placeable glTF/GLB model", asset.relative_path),
        );
    }
    Ok(())
}

fn sanitize_id(field: &str, value: &str) -> Result<String, EngineError> {
    if value.trim().is_empty() {
        return invalid_asset_placement(field, "id segment must be non-empty");
    }
    let mut sanitized = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    while sanitized.contains("__") {
        sanitized = sanitized.replace("__", "_");
    }
    let sanitized = sanitized.trim_matches('_').to_string();
    if sanitized.is_empty() {
        return invalid_asset_placement(field, "id segment sanitized to empty");
    }
    Ok(sanitized)
}

fn invalid_asset_placement<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_asset_placement_error(field, reason))
}

fn invalid_asset_placement_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_placement.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "asset_placement_tests.rs"]
mod tests;
