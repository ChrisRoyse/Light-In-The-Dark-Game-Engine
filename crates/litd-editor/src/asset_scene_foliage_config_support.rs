use super::*;

pub(super) fn validate_id(field: &'static str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return foliage_error(field, "value must be non-empty");
    }
    Ok(())
}

pub(super) fn validate_xz(field: &'static str, value: [f32; 2]) -> Result<(), EngineError> {
    if value.iter().any(|coord| !coord.is_finite()) {
        return foliage_error(field, "coordinates must be finite");
    }
    Ok(())
}

pub(super) fn validate_positive(field: &'static str, value: f32) -> Result<(), EngineError> {
    if !value.is_finite() || value <= 0.0 {
        return foliage_error(field, "value must be finite and positive");
    }
    Ok(())
}

pub(super) fn validate_finite(field: &'static str, value: f32) -> Result<(), EngineError> {
    if !value.is_finite() {
        return foliage_error(field, "value must be finite");
    }
    Ok(())
}

pub(super) fn validate_ordered_pair(
    field: &'static str,
    min: f32,
    max: f32,
) -> Result<(), EngineError> {
    if !min.is_finite() || !max.is_finite() || min <= 0.0 || max <= min {
        return foliage_error(field, "range must be finite, positive, and ordered");
    }
    Ok(())
}

pub(super) fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("litd-editor must live under the workspace crates directory")
        .to_path_buf()
}

pub(super) fn absolute_or_workspace_relative(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        workspace_root().join(path)
    }
}

pub(super) fn relative_to_workspace(path: &Path) -> String {
    path.strip_prefix(workspace_root())
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
        .replace('\\', "/")
}

pub(super) fn foliage_error_value(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_scene.foliage_config.{}", field.into()),
        reason: reason.into(),
    }
}

pub(super) fn foliage_error<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(foliage_error_value(field, reason))
}
