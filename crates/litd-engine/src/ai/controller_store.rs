use std::path::{Path, PathBuf};

use litd_hash::sha256_hex;

use crate::{
    EngineError,
    ai::{AiControllerRunReport, verify_ai_controller_run_integrity},
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};

const CONTROLLER_RUN_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FD2";
const CONTROLLER_RUN_VAULT_PURPOSE: &[u8] = b"litd-ai-controller-run-issue1361";
const CONTROLLER_RUN_COLLECTION: &str = "litd_ai_controller_run";
const CONTROLLER_RUN_ROW: &[u8] = b"ai/controller/run/latest";
const CONTROLLER_RUN_COLUMN: &[u8] = b"controller_run_json";

#[derive(Debug, Clone)]
pub struct AiControllerRunFileReadback {
    pub path: PathBuf,
    pub bytes: usize,
    pub sha256: String,
    pub fnv1a64: String,
    pub report: AiControllerRunReport,
}

pub fn write_ai_controller_run_json(
    path: impl AsRef<Path>,
    report: &AiControllerRunReport,
) -> Result<AiControllerRunFileReadback, EngineError> {
    verify_ai_controller_run_integrity(report)?;
    let path = path.as_ref();
    validate_path(path)?;
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| persistence_failed("serialize_controller_aster", error.to_string()))?;
    write_calyx_json_bytes(&controller_run_spec(path), &bytes)
        .map_err(|error| persistence_failed("write_controller_aster", error))?;
    let readback = load_ai_controller_run_json(path)?;
    if readback.report != *report {
        return Err(EngineError::AiReadbackMismatch {
            field: "ai_controller_run_aster".to_string(),
            expected: report.fnv1a64.clone(),
            actual: readback.fnv1a64,
        });
    }
    Ok(readback)
}

pub fn load_ai_controller_run_json(
    path: impl AsRef<Path>,
) -> Result<AiControllerRunFileReadback, EngineError> {
    let path = path.as_ref();
    validate_path(path)?;
    let readback = read_calyx_json_bytes(&controller_run_spec(path))
        .map_err(|error| persistence_failed("read_controller_aster", error))?
        .ok_or_else(|| {
            persistence_failed(
                "read_controller_aster",
                format!("{} has no controller run row", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let report = serde_json::from_slice::<AiControllerRunReport>(&bytes).map_err(|error| {
        persistence_failed(
            "deserialize_controller_aster",
            format!("{}: {error}", path.display()),
        )
    })?;
    verify_ai_controller_run_integrity(&report)?;
    Ok(AiControllerRunFileReadback {
        path: path.to_path_buf(),
        bytes: bytes.len(),
        sha256: sha256_hex(&bytes),
        fnv1a64: report.fnv1a64.clone(),
        report,
    })
}

fn validate_path(path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(persistence_failed(
            "controller_path",
            "path must be non-empty",
        ));
    }
    if path.is_file() {
        return Err(persistence_failed(
            "controller_path",
            format!(
                "{} is a file; expected Aster vault directory",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn controller_run_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: CONTROLLER_RUN_VAULT_ID,
        purpose: CONTROLLER_RUN_VAULT_PURPOSE,
        collection: CONTROLLER_RUN_COLLECTION,
        row_key: CONTROLLER_RUN_ROW,
        column: CONTROLLER_RUN_COLUMN,
    }
}

fn persistence_failed(op: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::AiPersistenceFailed {
        op: op.into(),
        reason: reason.into(),
    }
}
