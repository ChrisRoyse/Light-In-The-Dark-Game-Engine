use std::path::{Path, PathBuf};

use litd_hash::sha256_hex;

use crate::{
    EngineError,
    calyx_json_store::{
        CalyxJsonStoreReadback, CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes,
    },
    nav::{NavPath, verify_nav_path_integrity},
};

const NAV_PATH_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA3";
const NAV_PATH_VAULT_PURPOSE: &[u8] = b"litd-engine-nav-path-issue1359";
const NAV_PATH_COLLECTION: &str = "litd_engine_nav_path";
const NAV_PATH_ROW_KEY: &[u8] = b"nav/path/latest";
const NAV_PATH_COLUMN: &[u8] = b"path_json";

#[derive(Debug, Clone)]
pub struct NavPathAsterReadback {
    pub vault_dir: PathBuf,
    pub bytes: usize,
    pub sha256: String,
    pub fnv1a64: String,
    pub seq: u64,
    pub graph_rows: usize,
    pub path_result: NavPath,
}

pub fn write_nav_path_aster(
    vault_dir: impl AsRef<Path>,
    nav_path: &NavPath,
) -> Result<NavPathAsterReadback, EngineError> {
    verify_nav_path_integrity(nav_path)?;
    let vault_dir = vault_dir.as_ref();
    validate_path(vault_dir)?;
    let bytes = serde_json::to_vec_pretty(nav_path)
        .map_err(|error| persistence_failed("serialize_nav_path_aster", error.to_string()))?;
    let store_readback = write_calyx_json_bytes(&spec(vault_dir), &bytes)
        .map_err(|error| persistence_failed("write_nav_path_aster", error))?;
    let readback = parse_readback(store_readback)?;
    if readback.path_result != *nav_path {
        return Err(EngineError::NavReadbackMismatch {
            field: "nav_path_aster".to_string(),
            expected: nav_path.fnv1a64.clone(),
            actual: readback.fnv1a64,
        });
    }
    Ok(readback)
}

pub fn load_nav_path_aster(
    vault_dir: impl AsRef<Path>,
) -> Result<NavPathAsterReadback, EngineError> {
    let vault_dir = vault_dir.as_ref();
    validate_path(vault_dir)?;
    let readback = read_calyx_json_bytes(&spec(vault_dir))
        .map_err(|error| persistence_failed("read_nav_path_aster", error))?
        .ok_or_else(|| {
            persistence_failed(
                "read_nav_path_aster",
                format!("Aster row missing from {}", vault_dir.display()),
            )
        })?;
    parse_readback(readback)
}

fn validate_path(path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(persistence_failed(
            "nav_path_aster_path",
            "Aster vault directory must be non-empty",
        ));
    }
    if path.is_file() {
        return Err(persistence_failed(
            "nav_path_aster_path",
            format!(
                "{} is a file; nav path state must be a Calyx Aster vault directory",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn persistence_failed(op: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::NavPersistenceFailed {
        op: op.into(),
        reason: reason.into(),
    }
}

fn spec(vault_dir: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir,
        vault_id: NAV_PATH_VAULT_ID,
        purpose: NAV_PATH_VAULT_PURPOSE,
        collection: NAV_PATH_COLLECTION,
        row_key: NAV_PATH_ROW_KEY,
        column: NAV_PATH_COLUMN,
    }
}

fn parse_readback(readback: CalyxJsonStoreReadback) -> Result<NavPathAsterReadback, EngineError> {
    let path_result = serde_json::from_slice::<NavPath>(&readback.bytes).map_err(|error| {
        persistence_failed(
            "deserialize_nav_path_aster",
            format!("{}: {error}", readback.vault_dir.display()),
        )
    })?;
    verify_nav_path_integrity(&path_result)?;
    Ok(NavPathAsterReadback {
        vault_dir: readback.vault_dir,
        bytes: readback.byte_len,
        sha256: sha256_hex(&readback.bytes),
        fnv1a64: path_result.fnv1a64.clone(),
        seq: readback.seq,
        graph_rows: readback.graph_rows,
        path_result,
    })
}
