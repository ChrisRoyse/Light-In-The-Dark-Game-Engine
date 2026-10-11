use crate::EngineError;

pub(super) fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    let field = field.into();
    let reason = reason.into();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-ASSET-STREAM-INVALID-CONFIG",
        "asset stream config rejected"
    );
    Err(EngineError::AssetStreamInvalidConfig { field, reason })
}

pub(super) fn persistence(op: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let op = op.into();
    let reason = reason.into();
    tracing::error!(
        op = %op,
        reason = %reason,
        code = "LITD-ASSET-STREAM-PERSISTENCE-FAILED",
        "asset stream persistence failed"
    );
    EngineError::AssetStreamPersistenceFailed { op, reason }
}

pub(super) fn readback<T>(
    field: impl Into<String>,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> Result<T, EngineError> {
    let field = field.into();
    let expected = expected.into();
    let actual = actual.into();
    tracing::error!(
        field = %field,
        expected = %expected,
        actual = %actual,
        code = "LITD-ASSET-STREAM-READBACK-MISMATCH",
        "asset stream source-of-truth readback mismatch"
    );
    Err(EngineError::AssetStreamReadbackMismatch {
        field,
        expected,
        actual,
    })
}
