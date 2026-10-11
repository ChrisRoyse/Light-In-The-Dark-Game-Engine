use super::*;

pub(super) fn default_brush_state_path() -> PathBuf {
    workspace_root()
        .join("tmp")
        .join("litd-editor-foliage-brush-aster")
}

pub(super) fn brush_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: BRUSH_VAULT_ID,
        purpose: BRUSH_VAULT_PURPOSE,
        collection: BRUSH_COLLECTION,
        row_key: BRUSH_ROW_KEY,
        column: BRUSH_COLUMN,
    }
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

pub(super) fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("litd-editor must live under the workspace crates directory")
        .to_path_buf()
}

pub(super) fn persistence_error(field: &'static str, reason: String) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_scene.foliage_brush.io.{field}"),
        reason,
    }
}

pub(super) fn brush_error<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.asset_scene.foliage_brush.{}", field.into()),
        reason: reason.into(),
    })
}
