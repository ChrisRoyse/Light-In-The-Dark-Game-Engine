use std::{
    env, fs,
    path::{Component, Path, PathBuf},
};

use serde::Deserialize;

pub const GAMEEDITOR_CALYX_ROOT: &str = r"C:\code\GameEditor\Calyx";
pub const ENGINE_CONTENT_MEASUREMENT_VAULT_ENV: &str = "LITD_ENGINE_CALYX_CONTENT_PANEL_VAULT";
pub const ENGINE_CONTENT_PANEL_VAULT_NAME: &str = "gameeditor-engine-content-panel";

pub const CALYX_CANONICAL_ROOT_MISSING: &str = "LITD_CALYX_CANONICAL_ROOT_MISSING";
pub const CALYX_CANONICAL_ROOT_ESCAPE: &str = "LITD_CALYX_CANONICAL_ROOT_ESCAPE";
pub const CALYX_PATH_NOT_ABSOLUTE: &str = "LITD_CALYX_PATH_NOT_ABSOLUTE";
pub const CALYX_PATH_TRAVERSAL: &str = "LITD_CALYX_PATH_TRAVERSAL";
pub const CALYX_PATH_OUTSIDE_ROOT: &str = "LITD_CALYX_PATH_OUTSIDE_CANONICAL_ROOT";
pub const CALYX_PATH_MISSING: &str = "LITD_CALYX_CANONICAL_PATH_MISSING";
pub const CALYX_PATH_ESCAPE: &str = "LITD_CALYX_CANONICAL_PATH_ESCAPE";
pub const CALYX_VAULT_INDEX_INVALID: &str = "LITD_CALYX_VAULT_INDEX_INVALID";
pub const CALYX_ENGINE_PANEL_VAULT_MISSING: &str = "LITD_CALYX_ENGINE_PANEL_VAULT_MISSING";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct GameEditorCalyxPathError {
    pub code: &'static str,
    pub message: String,
}

impl GameEditorCalyxPathError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct VaultIndex {
    #[serde(default)]
    vaults: Vec<VaultIndexEntry>,
}

#[derive(Debug, Deserialize)]
struct VaultIndexEntry {
    name: String,
    vault_id: String,
    path: String,
}

pub fn canonical_gameeditor_calyx_root() -> Result<PathBuf, GameEditorCalyxPathError> {
    canonical_root(Path::new(GAMEEDITOR_CALYX_ROOT))
}

pub fn validate_gameeditor_calyx_home(
    path: impl AsRef<Path>,
) -> Result<PathBuf, GameEditorCalyxPathError> {
    let root = canonical_gameeditor_calyx_root()?;
    validate_existing_path_under_root(&root, path.as_ref(), "CALYX_HOME")
}

pub fn validate_existing_gameeditor_calyx_path(
    path: impl AsRef<Path>,
    label: &str,
) -> Result<PathBuf, GameEditorCalyxPathError> {
    let root = canonical_gameeditor_calyx_root()?;
    validate_existing_path_under_root(&root, path.as_ref(), label)
}

pub fn resolve_engine_content_panel_vault() -> Result<PathBuf, GameEditorCalyxPathError> {
    let root = canonical_gameeditor_calyx_root()?;
    if let Some(configured) = env::var_os(ENGINE_CONTENT_MEASUREMENT_VAULT_ENV) {
        if configured.is_empty() {
            return Err(GameEditorCalyxPathError::new(
                CALYX_PATH_MISSING,
                format!("{ENGINE_CONTENT_MEASUREMENT_VAULT_ENV} is set but empty"),
            ));
        }
        return validate_existing_path_under_root(
            &root,
            Path::new(&configured),
            ENGINE_CONTENT_MEASUREMENT_VAULT_ENV,
        );
    }
    resolve_named_vault_under_root(&root, ENGINE_CONTENT_PANEL_VAULT_NAME)
}

fn canonical_root(root: &Path) -> Result<PathBuf, GameEditorCalyxPathError> {
    let resolved = fs::canonicalize(root).map_err(|error| {
        GameEditorCalyxPathError::new(
            CALYX_CANONICAL_ROOT_MISSING,
            format!(
                "canonical GameEditor Calyx root {} is unavailable: {error}",
                root.display()
            ),
        )
    })?;
    if path_key(root) != path_key(&resolved) {
        return Err(GameEditorCalyxPathError::new(
            CALYX_CANONICAL_ROOT_ESCAPE,
            format!(
                "canonical GameEditor Calyx root {} resolves to {}; the root itself cannot be a link or junction",
                root.display(),
                resolved.display()
            ),
        ));
    }
    Ok(resolved)
}

fn validate_existing_path_under_root(
    canonical_root: &Path,
    candidate: &Path,
    label: &str,
) -> Result<PathBuf, GameEditorCalyxPathError> {
    if !candidate.is_absolute() {
        return Err(GameEditorCalyxPathError::new(
            CALYX_PATH_NOT_ABSOLUTE,
            format!(
                "{label} must be an absolute path beneath {}, got {}",
                canonical_root.display(),
                candidate.display()
            ),
        ));
    }
    if candidate
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(GameEditorCalyxPathError::new(
            CALYX_PATH_TRAVERSAL,
            format!(
                "{label} contains traversal components: {}",
                candidate.display()
            ),
        ));
    }
    if !path_key_is_within(&path_key(canonical_root), &path_key(candidate)) {
        return Err(GameEditorCalyxPathError::new(
            CALYX_PATH_OUTSIDE_ROOT,
            format!(
                "{label} {} is outside canonical GameEditor Calyx root {}; alternate Calyx roots are forbidden",
                candidate.display(),
                canonical_root.display()
            ),
        ));
    }
    let resolved = fs::canonicalize(candidate).map_err(|error| {
        GameEditorCalyxPathError::new(
            CALYX_PATH_MISSING,
            format!("{label} {} is unavailable: {error}", candidate.display()),
        )
    })?;
    if !path_key_is_within(&path_key(canonical_root), &path_key(&resolved)) {
        return Err(GameEditorCalyxPathError::new(
            CALYX_PATH_ESCAPE,
            format!(
                "{label} {} resolves outside canonical root to {}; junction and symlink escapes are forbidden",
                candidate.display(),
                resolved.display()
            ),
        ));
    }
    Ok(resolved)
}

fn resolve_named_vault_under_root(
    root: &Path,
    vault_name: &str,
) -> Result<PathBuf, GameEditorCalyxPathError> {
    let index_path = root.join("vaults").join("index.json");
    let index_path = validate_existing_path_under_root(root, &index_path, "Calyx vault index")?;
    let bytes = fs::read(&index_path).map_err(|error| {
        GameEditorCalyxPathError::new(
            CALYX_VAULT_INDEX_INVALID,
            format!("read {} failed: {error}", index_path.display()),
        )
    })?;
    let index: VaultIndex = serde_json::from_slice(&bytes).map_err(|error| {
        GameEditorCalyxPathError::new(
            CALYX_VAULT_INDEX_INVALID,
            format!("parse {} failed: {error}", index_path.display()),
        )
    })?;
    let matches = index
        .vaults
        .iter()
        .filter(|entry| entry.name == vault_name)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(GameEditorCalyxPathError::new(
            CALYX_ENGINE_PANEL_VAULT_MISSING,
            format!(
                "expected exactly one vault named {vault_name} in {}, found {}; create the real canonical vault instead of restoring an alternate root",
                index_path.display(),
                matches.len()
            ),
        ));
    }
    let entry = matches[0];
    let relative = Path::new(&entry.path);
    let components = relative.components().collect::<Vec<_>>();
    let valid_shape = matches!(components.as_slice(), [Component::Normal(vaults), Component::Normal(id)]
        if *vaults == "vaults" && *id == entry.vault_id.as_str());
    if relative.is_absolute() || !valid_shape {
        return Err(GameEditorCalyxPathError::new(
            CALYX_VAULT_INDEX_INVALID,
            format!(
                "vault {vault_name} has invalid path/id pairing path={} vault_id={}",
                entry.path, entry.vault_id
            ),
        ));
    }
    validate_existing_path_under_root(root, &root.join(relative), vault_name)
}

fn path_key(path: &Path) -> String {
    let mut value = path.to_string_lossy().replace('/', "\\");
    if let Some(stripped) = value.strip_prefix(r"\\?\") {
        value = stripped.to_string();
    }
    value.trim_end_matches('\\').to_ascii_lowercase()
}

fn path_key_is_within(root: &str, candidate: &str) -> bool {
    candidate == root
        || candidate
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('\\'))
}

#[cfg(test)]
#[path = "calyx_paths_tests.rs"]
mod tests;
