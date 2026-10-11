//! Durable viewport transform operations for gizmo-driven editor workflows.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use litd_engine::{
    EngineError,
    math::{Quat, Vec3},
};

use crate::{
    history::{EditorDocument, SelectionSet, UndoRedoStack},
    shell::SnapSettings,
    world_creation::{EditorWorldRecord, validate_world_record},
};

#[path = "viewport_transform_support.rs"]
mod viewport_transform_support;
use viewport_transform_support::transformed_document;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewportAxis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewportSelectionMode {
    Replace,
    Add,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewportAlignReference {
    Minimum,
    Maximum,
    Average,
    Coordinate,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportSelectionRect {
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewportObjectTransformReadback {
    pub object_id: String,
    pub translation: [f32; 3],
    pub rotation_xyzw: [f32; 4],
    pub yaw_y_degrees: f32,
    pub scale: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewportSelectionReport {
    pub mode: ViewportSelectionMode,
    pub rect: ViewportSelectionRect,
    pub hit_object_ids: Vec<String>,
    pub before_selected_ids: Vec<String>,
    pub after_selected_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewportTransformReport {
    pub operation: String,
    pub selected_object_ids: Vec<String>,
    pub changed_object_ids: Vec<String>,
    pub created_object_ids: Vec<String>,
    pub duplicate_id_map: BTreeMap<String, String>,
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub history_recorded: bool,
    pub readback: Vec<ViewportObjectTransformReadback>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ViewportTransformOperation {
    Move {
        delta: Vec3,
        snap: SnapSettings,
    },
    Rotate {
        axis: ViewportAxis,
        degrees: f32,
        pivot: Vec3,
        snap: SnapSettings,
    },
    Scale {
        factors: Vec3,
        pivot: Vec3,
        snap: SnapSettings,
    },
    Align {
        axis: ViewportAxis,
        reference: ViewportAlignReference,
        coordinate: Option<f32>,
    },
    Distribute {
        axis: ViewportAxis,
    },
    Duplicate {
        id_suffix: String,
        offset: Vec3,
        snap: SnapSettings,
    },
}

impl ViewportSelectionRect {
    pub fn from_corners(a: Vec3, b: Vec3) -> Result<Self, EngineError> {
        if !a.is_finite() || !b.is_finite() {
            return invalid_viewport_transform("selection_rect", "corners must be finite");
        }
        let rect = Self {
            min_x: a.x.min(b.x),
            max_x: a.x.max(b.x),
            min_z: a.z.min(b.z),
            max_z: a.z.max(b.z),
        };
        rect.validate()?;
        Ok(rect)
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if !self.min_x.is_finite()
            || !self.max_x.is_finite()
            || !self.min_z.is_finite()
            || !self.max_z.is_finite()
        {
            return invalid_viewport_transform("selection_rect", "bounds must be finite");
        }
        if self.min_x >= self.max_x || self.min_z >= self.max_z {
            return invalid_viewport_transform(
                "selection_rect",
                "rectangle must have positive area",
            );
        }
        Ok(())
    }

    fn contains_xz(&self, position: Vec3) -> bool {
        position.x >= self.min_x
            && position.x <= self.max_x
            && position.z >= self.min_z
            && position.z <= self.max_z
    }
}

impl ViewportAxis {
    fn rotation(self, radians: f32) -> Quat {
        match self {
            Self::X => Quat::from_rotation_x(radians),
            Self::Y => Quat::from_rotation_y(radians),
            Self::Z => Quat::from_rotation_z(radians),
        }
    }

    fn component(self, value: Vec3) -> f32 {
        match self {
            Self::X => value.x,
            Self::Y => value.y,
            Self::Z => value.z,
        }
    }

    fn set_component(self, value: &mut Vec3, component: f32) {
        match self {
            Self::X => value.x = component,
            Self::Y => value.y = component,
            Self::Z => value.z = component,
        }
    }
}

impl ViewportTransformOperation {
    fn label(&self) -> &'static str {
        match self {
            Self::Move { .. } => "viewport.move",
            Self::Rotate { .. } => "viewport.rotate",
            Self::Scale { .. } => "viewport.scale",
            Self::Align { .. } => "viewport.align",
            Self::Distribute { .. } => "viewport.distribute",
            Self::Duplicate { .. } => "viewport.duplicate",
        }
    }
}

pub fn select_objects_in_world_rect(
    document: &EditorDocument,
    selection: &mut SelectionSet,
    rect: ViewportSelectionRect,
    mode: ViewportSelectionMode,
) -> Result<ViewportSelectionReport, EngineError> {
    document.validate()?;
    rect.validate()?;
    let before_selected_ids = selection_ids(selection);
    let hit_object_ids = document
        .objects
        .iter()
        .filter(|(id, object)| {
            id.as_str() != "world" && rect.contains_xz(object.transform.translation)
        })
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();

    match mode {
        ViewportSelectionMode::Replace => {
            selection.clear();
            selection.select_many(hit_object_ids.iter().cloned());
        }
        ViewportSelectionMode::Add => selection.select_many(hit_object_ids.iter().cloned()),
        ViewportSelectionMode::Toggle => {
            for id in &hit_object_ids {
                selection.toggle(id.clone());
            }
        }
    }

    Ok(ViewportSelectionReport {
        mode,
        rect,
        hit_object_ids,
        before_selected_ids,
        after_selected_ids: selection_ids(selection),
    })
}

pub fn apply_viewport_operation(
    document: &mut EditorDocument,
    selection: &SelectionSet,
    operation: ViewportTransformOperation,
    history: Option<&mut UndoRedoStack>,
) -> Result<ViewportTransformReport, EngineError> {
    let before = document.clone();
    let (next, mut report) = transformed_document(document, selection, &operation)?;
    if let Some(history) = history {
        history.record(operation.label(), before, next.clone())?;
        report.history_recorded = true;
    }
    *document = next;
    Ok(report)
}

pub fn apply_viewport_operation_in_world_record(
    record: &mut EditorWorldRecord,
    selection: &SelectionSet,
    operation: ViewportTransformOperation,
    history: Option<&mut UndoRedoStack>,
) -> Result<ViewportTransformReport, EngineError> {
    let mut next = record.clone();
    let report = apply_viewport_operation(&mut next.document, selection, operation, history)?;
    next.document_fingerprint = report.after_fingerprint.clone();
    next.summary = format!(
        "world={} viewport_operation={} changed={} created={} fingerprint={}",
        next.spec.properties.display_name,
        report.operation,
        report.changed_object_ids.len(),
        report.created_object_ids.len(),
        next.document_fingerprint
    );
    validate_world_record(&next)?;
    *record = next;
    Ok(report)
}

fn selection_ids(selection: &SelectionSet) -> Vec<String> {
    selection.selected_ids.iter().cloned().collect()
}

fn invalid_viewport_transform<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_viewport_transform_error(field, reason))
}

fn invalid_viewport_transform_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.viewport_transform.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "viewport_transform_tests.rs"]
mod tests;
