//! #1543: `EngineError` size regression guard, split out of `error.rs` to keep that file
//! under the <500-line ratchet (#1466/#800 sec.6). Included as the body of the
//! `issue1480_error_size` module via `#[path]` from `error.rs`.

use super::EngineError;

/// Regression guard (#1480 item 1): `EngineError` must stay under clippy's
/// `result_large_err` threshold (128 bytes) so no `Result<_, EngineError>` in the
/// workspace trips the lint and no Ok path — including per-tick AI — moves a large
/// payload. The embedded external errors (ForgeError ~104 B, CalyxError ~56 B) are
/// boxed to keep this size independent of the Calyx error crates; if a new variant or
/// an unboxed embedded error pushes it back to 128, this fails loud with the number.
#[test]
fn engine_error_stays_below_result_large_err_threshold() {
    let size = std::mem::size_of::<EngineError>();
    assert!(
        size < 128,
        "EngineError is {size} bytes, at/over clippy's result_large_err threshold (128); \
         box the largest variant's payload (see ForgeOperationFailed) to shrink it"
    );
}
