use std::path::Path;

use crate::{
    EngineError,
    calyx_json_store::{
        CalyxJsonStoreReadback, CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes,
    },
};

use super::{RpgMapRuntimeState, RpgMapRuntimeStateReadback, validate_rpg_map_state};

const MAP_ACCESS_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCD";
const MAP_ACCESS_VAULT_PURPOSE: &[u8] = b"litd-rpg-map-access-issue1445";
const MAP_ACCESS_COLLECTION: &str = "litd_rpg_map_access";
const MAP_ACCESS_ROW: &[u8] = b"rpg/map_access/latest";
const MAP_ACCESS_COLUMN: &[u8] = b"state_json";

pub fn write_rpg_map_runtime_state(
    vault_dir: &Path,
    state: &RpgMapRuntimeState,
) -> Result<RpgMapRuntimeStateReadback, EngineError> {
    validate_rpg_map_state(state)?;
    let bytes =
        serde_json::to_vec_pretty(state).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize RPG map runtime state: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&map_access_spec(vault_dir), &bytes)
        .map_err(|error| map_store_error("state.write", error))?;
    if stored.bytes != bytes {
        return Err(map_store_error(
            "state.verify",
            "Aster map access row bytes changed after write",
        ));
    }
    parse_readback(stored)
}

pub fn read_rpg_map_runtime_state(
    vault_dir: &Path,
) -> Result<RpgMapRuntimeStateReadback, EngineError> {
    let readback = read_calyx_json_bytes(&map_access_spec(vault_dir))
        .map_err(|error| map_store_error("state.read", error))?
        .ok_or_else(|| {
            map_store_error(
                "state.read",
                format!(
                    "missing RPG map access Aster row at {}",
                    vault_dir.display()
                ),
            )
        })?;
    parse_readback(readback)
}

fn parse_readback(
    readback: CalyxJsonStoreReadback,
) -> Result<RpgMapRuntimeStateReadback, EngineError> {
    let state = serde_json::from_slice::<RpgMapRuntimeState>(&readback.bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse RPG map runtime state readback: {error}"),
        }
    })?;
    validate_rpg_map_state(&state)?;
    Ok(RpgMapRuntimeStateReadback {
        path: readback.vault_dir.display().to_string(),
        bytes: readback.byte_len,
        sha256: readback.sha256,
        state,
    })
}

fn map_access_spec(vault_dir: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir,
        vault_id: MAP_ACCESS_VAULT_ID,
        purpose: MAP_ACCESS_VAULT_PURPOSE,
        collection: MAP_ACCESS_COLLECTION,
        row_key: MAP_ACCESS_ROW,
        column: MAP_ACCESS_COLUMN,
    }
}

fn map_store_error(op: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: format!("map_access_store.{}", op.into()),
        reason: reason.to_string(),
    }
}
