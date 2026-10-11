use std::path::{Path, PathBuf};

use litd_hash::sha256_hex;

use crate::{
    EngineError,
    ai::{BehaviorRunReport, verify_behavior_run_integrity},
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};

const BEHAVIOR_RUN_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FD0";
const BEHAVIOR_RUN_VAULT_PURPOSE: &[u8] = b"litd-ai-behavior-run-issue1361";
const BEHAVIOR_RUN_COLLECTION: &str = "litd_ai_behavior_run";
const BEHAVIOR_RUN_ROW: &[u8] = b"ai/behavior/run/latest";
const BEHAVIOR_RUN_COLUMN: &[u8] = b"behavior_run_json";

#[derive(Debug, Clone)]
pub struct BehaviorTreeRunFileReadback {
    pub path: PathBuf,
    pub bytes: usize,
    pub sha256: String,
    pub fnv1a64: String,
    pub run: BehaviorRunReport,
}

pub fn write_behavior_run_json(
    path: impl AsRef<Path>,
    run: &BehaviorRunReport,
) -> Result<BehaviorTreeRunFileReadback, EngineError> {
    verify_behavior_run_integrity(run)?;
    let path = path.as_ref();
    validate_path(path)?;
    let bytes = serde_json::to_vec_pretty(run)
        .map_err(|error| persistence_failed("serialize_behavior_tree_aster", error.to_string()))?;
    write_calyx_json_bytes(&behavior_run_spec(path), &bytes)
        .map_err(|error| persistence_failed("write_behavior_tree_aster", error))?;
    let readback = load_behavior_run_json(path)?;
    if readback.run != *run {
        return Err(EngineError::AiReadbackMismatch {
            field: "behavior_tree_run_aster".to_string(),
            expected: run.fnv1a64.clone(),
            actual: readback.fnv1a64,
        });
    }
    Ok(readback)
}

pub fn load_behavior_run_json(
    path: impl AsRef<Path>,
) -> Result<BehaviorTreeRunFileReadback, EngineError> {
    let path = path.as_ref();
    validate_path(path)?;
    let readback = read_calyx_json_bytes(&behavior_run_spec(path))
        .map_err(|error| persistence_failed("read_behavior_tree_aster", error))?
        .ok_or_else(|| {
            persistence_failed(
                "read_behavior_tree_aster",
                format!("{} has no behavior run row", path.display()),
            )
        })?;
    let bytes = readback.bytes;
    let run = serde_json::from_slice::<BehaviorRunReport>(&bytes).map_err(|error| {
        persistence_failed(
            "deserialize_behavior_tree_aster",
            format!("{}: {error}", path.display()),
        )
    })?;
    verify_behavior_run_integrity(&run)?;
    Ok(BehaviorTreeRunFileReadback {
        path: path.to_path_buf(),
        bytes: bytes.len(),
        sha256: sha256_hex(&bytes),
        fnv1a64: run.fnv1a64.clone(),
        run,
    })
}

fn validate_path(path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(persistence_failed(
            "behavior_tree_path",
            "path must be non-empty",
        ));
    }
    if path.is_file() {
        return Err(persistence_failed(
            "behavior_tree_path",
            format!(
                "{} is a file; expected Aster vault directory",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn behavior_run_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: BEHAVIOR_RUN_VAULT_ID,
        purpose: BEHAVIOR_RUN_VAULT_PURPOSE,
        collection: BEHAVIOR_RUN_COLLECTION,
        row_key: BEHAVIOR_RUN_ROW,
        column: BEHAVIOR_RUN_COLUMN,
    }
}

fn persistence_failed(op: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::AiPersistenceFailed {
        op: op.into(),
        reason: reason.into(),
    }
}
