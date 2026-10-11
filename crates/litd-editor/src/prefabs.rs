use std::collections::{BTreeMap, BTreeSet};

use litd_engine::{EngineError, math::Transform};
use serde::{Deserialize, Serialize};

use crate::history::stable_fingerprint;

#[path = "prefabs_support.rs"]
mod prefabs_support;
pub use prefabs_support::{apply_prefab_instance, prefab_generated_object_id};
use prefabs_support::{
    invalid_prefab, invalid_prefab_error, validate_parent, validate_property_key,
    validate_stable_id, validate_template_path, validate_transform,
};

pub const PREFAB_ID_PROPERTY: &str = "prefab.id";
pub const PREFAB_INSTANCE_ID_PROPERTY: &str = "prefab.instance_id";
pub const PREFAB_TEMPLATE_PATH_PROPERTY: &str = "prefab.template_path";
pub const PREFAB_ROOT_FINGERPRINT_PROPERTY: &str = "prefab.root_fingerprint";
pub const PREFAB_TEMPLATE_PREFAB_ID_PROPERTY: &str = "prefab.template_prefab_id";
pub const PREFAB_TEMPLATE_FINGERPRINT_PROPERTY: &str = "prefab.template_fingerprint";
pub const PREFAB_OVERRIDE_KEYS_PROPERTY: &str = "prefab.override_keys";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrefabDefinition {
    pub prefab_id: String,
    pub name: String,
    pub source_issue: String,
    pub templates: Vec<PrefabObjectTemplate>,
    pub nested_prefabs: Vec<PrefabNestedInstance>,
    pub content_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrefabObjectTemplate {
    pub template_id: String,
    pub kind: String,
    pub parent_template_id: Option<String>,
    pub transform: Transform,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrefabNestedInstance {
    pub template_id: String,
    pub prefab_id: String,
    pub parent_template_id: Option<String>,
    pub transform: Transform,
    pub overrides: Vec<PrefabPropertyOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrefabPropertyOverride {
    pub template_path: String,
    pub property_key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrefabInstance {
    pub instance_id: String,
    pub prefab_id: String,
    pub source_fingerprint: String,
    pub parent_object_id: Option<String>,
    pub transform: Transform,
    pub overrides: Vec<PrefabPropertyOverride>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrefabLibrary {
    pub max_instances_per_prefab: usize,
    pub definitions: BTreeMap<String, PrefabDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrefabExpansionReport {
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub prefab_id: String,
    pub instance_id: String,
    pub previous_source_fingerprint: String,
    pub source_fingerprint: String,
    pub source_changed: bool,
    pub instance_count: usize,
    pub created_or_updated_object_ids: Vec<String>,
    pub removed_stale_object_ids: Vec<String>,
    pub preserved_override_count: usize,
}

impl PrefabDefinition {
    pub fn new(
        prefab_id: impl Into<String>,
        name: impl Into<String>,
        source_issue: impl Into<String>,
        templates: Vec<PrefabObjectTemplate>,
        nested_prefabs: Vec<PrefabNestedInstance>,
    ) -> Result<Self, EngineError> {
        let mut definition = Self {
            prefab_id: prefab_id.into(),
            name: name.into(),
            source_issue: source_issue.into(),
            templates,
            nested_prefabs,
            content_fingerprint: String::new(),
        };
        definition.content_fingerprint = definition.expected_fingerprint()?;
        definition.validate_content()?;
        Ok(definition)
    }

    pub fn expected_fingerprint(&self) -> Result<String, EngineError> {
        #[derive(Serialize)]
        struct FingerprintInput<'a> {
            prefab_id: &'a str,
            name: &'a str,
            source_issue: &'a str,
            templates: &'a [PrefabObjectTemplate],
            nested_prefabs: &'a [PrefabNestedInstance],
        }

        stable_fingerprint(&FingerprintInput {
            prefab_id: &self.prefab_id,
            name: &self.name,
            source_issue: &self.source_issue,
            templates: &self.templates,
            nested_prefabs: &self.nested_prefabs,
        })
    }

    pub(crate) fn validate_content(&self) -> Result<(), EngineError> {
        validate_stable_id("definition.prefab_id", &self.prefab_id)?;
        if self.name.trim().is_empty() {
            return invalid_prefab("definition.name", "name must be non-empty");
        }
        if self.source_issue.trim().is_empty() {
            return invalid_prefab("definition.source_issue", "source issue must be non-empty");
        }
        if self.templates.is_empty() && self.nested_prefabs.is_empty() {
            return invalid_prefab(
                "definition.templates",
                "prefab must define at least one object",
            );
        }
        let expected = self.expected_fingerprint()?;
        if self.content_fingerprint != expected {
            return invalid_prefab(
                "definition.content_fingerprint",
                format!(
                    "fingerprint mismatch for {}: stored {}, expected {}",
                    self.prefab_id, self.content_fingerprint, expected
                ),
            );
        }

        let mut direct_ids = BTreeSet::new();
        let mut object_ids = BTreeSet::new();
        for template in &self.templates {
            template.validate()?;
            if !direct_ids.insert(template.template_id.clone()) {
                return invalid_prefab(
                    "definition.templates",
                    format!("duplicate template id {}", template.template_id),
                );
            }
            object_ids.insert(template.template_id.clone());
        }
        for nested in &self.nested_prefabs {
            nested.validate()?;
            if nested.prefab_id == self.prefab_id {
                return invalid_prefab(
                    "definition.nested_prefabs",
                    "prefab cannot directly nest itself",
                );
            }
            if !direct_ids.insert(nested.template_id.clone()) {
                return invalid_prefab(
                    "definition.nested_prefabs",
                    format!("duplicate template id {}", nested.template_id),
                );
            }
        }
        for template in &self.templates {
            validate_parent(
                &object_ids,
                "template.parent_template_id",
                &template.parent_template_id,
            )?;
        }
        for nested in &self.nested_prefabs {
            validate_parent(
                &object_ids,
                "nested.parent_template_id",
                &nested.parent_template_id,
            )?;
        }
        Ok(())
    }
}

impl PrefabObjectTemplate {
    pub fn new(
        template_id: impl Into<String>,
        kind: impl Into<String>,
        parent_template_id: Option<String>,
        transform: Transform,
    ) -> Self {
        Self {
            template_id: template_id.into(),
            kind: kind.into(),
            parent_template_id,
            transform,
            properties: BTreeMap::new(),
        }
    }

    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    pub(crate) fn validate(&self) -> Result<(), EngineError> {
        validate_stable_id("template.template_id", &self.template_id)?;
        if self.kind.trim().is_empty() {
            return invalid_prefab("template.kind", "kind must be non-empty");
        }
        validate_transform("template.transform", &self.transform)?;
        for key in self.properties.keys() {
            validate_property_key("template.properties", key)?;
        }
        Ok(())
    }
}

impl PrefabNestedInstance {
    pub fn new(
        template_id: impl Into<String>,
        prefab_id: impl Into<String>,
        parent_template_id: Option<String>,
        transform: Transform,
    ) -> Self {
        Self {
            template_id: template_id.into(),
            prefab_id: prefab_id.into(),
            parent_template_id,
            transform,
            overrides: Vec::new(),
        }
    }

    pub fn with_override(mut self, override_: PrefabPropertyOverride) -> Self {
        self.overrides.push(override_);
        self
    }

    pub(crate) fn validate(&self) -> Result<(), EngineError> {
        validate_stable_id("nested.template_id", &self.template_id)?;
        validate_stable_id("nested.prefab_id", &self.prefab_id)?;
        validate_transform("nested.transform", &self.transform)?;
        for override_ in &self.overrides {
            override_.validate()?;
        }
        Ok(())
    }
}

impl PrefabPropertyOverride {
    pub fn new(
        template_path: impl Into<String>,
        property_key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let override_ = Self {
            template_path: template_path.into(),
            property_key: property_key.into(),
            value: value.into(),
        };
        override_.validate()?;
        Ok(override_)
    }

    pub(crate) fn validate(&self) -> Result<(), EngineError> {
        validate_template_path("override.template_path", &self.template_path)?;
        validate_property_key("override.property_key", &self.property_key)
    }
}

impl PrefabInstance {
    pub fn with_parent(mut self, parent_object_id: impl Into<String>) -> Self {
        self.parent_object_id = Some(parent_object_id.into());
        self
    }

    pub fn with_override(mut self, override_: PrefabPropertyOverride) -> Self {
        self.overrides.push(override_);
        self
    }

    pub(crate) fn validate(&self) -> Result<(), EngineError> {
        validate_stable_id("instance.instance_id", &self.instance_id)?;
        validate_stable_id("instance.prefab_id", &self.prefab_id)?;
        if self.source_fingerprint.trim().is_empty() {
            return invalid_prefab(
                "instance.source_fingerprint",
                "source fingerprint must be non-empty",
            );
        }
        if let Some(parent) = &self.parent_object_id {
            validate_stable_id("instance.parent_object_id", parent)?;
        }
        validate_transform("instance.transform", &self.transform)?;
        for override_ in &self.overrides {
            override_.validate()?;
        }
        Ok(())
    }
}

impl PrefabLibrary {
    pub fn new(max_instances_per_prefab: usize) -> Result<Self, EngineError> {
        if max_instances_per_prefab == 0 {
            return invalid_prefab(
                "library.max_instances_per_prefab",
                "must be greater than zero",
            );
        }
        Ok(Self {
            max_instances_per_prefab,
            definitions: BTreeMap::new(),
        })
    }

    pub fn insert_definition(&mut self, definition: PrefabDefinition) -> Result<(), EngineError> {
        definition.validate_content()?;
        let previous = self.definitions.get(&definition.prefab_id).cloned();
        self.definitions
            .insert(definition.prefab_id.clone(), definition.clone());
        if let Err(error) = self.validate_definition_graph(&definition.prefab_id) {
            if let Some(previous) = previous {
                self.definitions
                    .insert(definition.prefab_id.clone(), previous);
            } else {
                self.definitions.remove(&definition.prefab_id);
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn instantiate(
        &self,
        instance_id: impl Into<String>,
        prefab_id: impl Into<String>,
        transform: Transform,
    ) -> Result<PrefabInstance, EngineError> {
        let prefab_id = prefab_id.into();
        let definition = self.definition(&prefab_id)?;
        let instance = PrefabInstance {
            instance_id: instance_id.into(),
            prefab_id,
            source_fingerprint: definition.content_fingerprint.clone(),
            parent_object_id: None,
            transform,
            overrides: Vec::new(),
        };
        instance.validate()?;
        Ok(instance)
    }

    pub(crate) fn definition(&self, prefab_id: &str) -> Result<&PrefabDefinition, EngineError> {
        self.definitions.get(prefab_id).ok_or_else(|| {
            invalid_prefab_error(
                "library.definitions",
                format!("missing prefab definition {prefab_id}"),
            )
        })
    }

    fn validate_definition_graph(&self, prefab_id: &str) -> Result<(), EngineError> {
        let mut stack = Vec::new();
        self.visit_definition(prefab_id, &mut stack)
    }

    fn visit_definition(
        &self,
        prefab_id: &str,
        stack: &mut Vec<String>,
    ) -> Result<(), EngineError> {
        if stack.iter().any(|item| item == prefab_id) {
            return invalid_prefab(
                "definition.nested_prefabs",
                format!("nested prefab cycle: {}", stack.join(" -> ")),
            );
        }
        let definition = self.definition(prefab_id)?;
        stack.push(prefab_id.to_string());
        for nested in &definition.nested_prefabs {
            self.visit_definition(&nested.prefab_id, stack)?;
        }
        stack.pop();
        Ok(())
    }
}

#[cfg(test)]
#[path = "prefabs_tests.rs"]
mod tests;
