//! Shared persist->lower valve for editor-authored world scenery content.
//!
//! Doodad scatter (#809) and destructibles (#810) are authored in the editor,
//! persisted through the REAL Calyx content path (`put_content`), and then lowered
//! into a frozen, fingerprinted sim package the deterministic hot path consumes
//! without ever querying Calyx live. This module holds the code both features
//! share: the typed error, the diagnostic codes, and the persist+lower helper.

use crate::{
    calyx_content::{
        EngineCalyxContentError, EngineCalyxContentStore, EngineContentConstellationReadback,
        EngineContentRecord,
    },
    calyx_lowering::{
        EngineLoweredSimPackage, EngineLoweredSimReadback, EngineLoweringError, EngineLoweringValve,
    },
};

/// The complete result of authoring one world-content record: the durable Aster
/// readback (source of truth) plus the frozen lowered sim package derived from it.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldContentLowering {
    pub readback: EngineContentConstellationReadback,
    pub lowered_readback: EngineLoweredSimReadback,
    pub lowered_package: EngineLoweredSimPackage,
}

#[derive(Debug, thiserror::Error)]
pub enum WorldAuthoringError {
    #[error("world authoring field {field} is invalid [{code}]: {reason}")]
    Invalid {
        code: &'static str,
        field: String,
        reason: String,
    },
    #[error("world authoring Calyx persistence failed: {0}")]
    Content(#[from] EngineCalyxContentError),
    #[error("world authoring lowering failed: {0}")]
    Lowering(#[from] EngineLoweringError),
    #[error("world authoring codec failed: {0}")]
    Codec(String),
}

impl WorldAuthoringError {
    /// Stable diagnostic code for programmatic handling and log triage.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid { code, .. } => code,
            Self::Content(_) => "WORLD_AUTHORING_CONTENT",
            Self::Lowering(_) => "WORLD_AUTHORING_LOWERING",
            Self::Codec(_) => "WORLD_AUTHORING_CODEC",
        }
    }
}

/// Persist an authored world-content record through the real Calyx content store,
/// then lower the verified readback into a frozen sim package. Works with no ORT
/// or model cache present: the record persists as `pending_measurement` when the
/// learned lenses are unavailable, and lowering never depends on a learned lens.
pub fn persist_and_lower_world_content(
    store: &EngineCalyxContentStore,
    record: &EngineContentRecord,
) -> Result<WorldContentLowering, WorldAuthoringError> {
    let readback = store.put_content(record)?;
    let lowered_package = EngineLoweringValve::lower_content_for_sim(&readback)?;
    let lowered_readback = EngineLoweringValve::verify_lowered_package(&lowered_package)?;
    if lowered_package.source_cx_id != readback.cx_id {
        return invalid(
            "WORLD_AUTHORING_CX_MISMATCH",
            "lowered.source_cx_id",
            "lowered package cx_id must match the persisted readback cx_id",
        );
    }
    if lowered_package.source_fingerprint != readback.input_sha256 {
        return invalid(
            "WORLD_AUTHORING_FINGERPRINT_MISMATCH",
            "lowered.source_fingerprint",
            "lowered package fingerprint must match the persisted input sha256",
        );
    }
    Ok(WorldContentLowering {
        readback,
        lowered_readback,
        lowered_package,
    })
}

pub(crate) fn invalid<T>(
    code: &'static str,
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, WorldAuthoringError> {
    Err(WorldAuthoringError::Invalid {
        code,
        field: field.into(),
        reason: reason.into(),
    })
}

pub(crate) fn codec_failed(error: impl std::fmt::Display) -> WorldAuthoringError {
    WorldAuthoringError::Codec(error.to_string())
}
