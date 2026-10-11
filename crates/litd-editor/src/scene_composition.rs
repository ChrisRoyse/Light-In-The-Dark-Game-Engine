//! Scene composition operations that bind editor hierarchy changes to the engine scene graph.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use litd_engine::{
    EngineError,
    ecs::{Entity, World},
    math::{Mat4, Transform},
    scene::{SceneGraph, ScenePropagationStats},
};

use crate::{
    history::{EditorDocument, UndoRedoStack, stable_fingerprint},
    world_creation::{EditorWorldRecord, validate_world_record},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SceneCompositionOperation {
    SetParent {
        child_id: String,
        parent_id: Option<String>,
        preserve_world_transform: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneCompositionReport {
    pub operation: String,
    pub changed_object_ids: Vec<String>,
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub history_recorded: bool,
    pub graph_readback: SceneGraphReadback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneGraphReadback {
    pub node_count: usize,
    pub root_count: usize,
    pub propagation: ScenePropagationReadback,
    pub objects: Vec<SceneGraphObjectReadback>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenePropagationReadback {
    pub propagation_index: u64,
    pub root_count: usize,
    pub visited_nodes: usize,
    pub recomputed_nodes: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneGraphObjectReadback {
    pub object_id: String,
    pub entity: String,
    pub parent_object_id: Option<String>,
    pub child_object_ids: Vec<String>,
    pub child_count: usize,
    pub local_translation: [f32; 3],
    pub global_translation: [f32; 3],
}

struct EngineSceneGraphBuild {
    graph: SceneGraph,
    entities: BTreeMap<String, Entity>,
    reverse: HashMap<Entity, String>,
    propagation: ScenePropagationStats,
}

impl SceneCompositionOperation {
    fn label(&self) -> &'static str {
        match self {
            Self::SetParent { .. } => "scene.reparent",
        }
    }
}

impl SceneGraphReadback {
    pub fn object(&self, object_id: &str) -> Option<&SceneGraphObjectReadback> {
        self.objects
            .iter()
            .find(|object| object.object_id == object_id)
    }
}

pub fn apply_scene_composition_operation(
    document: &mut EditorDocument,
    operation: SceneCompositionOperation,
    history: Option<&mut UndoRedoStack>,
) -> Result<SceneCompositionReport, EngineError> {
    let before = document.clone();
    let before_fingerprint = stable_fingerprint(&before)?;
    let mut next = before.clone();
    let changed_object_ids = match &operation {
        SceneCompositionOperation::SetParent {
            child_id,
            parent_id,
            preserve_world_transform,
        } => {
            reparent_object(
                &before,
                &mut next,
                child_id,
                parent_id,
                *preserve_world_transform,
            )?;
            vec![child_id.clone()]
        }
    };
    next.validate()?;
    let graph_readback = build_scene_graph_readback(&next)?;
    let after_fingerprint = stable_fingerprint(&next)?;
    if before_fingerprint == after_fingerprint {
        return invalid_scene_composition(
            "document.no_change",
            format!("{} did not change the document", operation.label()),
        );
    }
    let mut report = SceneCompositionReport {
        operation: operation.label().to_string(),
        changed_object_ids,
        before_fingerprint,
        after_fingerprint,
        history_recorded: false,
        graph_readback,
    };
    if let Some(history) = history {
        history.record(operation.label(), before, next.clone())?;
        report.history_recorded = true;
    }
    *document = next;
    Ok(report)
}

pub fn apply_scene_composition_operation_in_world_record(
    record: &mut EditorWorldRecord,
    operation: SceneCompositionOperation,
    history: Option<&mut UndoRedoStack>,
) -> Result<SceneCompositionReport, EngineError> {
    let mut next = record.clone();
    let report = apply_scene_composition_operation(&mut next.document, operation, history)?;
    next.document_fingerprint = report.after_fingerprint.clone();
    next.summary = format!(
        "world={} scene_composition_operation={} changed={} fingerprint={}",
        next.spec.properties.display_name,
        report.operation,
        report.changed_object_ids.len(),
        next.document_fingerprint
    );
    validate_world_record(&next)?;
    *record = next;
    Ok(report)
}

pub fn build_scene_graph_readback(
    document: &EditorDocument,
) -> Result<SceneGraphReadback, EngineError> {
    scene_graph_readback(build_engine_scene_graph(document)?)
}

fn reparent_object(
    before: &EditorDocument,
    next: &mut EditorDocument,
    child_id: &str,
    parent_id: &Option<String>,
    preserve_world_transform: bool,
) -> Result<(), EngineError> {
    before.validate()?;
    validate_object_ref(before, "child_id", child_id)?;
    if child_id == "world" {
        return invalid_scene_composition("child_id", "root object world cannot be reparented");
    }
    if let Some(parent) = parent_id {
        validate_object_ref(before, "parent_id", parent)?;
        if parent == child_id {
            return invalid_scene_composition("parent_id", "object cannot parent itself");
        }
    }
    let current_parent = before.objects[child_id].parent.clone();
    if current_parent == *parent_id {
        return invalid_scene_composition(
            "parent_id",
            format!("{child_id} is already under the requested parent"),
        );
    }
    reject_parent_cycle(before, child_id, parent_id.as_deref())?;
    if preserve_world_transform {
        let graph = build_engine_scene_graph(before)?;
        let local = preserved_local_transform(&graph, child_id, parent_id.as_deref())?;
        next.objects.get_mut(child_id).unwrap().transform = local;
    }
    next.objects.get_mut(child_id).unwrap().parent = parent_id.clone();
    Ok(())
}

fn preserved_local_transform(
    build: &EngineSceneGraphBuild,
    child_id: &str,
    parent_id: Option<&str>,
) -> Result<Transform, EngineError> {
    let child_entity = *build.entities.get(child_id).ok_or_else(|| {
        invalid_scene_composition_error("child_id", format!("missing child object {child_id}"))
    })?;
    let child_global = build.graph.global_matrix(child_entity)?;
    let parent_global = match parent_id {
        Some(parent_id) => {
            let parent_entity = *build.entities.get(parent_id).ok_or_else(|| {
                invalid_scene_composition_error(
                    "parent_id",
                    format!("missing parent object {parent_id}"),
                )
            })?;
            build.graph.global_matrix(parent_entity)?
        }
        None => Mat4::IDENTITY,
    };
    let parent_inverse = parent_global.inverse();
    if !mat4_is_finite(parent_inverse) {
        return invalid_scene_composition(
            "parent_transform",
            "parent global transform is not invertible",
        );
    }
    transform_from_matrix(parent_inverse * child_global)
}

fn build_engine_scene_graph(
    document: &EditorDocument,
) -> Result<EngineSceneGraphBuild, EngineError> {
    document.validate()?;
    let mut world = World::default();
    let mut graph = SceneGraph::default();
    let mut entities = BTreeMap::new();
    for (id, object) in &document.objects {
        let entity = world.spawn(())?;
        graph.insert(entity, object.transform)?;
        entities.insert(id.clone(), entity);
    }
    for (id, object) in &document.objects {
        if let Some(parent_id) = &object.parent {
            let child = entities[id];
            let parent = *entities.get(parent_id).ok_or_else(|| {
                invalid_scene_composition_error(
                    "document.parent",
                    format!("object {id} references missing parent {parent_id}"),
                )
            })?;
            graph.set_parent(child, Some(parent))?;
        }
    }
    let propagation = graph.propagate()?;
    let reverse = entities
        .iter()
        .map(|(id, entity)| (*entity, id.clone()))
        .collect::<HashMap<_, _>>();
    Ok(EngineSceneGraphBuild {
        graph,
        entities,
        reverse,
        propagation,
    })
}

fn scene_graph_readback(build: EngineSceneGraphBuild) -> Result<SceneGraphReadback, EngineError> {
    let mut objects = Vec::with_capacity(build.entities.len());
    for (id, entity) in &build.entities {
        let local = build.graph.local_transform(*entity)?;
        let parent_object_id = build
            .graph
            .parent(*entity)?
            .and_then(|parent| build.reverse.get(&parent).cloned());
        let mut child_object_ids = build
            .graph
            .children(*entity)?
            .iter()
            .filter_map(|child| build.reverse.get(child).cloned())
            .collect::<Vec<_>>();
        child_object_ids.sort();
        objects.push(SceneGraphObjectReadback {
            object_id: id.clone(),
            entity: entity.to_string(),
            parent_object_id,
            child_count: child_object_ids.len(),
            child_object_ids,
            local_translation: local.translation.to_array(),
            global_translation: build.graph.global_translation(*entity)?.to_array(),
        });
    }
    Ok(SceneGraphReadback {
        node_count: build.graph.len(),
        root_count: build.graph.root_count(),
        propagation: ScenePropagationReadback::from(build.propagation),
        objects,
    })
}

fn reject_parent_cycle(
    document: &EditorDocument,
    child_id: &str,
    parent_id: Option<&str>,
) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    let mut cursor = parent_id;
    while let Some(id) = cursor {
        if id == child_id {
            return invalid_scene_composition(
                "parent.cycle",
                format!("parenting {child_id} under {id} would create a cycle"),
            );
        }
        if !seen.insert(id.to_string()) {
            return invalid_scene_composition(
                "parent.cycle",
                format!("existing parent chain contains a cycle at {id}"),
            );
        }
        cursor = document
            .objects
            .get(id)
            .and_then(|object| object.parent.as_deref());
    }
    Ok(())
}

fn validate_object_ref(
    document: &EditorDocument,
    field: &str,
    object_id: &str,
) -> Result<(), EngineError> {
    validate_single_line(field, object_id)?;
    if !document.objects.contains_key(object_id) {
        return invalid_scene_composition(field, format!("missing object {object_id}"));
    }
    Ok(())
}

fn transform_from_matrix(matrix: Mat4) -> Result<Transform, EngineError> {
    if !mat4_is_finite(matrix) {
        return invalid_scene_composition("transform", "matrix components must be finite");
    }
    let (scale, rotation, translation) = matrix.to_scale_rotation_translation();
    if !translation.is_finite() || !rotation.is_finite() || !scale.is_finite() {
        return invalid_scene_composition("transform", "decomposed transform must be finite");
    }
    if scale.x.abs() <= f32::EPSILON
        || scale.y.abs() <= f32::EPSILON
        || scale.z.abs() <= f32::EPSILON
        || rotation.length_squared() <= f32::EPSILON
    {
        return invalid_scene_composition(
            "transform",
            "decomposed transform must have non-zero scale and rotation",
        );
    }
    Ok(Transform::from_trs(
        translation,
        rotation.normalize(),
        scale,
    ))
}

fn mat4_is_finite(matrix: Mat4) -> bool {
    matrix.to_cols_array().iter().all(|value| value.is_finite())
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\t']) {
        return invalid_scene_composition(field, "value must be non-empty single-line text");
    }
    Ok(())
}

impl From<ScenePropagationStats> for ScenePropagationReadback {
    fn from(stats: ScenePropagationStats) -> Self {
        Self {
            propagation_index: stats.propagation_index,
            root_count: stats.root_count,
            visited_nodes: stats.visited_nodes,
            recomputed_nodes: stats.recomputed_nodes,
        }
    }
}

fn invalid_scene_composition<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_scene_composition_error(field, reason))
}

fn invalid_scene_composition_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.scene_composition.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "scene_composition_tests.rs"]
mod tests;
