use std::path::PathBuf;

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::history::{AutosaveRecord, EditorDocument};

#[path = "editor_preferences_store.rs"]
mod editor_preferences_store;

pub const EDITOR_PREFERENCES_SCHEMA_VERSION: u32 = 1;
pub const EDITOR_PREFERENCES_RECORD_KIND: &str = "litd.editor.preferences";
pub const EDITOR_AUTOSAVE_RECORD_KIND: &str = "litd.editor.autosave_snapshot";
pub const EDITOR_PREFERENCES_ENV: &str = "LITD_EDITOR_PREFERENCES";
pub const EDITOR_AUTOSAVE_ASTER_ENV: &str = "LITD_EDITOR_AUTOSAVE_ASTER";
pub const MIN_EDITOR_TEXT_SCALE: f32 = 1.0;
pub const MAX_EDITOR_TEXT_SCALE: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorThemePreference {
    Dark,
    Light,
    HighContrast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorUnitsPreference {
    WarcraftTiles,
    Meters,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorAutosavePreferences {
    pub interval_seconds: Option<u32>,
    pub crash_recovery_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorPreferences {
    pub schema_version: u32,
    pub record_kind: String,
    pub units: EditorUnitsPreference,
    pub theme: EditorThemePreference,
    pub snap_grid_size: f32,
    pub performance_profile: String,
    #[serde(default = "default_editor_text_scale")]
    pub text_scale: f32,
    pub autosave: EditorAutosavePreferences,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorAutosaveSnapshot {
    pub schema_version: u32,
    pub record_kind: String,
    pub preferences_fingerprint: String,
    pub autosave: AutosaveRecord,
    pub document: EditorDocument,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreferencesLoadOutcome {
    pub preferences: EditorPreferences,
    pub loaded_from_disk: bool,
    pub recovered_from_corruption: bool,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreferenceFileReadback {
    pub path: String,
    pub bytes: usize,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutosaveWriteReport {
    pub status: AutosaveWriteStatus,
    pub document_id: String,
    pub stable_fingerprint: String,
    pub seq: u64,
    pub graph_rows: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutosaveWriteStatus {
    Written,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryChoice {
    Accept,
    Decline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecoveryReport {
    pub accepted: bool,
    pub document_id: String,
    pub recovered_fingerprint: String,
    pub document: Option<EditorDocument>,
}

#[derive(Debug, Clone)]
pub struct EditorPreferencesStore {
    preferences_path: PathBuf,
    autosave_aster_dir: PathBuf,
}

impl Default for EditorAutosavePreferences {
    fn default() -> Self {
        Self {
            interval_seconds: Some(60),
            crash_recovery_enabled: true,
        }
    }
}

impl Default for EditorPreferences {
    fn default() -> Self {
        Self {
            schema_version: EDITOR_PREFERENCES_SCHEMA_VERSION,
            record_kind: EDITOR_PREFERENCES_RECORD_KIND.to_string(),
            units: EditorUnitsPreference::WarcraftTiles,
            theme: EditorThemePreference::Dark,
            snap_grid_size: 1.0,
            performance_profile: "balanced".to_string(),
            text_scale: default_editor_text_scale(),
            autosave: EditorAutosavePreferences::default(),
        }
    }
}

impl EditorPreferences {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema_version != EDITOR_PREFERENCES_SCHEMA_VERSION {
            return invalid_preferences(
                "schema_version",
                format!(
                    "expected {EDITOR_PREFERENCES_SCHEMA_VERSION}, got {}",
                    self.schema_version
                ),
            );
        }
        if self.record_kind != EDITOR_PREFERENCES_RECORD_KIND {
            return invalid_preferences(
                "record_kind",
                format!(
                    "expected {EDITOR_PREFERENCES_RECORD_KIND}, got {}",
                    self.record_kind
                ),
            );
        }
        if !self.snap_grid_size.is_finite() || self.snap_grid_size <= 0.0 {
            return invalid_preferences("snap_grid_size", "snap grid size must be positive");
        }
        if self.performance_profile.trim().is_empty() {
            return invalid_preferences("performance_profile", "performance profile is required");
        }
        validate_editor_text_scale(self.text_scale)?;
        if let Some(seconds) = self.autosave.interval_seconds
            && !(5..=3600).contains(&seconds)
        {
            return invalid_preferences(
                "autosave.interval_seconds",
                "autosave interval must be 5..=3600 seconds or off",
            );
        }
        Ok(())
    }
}

pub fn validate_editor_text_scale(text_scale: f32) -> Result<(), EngineError> {
    if !text_scale.is_finite()
        || !(MIN_EDITOR_TEXT_SCALE..=MAX_EDITOR_TEXT_SCALE).contains(&text_scale)
    {
        return invalid_preferences(
            "text_scale",
            format!(
                "text scale must be finite and within {MIN_EDITOR_TEXT_SCALE}..={MAX_EDITOR_TEXT_SCALE}, got {text_scale}"
            ),
        );
    }
    Ok(())
}

fn default_editor_text_scale() -> f32 {
    1.0
}

fn invalid_preferences<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(preferences_error(field, reason))
}

fn preferences_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.preferences.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "editor_preferences_tests.rs"]
mod tests;
