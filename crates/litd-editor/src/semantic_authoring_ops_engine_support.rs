use calyx_core::CxId;
use litd_engine::EngineError;
use sha2::{Digest, Sha256};

use super::{SemanticAuthoringReport, SemanticAuthoringStatus};

const SEMANTIC_AUTHORING_CX_SALT: &[u8] = b"litd-semantic-authoring-ops-v1";

pub(super) fn cx_for_constellation(constellation_id: &str) -> CxId {
    CxId::from_input(constellation_id.as_bytes(), 1, SEMANTIC_AUTHORING_CX_SALT)
}

pub(super) fn sha32(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut out = [0_u8; 32];
    out.copy_from_slice(&digest);
    out
}

pub(super) fn calyx_error(
    phase: &'static str,
) -> impl Fn(calyx_core::CalyxError) -> EngineError + Copy {
    move |error| semantic_authoring_error(phase, format!("{}: {}", error.code, error.message))
}

pub(in crate::semantic_authoring_ops) fn invalid_semantic_authoring<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(semantic_authoring_error(field, reason.into()))
}

pub(super) fn mark_no_neighbors_if_empty(
    report: &mut SemanticAuthoringReport,
    reason: &'static str,
) {
    if report.hits.is_empty() {
        report.status = SemanticAuthoringStatus::NoNeighbors;
        report.code = "LITD-SEMANTIC-NO-NEIGHBORS".to_string();
        report.reasons.push(reason.to_string());
    }
}

pub(super) fn semantic_authoring_error(
    field: impl Into<String>,
    reason: impl ToString,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.semantic_authoring.{}", field.into()),
        reason: reason.to_string(),
    }
}
