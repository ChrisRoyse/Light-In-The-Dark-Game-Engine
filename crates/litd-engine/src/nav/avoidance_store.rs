use std::path::{Path, PathBuf};

use litd_hash::sha256_hex;

use crate::{
    EngineError,
    calyx_json_store::{
        CalyxJsonStoreReadback, CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes,
    },
    nav::{NavAvoidanceResolution, verify_avoidance_resolution},
};

const NAV_AVOIDANCE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA4";
const NAV_AVOIDANCE_VAULT_PURPOSE: &[u8] = b"litd-engine-nav-avoidance-issue1359";
const NAV_AVOIDANCE_COLLECTION: &str = "litd_engine_nav_avoidance";
const NAV_AVOIDANCE_ROW_KEY: &[u8] = b"nav/avoidance/latest";
const NAV_AVOIDANCE_COLUMN: &[u8] = b"avoidance_json";

#[derive(Debug, Clone)]
pub struct NavAvoidanceAsterReadback {
    pub vault_dir: PathBuf,
    pub bytes: usize,
    pub sha256: String,
    pub state_fnv1a64: String,
    pub seq: u64,
    pub graph_rows: usize,
    pub resolution: NavAvoidanceResolution,
}

pub fn write_avoidance_aster(
    vault_dir: impl AsRef<Path>,
    resolution: &NavAvoidanceResolution,
) -> Result<NavAvoidanceAsterReadback, EngineError> {
    verify_avoidance_resolution(resolution)?;
    let vault_dir = vault_dir.as_ref();
    validate_path(vault_dir)?;
    let bytes = serde_json::to_vec_pretty(resolution)
        .map_err(|error| persistence_failed("serialize_avoidance_aster", error.to_string()))?;
    let store_readback = write_calyx_json_bytes(&spec(vault_dir), &bytes)
        .map_err(|error| persistence_failed("write_avoidance_aster", error))?;
    let readback = parse_readback(store_readback)?;
    if readback.resolution != *resolution {
        return Err(EngineError::NavReadbackMismatch {
            field: "avoidance_aster".to_string(),
            expected: resolution.state_fnv1a64.clone(),
            actual: readback.state_fnv1a64,
        });
    }
    Ok(readback)
}

pub fn load_avoidance_aster(
    vault_dir: impl AsRef<Path>,
) -> Result<NavAvoidanceAsterReadback, EngineError> {
    let vault_dir = vault_dir.as_ref();
    validate_path(vault_dir)?;
    let readback = read_calyx_json_bytes(&spec(vault_dir))
        .map_err(|error| persistence_failed("read_avoidance_aster", error))?
        .ok_or_else(|| {
            persistence_failed(
                "read_avoidance_aster",
                format!("Aster row missing from {}", vault_dir.display()),
            )
        })?;
    parse_readback(readback)
}

fn validate_path(path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(persistence_failed(
            "avoidance_aster_path",
            "Aster vault directory must be non-empty",
        ));
    }
    if path.is_file() {
        return Err(persistence_failed(
            "avoidance_aster_path",
            format!(
                "{} is a file; avoidance state must be a Calyx Aster vault directory",
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
        vault_id: NAV_AVOIDANCE_VAULT_ID,
        purpose: NAV_AVOIDANCE_VAULT_PURPOSE,
        collection: NAV_AVOIDANCE_COLLECTION,
        row_key: NAV_AVOIDANCE_ROW_KEY,
        column: NAV_AVOIDANCE_COLUMN,
    }
}

fn parse_readback(
    readback: CalyxJsonStoreReadback,
) -> Result<NavAvoidanceAsterReadback, EngineError> {
    let resolution =
        serde_json::from_slice::<NavAvoidanceResolution>(&readback.bytes).map_err(|error| {
            persistence_failed(
                "deserialize_avoidance_aster",
                format!("{}: {error}", readback.vault_dir.display()),
            )
        })?;
    verify_avoidance_resolution(&resolution)?;
    Ok(NavAvoidanceAsterReadback {
        vault_dir: readback.vault_dir,
        bytes: readback.byte_len,
        sha256: sha256_hex(&readback.bytes),
        state_fnv1a64: resolution.state_fnv1a64.clone(),
        seq: readback.seq,
        graph_rows: readback.graph_rows,
        resolution,
    })
}
