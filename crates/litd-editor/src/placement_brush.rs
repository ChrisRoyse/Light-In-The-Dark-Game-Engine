use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use serde::{Deserialize, Serialize};

use crate::{
    asset_placement::ContentBrowserAssetRef,
    prefabs::{PrefabLibrary, PrefabPropertyOverride},
};

#[path = "placement_brush_support.rs"]
mod placement_brush_support;
pub use placement_brush_support::{
    placement_brush_object_id, stamp_placement_brush_in_document,
    stamp_placement_brush_in_world_record,
};

pub const BRUSH_STAMP_ID_PROPERTY: &str = "placement.brush_stamp_id";
pub const BRUSH_INDEX_PROPERTY: &str = "placement.brush_index";
pub const BRUSH_SEED_PROPERTY: &str = "placement.brush_seed";
pub const BRUSH_CONTENT_KIND_PROPERTY: &str = "placement.brush_content_kind";
pub const BRUSH_VARIANT_INDEX_PROPERTY: &str = "placement.variant_index";
pub const BRUSH_YAW_DEGREES_PROPERTY: &str = "placement.yaw_degrees";
pub const BRUSH_SCALE_PROPERTY: &str = "placement.uniform_scale";
pub const BRUSH_SNAP_PROPERTY: &str = "placement.snap";
pub const BRUSH_TERRAIN_CELL_PROPERTY: &str = "placement.terrain_cell";
pub const BRUSH_TERRAIN_HEIGHT_PROPERTY: &str = "placement.terrain_height";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BrushRange {
    pub min: f32,
    pub max: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlacementBrushSnap {
    None,
    Grid { grid_size: f32 },
    Terrain,
    GridAndTerrain { grid_size: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacementBrushAsset {
    pub asset: ContentBrowserAssetRef,
    pub weight: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlacementBrushContent {
    Assets {
        variants: Vec<PlacementBrushAsset>,
    },
    Prefab {
        prefab_id: String,
        library: PrefabLibrary,
        overrides: Vec<PrefabPropertyOverride>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacementBrushRequest {
    pub stamp_id: String,
    pub center: Vec3,
    pub radius: f32,
    pub requested_count: usize,
    pub density_cap: usize,
    pub seed: u64,
    pub rotation_y_degrees: BrushRange,
    pub uniform_scale: BrushRange,
    pub snap: PlacementBrushSnap,
    pub clamp_to_playable_bounds: bool,
    pub content: PlacementBrushContent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacementBrushReport {
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub stamp_id: String,
    pub requested_count: usize,
    pub density_cap: usize,
    pub placed_count: usize,
    pub capped: bool,
    pub object_count: usize,
    pub grid_snapped: bool,
    pub terrain_snapped: bool,
    pub readback: Vec<PlacementBrushReadback>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacementBrushReadback {
    pub stamp_index: usize,
    pub object_ids: Vec<String>,
    pub content_kind: String,
    pub variant_index: Option<usize>,
    pub asset_id: Option<String>,
    pub asset_relative_path: Option<String>,
    pub prefab_id: Option<String>,
    pub translation: [f32; 3],
    pub yaw_degrees: f32,
    pub uniform_scale: f32,
    pub terrain_cell_id: Option<String>,
    pub terrain_height: Option<f32>,
}

impl BrushRange {
    pub fn constant(value: f32) -> Self {
        Self {
            min: value,
            max: value,
        }
    }

    fn validate(&self, field: &str, positive: bool) -> Result<(), EngineError> {
        if !self.min.is_finite() || !self.max.is_finite() {
            return invalid_placement_brush(field, "range bounds must be finite");
        }
        if self.min > self.max {
            return invalid_placement_brush(field, "range min must be <= max");
        }
        if positive && self.min <= 0.0 {
            return invalid_placement_brush(field, "range minimum must be positive");
        }
        Ok(())
    }
}

impl PlacementBrushSnap {
    pub fn grid_size(&self) -> Option<f32> {
        match self {
            Self::Grid { grid_size } | Self::GridAndTerrain { grid_size } => Some(*grid_size),
            Self::None | Self::Terrain => None,
        }
    }

    pub fn uses_terrain(&self) -> bool {
        matches!(self, Self::Terrain | Self::GridAndTerrain { .. })
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Grid { .. } => "grid",
            Self::Terrain => "terrain",
            Self::GridAndTerrain { .. } => "grid_and_terrain",
        }
    }

    fn validate(&self) -> Result<(), EngineError> {
        if let Some(grid) = self.grid_size()
            && (!grid.is_finite() || grid <= 0.0)
        {
            return invalid_placement_brush(
                "snap.grid_size",
                "grid size must be positive and finite",
            );
        }
        Ok(())
    }
}

impl PlacementBrushAsset {
    pub fn new(asset: ContentBrowserAssetRef, weight: u32) -> Self {
        Self { asset, weight }
    }

    fn validate(&self) -> Result<(), EngineError> {
        if self.weight == 0 {
            return invalid_placement_brush("asset.weight", "variant weight must be non-zero");
        }
        validate_asset_ref(&self.asset)
    }
}

impl PlacementBrushContent {
    fn validate(&self) -> Result<(), EngineError> {
        match self {
            Self::Assets { variants } => {
                if variants.is_empty() {
                    return invalid_placement_brush(
                        "content.assets",
                        "brush must contain at least one asset variant",
                    );
                }
                for variant in variants {
                    variant.validate()?;
                }
                Ok(())
            }
            Self::Prefab {
                prefab_id,
                library,
                overrides,
            } => {
                if prefab_id.trim().is_empty() {
                    return invalid_placement_brush("content.prefab_id", "must be non-empty");
                }
                let mut instance =
                    library.instantiate("placement_validation", prefab_id, Transform::IDENTITY)?;
                for override_ in overrides {
                    instance = instance.with_override(override_.clone());
                }
                instance.validate().map_err(|error| {
                    invalid_placement_brush_error("content.prefab", error.to_string())
                })
            }
        }
    }
}

impl PlacementBrushRequest {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_stable_segment("stamp_id", &self.stamp_id)?;
        if !self.center.is_finite() {
            return invalid_placement_brush("center", "center must be finite");
        }
        if !self.radius.is_finite() || self.radius <= 0.0 {
            return invalid_placement_brush("radius", "radius must be positive and finite");
        }
        if self.requested_count == 0 {
            return invalid_placement_brush("requested_count", "must be greater than zero");
        }
        if self.density_cap == 0 {
            return invalid_placement_brush("density_cap", "must be greater than zero");
        }
        self.rotation_y_degrees
            .validate("rotation_y_degrees", false)?;
        self.uniform_scale.validate("uniform_scale", true)?;
        self.snap.validate()?;
        self.content.validate()
    }

    pub fn target_count(&self) -> usize {
        self.requested_count.min(self.density_cap)
    }
}

fn validate_asset_ref(asset: &ContentBrowserAssetRef) -> Result<(), EngineError> {
    if asset.source.trim().is_empty()
        || asset.asset_id.trim().is_empty()
        || asset.asset_type.trim().is_empty()
        || asset.relative_path.trim().is_empty()
        || asset.sha256.trim().is_empty()
        || asset.bytes_on_disk == 0
    {
        return invalid_placement_brush(
            "asset",
            "source, id, type, relative path, sha256, and non-zero bytes are required",
        );
    }
    if !asset.is_placeable_model() {
        return invalid_placement_brush(
            "asset.relative_path",
            format!("{} is not a placeable glTF/GLB model", asset.relative_path),
        );
    }
    Ok(())
}

pub(crate) fn validate_stable_segment(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid_placement_brush(field, "id segment must be non-empty");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return invalid_placement_brush(
            field,
            format!("{value} must use only ASCII alnum, '_', '-', '.'"),
        );
    }
    Ok(())
}

pub(crate) fn invalid_placement_brush<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_placement_brush_error(field, reason))
}

pub(crate) fn invalid_placement_brush_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.placement_brush.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "placement_brush_tests.rs"]
mod tests;
