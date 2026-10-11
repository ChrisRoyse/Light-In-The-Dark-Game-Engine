use std::collections::{BTreeMap, BTreeSet};

use litd_engine::{
    EngineError,
    math::{Quat, Vec3},
};

use super::{
    ViewportAlignReference, ViewportAxis, ViewportObjectTransformReadback,
    ViewportTransformOperation, ViewportTransformReport, invalid_viewport_transform,
    invalid_viewport_transform_error,
};
use crate::history::{EditorDocument, SelectionSet, stable_fingerprint};

pub(super) fn transformed_document(
    document: &EditorDocument,
    selection: &SelectionSet,
    operation: &ViewportTransformOperation,
) -> Result<(EditorDocument, ViewportTransformReport), EngineError> {
    document.validate()?;
    let selected_ids = selected_object_ids(document, selection)?;
    let before_fingerprint = stable_fingerprint(document)?;
    let mut next = document.clone();
    let mut changed_ids = selected_ids.clone();
    let mut created_ids = Vec::new();
    let mut duplicate_id_map = BTreeMap::new();

    match operation {
        ViewportTransformOperation::Move { delta, snap } => {
            snap.validate()?;
            if !delta.is_finite() {
                return invalid_viewport_transform("move.delta", "delta must be finite");
            }
            let applied = snap.snap_translation_delta(*delta);
            if applied == Vec3::ZERO {
                return invalid_viewport_transform("move.delta", "snapped delta must not be zero");
            }
            for id in &selected_ids {
                let object = next
                    .objects
                    .get_mut(id)
                    .expect("selected ids were validated");
                object.transform.translation += applied;
            }
        }
        ViewportTransformOperation::Rotate {
            axis,
            degrees,
            pivot,
            snap,
        } => {
            snap.validate()?;
            if !degrees.is_finite() || !pivot.is_finite() {
                return invalid_viewport_transform("rotate", "degrees and pivot must be finite");
            }
            let applied_degrees = snap.snap_rotation_degrees(*degrees);
            if applied_degrees == 0.0 {
                return invalid_viewport_transform(
                    "rotate.degrees",
                    "snapped rotation must not be zero",
                );
            }
            let rotation = axis.rotation(applied_degrees.to_radians());
            for id in &selected_ids {
                let object = next
                    .objects
                    .get_mut(id)
                    .expect("selected ids were validated");
                let local = object.transform.translation - *pivot;
                object.transform.translation = *pivot + rotation * local;
                object.transform.rotation =
                    normalize_rotation(rotation * object.transform.rotation)?;
            }
        }
        ViewportTransformOperation::Scale {
            factors,
            pivot,
            snap,
        } => {
            snap.validate()?;
            if !factors.is_finite() || !pivot.is_finite() {
                return invalid_viewport_transform("scale", "factors and pivot must be finite");
            }
            let applied = Vec3::new(
                snap.snap_scale_factor(factors.x),
                snap.snap_scale_factor(factors.y),
                snap.snap_scale_factor(factors.z),
            );
            validate_scale_factors(applied)?;
            for id in &selected_ids {
                let object = next
                    .objects
                    .get_mut(id)
                    .expect("selected ids were validated");
                object.transform.translation =
                    *pivot + (object.transform.translation - *pivot) * applied;
                object.transform.scale *= applied;
                validate_nonzero_scale(object.transform.scale)?;
            }
        }
        ViewportTransformOperation::Align {
            axis,
            reference,
            coordinate,
        } => {
            let target = align_target(document, &selected_ids, *axis, *reference, *coordinate)?;
            for id in &selected_ids {
                let object = next
                    .objects
                    .get_mut(id)
                    .expect("selected ids were validated");
                axis.set_component(&mut object.transform.translation, target);
            }
        }
        ViewportTransformOperation::Distribute { axis } => {
            distribute_selection(&mut next, &selected_ids, *axis)?;
        }
        ViewportTransformOperation::Duplicate {
            id_suffix,
            offset,
            snap,
        } => {
            snap.validate()?;
            validate_id_suffix(id_suffix)?;
            if !offset.is_finite() {
                return invalid_viewport_transform("duplicate.offset", "offset must be finite");
            }
            let applied_offset = snap.snap_translation_delta(*offset);
            if applied_offset == Vec3::ZERO {
                return invalid_viewport_transform(
                    "duplicate.offset",
                    "snapped offset must not be zero",
                );
            }
            let duplicate_report =
                duplicate_objects(&mut next, &selected_ids, id_suffix, applied_offset)?;
            changed_ids = duplicate_report.created_ids.clone();
            created_ids = duplicate_report.created_ids;
            duplicate_id_map = duplicate_report.id_map;
        }
    }

    next.validate()?;
    let after_fingerprint = stable_fingerprint(&next)?;
    if before_fingerprint == after_fingerprint {
        return invalid_viewport_transform(
            "document.no_change",
            format!("{} did not change the document", operation.label()),
        );
    }
    let readback = object_readback(&next, &changed_ids)?;
    Ok((
        next,
        ViewportTransformReport {
            operation: operation.label().to_string(),
            selected_object_ids: selected_ids,
            changed_object_ids: changed_ids,
            created_object_ids: created_ids,
            duplicate_id_map,
            before_fingerprint,
            after_fingerprint,
            history_recorded: false,
            readback,
        },
    ))
}

fn selected_object_ids(
    document: &EditorDocument,
    selection: &SelectionSet,
) -> Result<Vec<String>, EngineError> {
    if selection.selected_ids.is_empty() {
        return invalid_viewport_transform("selection", "selection must not be empty");
    }
    let mut ids = Vec::new();
    for id in &selection.selected_ids {
        if id == "world" {
            return invalid_viewport_transform(
                "selection",
                "root object world cannot be transformed",
            );
        }
        if !document.objects.contains_key(id) {
            return invalid_viewport_transform(
                "selection",
                format!("selected id {id} is not an object"),
            );
        }
        ids.push(id.clone());
    }
    Ok(ids)
}

fn align_target(
    document: &EditorDocument,
    ids: &[String],
    axis: ViewportAxis,
    reference: ViewportAlignReference,
    coordinate: Option<f32>,
) -> Result<f32, EngineError> {
    let values = ids
        .iter()
        .map(|id| axis.component(document.objects[id].transform.translation))
        .collect::<Vec<_>>();
    let target = match reference {
        ViewportAlignReference::Minimum => values
            .iter()
            .fold(f32::INFINITY, |acc, value| acc.min(*value)),
        ViewportAlignReference::Maximum => values
            .iter()
            .fold(f32::NEG_INFINITY, |acc, value| acc.max(*value)),
        ViewportAlignReference::Average => values.iter().sum::<f32>() / values.len() as f32,
        ViewportAlignReference::Coordinate => coordinate.ok_or_else(|| {
            invalid_viewport_transform_error("align.coordinate", "coordinate is required")
        })?,
    };
    if !target.is_finite() {
        return invalid_viewport_transform("align.target", "target coordinate must be finite");
    }
    Ok(target)
}

fn distribute_selection(
    document: &mut EditorDocument,
    ids: &[String],
    axis: ViewportAxis,
) -> Result<(), EngineError> {
    if ids.len() < 3 {
        return invalid_viewport_transform(
            "distribute.selection",
            "at least three objects are required",
        );
    }
    let mut ordered = ids
        .iter()
        .map(|id| {
            (
                id.clone(),
                axis.component(document.objects[id].transform.translation),
            )
        })
        .collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.1
            .total_cmp(&right.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    let min = ordered.first().map(|(_, value)| *value).unwrap_or_default();
    let max = ordered.last().map(|(_, value)| *value).unwrap_or_default();
    let span = max - min;
    if span.abs() <= f32::EPSILON {
        return invalid_viewport_transform("distribute.span", "selection span must be non-zero");
    }
    let step = span / (ordered.len() as f32 - 1.0);
    for (index, (id, _)) in ordered.iter().enumerate() {
        let object = document
            .objects
            .get_mut(id)
            .expect("selected ids were validated");
        axis.set_component(&mut object.transform.translation, min + step * index as f32);
    }
    Ok(())
}

struct DuplicateReport {
    created_ids: Vec<String>,
    id_map: BTreeMap<String, String>,
}

fn duplicate_objects(
    document: &mut EditorDocument,
    ids: &[String],
    suffix: &str,
    offset: Vec3,
) -> Result<DuplicateReport, EngineError> {
    let mut used_ids = document.objects.keys().cloned().collect::<BTreeSet<_>>();
    let mut id_map = BTreeMap::new();
    for id in ids {
        let duplicate_id = unique_duplicate_id(id, suffix, &used_ids)?;
        used_ids.insert(duplicate_id.clone());
        id_map.insert(id.clone(), duplicate_id);
    }
    let mut created_ids = Vec::new();
    for id in ids {
        let mut object = document.objects[id].clone();
        object.id = id_map[id].clone();
        if let Some(parent) = &object.parent
            && let Some(mapped_parent) = id_map.get(parent)
        {
            object.parent = Some(mapped_parent.clone());
        }
        object.transform.translation += offset;
        document.upsert_object(object.clone())?;
        created_ids.push(object.id);
    }
    Ok(DuplicateReport {
        created_ids,
        id_map,
    })
}

fn unique_duplicate_id(
    id: &str,
    suffix: &str,
    used_ids: &BTreeSet<String>,
) -> Result<String, EngineError> {
    let base = format!("{id}{suffix}");
    if !used_ids.contains(&base) {
        return Ok(base);
    }
    for index in 2..10_000 {
        let candidate = format!("{base}_{index}");
        if !used_ids.contains(&candidate) {
            return Ok(candidate);
        }
    }
    invalid_viewport_transform(
        "duplicate.id",
        format!("could not reconcile duplicate id {base}"),
    )
}

fn object_readback(
    document: &EditorDocument,
    ids: &[String],
) -> Result<Vec<ViewportObjectTransformReadback>, EngineError> {
    ids.iter()
        .map(|id| {
            let object = document.objects.get(id).ok_or_else(|| {
                invalid_viewport_transform_error(
                    "readback",
                    format!("missing object {id} after operation"),
                )
            })?;
            Ok(ViewportObjectTransformReadback {
                object_id: id.clone(),
                translation: object.transform.translation.to_array(),
                rotation_xyzw: object.transform.rotation.to_array(),
                yaw_y_degrees: yaw_y_degrees(object.transform.rotation),
                scale: object.transform.scale.to_array(),
            })
        })
        .collect()
}

fn normalize_rotation(rotation: Quat) -> Result<Quat, EngineError> {
    if !rotation.is_finite() || rotation.length_squared() <= f32::EPSILON {
        return invalid_viewport_transform(
            "rotation",
            "rotation result must be finite and non-zero",
        );
    }
    Ok(rotation.normalize())
}

fn validate_scale_factors(factors: Vec3) -> Result<(), EngineError> {
    if factors.x <= 0.0 || factors.y <= 0.0 || factors.z <= 0.0 {
        return invalid_viewport_transform("scale.factors", "scale factors must be positive");
    }
    Ok(())
}

fn validate_nonzero_scale(scale: Vec3) -> Result<(), EngineError> {
    if scale.x.abs() <= f32::EPSILON
        || scale.y.abs() <= f32::EPSILON
        || scale.z.abs() <= f32::EPSILON
    {
        return invalid_viewport_transform(
            "scale.result",
            "scale result must not contain zero components",
        );
    }
    Ok(())
}

fn validate_id_suffix(suffix: &str) -> Result<(), EngineError> {
    if suffix.is_empty() {
        return invalid_viewport_transform("duplicate.id_suffix", "id suffix must be non-empty");
    }
    if !suffix
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return invalid_viewport_transform(
            "duplicate.id_suffix",
            "id suffix must use only ASCII alnum, '_', '-', '.'",
        );
    }
    Ok(())
}

fn yaw_y_degrees(rotation: Quat) -> f32 {
    (2.0 * rotation.y.atan2(rotation.w)).to_degrees()
}
