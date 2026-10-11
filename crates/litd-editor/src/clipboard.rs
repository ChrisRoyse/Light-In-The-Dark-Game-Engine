//! Cross-project editor clipboard payloads and paste transactions.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use litd_engine::{
    EngineError,
    math::{Quat, Transform, Vec3},
};

use crate::{
    asset_products::{AssetProduct, MODEL_PRODUCT_ID_PROPERTY},
    history::{
        EditorDocument, EditorObject, EditorRegion, EditorTerrainStamp, SelectionSet,
        stable_fingerprint,
    },
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardPayload {
    pub source_calyx_document_id: String,
    pub objects: Vec<EditorObject>,
    #[serde(default)]
    pub regions: Vec<EditorRegion>,
    #[serde(default)]
    pub terrain_stamps: Vec<EditorTerrainStamp>,
    #[serde(default)]
    pub asset_products: Vec<AssetProduct>,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ClipboardPlacement {
    InPlace,
    AtCursor { cursor: Vec3 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipboardTransform {
    Identity,
    MirrorHorizontal,
    MirrorVertical,
    Rotate90Clockwise,
    Rotate90CounterClockwise,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardPasteOptions {
    pub id_suffix: String,
    pub placement: ClipboardPlacement,
    pub transform: ClipboardTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardPasteReport {
    pub source_calyx_document_id: String,
    pub destination_calyx_document_id: String,
    pub id_map: BTreeMap<String, String>,
    pub pasted_object_ids: Vec<String>,
    pub pasted_region_ids: Vec<String>,
    pub pasted_terrain_stamp_ids: Vec<String>,
    pub imported_asset_product_ids: Vec<String>,
    pub dependency_count: usize,
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub placement: ClipboardPlacement,
    pub transform: ClipboardTransform,
}

impl ClipboardPasteOptions {
    pub fn in_place(id_suffix: impl Into<String>) -> Self {
        Self {
            id_suffix: id_suffix.into(),
            placement: ClipboardPlacement::InPlace,
            transform: ClipboardTransform::Identity,
        }
    }
}

pub fn copy_selection(
    document: &EditorDocument,
    selection: &SelectionSet,
) -> Result<ClipboardPayload, EngineError> {
    document.validate()?;
    if selection.selected_ids.is_empty() {
        return invalid_clipboard("selection", "selection must not be empty");
    }

    let mut objects = Vec::new();
    let mut regions = Vec::new();
    let mut terrain_stamps = Vec::new();
    let mut product_ids = BTreeSet::new();

    for id in &selection.selected_ids {
        if let Some(object) = document.objects.get(id) {
            collect_object_products(object, &mut product_ids);
            objects.push(object.clone());
        } else if let Some(region) = document.regions.get(id) {
            regions.push(region.clone());
        } else if let Some(stamp) = document.terrain_stamps.get(id) {
            terrain_stamps.push(stamp.clone());
        } else {
            return invalid_clipboard("selection", format!("missing selected id {id}"));
        }
    }

    let mut asset_products = Vec::new();
    for product_id in product_ids {
        let product = document.asset_products.get(&product_id).ok_or_else(|| {
            invalid_clipboard_error(
                "asset_products",
                format!("selected object references missing product {product_id}"),
            )
        })?;
        product.validate()?;
        asset_products.push(product.clone());
    }

    let fingerprint = stable_fingerprint(&(
        &document.calyx_document_id,
        &objects,
        &regions,
        &terrain_stamps,
        &asset_products,
    ))?;
    Ok(ClipboardPayload {
        source_calyx_document_id: document.calyx_document_id.clone(),
        objects,
        regions,
        terrain_stamps,
        asset_products,
        fingerprint,
    })
}

pub fn paste_clipboard(
    document: &mut EditorDocument,
    clipboard: &ClipboardPayload,
    suffix: &str,
) -> Result<Vec<String>, EngineError> {
    let report =
        paste_clipboard_with_options(document, clipboard, ClipboardPasteOptions::in_place(suffix))?;
    Ok(report.pasted_object_ids)
}

pub fn paste_clipboard_with_options(
    document: &mut EditorDocument,
    clipboard: &ClipboardPayload,
    options: ClipboardPasteOptions,
) -> Result<ClipboardPasteReport, EngineError> {
    validate_clipboard(clipboard)?;
    validate_options(&options)?;
    document.validate()?;

    let before_fingerprint = stable_fingerprint(document)?;
    let mut next = document.clone();
    let mut imported_asset_product_ids = Vec::new();
    for product in &clipboard.asset_products {
        product.validate()?;
        match next.asset_products.get(&product.product_id) {
            Some(existing) if existing == product => {}
            Some(_) => {
                return invalid_clipboard(
                    "asset_products.conflict",
                    format!("destination has conflicting product {}", product.product_id),
                );
            }
            None => {
                next.asset_products
                    .insert(product.product_id.clone(), product.clone());
                imported_asset_product_ids.push(product.product_id.clone());
            }
        }
    }

    let anchor = clipboard_anchor(clipboard);
    let offset = match options.placement {
        ClipboardPlacement::InPlace => Vec3::ZERO,
        ClipboardPlacement::AtCursor { cursor } => {
            if !cursor.is_finite() {
                return invalid_clipboard("placement.cursor", "cursor must be finite");
            }
            cursor - anchor
        }
    };
    let mut used_ids = all_document_ids(&next);
    let mut id_map = BTreeMap::new();
    reserve_ids(clipboard, &options.id_suffix, &mut used_ids, &mut id_map)?;

    let mut pasted_object_ids = Vec::new();
    for object in &clipboard.objects {
        let mut copy = object.clone();
        copy.id = id_map[&object.id].clone();
        if let Some(parent) = &copy.parent {
            copy.parent = id_map.get(parent).cloned().or_else(|| Some(parent.clone()));
        }
        copy.transform = transform_model(copy.transform, anchor, offset, options.transform);
        next.upsert_object(copy.clone())?;
        pasted_object_ids.push(copy.id);
    }

    let mut pasted_region_ids = Vec::new();
    for region in &clipboard.regions {
        let mut copy = region.clone();
        copy.id = id_map[&region.id].clone();
        copy.center = transform_point(copy.center, anchor, offset, options.transform);
        next.upsert_region(copy.clone())?;
        pasted_region_ids.push(copy.id);
    }

    let mut pasted_terrain_stamp_ids = Vec::new();
    for stamp in &clipboard.terrain_stamps {
        let mut copy = stamp.clone();
        copy.id = id_map[&stamp.id].clone();
        copy.transform = transform_model(copy.transform, anchor, offset, options.transform);
        next.upsert_terrain_stamp(copy.clone())?;
        pasted_terrain_stamp_ids.push(copy.id);
    }

    next.validate()?;
    let after_fingerprint = stable_fingerprint(&next)?;
    *document = next;

    Ok(ClipboardPasteReport {
        source_calyx_document_id: clipboard.source_calyx_document_id.clone(),
        destination_calyx_document_id: document.calyx_document_id.clone(),
        id_map,
        pasted_object_ids,
        pasted_region_ids,
        pasted_terrain_stamp_ids,
        imported_asset_product_ids,
        dependency_count: clipboard
            .asset_products
            .iter()
            .map(|product| product.dependencies.len())
            .sum(),
        before_fingerprint,
        after_fingerprint,
        placement: options.placement,
        transform: options.transform,
    })
}

fn validate_clipboard(clipboard: &ClipboardPayload) -> Result<(), EngineError> {
    if clipboard.source_calyx_document_id.trim().is_empty() {
        return invalid_clipboard("source", "source document id must be non-empty");
    }
    if clipboard.objects.is_empty()
        && clipboard.regions.is_empty()
        && clipboard.terrain_stamps.is_empty()
    {
        return invalid_clipboard("payload", "clipboard payload must contain editor data");
    }
    let expected = stable_fingerprint(&(
        &clipboard.source_calyx_document_id,
        &clipboard.objects,
        &clipboard.regions,
        &clipboard.terrain_stamps,
        &clipboard.asset_products,
    ))?;
    if clipboard.fingerprint != expected {
        return invalid_clipboard("fingerprint", "clipboard payload fingerprint mismatch");
    }
    let product_ids = clipboard
        .asset_products
        .iter()
        .map(|product| product.product_id.as_str())
        .collect::<BTreeSet<_>>();
    for object in &clipboard.objects {
        object_referenced_product(object, &product_ids)?;
    }
    Ok(())
}

fn validate_options(options: &ClipboardPasteOptions) -> Result<(), EngineError> {
    if options.id_suffix.trim().is_empty() {
        return invalid_clipboard("suffix", "id_suffix must be non-empty");
    }
    Ok(())
}

fn collect_object_products(object: &EditorObject, product_ids: &mut BTreeSet<String>) {
    if let Some(product_id) = object.properties.get(MODEL_PRODUCT_ID_PROPERTY) {
        product_ids.insert(product_id.clone());
    }
}

fn object_referenced_product(
    object: &EditorObject,
    product_ids: &BTreeSet<&str>,
) -> Result<(), EngineError> {
    if let Some(product_id) = object.properties.get(MODEL_PRODUCT_ID_PROPERTY)
        && !product_ids.contains(product_id.as_str())
    {
        return invalid_clipboard(
            "asset_products.missing",
            format!(
                "object {} references missing product {product_id}",
                object.id
            ),
        );
    }
    Ok(())
}

fn reserve_ids(
    clipboard: &ClipboardPayload,
    suffix: &str,
    used_ids: &mut BTreeSet<String>,
    id_map: &mut BTreeMap<String, String>,
) -> Result<(), EngineError> {
    for id in clipboard
        .objects
        .iter()
        .map(|object| object.id.as_str())
        .chain(clipboard.regions.iter().map(|region| region.id.as_str()))
        .chain(
            clipboard
                .terrain_stamps
                .iter()
                .map(|stamp| stamp.id.as_str()),
        )
    {
        let base = format!("{id}{suffix}");
        let resolved = unique_id(&base, used_ids)?;
        used_ids.insert(resolved.clone());
        id_map.insert(id.to_string(), resolved);
    }
    Ok(())
}

fn unique_id(base: &str, used_ids: &BTreeSet<String>) -> Result<String, EngineError> {
    if base.trim().is_empty() {
        return invalid_clipboard("id", "resolved id must be non-empty");
    }
    if !used_ids.contains(base) {
        return Ok(base.to_string());
    }
    for index in 2..10_000 {
        let candidate = format!("{base}_{index}");
        if !used_ids.contains(&candidate) {
            return Ok(candidate);
        }
    }
    invalid_clipboard("id", format!("could not reconcile duplicate id {base}"))
}

fn all_document_ids(document: &EditorDocument) -> BTreeSet<String> {
    document
        .objects
        .keys()
        .chain(document.regions.keys())
        .chain(document.terrain_stamps.keys())
        .cloned()
        .collect()
}

fn clipboard_anchor(clipboard: &ClipboardPayload) -> Vec3 {
    let mut sum = Vec3::ZERO;
    let mut count = 0.0_f32;
    for object in &clipboard.objects {
        sum += object.transform.translation;
        count += 1.0;
    }
    for region in &clipboard.regions {
        sum += region.center;
        count += 1.0;
    }
    for stamp in &clipboard.terrain_stamps {
        sum += stamp.transform.translation;
        count += 1.0;
    }
    if count > 0.0 { sum / count } else { Vec3::ZERO }
}

fn transform_model(
    mut transform: Transform,
    anchor: Vec3,
    offset: Vec3,
    clipboard_transform: ClipboardTransform,
) -> Transform {
    transform.translation =
        transform_point(transform.translation, anchor, offset, clipboard_transform);
    match clipboard_transform {
        ClipboardTransform::Identity => {}
        ClipboardTransform::MirrorHorizontal => {
            transform.scale.x = -transform.scale.x;
        }
        ClipboardTransform::MirrorVertical => {
            transform.scale.z = -transform.scale.z;
        }
        ClipboardTransform::Rotate90Clockwise => {
            transform.rotation =
                Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2) * transform.rotation;
        }
        ClipboardTransform::Rotate90CounterClockwise => {
            transform.rotation =
                Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * transform.rotation;
        }
    }
    transform
}

fn transform_point(
    point: Vec3,
    anchor: Vec3,
    offset: Vec3,
    clipboard_transform: ClipboardTransform,
) -> Vec3 {
    let local = point - anchor;
    let transformed = match clipboard_transform {
        ClipboardTransform::Identity => local,
        ClipboardTransform::MirrorHorizontal => Vec3::new(-local.x, local.y, local.z),
        ClipboardTransform::MirrorVertical => Vec3::new(local.x, local.y, -local.z),
        ClipboardTransform::Rotate90Clockwise => Vec3::new(local.z, local.y, -local.x),
        ClipboardTransform::Rotate90CounterClockwise => Vec3::new(-local.z, local.y, local.x),
    };
    anchor + transformed + offset
}

fn invalid_clipboard<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_clipboard_error(field, reason))
}

fn invalid_clipboard_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.clipboard.{}", field.into()),
        reason: reason.into(),
    }
}
