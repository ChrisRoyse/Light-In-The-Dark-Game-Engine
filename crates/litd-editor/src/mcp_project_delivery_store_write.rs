//! Atomic temp-file byte writers for the delivery store (#1466 split of
//! `mcp_project_delivery_store.rs`).
//!
//! Write-to-temp + fsync + readback-verify + rename, for both create-new and replace
//! semantics. The manifest/blob store logic stays in `mcp_project_delivery_store.rs`; these
//! helpers are `pub(super)` so it calls them unchanged.

use std::{fs, io::Write, path::Path, process};

use litd_engine::EngineError;

use super::super::mcp_project_delivery_validation::delivery_error;

pub(super) fn write_bytes_create_new(
    path: &Path,
    bytes: &[u8],
    label: &str,
) -> Result<(), EngineError> {
    let parent = parent_dir(path, label)?;
    fs::create_dir_all(parent).map_err(|error| {
        delivery_error(
            format!("{label}.parent"),
            format!("create {}: {error}", parent.display()),
        )
    })?;
    let temp_path = temp_path_for(parent, path, label);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|error| {
            delivery_error(
                format!("{label}.temp.create"),
                format!("create {}: {error}", temp_path.display()),
            )
        })?;
    file.write_all(bytes).map_err(|error| {
        delivery_error(
            format!("{label}.temp.write"),
            format!("write {}: {error}", temp_path.display()),
        )
    })?;
    file.sync_all().map_err(|error| {
        delivery_error(
            format!("{label}.temp.sync"),
            format!("sync {}: {error}", temp_path.display()),
        )
    })?;
    drop(file);
    verify_temp_bytes(&temp_path, bytes, label)?;
    fs::rename(&temp_path, path).map_err(|error| {
        delivery_error(
            format!("{label}.rename"),
            format!(
                "rename {} -> {}: {error}",
                temp_path.display(),
                path.display()
            ),
        )
    })
}

pub(super) fn write_bytes_replace(
    path: &Path,
    bytes: &[u8],
    label: &str,
) -> Result<(), EngineError> {
    let parent = parent_dir(path, label)?;
    fs::create_dir_all(parent).map_err(|error| {
        delivery_error(
            format!("{label}.parent"),
            format!("create {}: {error}", parent.display()),
        )
    })?;
    let temp_path = temp_path_for(parent, path, label);
    fs::write(&temp_path, bytes).map_err(|error| {
        delivery_error(
            format!("{label}.temp.write"),
            format!("write {}: {error}", temp_path.display()),
        )
    })?;
    verify_temp_bytes(&temp_path, bytes, label)?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| {
            delivery_error(
                format!("{label}.replace"),
                format!("remove {}: {error}", path.display()),
            )
        })?;
    }
    fs::rename(&temp_path, path).map_err(|error| {
        delivery_error(
            format!("{label}.rename"),
            format!(
                "rename {} -> {}: {error}",
                temp_path.display(),
                path.display()
            ),
        )
    })
}

fn verify_temp_bytes(path: &Path, expected: &[u8], label: &str) -> Result<(), EngineError> {
    let persisted = fs::read(path).map_err(|error| {
        delivery_error(
            format!("{label}.temp.readback"),
            format!("read {}: {error}", path.display()),
        )
    })?;
    if persisted != expected {
        return Err(delivery_error(
            format!("{label}.temp.readback"),
            "temp readback bytes changed",
        ));
    }
    Ok(())
}

fn temp_path_for(parent: &Path, path: &Path, label: &str) -> std::path::PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(label);
    parent.join(format!("{name}.{}.tmp", process::id()))
}

fn parent_dir<'a>(path: &'a Path, label: &str) -> Result<&'a Path, EngineError> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| delivery_error(format!("{label}.path"), "path has no parent"))
}
