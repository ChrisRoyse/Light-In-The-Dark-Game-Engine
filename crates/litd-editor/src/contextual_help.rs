//! One project-grounded help path for a creator's current question.
//!
//! The resolver deliberately returns one real editor command, not a menu of
//! documentation links. Its project facts and target come from the validated
//! [`EditorDocument`] supplied by the caller.

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::{
    editor_commands::{
        COMMAND_CREATE_WORLD, COMMAND_MODE_AI_COAUTHOR, COMMAND_MODE_CINEMATICS,
        COMMAND_MODE_OBJECTS, COMMAND_MODE_TERRAIN, COMMAND_MODE_TRIGGERS, COMMAND_MODE_VFX,
        COMMAND_TOGGLE_PLAYTEST, COMMAND_VALIDATE_WORLD, default_editor_command_registry,
    },
    history::{EditorDocument, stable_fingerprint},
    shell::EditorMode,
};

pub const CONTEXTUAL_HELP_SCHEMA: &str = "litd.editor.contextual_help.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextualHelpRequest {
    pub query: String,
    pub active_mode: EditorMode,
    #[serde(default)]
    pub selected_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextualHelpProjectContext {
    pub document_id: String,
    pub revision_id: String,
    pub document_fingerprint: String,
    pub active_mode: EditorMode,
    pub object_count: usize,
    pub region_count: usize,
    pub named_camera_count: usize,
    pub cinematic_count: usize,
    pub selected_id: Option<String>,
    pub selected_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextualHelpRecommendation {
    pub headline: String,
    pub command_id: String,
    pub command_title: String,
    pub gui_route: String,
    pub target_id: String,
    pub target_kind: String,
    pub why_this_step: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextualHelpAnswer {
    pub schema: String,
    pub source_issue: u32,
    pub query: String,
    pub context: ContextualHelpProjectContext,
    pub recommendation: ContextualHelpRecommendation,
}

pub fn resolve_contextual_help(
    document: &EditorDocument,
    request: ContextualHelpRequest,
) -> Result<ContextualHelpAnswer, EngineError> {
    document.validate()?;
    validate_query(&request.query)?;
    let selected_kind = request
        .selected_id
        .as_deref()
        .map(|id| selected_kind(document, id))
        .transpose()?;
    let command_id = choose_command(
        &request.query,
        request.active_mode,
        selected_kind.as_deref(),
    );
    let registry = default_editor_command_registry()?;
    let command = registry.get(command_id).ok_or_else(|| {
        contextual_help_error(
            "command",
            format!("recommended command {command_id} is missing from the editor registry"),
        )
    })?;
    let (target_id, target_kind) = recommendation_target(
        document,
        request.selected_id.as_deref(),
        selected_kind.as_deref(),
        command_id,
    );
    let context = ContextualHelpProjectContext {
        document_id: document.calyx_document_id.clone(),
        revision_id: document.revision_id.clone(),
        document_fingerprint: stable_fingerprint(document)?,
        active_mode: request.active_mode,
        object_count: document.objects.len(),
        region_count: document.regions.len(),
        named_camera_count: document.named_cameras.len(),
        cinematic_count: document.cinematic_timelines.len(),
        selected_id: request.selected_id.clone(),
        selected_kind,
    };
    let recommendation = ContextualHelpRecommendation {
        headline: recommendation_headline(command_id).to_string(),
        command_id: command.command_id.clone(),
        command_title: command.title.clone(),
        gui_route: command.gui_route.clone(),
        target_id,
        target_kind,
        why_this_step: format!(
            "One next step for {} mode, grounded in project {} ({} objects, {} regions).",
            request.active_mode.label(),
            document.world_properties.display_name,
            context.object_count,
            context.region_count
        ),
    };
    Ok(ContextualHelpAnswer {
        schema: CONTEXTUAL_HELP_SCHEMA.to_string(),
        source_issue: 900,
        query: request.query.trim().to_string(),
        context,
        recommendation,
    })
}

fn choose_command(
    query: &str,
    active_mode: EditorMode,
    selected_kind: Option<&str>,
) -> &'static str {
    let query = query.to_ascii_lowercase();
    if contains_any(&query, &["error", "fix", "validate", "check", "broken"]) {
        COMMAND_VALIDATE_WORLD
    } else if contains_any(&query, &["play", "test", "preview", "see it", "live"]) {
        COMMAND_TOGGLE_PLAYTEST
    } else if contains_any(&query, &["new world", "start", "template"]) {
        COMMAND_CREATE_WORLD
    } else if contains_any(&query, &["cinematic", "cutscene", "story", "camera"]) {
        COMMAND_MODE_CINEMATICS
    } else if contains_any(&query, &["effect", "particle", "vfx"]) {
        COMMAND_MODE_VFX
    } else if contains_any(&query, &["trigger", "logic", "event", "condition"]) {
        COMMAND_MODE_TRIGGERS
    } else if contains_any(&query, &["terrain", "ground", "map", "region"]) {
        COMMAND_MODE_TERRAIN
    } else if contains_any(&query, &["object", "unit", "character", "prop"]) {
        COMMAND_MODE_OBJECTS
    } else if contains_any(&query, &["ai", "co-author", "coauthor"]) {
        COMMAND_MODE_AI_COAUTHOR
    } else {
        command_for_context(active_mode, selected_kind)
    }
}

fn command_for_context(active_mode: EditorMode, selected_kind: Option<&str>) -> &'static str {
    match selected_kind {
        Some("object") => COMMAND_MODE_OBJECTS,
        Some("region") => COMMAND_MODE_TERRAIN,
        Some("named_camera" | "cinematic") => COMMAND_MODE_CINEMATICS,
        _ => match active_mode {
            EditorMode::Terrain => COMMAND_MODE_TERRAIN,
            EditorMode::Objects => COMMAND_MODE_OBJECTS,
            EditorMode::Triggers => COMMAND_MODE_TRIGGERS,
            EditorMode::Cinematics => COMMAND_MODE_CINEMATICS,
            EditorMode::Vfx => COMMAND_MODE_VFX,
            EditorMode::AiCoAuthor => COMMAND_MODE_AI_COAUTHOR,
        },
    }
}

fn selected_kind(document: &EditorDocument, selected_id: &str) -> Result<String, EngineError> {
    let kind = if document.objects.contains_key(selected_id) {
        "object"
    } else if document.regions.contains_key(selected_id) {
        "region"
    } else if document.named_cameras.contains_key(selected_id) {
        "named_camera"
    } else if document.cinematic_timelines.contains_key(selected_id) {
        "cinematic"
    } else {
        return Err(contextual_help_error(
            "selected_id",
            format!("selected id {selected_id} does not exist in the active project"),
        ));
    };
    Ok(kind.to_string())
}

fn recommendation_target(
    document: &EditorDocument,
    selected_id: Option<&str>,
    selected_kind: Option<&str>,
    command_id: &str,
) -> (String, String) {
    if let (Some(id), Some(kind)) = (selected_id, selected_kind) {
        return (id.to_string(), kind.to_string());
    }
    match command_id {
        COMMAND_MODE_OBJECTS => document
            .objects
            .keys()
            .next()
            .map(|id| (id.clone(), "object".to_string())),
        COMMAND_MODE_TERRAIN => document
            .regions
            .keys()
            .next()
            .map(|id| (id.clone(), "region".to_string())),
        COMMAND_MODE_CINEMATICS => document
            .cinematic_timelines
            .keys()
            .next()
            .map(|id| (id.clone(), "cinematic".to_string()))
            .or_else(|| {
                document
                    .named_cameras
                    .keys()
                    .next()
                    .map(|id| (id.clone(), "named_camera".to_string()))
            }),
        _ => None,
    }
    .unwrap_or_else(|| (document.calyx_document_id.clone(), "project".to_string()))
}

fn recommendation_headline(command_id: &str) -> &'static str {
    match command_id {
        COMMAND_CREATE_WORLD => "Start with one opinionated world",
        COMMAND_VALIDATE_WORLD => "Run one clear world check",
        COMMAND_TOGGLE_PLAYTEST => "See the current world live",
        COMMAND_MODE_TERRAIN => "Shape the part of the world that matters",
        COMMAND_MODE_OBJECTS => "Work on one visible object",
        COMMAND_MODE_TRIGGERS => "Build one cause-and-effect rule",
        COMMAND_MODE_CINEMATICS => "Tell the next story beat",
        COMMAND_MODE_VFX => "Polish one visible effect",
        COMMAND_MODE_AI_COAUTHOR => "Ask the co-author about this project",
        _ => "Take one grounded next step",
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn validate_query(query: &str) -> Result<(), EngineError> {
    if query.trim().is_empty() || query.contains(['\r', '\n', '\t']) || query.chars().count() > 280
    {
        return Err(contextual_help_error(
            "query",
            "help query must be non-empty, single-line, and at most 280 characters",
        ));
    }
    Ok(())
}

fn contextual_help_error(field: &str, reason: impl Into<String>) -> EngineError {
    let reason = reason.into();
    tracing::error!(
        target: "litd_editor::contextual_help",
        field,
        reason,
        code = "LITD-CONTEXTUAL-HELP-INVALID",
        "contextual help request rejected"
    );
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.contextual_help.{field}"),
        reason,
    }
}

#[cfg(test)]
#[path = "contextual_help_tests.rs"]
mod tests;
