//! #1825: shared fail-loud diagnostics for the real-asset-vault fixture tests.
//!
//! These `litd-editor --lib` tests exercise real import/validation logic against the real
//! `local-asset-vault` (real glTF/PNG/OGG bytes). Fixture RESOLUTION is already
//! worktree-portable -- every call site resolves the vault at RUNTIME through
//! `litd_engine::asset_vault_root` (#1543), so the primary checkout and any linked git
//! worktree find the same materialized vault. There is no hardcoded machine path to fix.
//!
//! What was missing (#1825): when a REQUIRED real asset -- or the full-vault provisioning
//! as a whole -- is absent, each test failed with a bare, class-specific message
//! (`missing real vault record ...`, `assertion failed: record_count() >= 10_700`, or a raw
//! `os error 3`). In a fresh worktree on a box whose vault is not fully materialized, 42 such
//! failures obscured real regressions because none of them named the PROVISIONING step.
//!
//! This module is the single source of truth for that diagnostic. Every fixture failure now
//! emits ONE structured line naming the resolved vault root, the current record count, the
//! exact missing fixture / shortfall, and the precise remediation. It never silently skips and
//! never mocks the data (AGENTS.md): on a properly-provisioned box the tests pass unchanged; on
//! an unprovisioned one they fail LOUD and actionable.

use std::fmt::Debug;

use crate::asset_vault::AssetVaultCatalog;

/// Full-vault record floor. The canonical `local-asset-vault` (all CC0 intake sources
/// materialized) holds well over this many records; the M1-curated subset (#1811
/// CLEANUP-LEDGER, which intentionally dropped the audio / polyhaven / kaykit corpora) holds
/// far fewer. Tests that assert against the full corpus require at least this many records.
pub(crate) const FULL_VAULT_MIN_RECORDS: usize = 10_700;

/// The resolved vault root, or the resolver's own fail-loud line if the vault is unresolvable.
fn resolved_vault_root() -> String {
    match litd_engine::asset_vault_root::local_asset_vault_root() {
        Ok(root) => root.display().to_string(),
        Err(error) => format!("<unresolved: {error}>"),
    }
}

/// The current record count of the default catalog, or the load error if it cannot load.
fn current_record_count() -> String {
    match AssetVaultCatalog::load_default_cached() {
        Ok(catalog) => catalog.record_count().to_string(),
        Err(error) => format!("<catalog load failed: {error}>"),
    }
}

/// The shared remediation paragraph for an absent asset-vault fixture. Names the exact
/// provisioning step and the tracking issues so the failure is unmistakably an environment
/// gap, not the contributor's change.
pub(crate) fn asset_vault_remediation() -> String {
    format!(
        "resolved_vault_root={} current_record_count={} -- PROVISIONING STEP: re-materialize the \
         FULL local-asset-vault beside the primary checkout via the free CC0 asset-intake \
         pipeline (#1717 zero-paid intake / #782 asset-gen), tracked by #1811; or set \
         LITD_LOCAL_ASSET_VAULT to an absolute path to a fully materialized vault. The vault is a \
         large gitignored directory that does NOT travel with a git worktree. NOTE: the \
         M1-curated vault (#1811 CLEANUP-LEDGER) intentionally dropped the audio / polyhaven / \
         kaykit corpora, so full-corpus fixture tests require a fully materialized vault, not the \
         M1 subset.",
        resolved_vault_root(),
        current_record_count(),
    )
}

/// The remediation paragraph for the absent canonical Calyx substrate (#1814 / #779). The
/// canonical root is hard-pinned to `C:\code\GameEditor\Calyx` by doctrine (#779); tests reach
/// it through the documented `litd_engine::calyx_paths` resolver (there is no foreign hardcoded
/// path in the test code). When the substrate is a stub, the resolver already fails loud with
/// `LITD_CALYX_CANONICAL_PATH_MISSING`; this names the provisioning step for that error.
pub(crate) fn calyx_substrate_remediation() -> String {
    "PROVISIONING STEP: provision the canonical Calyx substrate on this box \
     (C:\\code\\GameEditor\\Calyx: vaults/index.json + the gameeditor-engine-content-panel vault \
     + ort-gpu-venv), tracked by #1814; the canonical root is hard-pinned by doctrine (#779) and \
     resolved through litd_engine::calyx_paths -- this is a substrate provisioning gap, not a \
     test-code path defect."
        .to_string()
}

/// Panic message for a specific missing vault fixture (a record lookup or a raw file path).
pub(crate) fn missing_fixture(relative_path: &str) -> String {
    format!(
        "ASSET_VAULT_FIXTURE_MISSING relative_path={relative_path} -- this real-asset fixture is \
         absent from the vault on this box. {}",
        asset_vault_remediation()
    )
}

/// Loud precondition for a full-corpus fixture test: the vault must hold the full record floor.
/// Passes silently on a fully-provisioned box; panics with the provisioning step otherwise.
pub(crate) fn require_full_vault(catalog: &AssetVaultCatalog) {
    let count = catalog.record_count();
    assert!(
        count >= FULL_VAULT_MIN_RECORDS,
        "ASSET_VAULT_UNDERMATERIALIZED current_record_count={count} required_floor={FULL_VAULT_MIN_RECORDS} \
         -- this test asserts against the FULL asset corpus but the vault is under-materialized. {}",
        asset_vault_remediation()
    );
}

/// Unwrap a `Result` whose `Err` means a required asset-vault fixture is absent, naming both the
/// underlying (asset-naming) error and the provisioning step.
pub(crate) fn expect_asset_vault<T, E: Debug>(result: Result<T, E>, context: &str) -> T {
    result.unwrap_or_else(|error| {
        panic!(
            "ASSET_VAULT_FIXTURE_MISSING context={context} error={error:?} -- {}",
            asset_vault_remediation()
        )
    })
}

/// Unwrap a `Result` whose `Err` means the canonical Calyx substrate is unprovisioned, naming
/// both the underlying `LITD_CALYX_CANONICAL_PATH_MISSING` error and the provisioning step.
pub(crate) fn expect_calyx_substrate<T, E: Debug>(result: Result<T, E>, context: &str) -> T {
    result.unwrap_or_else(|error| {
        panic!(
            "CALYX_SUBSTRATE_UNPROVISIONED context={context} error={error:?} -- {}",
            calyx_substrate_remediation()
        )
    })
}
