//! Versioned Trigger Editor project persistence (#803).
//!
//! Persisted projects carry both a file-format schema and the stable fingerprint
//! of the complete authored project. Reopen validates the envelope, the project,
//! and the fingerprint before returning any state. Missing, corrupt, stale-schema,
//! and tampered files are errors; callers never receive an empty replacement.

use std::{fs, path::Path};

use litd_engine::EngineError;
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use super::{
    TriggerEditorProject,
    support::{trigger_editor_error, trigger_editor_error_value},
};

pub const TRIGGER_EDITOR_PERSISTENCE_SCHEMA: &str = "litd.editor.trigger_editor.persistence.v1";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TriggerEditorReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub project_fingerprint: String,
    pub graph_count: usize,
    pub event_count: usize,
    pub condition_count: usize,
    pub action_count: usize,
    pub project: TriggerEditorProject,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedTriggerEditorProject {
    schema: String,
    project_fingerprint: String,
    project: TriggerEditorProject,
}

pub fn save_trigger_editor_project(
    path: &Path,
    project: &TriggerEditorProject,
) -> Result<TriggerEditorReadback, EngineError> {
    project.validate()?;
    let persisted = PersistedTriggerEditorProject {
        schema: TRIGGER_EDITOR_PERSISTENCE_SCHEMA.to_string(),
        project_fingerprint: project.stable_fingerprint()?,
        project: project.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&persisted).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize Trigger Editor project: {error}"),
        }
    })?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            trigger_editor_error_value(
                "persistence.write.mkdir",
                format!("failed to create {}: {error}", parent.display()),
            )
        })?;
    }
    fs::write(path, &bytes).map_err(|error| {
        trigger_editor_error_value(
            "persistence.write",
            format!("failed to write {}: {error}", path.display()),
        )
    })?;
    let readback = read_trigger_editor_project(path)?;
    if readback.project != *project || readback.project_fingerprint != persisted.project_fingerprint
    {
        return trigger_editor_error(
            "persistence.readback",
            "saved project changed during durable readback",
        );
    }
    Ok(readback)
}

pub fn read_trigger_editor_project(path: &Path) -> Result<TriggerEditorReadback, EngineError> {
    let bytes = fs::read(path).map_err(|error| {
        trigger_editor_error_value(
            "persistence.read",
            format!("failed to read {}: {error}", path.display()),
        )
    })?;
    let persisted: PersistedTriggerEditorProject =
        serde_json::from_slice(&bytes).map_err(|error| {
            trigger_editor_error_value(
                "persistence.decode",
                format!(
                    "invalid Trigger Editor project JSON at {}: {error}",
                    path.display()
                ),
            )
        })?;
    if persisted.schema != TRIGGER_EDITOR_PERSISTENCE_SCHEMA {
        return trigger_editor_error(
            "persistence.schema",
            format!(
                "expected {TRIGGER_EDITOR_PERSISTENCE_SCHEMA}, got {}",
                persisted.schema
            ),
        );
    }
    persisted.project.validate()?;
    let actual_fingerprint = persisted.project.stable_fingerprint()?;
    if persisted.project_fingerprint != actual_fingerprint {
        return trigger_editor_error(
            "persistence.fingerprint",
            format!(
                "stored project fingerprint {} does not match actual {actual_fingerprint}",
                persisted.project_fingerprint
            ),
        );
    }
    Ok(readback(path, bytes, persisted.project, actual_fingerprint))
}

fn readback(
    path: &Path,
    bytes: Vec<u8>,
    project: TriggerEditorProject,
    project_fingerprint: String,
) -> TriggerEditorReadback {
    TriggerEditorReadback {
        path: path.display().to_string(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
        project_fingerprint,
        graph_count: project.graphs.len(),
        event_count: project.graphs.iter().map(|graph| graph.events.len()).sum(),
        condition_count: project
            .graphs
            .iter()
            .map(|graph| graph.conditions.len())
            .sum(),
        action_count: project.graphs.iter().map(|graph| graph.actions.len()).sum(),
        project,
    }
}
