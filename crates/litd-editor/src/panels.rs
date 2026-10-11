//! Editor panel data: scene hierarchy, reflection inspector, and Calyx content browser.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use litd_engine::{
    EngineError,
    ecs::Entity,
    reflection::{ReflectType, inspect},
    scene::SceneGraph,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneHierarchyPanel {
    pub root_count: usize,
    pub node_count: usize,
    pub selected_entity: Option<String>,
    pub nodes: Vec<SceneHierarchyNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneHierarchyNode {
    pub entity: String,
    pub label: String,
    pub depth: usize,
    pub parent: Option<String>,
    pub child_count: usize,
    pub dirty: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InspectorPanel {
    pub entity: String,
    pub title: String,
    pub type_name: String,
    pub fingerprint: String,
    pub properties: Vec<InspectorProperty>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InspectorProperty {
    pub name: String,
    pub type_name: String,
    pub value_preview: String,
    pub editable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentBrowserPanel {
    pub query: String,
    pub backend: String,
    pub result_count: usize,
    pub results: Vec<CalyxContentResult>,
    pub dedup_groups: Vec<CalyxDedupGroup>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalyxContentResult {
    pub asset_id: String,
    pub display_name: String,
    pub score: f32,
    pub provenance: CalyxProvenance,
    pub required_modalities: Vec<String>,
    pub lens_hits: Vec<CalyxLensHit>,
    pub grounded: bool,
    pub lowering_ready: bool,
    pub dedup_group_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalyxProvenance {
    pub source_path: String,
    pub registry_path: String,
    pub registry_sha256: String,
    pub evidence_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalyxLensHit {
    pub lens_id: String,
    pub name: String,
    pub modality: String,
    pub manifest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalyxDedupGroup {
    pub group_id: String,
    pub canonical_asset_id: String,
    pub duplicate_asset_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorPanelsSnapshot {
    pub hierarchy: SceneHierarchyPanel,
    pub inspector: InspectorPanel,
    pub content_browser: ContentBrowserPanel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityLabel {
    pub entity: EntityKey,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityKey {
    pub index: u32,
    pub generation: u32,
}

impl From<Entity> for EntityKey {
    fn from(entity: Entity) -> Self {
        Self {
            index: entity.index(),
            generation: entity.generation(),
        }
    }
}

pub fn build_scene_hierarchy_panel(
    graph: &SceneGraph,
    labels: &[EntityLabel],
    selected: Option<Entity>,
) -> Result<SceneHierarchyPanel, EngineError> {
    let label_map = labels
        .iter()
        .map(|label| (label.entity, label.label.clone()))
        .collect::<BTreeMap<_, _>>();
    let info_map = graph
        .node_infos()
        .into_iter()
        .map(|info| (EntityKey::from(info.entity), info))
        .collect::<BTreeMap<_, _>>();
    let mut roots = info_map
        .values()
        .filter(|info| info.parent.is_none())
        .map(|info| info.entity)
        .collect::<Vec<_>>();
    roots.sort_by_key(|entity| (entity.index(), entity.generation()));

    let mut nodes = Vec::new();
    for root in roots {
        push_hierarchy_node(graph, &info_map, &label_map, selected, root, 0, &mut nodes)?;
    }
    Ok(SceneHierarchyPanel {
        root_count: graph.root_count(),
        node_count: graph.len(),
        selected_entity: selected.map(|entity| entity.to_string()),
        nodes,
    })
}

pub fn build_reflection_inspector<T>(
    entity: Entity,
    title: impl Into<String>,
    value: &T,
) -> Result<InspectorPanel, EngineError>
where
    T: ReflectType + Serialize,
{
    let reflected = inspect(value)?;
    let properties = reflected
        .fields
        .iter()
        .map(|field| InspectorProperty {
            name: field.name.clone(),
            type_name: field.type_name.clone(),
            value_preview: preview_value(&field.value),
            editable: true,
        })
        .collect();
    Ok(InspectorPanel {
        entity: entity.to_string(),
        title: title.into(),
        type_name: reflected.descriptor.type_name,
        fingerprint: reflected.fingerprint,
        properties,
    })
}

pub fn build_calyx_content_browser_from_paths(
    query: impl Into<String>,
    paths: &[PathBuf],
) -> Result<ContentBrowserPanel, EngineError> {
    let query = query.into();
    let mut panels = Vec::with_capacity(paths.len());
    for path in paths {
        panels.push(CalyxPanelObject::from_path(path)?);
    }
    build_calyx_content_browser(query, panels)
}

pub fn build_calyx_content_browser(
    query: String,
    panels: Vec<CalyxPanelObject>,
) -> Result<ContentBrowserPanel, EngineError> {
    if panels.is_empty() {
        return invalid_panel(
            "content_browser.sources",
            "must contain at least one Calyx source",
        );
    }
    let mut results = Vec::with_capacity(panels.len());
    for (index, panel) in panels.iter().enumerate() {
        let lens_hits = panel
            .lenses
            .iter()
            .map(|lens| CalyxLensHit {
                lens_id: lens.lens_id.clone(),
                name: lens.name.clone(),
                modality: lens.modality.clone(),
                manifest: lens.manifest.clone(),
            })
            .collect::<Vec<_>>();
        let grounded = !panel.evidence_refs.is_empty()
            && !panel.registry_ref.sha256_hex.is_empty()
            && lens_hits.len() >= 10;
        let lowering_ready = panel.base_template.a37_gate_eligible
            && panel.base_template.a37_status == "gate_passed";
        let dedup_group_id = format!("calyx:{}", panel.base_template.template_id);
        results.push(CalyxContentResult {
            asset_id: format!("{}#{index}", panel.name),
            display_name: panel.name.clone(),
            score: 1.0 - index as f32 * 0.01,
            provenance: CalyxProvenance {
                source_path: panel.source_path.clone(),
                registry_path: panel.registry_ref.path.clone(),
                registry_sha256: panel.registry_ref.sha256_hex.clone(),
                evidence_count: panel.evidence_refs.len(),
            },
            required_modalities: panel.required_modalities.clone(),
            lens_hits,
            grounded,
            lowering_ready,
            dedup_group_id,
        });
    }
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    for result in &results {
        groups
            .entry(result.dedup_group_id.clone())
            .or_default()
            .push(result.asset_id.clone());
    }
    let dedup_groups = groups
        .into_iter()
        .map(|(group_id, mut ids)| {
            ids.sort();
            let canonical_asset_id = ids[0].clone();
            CalyxDedupGroup {
                group_id,
                canonical_asset_id,
                duplicate_asset_ids: ids.into_iter().skip(1).collect(),
            }
        })
        .collect::<Vec<_>>();
    let panel = ContentBrowserPanel {
        query,
        backend: "calyx.panel_json".to_string(),
        result_count: results.len(),
        results,
        dedup_groups,
    };
    validate_content_browser(&panel)?;
    Ok(panel)
}

impl ContentBrowserPanel {
    pub fn first_lens_modalities(&self) -> String {
        self.results
            .first()
            .map(|result| {
                result
                    .lens_hits
                    .iter()
                    .map(|lens| lens.modality.as_str())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalyxPanelObject {
    #[serde(skip)]
    pub source_path: String,
    pub schema_version: u32,
    pub name: String,
    pub base_template: CalyxBaseTemplate,
    pub registry_ref: CalyxRegistryRef,
    #[serde(default)]
    pub required_modalities: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<CalyxEvidenceRef>,
    #[serde(default)]
    pub lenses: Vec<CalyxLensRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalyxBaseTemplate {
    pub name: String,
    pub template_id: String,
    pub content_lens_count: usize,
    pub a37_gate_eligible: bool,
    pub a37_status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalyxRegistryRef {
    pub path: String,
    pub sha256_hex: String,
    pub size_bytes: u64,
    pub lens_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalyxEvidenceRef {
    pub path: String,
    pub sha256_hex: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalyxLensRecord {
    pub lens_id: String,
    pub name: String,
    pub modality: String,
    pub manifest: String,
}

impl CalyxPanelObject {
    pub fn from_path(path: &Path) -> Result<Self, EngineError> {
        let text =
            fs::read_to_string(path).map_err(|error| EngineError::UiRenderInvalidConfig {
                field: "editor.panels.calyx.source_path".to_string(),
                reason: format!("failed to read Calyx panel JSON: {error}"),
            })?;
        Self::from_json_str(path.display().to_string(), &text)
    }

    pub fn from_json_str(source_path: String, text: &str) -> Result<Self, EngineError> {
        let mut panel: Self =
            serde_json::from_str(text).map_err(|error| EngineError::SerializationFailed {
                format: "json",
                reason: error.to_string(),
            })?;
        panel.source_path = source_path;
        Ok(panel)
    }
}

fn push_hierarchy_node(
    graph: &SceneGraph,
    info_map: &BTreeMap<EntityKey, litd_engine::scene::SceneNodeInfo>,
    labels: &BTreeMap<EntityKey, String>,
    selected: Option<Entity>,
    entity: Entity,
    depth: usize,
    nodes: &mut Vec<SceneHierarchyNode>,
) -> Result<(), EngineError> {
    let key = EntityKey::from(entity);
    let info = info_map
        .get(&key)
        .ok_or_else(|| EngineError::SceneNodeMissing {
            entity: entity.to_string(),
        })?;
    nodes.push(SceneHierarchyNode {
        entity: entity.to_string(),
        label: labels
            .get(&key)
            .cloned()
            .unwrap_or_else(|| entity.to_string()),
        depth,
        parent: info.parent.map(|parent| parent.to_string()),
        child_count: info.child_count,
        dirty: info.dirty,
        selected: selected == Some(entity),
    });
    let mut children = graph.children(entity)?.to_vec();
    children.sort_by_key(|child| (child.index(), child.generation()));
    for child in children {
        push_hierarchy_node(graph, info_map, labels, selected, child, depth + 1, nodes)?;
    }
    Ok(())
}

fn preview_value(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(values) => format!("array[{}]", values.len()),
        Value::Object(_) => value.to_string(),
    }
}

fn validate_content_browser(panel: &ContentBrowserPanel) -> Result<(), EngineError> {
    for result in &panel.results {
        if result.lens_hits.len() < 10 {
            return invalid_panel(
                "content_browser.lenses",
                "each result needs at least 10 lenses",
            );
        }
        if !result.grounded || !result.lowering_ready {
            return invalid_panel(
                "content_browser.provenance",
                "results must be grounded and lowering-ready",
            );
        }
        if result.provenance.source_path.is_empty() || result.provenance.registry_sha256.is_empty()
        {
            return invalid_panel(
                "content_browser.provenance",
                "source and registry are required",
            );
        }
    }
    Ok(())
}

fn invalid_panel<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.panels.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "panels_tests.rs"]
mod tests;
