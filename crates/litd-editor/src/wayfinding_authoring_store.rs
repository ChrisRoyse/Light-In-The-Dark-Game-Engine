use std::path::Path;

use litd_engine::{
    EngineError,
    calyx_json_store::{
        CalyxJsonStoreReadback, CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes,
    },
};

use super::{
    WayfindingAuthoringLedger, WayfindingAuthoringReadback, validate_wayfinding_ledger,
    wayfinding_error,
};

const WAYFINDING_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FWF";
const WAYFINDING_VAULT_PURPOSE: &[u8] = b"litd-wayfinding-authoring-issue1447";
const WAYFINDING_COLLECTION: &str = "litd_wayfinding_authoring";
const WAYFINDING_ROW: &[u8] = b"wayfinding/authoring/latest";
const WAYFINDING_COLUMN: &[u8] = b"ledger_json";

pub fn write_wayfinding_ledger(
    vault_dir: &Path,
    ledger: &WayfindingAuthoringLedger,
) -> Result<WayfindingAuthoringReadback, EngineError> {
    validate_wayfinding_ledger(ledger)?;
    let bytes =
        serde_json::to_vec_pretty(ledger).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize wayfinding authoring ledger: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&wayfinding_spec(vault_dir), &bytes)
        .map_err(|error| wayfinding_error("store.write", error))?;
    if stored.bytes != bytes {
        return Err(wayfinding_error(
            "store.verify",
            "Aster wayfinding ledger row bytes changed after write",
        ));
    }
    parse_readback(stored)
}

pub fn read_wayfinding_ledger(
    vault_dir: &Path,
) -> Result<WayfindingAuthoringReadback, EngineError> {
    let readback = read_calyx_json_bytes(&wayfinding_spec(vault_dir))
        .map_err(|error| wayfinding_error("store.read", error))?
        .ok_or_else(|| {
            wayfinding_error(
                "store.read",
                format!(
                    "missing wayfinding authoring Aster row at {}",
                    vault_dir.display()
                ),
            )
        })?;
    parse_readback(readback)
}

fn parse_readback(
    readback: CalyxJsonStoreReadback,
) -> Result<WayfindingAuthoringReadback, EngineError> {
    let ledger =
        serde_json::from_slice::<WayfindingAuthoringLedger>(&readback.bytes).map_err(|error| {
            EngineError::SerializationFailed {
                format: "json",
                reason: format!("parse wayfinding authoring ledger readback: {error}"),
            }
        })?;
    validate_wayfinding_ledger(&ledger)?;
    Ok(WayfindingAuthoringReadback {
        path: readback.vault_dir.display().to_string(),
        bytes: readback.byte_len as u64,
        sha256: readback.sha256,
        ledger,
    })
}

fn wayfinding_spec(vault_dir: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir,
        vault_id: WAYFINDING_VAULT_ID,
        purpose: WAYFINDING_VAULT_PURPOSE,
        collection: WAYFINDING_COLLECTION,
        row_key: WAYFINDING_ROW,
        column: WAYFINDING_COLUMN,
    }
}
