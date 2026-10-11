use std::collections::{BTreeMap, BTreeSet};

use litd_engine::{
    EngineError,
    math::{Transform, Vec3},
};
use litd_hash::stable_fingerprint;
use serde::{Deserialize, Serialize};

use crate::{
    history::{EditorDocument, EditorObject, EditorRegion},
    mcp_trigger_tools::EcaGraph,
    object_editor::issue732_base_document,
    trigger_editor::unit_dies_create_unit_graph,
};

#[path = "object_manager_support.rs"]
mod object_manager_support;
use object_manager_support::{
    count, entry, object_manager_error, object_manager_error_value, object_refs, reference_counts,
    replace_ref_tokens, trigger_refs,
};

pub const OBJECT_MANAGER_SCHEMA: &str = "litd.editor.object_manager.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectManagerProject {
    pub schema: String,
    pub document: EditorDocument,
    pub triggers: Vec<EcaGraph>,
    pub assets: Vec<ObjectManagerAssetRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectManagerAssetRef {
    pub asset_id: String,
    pub asset_type: String,
    pub title: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectManagerEntry {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub reference_count: usize,
    pub orphan: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectManagerReference {
    pub source_id: String,
    pub source_kind: String,
    pub target_id: String,
    pub target_kind: String,
    pub field: String,
    pub trigger_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectManagerIndex {
    pub counts: BTreeMap<String, usize>,
    pub entries: Vec<ObjectManagerEntry>,
    pub references: Vec<ObjectManagerReference>,
    pub orphan_ids: Vec<String>,
    pub fingerprint: String,
}

impl ObjectManagerProject {
    pub fn stable_fingerprint(&self) -> Result<String, EngineError> {
        stable_fingerprint(self).map_err(|error| EngineError::SerializationFailed {
            format: "ron",
            reason: error.to_string(),
        })
    }
}

pub fn issue736_sample_project() -> Result<ObjectManagerProject, EngineError> {
    let mut document = issue732_base_document();
    document.upsert_region(EditorRegion::new(
        "region.issue736.spawn",
        "Guardian Spawn",
        Vec3::new(12.0, 0.0, 8.0),
        Vec3::new(3.0, 2.0, 3.0),
    ))?;
    document.upsert_object(
        EditorObject::new("unit.issue736.guard", "Unit", None, Transform::IDENTITY)
            .with_property("object.rawcode", "u736")
            .with_property("object.display_name", "Object Manager Guard")
            .with_property("stats.hit_points", "520")
            .with_property("stats.move_speed", "310")
            .with_property("abilities.normal", "ability.issue732.defend")
            .with_property("region.spawn", "region.issue736.spawn")
            .with_property("asset.model", "asset.issue736.guard_model"),
    )?;
    let mut trigger = unit_dies_create_unit_graph(true, true);
    trigger.trigger_id = "trg_issue736_guard_death".to_string();
    trigger.display_name = "Guard Death Reinforcement".to_string();
    trigger.events[0]
        .args
        .insert("unit_id".to_string(), "unit.issue736.guard".to_string());
    let project = ObjectManagerProject {
        schema: OBJECT_MANAGER_SCHEMA.to_string(),
        document,
        triggers: vec![trigger],
        assets: vec![
            ObjectManagerAssetRef {
                asset_id: "asset.issue736.guard_model".to_string(),
                asset_type: "Model".to_string(),
                title: "Guard Model".to_string(),
                relative_path: "models/issue736/guard.glb".to_string(),
            },
            ObjectManagerAssetRef {
                asset_id: "asset.issue736.banner_icon".to_string(),
                asset_type: "Image".to_string(),
                title: "Banner Icon".to_string(),
                relative_path: "textures/issue736/banner.png".to_string(),
            },
        ],
    };
    project.document.validate()?;
    Ok(project)
}

pub fn build_object_manager_index(
    project: &ObjectManagerProject,
) -> Result<ObjectManagerIndex, EngineError> {
    if project.schema != OBJECT_MANAGER_SCHEMA {
        return object_manager_error("schema", "unexpected object manager schema");
    }
    let mut entries = Vec::new();
    let mut references = Vec::new();
    let mut counts = BTreeMap::new();
    let object_ids = project
        .document
        .objects
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let region_ids = project
        .document
        .regions
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let asset_ids = project
        .assets
        .iter()
        .map(|asset| asset.asset_id.clone())
        .collect::<BTreeSet<_>>();
    for object in project.document.objects.values() {
        let orphan = object
            .parent
            .as_ref()
            .is_some_and(|parent| !object_ids.contains(parent));
        entries.push(entry(
            &object.id,
            &object.kind,
            object
                .properties
                .get("object.display_name")
                .map(String::as_str)
                .unwrap_or(&object.id),
            orphan,
        ));
        count(&mut counts, "objects", 1);
        count(&mut counts, &object.kind.to_ascii_lowercase(), 1);
        object_refs(
            object,
            &object_ids,
            &region_ids,
            &asset_ids,
            &mut references,
        );
    }
    for region in project.document.regions.values() {
        entries.push(entry(&region.id, "Region", &region.name, false));
    }
    for trigger in &project.triggers {
        entries.push(entry(
            &trigger.trigger_id,
            "Trigger",
            &trigger.display_name,
            false,
        ));
        trigger_refs(
            trigger,
            &object_ids,
            &region_ids,
            &asset_ids,
            &mut references,
        );
    }
    for asset in &project.assets {
        entries.push(entry(
            &asset.asset_id,
            &asset.asset_type,
            &asset.title,
            false,
        ));
    }
    count(&mut counts, "regions", project.document.regions.len());
    count(&mut counts, "triggers", project.triggers.len());
    count(&mut counts, "assets", project.assets.len());
    count(&mut counts, "total", entries.len());
    let reference_counts = reference_counts(&references);
    for entry in &mut entries {
        entry.reference_count = reference_counts.get(&entry.id).copied().unwrap_or(0);
    }
    entries.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.id.cmp(&b.id)));
    references.sort_by(|a, b| {
        a.target_id
            .cmp(&b.target_id)
            .then_with(|| a.source_id.cmp(&b.source_id))
            .then_with(|| a.field.cmp(&b.field))
    });
    let orphan_ids = entries
        .iter()
        .filter(|entry| entry.orphan)
        .map(|entry| entry.id.clone())
        .collect::<Vec<_>>();
    Ok(ObjectManagerIndex {
        counts,
        entries,
        references,
        orphan_ids,
        fingerprint: project.stable_fingerprint()?,
    })
}

/// Generation-keyed memoization of [`build_object_manager_index`] (#1461).
///
/// `build_object_manager_index` is O(project): it materializes three `BTreeSet`s of every
/// object/region/asset id, walks every object and trigger to collect references, detects
/// orphans, and RON-serializes the whole project for the fingerprint. The Object Manager
/// panel is immediate-mode and redraws every frame, so calling it unconditionally churned
/// thousands of allocations at 60 fps just to show eight rows — the classic immediate-mode
/// "recompute derived data every frame" waste.
///
/// The fix is the standard memoize-and-invalidate pattern: the owner holds a monotonically
/// increasing generation that it bumps on any mutation of the project, and the cache
/// rebuilds only when that generation advances. The generation is an O(1) key — unlike the
/// project fingerprint, which would re-serialize the whole project every frame (the very
/// cost being removed here, and #1461's separate item 3). A never-mutated project therefore
/// builds its index exactly once; a future edit path stays correct by bumping the
/// generation, so the cache can never silently serve stale rows.
#[derive(Debug, Default, Clone)]
pub struct ObjectManagerIndexCache {
    cached: Option<(u64, ObjectManagerIndex)>,
}

impl ObjectManagerIndexCache {
    /// Return the index for `project` at `generation`, rebuilding only when the cached
    /// generation differs (or nothing is cached yet). When the generation is unchanged the
    /// stored index is returned without touching the project — no rebuild, no fingerprint
    /// serialization, no allocations.
    pub fn get_or_build(
        &mut self,
        generation: u64,
        project: &ObjectManagerProject,
    ) -> Result<&ObjectManagerIndex, EngineError> {
        let stale = match &self.cached {
            Some((cached_generation, _)) => *cached_generation != generation,
            None => true,
        };
        if stale {
            let index = build_object_manager_index(project)?;
            self.cached = Some((generation, index));
        }
        Ok(&self
            .cached
            .as_ref()
            .expect("cache populated above when stale")
            .1)
    }

    /// The generation whose index is currently cached, or `None` if nothing is cached.
    /// Exposed for verification/tests that need to prove a rebuild did or did not happen.
    pub fn cached_generation(&self) -> Option<u64> {
        self.cached.as_ref().map(|(generation, _)| *generation)
    }
}

pub fn find_references(index: &ObjectManagerIndex, target_id: &str) -> Vec<ObjectManagerReference> {
    index
        .references
        .iter()
        .filter(|reference| reference.target_id == target_id)
        .cloned()
        .collect()
}

pub fn search_entries(
    index: &ObjectManagerIndex,
    query: &str,
    limit: usize,
) -> Vec<ObjectManagerEntry> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() || limit == 0 {
        return Vec::new();
    }
    index
        .entries
        .iter()
        .filter(|entry| {
            entry.id.to_ascii_lowercase().contains(&query)
                || entry.label.to_ascii_lowercase().contains(&query)
                || entry.kind.to_ascii_lowercase().contains(&query)
        })
        .take(limit)
        .cloned()
        .collect()
}

pub fn rename_object_and_refs(
    project: &ObjectManagerProject,
    old_id: &str,
    new_id: &str,
) -> Result<ObjectManagerProject, EngineError> {
    if old_id.trim().is_empty() || new_id.trim().is_empty() {
        return object_manager_error("rename", "old and new object ids must be non-empty");
    }
    let mut next = project.clone();
    if next.document.objects.contains_key(new_id) {
        return object_manager_error("rename", format!("object {new_id} already exists"));
    }
    let mut object =
        next.document.objects.remove(old_id).ok_or_else(|| {
            object_manager_error_value("rename", format!("missing object {old_id}"))
        })?;
    object.id = new_id.to_string();
    next.document.objects.insert(new_id.to_string(), object);
    for object in next.document.objects.values_mut() {
        if object.parent.as_deref() == Some(old_id) {
            object.parent = Some(new_id.to_string());
        }
        for value in object.properties.values_mut() {
            *value = replace_ref_tokens(value, old_id, new_id);
        }
    }
    for graph in &mut next.triggers {
        for node in graph
            .events
            .iter_mut()
            .chain(graph.conditions.iter_mut())
            .chain(graph.actions.iter_mut())
        {
            for value in node.args.values_mut() {
                *value = replace_ref_tokens(value, old_id, new_id);
            }
        }
    }
    next.document.validate()?;
    Ok(next)
}

#[cfg(test)]
#[path = "object_manager_tests.rs"]
mod tests;
