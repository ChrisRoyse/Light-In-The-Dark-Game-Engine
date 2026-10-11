use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;

use crate::{
    history::{EditorDocument, stable_fingerprint},
    terrain_bulk_types::{invalid_bulk, invalid_bulk_error, validate_cliff_level},
};

pub use crate::terrain_bulk_types::{
    EditorTerrainCell, OBJECT_TYPE_ID_PROPERTY, TerrainBulkCatalog, TerrainBulkOperation,
    TerrainBulkOperationReport, TerrainBulkScope, terrain_bulk_summary, validate_terrain_cells,
};

pub fn replace_tiles(
    document: &mut EditorDocument,
    catalog: &TerrainBulkCatalog,
    old_tile_id: &str,
    new_tile_id: &str,
    scope: &TerrainBulkScope,
) -> Result<TerrainBulkOperationReport, EngineError> {
    catalog.validate()?;
    require_distinct("tile", old_tile_id, new_tile_id)?;
    catalog.tile_variation_count(old_tile_id)?;
    let new_variation_count = catalog.tile_variation_count(new_tile_id)?;
    let before = document.clone();
    let mut after = before.clone();
    let affected_ids = scoped_cell_ids(&after, scope)?
        .into_iter()
        .filter(|id| after.terrain_cells[id].tile_id == old_tile_id)
        .collect::<Vec<_>>();
    require_matches(TerrainBulkOperation::ReplaceTiles, &affected_ids)?;
    for id in &affected_ids {
        after.terrain_cells.get_mut(id).expect("known id").tile_id = new_tile_id.to_string();
    }
    retarget_matching_variations(&mut after, &affected_ids, new_tile_id, new_variation_count);
    finish_bulk_operation(
        document,
        before,
        after,
        TerrainBulkOperation::ReplaceTiles,
        scope,
        affected_ids,
    )
}

pub fn replace_cliff_type(
    document: &mut EditorDocument,
    catalog: &TerrainBulkCatalog,
    old_cliff_type: &str,
    new_cliff_type: &str,
    scope: &TerrainBulkScope,
) -> Result<TerrainBulkOperationReport, EngineError> {
    catalog.validate()?;
    require_distinct("cliff_type", old_cliff_type, new_cliff_type)?;
    require_in_set("catalog.cliff_type", &catalog.cliff_types, old_cliff_type)?;
    require_in_set("catalog.cliff_type", &catalog.cliff_types, new_cliff_type)?;
    let before = document.clone();
    let mut after = before.clone();
    let affected_ids = scoped_cell_ids(&after, scope)?
        .into_iter()
        .filter(|id| after.terrain_cells[id].cliff_type == old_cliff_type)
        .collect::<Vec<_>>();
    require_matches(TerrainBulkOperation::ReplaceCliffType, &affected_ids)?;
    for id in &affected_ids {
        after
            .terrain_cells
            .get_mut(id)
            .expect("known id")
            .cliff_type = new_cliff_type.to_string();
    }
    finish_bulk_operation(
        document,
        before,
        after,
        TerrainBulkOperation::ReplaceCliffType,
        scope,
        affected_ids,
    )
}

pub fn replace_doodads(
    document: &mut EditorDocument,
    catalog: &TerrainBulkCatalog,
    old_type_id: &str,
    new_type_id: &str,
    scope: &TerrainBulkScope,
) -> Result<TerrainBulkOperationReport, EngineError> {
    replace_objects(ReplaceObjectsArgs {
        document,
        catalog,
        object_kind: "Doodad",
        valid_types: &catalog.doodad_types,
        old_type_id,
        new_type_id,
        scope,
        operation: TerrainBulkOperation::ReplaceDoodads,
    })
}

pub fn replace_units(
    document: &mut EditorDocument,
    catalog: &TerrainBulkCatalog,
    old_type_id: &str,
    new_type_id: &str,
    scope: &TerrainBulkScope,
) -> Result<TerrainBulkOperationReport, EngineError> {
    replace_objects(ReplaceObjectsArgs {
        document,
        catalog,
        object_kind: "Unit",
        valid_types: &catalog.unit_types,
        old_type_id,
        new_type_id,
        scope,
        operation: TerrainBulkOperation::ReplaceUnits,
    })
}

pub fn adjust_cliff_levels(
    document: &mut EditorDocument,
    delta: i16,
    scope: &TerrainBulkScope,
) -> Result<TerrainBulkOperationReport, EngineError> {
    if delta == 0 {
        return invalid_bulk("cliff_level.delta", "delta must be non-zero");
    }
    let before = document.clone();
    let mut after = before.clone();
    let affected_ids = scoped_cell_ids(&after, scope)?;
    require_matches(TerrainBulkOperation::AdjustCliffLevels, &affected_ids)?;
    for id in &affected_ids {
        let cell = after.terrain_cells.get_mut(id).expect("known id");
        cell.cliff_level = cell
            .cliff_level
            .checked_add(delta)
            .ok_or_else(|| invalid_bulk_error("cliff_level", "cliff level overflowed"))?;
        validate_cliff_level(cell.cliff_level)?;
    }
    finish_bulk_operation(
        document,
        before,
        after,
        TerrainBulkOperation::AdjustCliffLevels,
        scope,
        affected_ids,
    )
}

pub fn reset_height_field(
    document: &mut EditorDocument,
    base_height: f32,
    scope: &TerrainBulkScope,
) -> Result<TerrainBulkOperationReport, EngineError> {
    if !base_height.is_finite() {
        return invalid_bulk("height.base", "base height must be finite");
    }
    let before = document.clone();
    let mut after = before.clone();
    let affected_ids = scoped_cell_ids(&after, scope)?;
    require_matches(TerrainBulkOperation::ResetHeightField, &affected_ids)?;
    for id in &affected_ids {
        after.terrain_cells.get_mut(id).expect("known id").height = base_height;
    }
    finish_bulk_operation(
        document,
        before,
        after,
        TerrainBulkOperation::ResetHeightField,
        scope,
        affected_ids,
    )
}

struct ReplaceObjectsArgs<'a> {
    document: &'a mut EditorDocument,
    catalog: &'a TerrainBulkCatalog,
    object_kind: &'a str,
    valid_types: &'a BTreeSet<String>,
    old_type_id: &'a str,
    new_type_id: &'a str,
    scope: &'a TerrainBulkScope,
    operation: TerrainBulkOperation,
}

fn replace_objects(
    args: ReplaceObjectsArgs<'_>,
) -> Result<TerrainBulkOperationReport, EngineError> {
    let ReplaceObjectsArgs {
        document,
        catalog,
        object_kind,
        valid_types,
        old_type_id,
        new_type_id,
        scope,
        operation,
    } = args;
    catalog.validate()?;
    require_distinct("object.type", old_type_id, new_type_id)?;
    require_in_set("catalog.object_type", valid_types, old_type_id)?;
    require_in_set("catalog.object_type", valid_types, new_type_id)?;
    let before = document.clone();
    let mut after = before.clone();
    let candidate_ids = scoped_object_ids(&after, scope)?;
    for id in &candidate_ids {
        let object = &after.objects[id];
        if object.kind == object_kind && !object.properties.contains_key(OBJECT_TYPE_ID_PROPERTY) {
            return invalid_bulk(
                "object.type_id",
                format!(
                    "{} {} is missing {OBJECT_TYPE_ID_PROPERTY}",
                    object.kind, object.id
                ),
            );
        }
    }
    let affected_ids = candidate_ids
        .into_iter()
        .filter(|id| {
            let object = &after.objects[id];
            object.kind == object_kind
                && object
                    .properties
                    .get(OBJECT_TYPE_ID_PROPERTY)
                    .is_some_and(|value| value == old_type_id)
        })
        .collect::<Vec<_>>();
    require_matches(operation, &affected_ids)?;
    for id in &affected_ids {
        after
            .objects
            .get_mut(id)
            .expect("known id")
            .properties
            .insert(OBJECT_TYPE_ID_PROPERTY.to_string(), new_type_id.to_string());
    }
    finish_bulk_operation(document, before, after, operation, scope, affected_ids)
}

fn finish_bulk_operation(
    document: &mut EditorDocument,
    before: EditorDocument,
    after: EditorDocument,
    operation: TerrainBulkOperation,
    scope: &TerrainBulkScope,
    affected_ids: Vec<String>,
) -> Result<TerrainBulkOperationReport, EngineError> {
    before.validate()?;
    after.validate()?;
    let before_fingerprint = stable_fingerprint(&before)?;
    let after_fingerprint = stable_fingerprint(&after)?;
    if before_fingerprint == after_fingerprint {
        return invalid_bulk(
            "operation.no_change",
            "bulk operation did not change the document",
        );
    }
    let report = TerrainBulkOperationReport {
        operation,
        scope: scope.label(),
        matched_count: affected_ids.len(),
        changed_count: affected_ids.len(),
        skipped_count: 0,
        affected_ids,
        before_fingerprint,
        after_fingerprint,
        before_summary: terrain_bulk_summary(&before),
        after_summary: terrain_bulk_summary(&after),
    };
    *document = after;
    Ok(report)
}

fn retarget_matching_variations(
    document: &mut EditorDocument,
    cell_ids: &[String],
    new_tile_id: &str,
    new_variation_count: u16,
) {
    let coords = cell_ids
        .iter()
        .filter_map(|id| {
            document
                .terrain_cells
                .get(id)
                .map(|cell| (cell.tile_x, cell.tile_z))
        })
        .collect::<BTreeSet<_>>();
    for variation in document.terrain_tile_variations.values_mut() {
        if coords.contains(&(variation.tile_x, variation.tile_z)) {
            variation.tileset_id = new_tile_id.to_string();
            variation.variation_count = new_variation_count;
            variation.variation_index = variation.deterministic_index_for_seed(variation.seed);
        }
    }
}

fn scoped_cell_ids(
    document: &EditorDocument,
    scope: &TerrainBulkScope,
) -> Result<Vec<String>, EngineError> {
    match scope {
        TerrainBulkScope::WholeMap => Ok(document.terrain_cells.keys().cloned().collect()),
        TerrainBulkScope::Selection {
            terrain_cell_ids, ..
        } => {
            validate_ids_exist(
                "scope.terrain_cell",
                terrain_cell_ids,
                &document.terrain_cells,
            )?;
            Ok(terrain_cell_ids.iter().cloned().collect())
        }
    }
}

fn scoped_object_ids(
    document: &EditorDocument,
    scope: &TerrainBulkScope,
) -> Result<Vec<String>, EngineError> {
    match scope {
        TerrainBulkScope::WholeMap => Ok(document.objects.keys().cloned().collect()),
        TerrainBulkScope::Selection { object_ids, .. } => {
            validate_ids_exist("scope.object", object_ids, &document.objects)?;
            Ok(object_ids.iter().cloned().collect())
        }
    }
}

fn validate_ids_exist<T>(
    field: &str,
    ids: &BTreeSet<String>,
    values: &BTreeMap<String, T>,
) -> Result<(), EngineError> {
    for id in ids {
        if !values.contains_key(id) {
            return invalid_bulk(field, format!("missing selected id {id}"));
        }
    }
    Ok(())
}

fn require_distinct(field: &str, old: &str, new: &str) -> Result<(), EngineError> {
    if old.trim().is_empty() || new.trim().is_empty() {
        return invalid_bulk(field, "replacement ids must be non-empty");
    }
    if old == new {
        return invalid_bulk(field, "replacement ids must differ");
    }
    Ok(())
}

fn require_in_set(field: &str, set: &BTreeSet<String>, value: &str) -> Result<(), EngineError> {
    if !set.contains(value) {
        return invalid_bulk(field, format!("missing target type {value}"));
    }
    Ok(())
}

fn require_matches(
    operation: TerrainBulkOperation,
    affected_ids: &[String],
) -> Result<(), EngineError> {
    if affected_ids.is_empty() {
        return invalid_bulk(
            "operation.matches",
            format!("{operation:?} matched no source records in scope"),
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "terrain_bulk_ops_tests.rs"]
mod tests;
