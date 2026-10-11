use std::path::{Path, PathBuf};

use litd_hash::sha256_hex;

use crate::{
    EngineError,
    calyx_json_store::{
        CalyxJsonStoreReadback, CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes,
    },
    nav::{BakedNavMesh, verify_navmesh_integrity},
};

const NAVMESH_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FA2";
const NAVMESH_VAULT_PURPOSE: &[u8] = b"litd-engine-navmesh-issue1359";
const NAVMESH_COLLECTION: &str = "litd_engine_navmesh";
const NAVMESH_ROW_KEY: &[u8] = b"nav/navmesh/latest";
const NAVMESH_COLUMN: &[u8] = b"navmesh_json";

#[derive(Debug, Clone)]
pub struct NavMeshAsterReadback {
    pub vault_dir: PathBuf,
    pub bytes: usize,
    pub sha256: String,
    pub fnv1a64: String,
    pub seq: u64,
    pub graph_rows: usize,
    pub navmesh: BakedNavMesh,
}

pub fn write_navmesh_aster(
    vault_dir: impl AsRef<Path>,
    navmesh: &BakedNavMesh,
) -> Result<NavMeshAsterReadback, EngineError> {
    verify_navmesh_integrity(navmesh)?;
    let vault_dir = vault_dir.as_ref();
    validate_path(vault_dir)?;
    let bytes = serde_json::to_vec_pretty(navmesh)
        .map_err(|error| persistence_failed("serialize_navmesh_aster", error.to_string()))?;
    let store_readback = write_calyx_json_bytes(&spec(vault_dir), &bytes)
        .map_err(|error| persistence_failed("write_navmesh_aster", error))?;
    let readback = parse_readback(store_readback)?;
    if readback.navmesh != *navmesh {
        return Err(EngineError::NavReadbackMismatch {
            field: "navmesh_aster".to_string(),
            expected: navmesh.fnv1a64.clone(),
            actual: readback.fnv1a64,
        });
    }
    Ok(readback)
}

pub fn load_navmesh_aster(
    vault_dir: impl AsRef<Path>,
) -> Result<NavMeshAsterReadback, EngineError> {
    let vault_dir = vault_dir.as_ref();
    validate_path(vault_dir)?;
    let readback = read_calyx_json_bytes(&spec(vault_dir))
        .map_err(|error| persistence_failed("read_navmesh_aster", error))?
        .ok_or_else(|| {
            persistence_failed(
                "read_navmesh_aster",
                format!("Aster row missing from {}", vault_dir.display()),
            )
        })?;
    parse_readback(readback)
}

fn validate_path(path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(persistence_failed(
            "navmesh_aster_path",
            "Aster vault directory must be non-empty",
        ));
    }
    if path.is_file() {
        return Err(persistence_failed(
            "navmesh_aster_path",
            format!(
                "{} is a file; navmesh state must be a Calyx Aster vault directory",
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
        vault_id: NAVMESH_VAULT_ID,
        purpose: NAVMESH_VAULT_PURPOSE,
        collection: NAVMESH_COLLECTION,
        row_key: NAVMESH_ROW_KEY,
        column: NAVMESH_COLUMN,
    }
}

fn parse_readback(readback: CalyxJsonStoreReadback) -> Result<NavMeshAsterReadback, EngineError> {
    let navmesh = serde_json::from_slice::<BakedNavMesh>(&readback.bytes).map_err(|error| {
        persistence_failed(
            "deserialize_navmesh_aster",
            format!("{}: {error}", readback.vault_dir.display()),
        )
    })?;
    verify_navmesh_integrity(&navmesh)?;
    Ok(NavMeshAsterReadback {
        vault_dir: readback.vault_dir,
        bytes: readback.byte_len,
        sha256: sha256_hex(&readback.bytes),
        fnv1a64: navmesh.fnv1a64.clone(),
        seq: readback.seq,
        graph_rows: readback.graph_rows,
        navmesh,
    })
}
