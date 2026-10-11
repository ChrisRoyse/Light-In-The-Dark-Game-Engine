use litd_engine::{
    EngineError,
    math::{Quat, Transform, Vec3},
};
use serde::{Deserialize, Serialize};

use crate::{
    history::{EditorDocument, EditorObject, EditorRegion, stable_fingerprint},
    terrain_bulk_ops::OBJECT_TYPE_ID_PROPERTY,
    terrain_pathing::{TerrainPathMovement, effective_pathing_for_cell},
};

pub const TERRAIN_CELL_PROPERTY: &str = "terrain.cell_id";
pub const TERRAIN_TILE_X_PROPERTY: &str = "terrain.tile_x";
pub const TERRAIN_TILE_Z_PROPERTY: &str = "terrain.tile_z";
pub const TERRAIN_PATHING_PROPERTY: &str = "terrain.pathing";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerrainPlacementKind {
    Unit,
    Doodad,
    StartLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainObjectPlacementRequest {
    pub object_id: String,
    pub kind: TerrainPlacementKind,
    pub type_id: String,
    pub target_tile_x: i32,
    pub target_tile_z: i32,
    pub yaw_degrees: f32,
    pub clamp_to_playable_bounds: bool,
    pub reject_unwalkable_ground: bool,
    pub owner: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainRegionPlacementRequest {
    pub region_id: String,
    pub name: String,
    pub center_tile_x: i32,
    pub center_tile_z: i32,
    pub half_extent_tiles_x: f32,
    pub half_extent_tiles_z: f32,
    pub clamp_to_playable_bounds: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainPlacementReport {
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub entity_id: String,
    pub entity_kind: String,
    pub requested_tile: [i32; 2],
    pub placed_tile: [i32; 2],
    pub terrain_cell_id: String,
    pub clamped: bool,
    pub translation: [f32; 3],
    pub pathing: String,
}

#[derive(Debug, Clone)]
struct ResolvedTerrainCell {
    id: String,
    tile_x: i32,
    tile_z: i32,
    height: f32,
    clamped: bool,
}

impl TerrainPlacementKind {
    fn object_kind(self) -> &'static str {
        match self {
            Self::Unit => "Unit",
            Self::Doodad => "Doodad",
            Self::StartLocation => "StartLocation",
        }
    }
}

impl TerrainObjectPlacementRequest {
    fn validate(&self) -> Result<(), EngineError> {
        if self.object_id.trim().is_empty() || self.type_id.trim().is_empty() {
            return invalid_terrain_authoring("object", "object_id and type_id are required");
        }
        if !self.yaw_degrees.is_finite() {
            return invalid_terrain_authoring("object.yaw", "yaw_degrees must be finite");
        }
        Ok(())
    }
}

impl TerrainRegionPlacementRequest {
    fn validate(&self) -> Result<(), EngineError> {
        if self.region_id.trim().is_empty() || self.name.trim().is_empty() {
            return invalid_terrain_authoring("region", "region_id and name are required");
        }
        if !self.half_extent_tiles_x.is_finite()
            || !self.half_extent_tiles_z.is_finite()
            || self.half_extent_tiles_x <= 0.0
            || self.half_extent_tiles_z <= 0.0
        {
            return invalid_terrain_authoring(
                "region.extents",
                "region half extents must be positive finite values",
            );
        }
        Ok(())
    }
}

pub fn place_terrain_object(
    document: &mut EditorDocument,
    request: &TerrainObjectPlacementRequest,
) -> Result<TerrainPlacementReport, EngineError> {
    request.validate()?;
    document.validate()?;
    require_world_root(document)?;
    if document.objects.contains_key(&request.object_id) {
        return invalid_terrain_authoring(
            "object_id",
            format!("object {} already exists", request.object_id),
        );
    }

    let before_fingerprint = stable_fingerprint(document)?;
    let resolved = resolve_terrain_cell(
        document,
        request.target_tile_x,
        request.target_tile_z,
        request.clamp_to_playable_bounds,
    )?;
    let flags = effective_pathing_for_cell(document, &resolved.id)?;
    if request.reject_unwalkable_ground
        && !flags.allows(TerrainPathMovement::GroundUnit)
        && request.kind == TerrainPlacementKind::Unit
    {
        return invalid_terrain_authoring(
            "object.pathing",
            format!("cell {} does not allow ground-unit placement", resolved.id),
        );
    }

    let translation = Vec3::new(
        resolved.tile_x as f32 + 0.5,
        resolved.height,
        resolved.tile_z as f32 + 0.5,
    );
    let mut object = EditorObject::new(
        request.object_id.clone(),
        request.kind.object_kind(),
        Some("world".to_string()),
        Transform::from_trs(
            translation,
            Quat::from_rotation_y(request.yaw_degrees.to_radians()),
            Vec3::ONE,
        ),
    )
    .with_property(OBJECT_TYPE_ID_PROPERTY, request.type_id.clone())
    .with_property(TERRAIN_CELL_PROPERTY, resolved.id.clone())
    .with_property(TERRAIN_TILE_X_PROPERTY, resolved.tile_x.to_string())
    .with_property(TERRAIN_TILE_Z_PROPERTY, resolved.tile_z.to_string())
    .with_property(TERRAIN_PATHING_PROPERTY, pathing_label(flags));
    if let Some(owner) = &request.owner {
        object.properties.insert("owner".to_string(), owner.clone());
    }
    if request.kind == TerrainPlacementKind::StartLocation {
        object
            .properties
            .insert("kind".to_string(), "start_location".to_string());
    }

    let mut next = document.clone();
    next.upsert_object(object)?;
    next.validate()?;
    let after_fingerprint = stable_fingerprint(&next)?;
    if before_fingerprint == after_fingerprint {
        return invalid_terrain_authoring("object.no_change", "object placement changed no data");
    }
    *document = next;
    Ok(TerrainPlacementReport {
        before_fingerprint,
        after_fingerprint,
        entity_id: request.object_id.clone(),
        entity_kind: request.kind.object_kind().to_string(),
        requested_tile: [request.target_tile_x, request.target_tile_z],
        placed_tile: [resolved.tile_x, resolved.tile_z],
        terrain_cell_id: resolved.id,
        clamped: resolved.clamped,
        translation: translation.to_array(),
        pathing: pathing_label(flags),
    })
}

pub fn place_terrain_region(
    document: &mut EditorDocument,
    request: &TerrainRegionPlacementRequest,
) -> Result<TerrainPlacementReport, EngineError> {
    request.validate()?;
    document.validate()?;
    if document.regions.contains_key(&request.region_id) {
        return invalid_terrain_authoring(
            "region_id",
            format!("region {} already exists", request.region_id),
        );
    }

    let before_fingerprint = stable_fingerprint(document)?;
    let resolved = resolve_terrain_cell(
        document,
        request.center_tile_x,
        request.center_tile_z,
        request.clamp_to_playable_bounds,
    )?;
    let flags = effective_pathing_for_cell(document, &resolved.id)?;
    let center = Vec3::new(
        resolved.tile_x as f32 + 0.5,
        resolved.height,
        resolved.tile_z as f32 + 0.5,
    );
    let region = EditorRegion::new(
        request.region_id.clone(),
        request.name.clone(),
        center,
        Vec3::new(
            request.half_extent_tiles_x,
            16.0,
            request.half_extent_tiles_z,
        ),
    )
    .with_property(TERRAIN_CELL_PROPERTY, resolved.id.clone())
    .with_property(TERRAIN_TILE_X_PROPERTY, resolved.tile_x.to_string())
    .with_property(TERRAIN_TILE_Z_PROPERTY, resolved.tile_z.to_string());

    let mut next = document.clone();
    next.upsert_region(region)?;
    next.validate()?;
    let after_fingerprint = stable_fingerprint(&next)?;
    if before_fingerprint == after_fingerprint {
        return invalid_terrain_authoring("region.no_change", "region placement changed no data");
    }
    *document = next;
    Ok(TerrainPlacementReport {
        before_fingerprint,
        after_fingerprint,
        entity_id: request.region_id.clone(),
        entity_kind: "Region".to_string(),
        requested_tile: [request.center_tile_x, request.center_tile_z],
        placed_tile: [resolved.tile_x, resolved.tile_z],
        terrain_cell_id: resolved.id,
        clamped: resolved.clamped,
        translation: center.to_array(),
        pathing: pathing_label(flags),
    })
}

fn resolve_terrain_cell(
    document: &EditorDocument,
    target_tile_x: i32,
    target_tile_z: i32,
    clamp: bool,
) -> Result<ResolvedTerrainCell, EngineError> {
    let (min_x, max_x, min_z, max_z) = terrain_bounds(document)?;
    let tile_x = if clamp {
        target_tile_x.clamp(min_x, max_x)
    } else {
        target_tile_x
    };
    let tile_z = if clamp {
        target_tile_z.clamp(min_z, max_z)
    } else {
        target_tile_z
    };
    let cell = document
        .terrain_cells
        .values()
        .find(|cell| cell.tile_x == tile_x && cell.tile_z == tile_z)
        .ok_or_else(|| {
            invalid_terrain_authoring_error(
                "target",
                format!(
                    "missing terrain cell at {},{} within bounds {},{}..{},{}",
                    target_tile_x, target_tile_z, min_x, min_z, max_x, max_z
                ),
            )
        })?;
    Ok(ResolvedTerrainCell {
        id: cell.id.clone(),
        tile_x: cell.tile_x,
        tile_z: cell.tile_z,
        height: cell.height,
        clamped: tile_x != target_tile_x || tile_z != target_tile_z,
    })
}

fn terrain_bounds(document: &EditorDocument) -> Result<(i32, i32, i32, i32), EngineError> {
    if document.terrain_cells.is_empty() {
        return invalid_terrain_authoring("terrain_cells", "document has no terrain cells");
    }
    let min_x = document
        .terrain_cells
        .values()
        .map(|cell| cell.tile_x)
        .min()
        .unwrap();
    let max_x = document
        .terrain_cells
        .values()
        .map(|cell| cell.tile_x)
        .max()
        .unwrap();
    let min_z = document
        .terrain_cells
        .values()
        .map(|cell| cell.tile_z)
        .min()
        .unwrap();
    let max_z = document
        .terrain_cells
        .values()
        .map(|cell| cell.tile_z)
        .max()
        .unwrap();
    Ok((min_x, max_x, min_z, max_z))
}

fn require_world_root(document: &EditorDocument) -> Result<(), EngineError> {
    if document.objects.contains_key("world") {
        Ok(())
    } else {
        invalid_terrain_authoring("document.world", "document must contain root object world")
    }
}

fn pathing_label(flags: crate::terrain_pathing::TerrainPathingFlags) -> String {
    format!(
        "walkable={};buildable={};flyable={};blocked={}",
        flags.walkable, flags.buildable, flags.flyable, flags.blocked
    )
}

fn invalid_terrain_authoring<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_terrain_authoring_error(field, reason))
}

fn invalid_terrain_authoring_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.terrain_authoring.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "terrain_authoring_tests.rs"]
mod tests;
