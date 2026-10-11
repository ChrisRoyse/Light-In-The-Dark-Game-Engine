use std::collections::BTreeMap;

use litd_engine::{
    EngineError,
    math::{Quat, Transform, Vec3},
};

use crate::{
    asset_placement::{
        PLACED_ASSET_ID_PROPERTY, PLACED_ASSET_INDEX_PROPERTY, PLACED_ASSET_RELATIVE_PATH_PROPERTY,
        PLACED_ASSET_REQUEST_PROPERTY, PLACED_ASSET_SHA256_PROPERTY, PLACED_ASSET_SOURCE_PROPERTY,
        PLACED_ASSET_TYPE_PROPERTY,
    },
    history::{EditorDocument, EditorObject, stable_fingerprint},
    prefabs::apply_prefab_instance,
    world_creation::{EditorWorldRecord, validate_world_record},
};

use super::{
    BRUSH_CONTENT_KIND_PROPERTY, BRUSH_INDEX_PROPERTY, BRUSH_SCALE_PROPERTY, BRUSH_SEED_PROPERTY,
    BRUSH_SNAP_PROPERTY, BRUSH_STAMP_ID_PROPERTY, BRUSH_TERRAIN_CELL_PROPERTY,
    BRUSH_TERRAIN_HEIGHT_PROPERTY, BRUSH_VARIANT_INDEX_PROPERTY, BRUSH_YAW_DEGREES_PROPERTY,
    BrushRange, PlacementBrushAsset, PlacementBrushContent, PlacementBrushReadback,
    PlacementBrushReport, PlacementBrushRequest, invalid_placement_brush,
    invalid_placement_brush_error, validate_stable_segment,
};

#[derive(Debug, Clone)]
struct BrushSample {
    index: usize,
    translation: Vec3,
    yaw_degrees: f32,
    uniform_scale: f32,
    variant_index: Option<usize>,
    terrain_cell_id: Option<String>,
    terrain_height: Option<f32>,
}

pub fn stamp_placement_brush_in_world_record(
    record: &mut EditorWorldRecord,
    request: &PlacementBrushRequest,
) -> Result<PlacementBrushReport, EngineError> {
    let report = stamp_placement_brush_in_document(&mut record.document, request)?;
    record.document_fingerprint = stable_fingerprint(&record.document)?;
    record.summary = format!(
        "world={} placement_brush={} placed={} fingerprint={}",
        record.spec.properties.display_name,
        request.stamp_id,
        report.placed_count,
        record.document_fingerprint
    );
    validate_world_record(record)?;
    Ok(report)
}

pub fn stamp_placement_brush_in_document(
    document: &mut EditorDocument,
    request: &PlacementBrushRequest,
) -> Result<PlacementBrushReport, EngineError> {
    request.validate()?;
    document.validate()?;
    if !document.objects.contains_key("world") {
        return invalid_placement_brush(
            "document.world",
            "document must contain root object world",
        );
    }
    let before_fingerprint = stable_fingerprint(document)?;
    let mut next = document.clone();
    let mut rng = StableRng::new(request.seed ^ hash_str(&request.stamp_id));
    let mut readback = Vec::new();

    for index in 0..request.target_count() {
        let sample = sample_point(document, request, index, &mut rng)?;
        match &request.content {
            PlacementBrushContent::Assets { variants } => {
                let variant_index = sample.variant_index.ok_or_else(|| {
                    invalid_placement_brush_error(
                        "sample.variant",
                        "asset variant was not selected",
                    )
                })?;
                let variant = variants.get(variant_index).ok_or_else(|| {
                    invalid_placement_brush_error(
                        "sample.variant",
                        format!("asset variant index {variant_index} is out of range"),
                    )
                })?;
                readback.push(insert_asset_object(&mut next, request, &sample, variant)?);
            }
            PlacementBrushContent::Prefab {
                prefab_id,
                library,
                overrides,
            } => {
                readback.push(insert_prefab_instance(
                    &mut next, request, &sample, prefab_id, library, overrides,
                )?);
            }
        }
    }

    next.validate()?;
    let after_fingerprint = stable_fingerprint(&next)?;
    if before_fingerprint == after_fingerprint {
        return invalid_placement_brush(
            "document.no_change",
            "placement brush did not change the document",
        );
    }
    *document = next;
    Ok(PlacementBrushReport {
        before_fingerprint,
        after_fingerprint,
        stamp_id: request.stamp_id.clone(),
        requested_count: request.requested_count,
        density_cap: request.density_cap,
        placed_count: readback.len(),
        capped: request.requested_count > request.density_cap,
        object_count: document.objects.len(),
        grid_snapped: request.snap.grid_size().is_some(),
        terrain_snapped: request.snap.uses_terrain(),
        readback,
    })
}

pub fn placement_brush_object_id(
    stamp_id: &str,
    index: usize,
    asset_id: &str,
) -> Result<String, EngineError> {
    validate_stable_segment("stamp_id", stamp_id)?;
    Ok(format!(
        "brush_{}_{}_{}",
        stamp_id,
        index,
        sanitize_segment("asset_id", asset_id)?
    ))
}

fn insert_asset_object(
    document: &mut EditorDocument,
    request: &PlacementBrushRequest,
    sample: &BrushSample,
    variant: &PlacementBrushAsset,
) -> Result<PlacementBrushReadback, EngineError> {
    let object_id =
        placement_brush_object_id(&request.stamp_id, sample.index, &variant.asset.asset_id)?;
    if document.objects.contains_key(&object_id) {
        return invalid_placement_brush(
            "document.objects",
            format!("placed object id collision {object_id}"),
        );
    }
    let mut object = EditorObject::new(
        object_id.clone(),
        "Doodad",
        Some("world".to_string()),
        sample_transform(sample),
    );
    object.properties = brush_properties(request, sample, "asset");
    object.properties.insert(
        PLACED_ASSET_SOURCE_PROPERTY.to_string(),
        "placement_brush".to_string(),
    );
    object.properties.insert(
        PLACED_ASSET_REQUEST_PROPERTY.to_string(),
        request.stamp_id.clone(),
    );
    object.properties.insert(
        PLACED_ASSET_INDEX_PROPERTY.to_string(),
        sample.index.to_string(),
    );
    object.properties.insert(
        PLACED_ASSET_RELATIVE_PATH_PROPERTY.to_string(),
        variant.asset.relative_path.clone(),
    );
    object.properties.insert(
        PLACED_ASSET_SHA256_PROPERTY.to_string(),
        variant.asset.sha256.clone(),
    );
    object.properties.insert(
        PLACED_ASSET_ID_PROPERTY.to_string(),
        variant.asset.asset_id.clone(),
    );
    object.properties.insert(
        PLACED_ASSET_TYPE_PROPERTY.to_string(),
        variant.asset.asset_type.clone(),
    );
    document.upsert_object(object)?;
    let stored = document.objects.get(&object_id).ok_or_else(|| {
        invalid_placement_brush_error(
            "document.readback",
            format!("placed object {object_id} missing after upsert"),
        )
    })?;
    Ok(PlacementBrushReadback {
        stamp_index: sample.index,
        object_ids: vec![object_id],
        content_kind: "asset".to_string(),
        variant_index: sample.variant_index,
        asset_id: Some(variant.asset.asset_id.clone()),
        asset_relative_path: stored
            .properties
            .get(PLACED_ASSET_RELATIVE_PATH_PROPERTY)
            .cloned(),
        prefab_id: None,
        translation: stored.transform.translation.to_array(),
        yaw_degrees: sample.yaw_degrees,
        uniform_scale: sample.uniform_scale,
        terrain_cell_id: sample.terrain_cell_id.clone(),
        terrain_height: sample.terrain_height,
    })
}

fn insert_prefab_instance(
    document: &mut EditorDocument,
    request: &PlacementBrushRequest,
    sample: &BrushSample,
    prefab_id: &str,
    library: &crate::prefabs::PrefabLibrary,
    overrides: &[crate::prefabs::PrefabPropertyOverride],
) -> Result<PlacementBrushReadback, EngineError> {
    let instance_id = format!("{}_{}", request.stamp_id, sample.index);
    let mut instance = library.instantiate(&instance_id, prefab_id, sample_transform(sample))?;
    for override_ in overrides {
        instance = instance.with_override(override_.clone());
    }
    let report = apply_prefab_instance(document, library, &instance)?;
    for id in &report.created_or_updated_object_ids {
        let object = document.objects.get_mut(id).ok_or_else(|| {
            invalid_placement_brush_error(
                "document.prefab_readback",
                format!("prefab generated object {id} missing after expansion"),
            )
        })?;
        for (key, value) in brush_properties(request, sample, "prefab") {
            object.properties.insert(key, value);
        }
    }
    document.validate()?;
    let object_ids = report.created_or_updated_object_ids;
    Ok(PlacementBrushReadback {
        stamp_index: sample.index,
        object_ids,
        content_kind: "prefab".to_string(),
        variant_index: None,
        asset_id: None,
        asset_relative_path: None,
        prefab_id: Some(prefab_id.to_string()),
        translation: sample.translation.to_array(),
        yaw_degrees: sample.yaw_degrees,
        uniform_scale: sample.uniform_scale,
        terrain_cell_id: sample.terrain_cell_id.clone(),
        terrain_height: sample.terrain_height,
    })
}

fn sample_point(
    document: &EditorDocument,
    request: &PlacementBrushRequest,
    index: usize,
    rng: &mut StableRng,
) -> Result<BrushSample, EngineError> {
    let radius = request.radius * rng.unit_f32().sqrt();
    let theta = rng.unit_f32() * std::f32::consts::TAU;
    let mut translation = Vec3::new(
        request.center.x + radius * theta.cos(),
        request.center.y,
        request.center.z + radius * theta.sin(),
    );
    if let Some(grid) = request.snap.grid_size() {
        translation.x = snap_value(translation.x, grid);
        translation.z = snap_value(translation.z, grid);
    }
    let bounds = &document.world_properties.playable_bounds;
    let min_x = bounds.min_tile_x as f32;
    let min_z = bounds.min_tile_z as f32;
    let max_x = (bounds.max_tile_x_exclusive - 1) as f32;
    let max_z = (bounds.max_tile_z_exclusive - 1) as f32;
    if request.clamp_to_playable_bounds {
        translation.x = translation.x.clamp(min_x, max_x);
        translation.z = translation.z.clamp(min_z, max_z);
    } else if translation.x < min_x
        || translation.x > max_x
        || translation.z < min_z
        || translation.z > max_z
    {
        return invalid_placement_brush(
            "sample.position",
            format!(
                "sample {},{} is outside playable tile bounds {},{}..{},{}",
                translation.x, translation.z, min_x, min_z, max_x, max_z
            ),
        );
    }

    let mut terrain_cell_id = None;
    let mut terrain_height = None;
    if request.snap.uses_terrain() {
        let cell = terrain_cell_at(document, translation.x, translation.z)?;
        translation.y = cell.height;
        terrain_cell_id = Some(cell.id.clone());
        terrain_height = Some(cell.height);
    }

    Ok(BrushSample {
        index,
        translation,
        yaw_degrees: sample_range(request.rotation_y_degrees, rng),
        uniform_scale: sample_range(request.uniform_scale, rng),
        variant_index: variant_index(&request.content, rng)?,
        terrain_cell_id,
        terrain_height,
    })
}

fn variant_index(
    content: &PlacementBrushContent,
    rng: &mut StableRng,
) -> Result<Option<usize>, EngineError> {
    let PlacementBrushContent::Assets { variants } = content else {
        return Ok(None);
    };
    let total = variants
        .iter()
        .map(|variant| u64::from(variant.weight))
        .sum::<u64>();
    if total == 0 {
        return invalid_placement_brush("asset.weight", "total variant weight must be non-zero");
    }
    let mut ticket = rng.next_u64() % total;
    for (index, variant) in variants.iter().enumerate() {
        let weight = u64::from(variant.weight);
        if ticket < weight {
            return Ok(Some(index));
        }
        ticket -= weight;
    }
    invalid_placement_brush("asset.weight", "weighted variant selection fell through")
}

fn brush_properties(
    request: &PlacementBrushRequest,
    sample: &BrushSample,
    content_kind: &str,
) -> BTreeMap<String, String> {
    let mut properties = BTreeMap::from([
        (
            BRUSH_STAMP_ID_PROPERTY.to_string(),
            request.stamp_id.clone(),
        ),
        (BRUSH_INDEX_PROPERTY.to_string(), sample.index.to_string()),
        (BRUSH_SEED_PROPERTY.to_string(), request.seed.to_string()),
        (
            BRUSH_CONTENT_KIND_PROPERTY.to_string(),
            content_kind.to_string(),
        ),
        (
            BRUSH_YAW_DEGREES_PROPERTY.to_string(),
            format!("{:.6}", sample.yaw_degrees),
        ),
        (
            BRUSH_SCALE_PROPERTY.to_string(),
            format!("{:.6}", sample.uniform_scale),
        ),
        (
            BRUSH_SNAP_PROPERTY.to_string(),
            request.snap.label().to_string(),
        ),
    ]);
    if let Some(index) = sample.variant_index {
        properties.insert(BRUSH_VARIANT_INDEX_PROPERTY.to_string(), index.to_string());
    }
    if let Some(cell) = &sample.terrain_cell_id {
        properties.insert(BRUSH_TERRAIN_CELL_PROPERTY.to_string(), cell.clone());
    }
    if let Some(height) = sample.terrain_height {
        properties.insert(
            BRUSH_TERRAIN_HEIGHT_PROPERTY.to_string(),
            format!("{height:.6}"),
        );
    }
    properties
}

fn sample_transform(sample: &BrushSample) -> Transform {
    Transform::from_trs(
        sample.translation,
        Quat::from_rotation_y(sample.yaw_degrees.to_radians()),
        Vec3::splat(sample.uniform_scale),
    )
}

fn terrain_cell_at(
    document: &EditorDocument,
    x: f32,
    z: f32,
) -> Result<&crate::terrain_bulk_ops::EditorTerrainCell, EngineError> {
    let tile_x = x.floor() as i32;
    let tile_z = z.floor() as i32;
    document
        .terrain_cells
        .values()
        .find(|cell| cell.tile_x == tile_x && cell.tile_z == tile_z)
        .ok_or_else(|| {
            invalid_placement_brush_error(
                "terrain_cell",
                format!("missing terrain cell at tile {tile_x},{tile_z}"),
            )
        })
}

fn sample_range(range: BrushRange, rng: &mut StableRng) -> f32 {
    if (range.max - range.min).abs() <= f32::EPSILON {
        range.min
    } else {
        range.min + (range.max - range.min) * rng.unit_f32()
    }
}

fn snap_value(value: f32, step: f32) -> f32 {
    (value / step).round() * step
}

fn sanitize_segment(field: &str, value: &str) -> Result<String, EngineError> {
    if value.trim().is_empty() {
        return invalid_placement_brush(field, "id segment must be non-empty");
    }
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if sanitized.is_empty() {
        return invalid_placement_brush(field, "id segment sanitized to empty");
    }
    Ok(sanitized)
}

fn hash_str(value: &str) -> u64 {
    litd_hash::fnv1a64(value.as_bytes())
}

struct StableRng {
    state: u64,
}

impl StableRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
        value ^ (value >> 31)
    }

    fn unit_f32(&mut self) -> f32 {
        let bits = (self.next_u64() >> 40) as u32;
        bits as f32 / 16_777_216.0
    }
}
