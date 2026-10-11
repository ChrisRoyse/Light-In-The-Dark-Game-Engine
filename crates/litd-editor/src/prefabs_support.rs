use std::collections::{BTreeMap, BTreeSet};

use litd_engine::{EngineError, math::Transform};

use crate::history::{EditorDocument, EditorObject, stable_fingerprint};

use super::{
    PREFAB_ID_PROPERTY, PREFAB_INSTANCE_ID_PROPERTY, PREFAB_OVERRIDE_KEYS_PROPERTY,
    PREFAB_ROOT_FINGERPRINT_PROPERTY, PREFAB_TEMPLATE_FINGERPRINT_PROPERTY,
    PREFAB_TEMPLATE_PATH_PROPERTY, PREFAB_TEMPLATE_PREFAB_ID_PROPERTY, PrefabExpansionReport,
    PrefabInstance, PrefabLibrary, PrefabPropertyOverride,
};

#[derive(Debug, Clone)]
struct ExpandedTemplate {
    template_path: String,
    source_prefab_id: String,
    source_fingerprint: String,
    kind: String,
    parent_path: Option<String>,
    transform: Transform,
    properties: BTreeMap<String, String>,
    override_keys: BTreeSet<String>,
}

pub fn apply_prefab_instance(
    document: &mut EditorDocument,
    library: &PrefabLibrary,
    instance: &PrefabInstance,
) -> Result<PrefabExpansionReport, EngineError> {
    instance.validate()?;
    document.validate()?;
    if library.max_instances_per_prefab == 0 {
        return invalid_prefab(
            "library.max_instances_per_prefab",
            "must be greater than zero",
        );
    }
    let definition = library.definition(&instance.prefab_id)?;
    definition.validate_content()?;
    let source_fingerprint = definition.content_fingerprint.clone();
    let existing_instances = prefab_instance_ids(document, &instance.prefab_id);
    let existing_instance = existing_instances.contains(&instance.instance_id);
    if !existing_instance && instance.source_fingerprint != source_fingerprint {
        return invalid_prefab(
            "instance.source_fingerprint",
            format!(
                "new instance {} uses stale source {}, current {}",
                instance.instance_id, instance.source_fingerprint, source_fingerprint
            ),
        );
    }
    if !existing_instance && existing_instances.len() >= library.max_instances_per_prefab {
        return invalid_prefab(
            "library.max_instances_per_prefab",
            format!(
                "prefab {} already has {} instances, max {}",
                instance.prefab_id,
                existing_instances.len(),
                library.max_instances_per_prefab
            ),
        );
    }
    if let Some(parent) = &instance.parent_object_id
        && !document.objects.contains_key(parent)
    {
        return invalid_prefab(
            "instance.parent_object_id",
            format!("missing parent object {parent}"),
        );
    }

    let mut expanded = expand_definition(
        library,
        &instance.prefab_id,
        instance.transform,
        &[],
        None,
        &mut Vec::new(),
    )?;
    apply_overrides(&mut expanded, &instance.overrides)?;
    let preserved_override_count = instance.overrides.len();
    let before_fingerprint = stable_fingerprint(document)?;
    let existing_ids = generated_object_ids_for_instance(document, &instance.instance_id);
    let mut next = document.clone();
    for id in &existing_ids {
        next.objects.remove(id);
    }

    let mut created = Vec::new();
    for template in &expanded {
        let object_id = prefab_generated_object_id(&instance.instance_id, &template.template_path)?;
        if next.objects.contains_key(&object_id) {
            return invalid_prefab(
                "document.objects",
                format!("generated object id collision {object_id}"),
            );
        }
        let parent = match &template.parent_path {
            Some(path) => Some(prefab_generated_object_id(&instance.instance_id, path)?),
            None => instance.parent_object_id.clone(),
        };
        let mut object = EditorObject::new(
            object_id.clone(),
            &template.kind,
            parent,
            template.transform,
        );
        object.properties = template.properties.clone();
        object
            .properties
            .insert(PREFAB_ID_PROPERTY.to_string(), instance.prefab_id.clone());
        object.properties.insert(
            PREFAB_INSTANCE_ID_PROPERTY.to_string(),
            instance.instance_id.clone(),
        );
        object.properties.insert(
            PREFAB_TEMPLATE_PATH_PROPERTY.to_string(),
            template.template_path.clone(),
        );
        object.properties.insert(
            PREFAB_ROOT_FINGERPRINT_PROPERTY.to_string(),
            source_fingerprint.clone(),
        );
        object.properties.insert(
            PREFAB_TEMPLATE_PREFAB_ID_PROPERTY.to_string(),
            template.source_prefab_id.clone(),
        );
        object.properties.insert(
            PREFAB_TEMPLATE_FINGERPRINT_PROPERTY.to_string(),
            template.source_fingerprint.clone(),
        );
        object.properties.insert(
            PREFAB_OVERRIDE_KEYS_PROPERTY.to_string(),
            template
                .override_keys
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(","),
        );
        next.upsert_object(object)?;
        created.push(object_id);
    }
    next.validate()?;
    let after_fingerprint = stable_fingerprint(&next)?;
    let generated_set = created.iter().cloned().collect::<BTreeSet<_>>();
    let removed_stale_object_ids = existing_ids
        .into_iter()
        .filter(|id| !generated_set.contains(id))
        .collect::<Vec<_>>();
    *document = next;
    Ok(PrefabExpansionReport {
        before_fingerprint,
        after_fingerprint,
        prefab_id: instance.prefab_id.clone(),
        instance_id: instance.instance_id.clone(),
        previous_source_fingerprint: instance.source_fingerprint.clone(),
        source_fingerprint: source_fingerprint.clone(),
        source_changed: instance.source_fingerprint != source_fingerprint,
        instance_count: if existing_instance {
            existing_instances.len()
        } else {
            existing_instances.len() + 1
        },
        created_or_updated_object_ids: created,
        removed_stale_object_ids,
        preserved_override_count,
    })
}

pub fn prefab_generated_object_id(
    instance_id: &str,
    template_path: &str,
) -> Result<String, EngineError> {
    validate_stable_id("generated.instance_id", instance_id)?;
    validate_template_path("generated.template_path", template_path)?;
    Ok(format!(
        "prefab::{instance_id}::{}",
        template_path.replace('/', "::")
    ))
}

fn expand_definition(
    library: &PrefabLibrary,
    prefab_id: &str,
    base_transform: Transform,
    prefix: &[String],
    parent_path: Option<String>,
    stack: &mut Vec<String>,
) -> Result<Vec<ExpandedTemplate>, EngineError> {
    if stack.iter().any(|item| item == prefab_id) {
        return invalid_prefab(
            "definition.nested_prefabs",
            format!("nested prefab cycle: {}", stack.join(" -> ")),
        );
    }
    let definition = library.definition(prefab_id)?;
    stack.push(prefab_id.to_string());
    let mut expanded = Vec::new();
    for template in &definition.templates {
        let path = path_with_leaf(prefix, &template.template_id);
        let parent = template
            .parent_template_id
            .as_ref()
            .map(|parent| path_with_leaf(prefix, parent))
            .or_else(|| parent_path.clone());
        expanded.push(ExpandedTemplate {
            template_path: path,
            source_prefab_id: definition.prefab_id.clone(),
            source_fingerprint: definition.content_fingerprint.clone(),
            kind: template.kind.clone(),
            parent_path: parent,
            transform: compose_transform(base_transform, template.transform),
            properties: template.properties.clone(),
            override_keys: BTreeSet::new(),
        });
    }
    for nested in &definition.nested_prefabs {
        let mut nested_prefix = prefix.to_vec();
        nested_prefix.push(nested.template_id.clone());
        let nested_parent = nested
            .parent_template_id
            .as_ref()
            .map(|parent| path_with_leaf(prefix, parent))
            .or_else(|| parent_path.clone());
        let mut children = expand_definition(
            library,
            &nested.prefab_id,
            compose_transform(base_transform, nested.transform),
            &nested_prefix,
            nested_parent,
            stack,
        )?;
        apply_nested_overrides(&mut children, &nested_prefix, &nested.overrides)?;
        expanded.extend(children);
    }
    stack.pop();
    Ok(expanded)
}

fn apply_nested_overrides(
    expanded: &mut [ExpandedTemplate],
    prefix: &[String],
    overrides: &[PrefabPropertyOverride],
) -> Result<(), EngineError> {
    let scoped = overrides
        .iter()
        .map(|override_| PrefabPropertyOverride {
            template_path: path_with_relative(prefix, &override_.template_path),
            property_key: override_.property_key.clone(),
            value: override_.value.clone(),
        })
        .collect::<Vec<_>>();
    apply_overrides(expanded, &scoped)
}

fn apply_overrides(
    expanded: &mut [ExpandedTemplate],
    overrides: &[PrefabPropertyOverride],
) -> Result<(), EngineError> {
    for override_ in overrides {
        override_.validate()?;
        let Some(template) = expanded
            .iter_mut()
            .find(|template| template.template_path == override_.template_path)
        else {
            return invalid_prefab(
                "override.template_path",
                format!(
                    "override target {} was not generated",
                    override_.template_path
                ),
            );
        };
        template
            .properties
            .insert(override_.property_key.clone(), override_.value.clone());
        template
            .override_keys
            .insert(override_.property_key.clone());
    }
    Ok(())
}

fn prefab_instance_ids(document: &EditorDocument, prefab_id: &str) -> BTreeSet<String> {
    document
        .objects
        .values()
        .filter(|object| {
            object
                .properties
                .get(PREFAB_ID_PROPERTY)
                .map(String::as_str)
                == Some(prefab_id)
        })
        .filter_map(|object| object.properties.get(PREFAB_INSTANCE_ID_PROPERTY).cloned())
        .collect()
}

fn generated_object_ids_for_instance(document: &EditorDocument, instance_id: &str) -> Vec<String> {
    document
        .objects
        .values()
        .filter(|object| {
            object
                .properties
                .get(PREFAB_INSTANCE_ID_PROPERTY)
                .map(String::as_str)
                == Some(instance_id)
        })
        .map(|object| object.id.clone())
        .collect()
}

fn compose_transform(parent: Transform, child: Transform) -> Transform {
    let matrix = parent.matrix() * child.matrix();
    let (scale, rotation, translation) = matrix.to_scale_rotation_translation();
    Transform::from_trs(translation, rotation, scale)
}

fn path_with_leaf(prefix: &[String], leaf: &str) -> String {
    let mut parts = prefix.to_vec();
    parts.push(leaf.to_string());
    parts.join("/")
}

fn path_with_relative(prefix: &[String], relative: &str) -> String {
    if prefix.is_empty() {
        relative.to_string()
    } else {
        format!("{}/{}", prefix.join("/"), relative)
    }
}

pub(crate) fn validate_parent(
    ids: &BTreeSet<String>,
    field: &str,
    parent: &Option<String>,
) -> Result<(), EngineError> {
    if let Some(parent) = parent {
        validate_stable_id(field, parent)?;
        if !ids.contains(parent) {
            return invalid_prefab(field, format!("missing parent template {parent}"));
        }
    }
    Ok(())
}

pub(crate) fn validate_stable_id(field: &str, id: &str) -> Result<(), EngineError> {
    if id.trim().is_empty() {
        return invalid_prefab(field, "id must be non-empty");
    }
    if !id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return invalid_prefab(
            field,
            format!("id {id} must use only ASCII alnum, '_', '-', '.'"),
        );
    }
    Ok(())
}

pub(crate) fn validate_template_path(field: &str, path: &str) -> Result<(), EngineError> {
    if path.trim().is_empty() {
        return invalid_prefab(field, "template path must be non-empty");
    }
    for segment in path.split('/') {
        validate_stable_id(field, segment)?;
    }
    Ok(())
}

pub(crate) fn validate_property_key(field: &str, key: &str) -> Result<(), EngineError> {
    if key.trim().is_empty() {
        return invalid_prefab(field, "property key must be non-empty");
    }
    if key.starts_with("prefab.") {
        return invalid_prefab(
            field,
            format!("property key {key} uses reserved prefab namespace"),
        );
    }
    Ok(())
}

pub(crate) fn validate_transform(field: &str, transform: &Transform) -> Result<(), EngineError> {
    if !transform.translation.is_finite()
        || !transform.rotation.is_finite()
        || !transform.scale.is_finite()
    {
        return invalid_prefab(field, "transform must be finite");
    }
    Ok(())
}

pub(crate) fn invalid_prefab<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_prefab_error(field, reason))
}

pub(crate) fn invalid_prefab_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.prefabs.{}", field.into()),
        reason: reason.into(),
    }
}
