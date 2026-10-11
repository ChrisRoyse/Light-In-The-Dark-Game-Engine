use std::path::{Path, PathBuf};

use litd_hash::sha256_hex;

use crate::{
    EngineError,
    ai::{PerceptionRunReport, verify_perception_run_integrity},
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};

const PERCEPTION_RUN_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FD1";
const PERCEPTION_RUN_VAULT_PURPOSE: &[u8] = b"litd-ai-perception-run-issue1361";
const PERCEPTION_RUN_COLLECTION: &str = "litd_ai_perception_run";
const PERCEPTION_RUN_ROW: &[u8] = b"ai/perception/run/latest";
const PERCEPTION_RUN_COLUMN: &[u8] = b"perception_run_json";

#[derive(Debug, Clone)]
pub struct PerceptionRunFileReadback {
    pub path: PathBuf,
    pub bytes: usize,
    pub sha256: String,
    pub fnv1a64: String,
    pub report: PerceptionRunReport,
}

pub fn write_perception_run_json(
    path: impl AsRef<Path>,
    report: &PerceptionRunReport,
) -> Result<PerceptionRunFileReadback, EngineError> {
    verify_perception_run_integrity(report)?;
    let path = path.as_ref();
    validate_path(path)?;
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| persistence_failed("serialize_perception_aster", error.to_string()))?;
    write_calyx_json_bytes(&perception_run_spec(path), &bytes)
        .map_err(|error| persistence_failed("write_perception_aster", error))?;
    let readback = load_perception_run_json(path)?;
    if readback.report != *report {
        return Err(EngineError::AiReadbackMismatch {
            field: "perception_run_aster".to_string(),
            expected: report.fnv1a64.clone(),
            actual: readback.fnv1a64,
        });
    }
    Ok(readback)
}

pub fn load_perception_run_json(
    path: impl AsRef<Path>,
) -> Result<PerceptionRunFileReadback, EngineError> {
    let path = path.as_ref();
    validate_path(path)?;
    let readback = read_calyx_json_bytes(&perception_run_spec(path))
        .map_err(|error| persistence_failed("read_perception_aster", error))?
        .ok_or_else(|| {
            persistence_failed(
                "read_perception_aster",
                format!("{} has no perception run row", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let report = serde_json::from_slice::<PerceptionRunReport>(&bytes).map_err(|error| {
        persistence_failed(
            "deserialize_perception_aster",
            format!("{}: {error}", path.display()),
        )
    })?;
    verify_perception_run_integrity(&report)?;
    Ok(PerceptionRunFileReadback {
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
            "perception_path",
            "path must be non-empty",
        ));
    }
    if path.is_file() {
        return Err(persistence_failed(
            "perception_path",
            format!(
                "{} is a file; expected Aster vault directory",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn perception_run_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: PERCEPTION_RUN_VAULT_ID,
        purpose: PERCEPTION_RUN_VAULT_PURPOSE,
        collection: PERCEPTION_RUN_COLLECTION,
        row_key: PERCEPTION_RUN_ROW,
        column: PERCEPTION_RUN_COLUMN,
    }
}

fn persistence_failed(op: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::AiPersistenceFailed {
        op: op.into(),
        reason: reason.into(),
    }
}
