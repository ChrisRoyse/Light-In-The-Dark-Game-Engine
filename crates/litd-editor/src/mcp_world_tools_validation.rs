use std::collections::{BTreeMap, BTreeSet};

use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};

use crate::{
    history::EditorDocument,
    terrain_pathing::{TerrainPathMovement, effective_pathing_for_cell},
    world_creation::{
        EditorPlayableBounds, EditorWorldCreationSpec, EditorWorldProperties,
        create_editor_world_record,
    },
};

use super::{WorldStartLocation, invalid_mcp_world};

pub(super) fn validate_initial_document(
    document: Option<&EditorDocument>,
) -> Result<(), EngineError> {
    if let Some(document) = document {
        document.validate()?;
    }
    Ok(())
}

pub(super) fn validate_start_locations(starts: &[WorldStartLocation]) -> Result<(), EngineError> {
    if starts.is_empty() {
        return Err(invalid_mcp_world(
            "starts",
            "at least one start location is required",
        ));
    }
    let mut players = BTreeSet::new();
    let mut objects = BTreeSet::new();
    let mut slots = BTreeSet::new();
    for start in starts {
        validate_single_line("start.player_id", &start.player_id)?;
        validate_single_line("start.object_id", &start.object_id)?;
        if !players.insert(start.player_id.as_str()) {
            return Err(invalid_mcp_world(
                "start.player_id",
                format!("duplicate player id {}", start.player_id),
            ));
        }
        if !objects.insert(start.object_id.as_str()) {
            return Err(invalid_mcp_world(
                "start.object_id",
                format!("duplicate object id {}", start.object_id),
            ));
        }
        if !slots.insert(start.slot) {
            return Err(invalid_mcp_world(
                "start.slot",
                format!("duplicate player slot {}", start.slot),
            ));
        }
        if !start.position.is_finite() || !start.yaw_radians.is_finite() {
            return Err(invalid_mcp_world(
                "start.transform",
                "position and yaw must be finite",
            ));
        }
    }
    // Slots must be contiguous from 0 so runtime code can index players by slot.
    let expected: BTreeSet<u32> = (0..starts.len() as u32).collect();
    if slots != expected {
        return Err(invalid_mcp_world(
            "start.slot",
            format!(
                "player slots must be contiguous 0..{}; got {:?}",
                starts.len(),
                slots
            ),
        ));
    }
    Ok(())
}

/// #817: every start location must sit on real, walkable ground inside the map's
/// playable bounds. Runs after the world document is loaded (it needs terrain +
/// pathing) and fails loud with a typed diagnostic if a start is off-map or on
/// unpathable ground -- authored start points that a ground unit could never
/// stand on are rejected rather than silently accepted.
pub(super) fn validate_start_locations_against_world(
    document: &EditorDocument,
    starts: &[WorldStartLocation],
) -> Result<(), EngineError> {
    let bounds = &document.world_properties.playable_bounds;
    let cells_by_coord: BTreeMap<(i32, i32), &str> = document
        .terrain_cells
        .values()
        .map(|cell| ((cell.tile_x, cell.tile_z), cell.id.as_str()))
        .collect();
    for start in starts {
        let tile_x = start.position.x.floor() as i32;
        let tile_z = start.position.z.floor() as i32;
        if tile_x < bounds.min_tile_x
            || tile_x >= bounds.max_tile_x_exclusive
            || tile_z < bounds.min_tile_z
            || tile_z >= bounds.max_tile_z_exclusive
        {
            return Err(invalid_mcp_world(
                "start.bounds",
                format!(
                    "start slot {} ({}) at world ({:.3},{:.3}) maps to tile ({tile_x},{tile_z}) outside playable bounds [{}..{}, {}..{}]",
                    start.slot,
                    start.player_id,
                    start.position.x,
                    start.position.z,
                    bounds.min_tile_x,
                    bounds.max_tile_x_exclusive,
                    bounds.min_tile_z,
                    bounds.max_tile_z_exclusive,
                ),
            ));
        }
        let cell_id = cells_by_coord.get(&(tile_x, tile_z)).ok_or_else(|| {
            invalid_mcp_world(
                "start.ground",
                format!(
                    "start slot {} ({}) at tile ({tile_x},{tile_z}) has no terrain cell to stand on",
                    start.slot, start.player_id,
                ),
            )
        })?;
        let flags = effective_pathing_for_cell(document, cell_id)?;
        if !flags.allows(TerrainPathMovement::GroundUnit) {
            return Err(invalid_mcp_world(
                "start.pathing",
                format!(
                    "start slot {} ({}) at tile ({tile_x},{tile_z}) cell {cell_id} is not walkable ground (walkable={}, blocked={})",
                    start.slot, start.player_id, flags.walkable, flags.blocked,
                ),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return Err(invalid_mcp_world(
            field,
            "value must be non-empty single-line text",
        ));
    }
    Ok(())
}

pub fn sample_issue918_document() -> EditorDocument {
    let spec = EditorWorldCreationSpec {
        calyx_document_id: "calyx://world/issue918-mcp-world".to_string(),
        revision_id: "issue918-world-rev-1".to_string(),
        properties: EditorWorldProperties {
            display_name: "Issue 918 MCP World".to_string(),
            description: "Known world document for terrain/world MCP FSV".to_string(),
            width_tiles: 4,
            height_tiles: 4,
            initial_tile_id: "tile.ashen_grass.default".to_string(),
            initial_cliff_type: "cliff.natural".to_string(),
            random_height_seed: Some(918),
            playable_bounds: EditorPlayableBounds {
                min_tile_x: 0,
                min_tile_z: 0,
                max_tile_x_exclusive: 4,
                max_tile_z_exclusive: 4,
            },
            ..EditorWorldProperties::default()
        },
    };
    let mut document = create_editor_world_record(spec)
        .expect("issue918 sample world must validate")
        .document;
    document
        .upsert_object(crate::history::EditorObject::new(
            "unit.issue918.builder",
            "Unit",
            None,
            Transform::from_translation(Vec3::new(1.0, 0.0, 1.0)),
        ))
        .expect("issue918 sample unit must validate");
    document
}
