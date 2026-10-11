//! Editor world preflight validation for play and publish gates.

use std::collections::BTreeSet;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::{
    asset_products::{
        AssetProductKind, MODEL_PRODUCT_ID_PROPERTY, MODEL_SOURCE_PATH_PROPERTY,
        MODEL_SOURCE_SHA256_PROPERTY,
    },
    history::EditorDocument,
};

pub const WORLD_VALIDATION_SCHEMA_VERSION: u32 = 1;
pub const WORLD_VALIDATION_SOURCE_ISSUE: u32 = 1218;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldValidationUseCase {
    Edit,
    Play,
    Publish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldValidationSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldValidationIssue {
    pub code: String,
    pub severity: WorldValidationSeverity,
    pub subject: String,
    pub message: String,
    pub fix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldValidationReport {
    pub schema_version: u32,
    pub source_issue: u32,
    pub use_case: WorldValidationUseCase,
    pub issue_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub blocking: bool,
    pub summary: String,
    pub issues: Vec<WorldValidationIssue>,
}

impl WorldValidationReport {
    pub fn ok(use_case: WorldValidationUseCase) -> Self {
        Self::from_issues(use_case, Vec::new())
    }

    fn from_issues(use_case: WorldValidationUseCase, issues: Vec<WorldValidationIssue>) -> Self {
        let error_count = issues
            .iter()
            .filter(|issue| issue.severity == WorldValidationSeverity::Error)
            .count();
        let warning_count = issues.len().saturating_sub(error_count);
        let blocking = error_count > 0;
        let summary = if issues.is_empty() {
            "ok: no world validation issues".to_string()
        } else {
            issues
                .iter()
                .map(|issue| format!("{}:{}:{}", issue.code, issue.subject, issue.fix))
                .collect::<Vec<_>>()
                .join(" | ")
        };
        Self {
            schema_version: WORLD_VALIDATION_SCHEMA_VERSION,
            source_issue: WORLD_VALIDATION_SOURCE_ISSUE,
            use_case,
            issue_count: issues.len(),
            error_count,
            warning_count,
            blocking,
            summary,
            issues,
        }
    }

    pub fn error_summary(&self) -> String {
        self.issues
            .iter()
            .filter(|issue| issue.severity == WorldValidationSeverity::Error)
            .map(|issue| format!("{}: {}", issue.subject, issue.message))
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

pub fn validate_editor_world(
    document: &EditorDocument,
    use_case: WorldValidationUseCase,
) -> WorldValidationReport {
    let mut issues = Vec::new();
    validate_document_identity(document, &mut issues);
    validate_object_graph(document, &mut issues);
    validate_property_references(document, &mut issues);
    validate_play_requirements(document, use_case, &mut issues);
    validate_quest_soft_locks(document, &mut issues);
    validate_structural_document(document, &mut issues);
    WorldValidationReport::from_issues(use_case, issues)
}

pub fn require_editor_world_valid(
    document: &EditorDocument,
    use_case: WorldValidationUseCase,
) -> Result<WorldValidationReport, EngineError> {
    let report = validate_editor_world(document, use_case);
    if report.blocking {
        return Err(EngineError::UiRenderInvalidConfig {
            field: "editor.validation.blocking".to_string(),
            reason: report.error_summary(),
        });
    }
    document.validate()?;
    Ok(report)
}

fn validate_document_identity(document: &EditorDocument, issues: &mut Vec<WorldValidationIssue>) {
    if document.calyx_document_id.trim().is_empty() || document.revision_id.trim().is_empty() {
        push_error(
            issues,
            "LITD-VAL-DOCUMENT-ID",
            "document",
            "document and revision ids are required before validation can be persisted",
            "set non-empty calyx_document_id and revision_id",
        );
    }
}

fn validate_object_graph(document: &EditorDocument, issues: &mut Vec<WorldValidationIssue>) {
    let world_count = document
        .objects
        .values()
        .filter(|object| object.kind == "World")
        .count();
    if world_count == 0 {
        push_error(
            issues,
            "LITD-VAL-WORLD-MISSING",
            "world",
            "world has no root World object",
            "create one World root object and parent map objects under it",
        );
    }
    for object in document.objects.values() {
        if let Some(parent) = &object.parent
            && !document.objects.contains_key(parent)
        {
            push_error(
                issues,
                "LITD-VAL-DANGLING-PARENT",
                &object.id,
                format!("object {} references missing parent {parent}", object.id),
                format!("create parent {parent} or clear {}.parent", object.id),
            );
        }
    }
    validate_parent_cycles(document, issues);
}

fn validate_parent_cycles(document: &EditorDocument, issues: &mut Vec<WorldValidationIssue>) {
    for object in document.objects.values() {
        let mut seen = BTreeSet::new();
        let mut current = object.parent.as_deref();
        while let Some(parent) = current {
            if !seen.insert(parent.to_string()) {
                push_error(
                    issues,
                    "LITD-VAL-PARENT-CYCLE",
                    &object.id,
                    format!("object {} is in a parent cycle through {parent}", object.id),
                    "break the parent chain so every object reaches a root World object",
                );
                break;
            }
            current = document
                .objects
                .get(parent)
                .and_then(|next| next.parent.as_deref());
        }
    }
}

fn validate_property_references(document: &EditorDocument, issues: &mut Vec<WorldValidationIssue>) {
    for object in document.objects.values() {
        for (key, value) in &object.properties {
            if is_object_ref_key(key) && !document.objects.contains_key(value) {
                push_error(
                    issues,
                    "LITD-VAL-DANGLING-OBJECT-REF",
                    &object.id,
                    format!("property {key} references missing object {value}"),
                    format!("create object {value} or update {}.{key}", object.id),
                );
            }
            if is_region_ref_key(key) && !document.regions.contains_key(value) {
                push_error(
                    issues,
                    "LITD-VAL-DANGLING-REGION-REF",
                    &object.id,
                    format!("property {key} references missing region {value}"),
                    format!("create region {value} or update {}.{key}", object.id),
                );
            }
        }
    }
}

fn validate_play_requirements(
    document: &EditorDocument,
    use_case: WorldValidationUseCase,
    issues: &mut Vec<WorldValidationIssue>,
) {
    if !matches!(
        use_case,
        WorldValidationUseCase::Play | WorldValidationUseCase::Publish
    ) {
        return;
    }
    let units = document
        .objects
        .values()
        .filter(|object| object.kind == "Unit")
        .collect::<Vec<_>>();
    if units.is_empty() {
        push_error(
            issues,
            "LITD-VAL-PLAY-NO-UNITS",
            "play",
            "Playtest needs at least one Unit object to spawn and anchor the camera",
            "add a Unit object with a valid mesh assignment",
        );
    }
    for unit in units {
        validate_unit_model(document, &unit.id, &unit.properties, issues);
        validate_unit_damage_stat(&unit.id, &unit.properties, issues);
    }
}

fn validate_unit_model(
    document: &EditorDocument,
    unit_id: &str,
    properties: &std::collections::BTreeMap<String, String>,
    issues: &mut Vec<WorldValidationIssue>,
) {
    let Some(product_id) = properties.get(MODEL_PRODUCT_ID_PROPERTY) else {
        push_error(
            issues,
            "LITD-VAL-UNIT-MISSING-MODEL",
            unit_id,
            "unit has no model_product_id and cannot produce a play presentation body",
            format!("assign a real mesh asset product to unit {unit_id}"),
        );
        return;
    };
    let Some(product) = document.asset_products.get(product_id) else {
        push_error(
            issues,
            "LITD-VAL-UNIT-MISSING-PRODUCT",
            unit_id,
            format!("unit references missing asset product {product_id}"),
            format!("restore product {product_id} or reassign {unit_id} to an existing mesh"),
        );
        return;
    };
    if product.product_kind != AssetProductKind::Mesh {
        push_error(
            issues,
            "LITD-VAL-UNIT-NON-MESH",
            unit_id,
            "unit model_product_id must reference a Mesh product",
            format!("replace {product_id} with a Mesh asset product"),
        );
    }
    if properties.get(MODEL_SOURCE_SHA256_PROPERTY) != Some(&product.source_sha256)
        || properties.get(MODEL_SOURCE_PATH_PROPERTY) != Some(&product.source_relative_path)
    {
        push_error(
            issues,
            "LITD-VAL-UNIT-PROVENANCE-MISMATCH",
            unit_id,
            "unit model provenance does not match the assigned asset product",
            format!("refresh model provenance properties on {unit_id}"),
        );
    }
}

fn validate_unit_damage_stat(
    unit_id: &str,
    properties: &std::collections::BTreeMap<String, String>,
    issues: &mut Vec<WorldValidationIssue>,
) {
    let Some(value) = properties.get("stat.damage") else {
        return;
    };
    let Ok(damage) = value.parse::<i64>() else {
        push_error(
            issues,
            "LITD-VAL-UNIT-DAMAGE-INVALID",
            unit_id,
            format!("unit stat.damage value {value:?} is not a positive integer"),
            format!("set {unit_id}.stat.damage to a positive integer"),
        );
        return;
    };
    if damage <= 0 {
        push_error(
            issues,
            "LITD-VAL-UNIT-DAMAGE-INVALID",
            unit_id,
            format!("unit stat.damage value {damage} is not a positive integer"),
            format!("set {unit_id}.stat.damage to a positive integer"),
        );
    }
}

fn validate_quest_soft_locks(document: &EditorDocument, issues: &mut Vec<WorldValidationIssue>) {
    for quest in document
        .objects
        .values()
        .filter(|object| object.kind == "Quest")
    {
        for required in ["start_object", "complete_object"] {
            let Some(target) = quest.properties.get(required) else {
                push_error(
                    issues,
                    "LITD-VAL-QUEST-MISSING-ENDPOINT",
                    &quest.id,
                    format!("quest is missing required {required} endpoint"),
                    format!("set {}.{required} to an existing object id", quest.id),
                );
                continue;
            };
            if !document.objects.contains_key(target) {
                push_error(
                    issues,
                    "LITD-VAL-QUEST-DANGLING-ENDPOINT",
                    &quest.id,
                    format!("quest {required} references missing object {target}"),
                    format!("create object {target} or update {}.{required}", quest.id),
                );
            }
        }
        if let Some(item) = quest.properties.get("required_item")
            && !document
                .objects
                .get(item)
                .map(|object| object.kind == "Item")
                .unwrap_or(false)
        {
            push_error(
                issues,
                "LITD-VAL-QUEST-UNOBTAINABLE-ITEM",
                &quest.id,
                format!("quest requires item {item}, but no Item object with that id exists"),
                format!("create Item {item} or remove {}.required_item", quest.id),
            );
        }
    }
}

fn validate_structural_document(document: &EditorDocument, issues: &mut Vec<WorldValidationIssue>) {
    if let Err(error) = document.validate() {
        push_error(
            issues,
            "LITD-VAL-DOCUMENT-STRUCTURAL",
            "document",
            format!("editor document failed structural validation: {error}"),
            "fix the document-level validation error before play or publish",
        );
    }
}

fn is_object_ref_key(key: &str) -> bool {
    matches!(
        key,
        "requires_object" | "target_object" | "spawn_object" | "unlock_object"
    ) || key.ends_with("_object_ref")
}

fn is_region_ref_key(key: &str) -> bool {
    matches!(key, "requires_region" | "target_region" | "spawn_region")
        || key.ends_with("_region_ref")
}

fn push_error(
    issues: &mut Vec<WorldValidationIssue>,
    code: &str,
    subject: impl Into<String>,
    message: impl Into<String>,
    fix: impl Into<String>,
) {
    issues.push(WorldValidationIssue {
        code: code.to_string(),
        severity: WorldValidationSeverity::Error,
        subject: subject.into(),
        message: message.into(),
        fix: fix.into(),
    });
}

#[cfg(test)]
#[path = "world_validation_tests.rs"]
mod tests;
