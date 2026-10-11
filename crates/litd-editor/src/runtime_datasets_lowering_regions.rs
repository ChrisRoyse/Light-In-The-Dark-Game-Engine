use std::collections::BTreeMap;

use litd_engine::EngineError;
use litd_engine::game_runtime::{LoweredRegion, LoweredRegionSet, lower_region_set};

use crate::history::EditorDocument;

use super::lower_error;

/// #1856: lower every `EditorRegion` into a fingerprinted region set and a name/id
/// handle map. World center±half_extents floor to inclusive integer cell rects
/// (same cell convention as resource-node placement).
pub(super) fn lower_document_regions(
    document: &EditorDocument,
) -> Result<(Option<LoweredRegionSet>, BTreeMap<String, u32>), EngineError> {
    let mut regions_in: Vec<_> = document.regions.values().cloned().collect();
    regions_in.sort_by(|a, b| a.id.cmp(&b.id));
    if regions_in.is_empty() {
        return Ok((None, BTreeMap::new()));
    }
    let mut handles = BTreeMap::new();
    let mut lowered = Vec::with_capacity(regions_in.len());
    for (index, region) in regions_in.iter().enumerate() {
        if region.id.trim().is_empty() || region.name.trim().is_empty() {
            return lower_error("regions", "region id and name must be non-empty");
        }
        let region_id =
            u32::try_from(index + 1).map_err(|_| EngineError::GamePackageInvalidConfig {
                field: "litd_editor.runtime_datasets_lowering.regions".to_string(),
                reason: "region count exceeds u32".to_string(),
            })?;
        let min_x = (region.center.x - region.half_extents.x).floor() as i32;
        let max_x = (region.center.x + region.half_extents.x).floor() as i32;
        let min_z = (region.center.z - region.half_extents.z).floor() as i32;
        let max_z = (region.center.z + region.half_extents.z).floor() as i32;
        if min_x > max_x || min_z > max_z {
            return lower_error(
                &region.id,
                format!(
                    "region {} collapses to inverted cell rect min=({min_x},{min_z}) \
                     max=({max_x},{max_z})",
                    region.name
                ),
            );
        }
        handles.insert(region.id.clone(), region_id);
        handles.insert(region.name.clone(), region_id);
        lowered.push(LoweredRegion {
            region_id,
            name: region.name.clone(),
            min_x,
            min_z,
            max_x,
            max_z,
        });
    }
    Ok((Some(lower_region_set(lowered)?), handles))
}
