use std::path::{Path, PathBuf};

use calyx_aster::{cf::ColumnFamily, vault::AsterVault};
use calyx_core::{CalyxError, SystemClock, VaultId};
use litd_engine::EngineError;
use litd_hash::sha256_hex;

const EDITOR_STATE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAX";
const EDITOR_STATE_VAULT_PURPOSE: &[u8] = b"litd-editor-state-issue1357";
const SHELL_STATE_KEY: &[u8] = b"editor/shell/state/latest";
const PREFERENCES_KEY: &[u8] = b"editor/preferences/latest";
const HISTORY_DOCUMENT_PREFIX: &str = "editor/history/document/";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EditorStateReadback {
    pub vault_dir: String,
    pub cf: String,
    pub key_hex: String,
    pub seq: u64,
    pub graph_rows: usize,
    pub bytes: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EditorStateGraphRow {
    pub key_hex: String,
    pub bytes: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorStateStore {
    vault_dir: PathBuf,
}

impl EditorStateStore {
    pub fn new(vault_dir: impl Into<PathBuf>) -> Self {
        Self {
            vault_dir: vault_dir.into(),
        }
    }

    pub fn vault_dir(&self) -> &Path {
        &self.vault_dir
    }

    pub fn shell_state_exists(&self) -> Result<bool, EngineError> {
        Ok(self.read_shell_state()?.is_some())
    }

    pub fn put_shell_state(&self, bytes: &[u8]) -> Result<EditorStateReadback, EngineError> {
        self.put_graph(SHELL_STATE_KEY.to_vec(), bytes)
    }

    pub fn read_shell_state(&self) -> Result<Option<Vec<u8>>, EngineError> {
        self.read_latest(SHELL_STATE_KEY)
    }

    pub fn put_preferences(&self, bytes: &[u8]) -> Result<EditorStateReadback, EngineError> {
        self.put_graph(PREFERENCES_KEY.to_vec(), bytes)
    }

    pub fn read_preferences(&self) -> Result<Option<Vec<u8>>, EngineError> {
        self.read_latest(PREFERENCES_KEY)
    }

    pub fn put_history_bytes(
        &self,
        document_id: &str,
        bytes: &[u8],
    ) -> Result<EditorStateReadback, EngineError> {
        self.put_graph(history_document_key(document_id)?, bytes)
    }

    pub fn read_history_bytes_at(
        &self,
        document_id: &str,
        seq: u64,
    ) -> Result<Option<Vec<u8>>, EngineError> {
        self.read_at(&history_document_key(document_id)?, seq)
    }

    pub fn read_graph_rows(&self) -> Result<Vec<EditorStateGraphRow>, EngineError> {
        let vault = self.open_vault()?;
        let rows = vault
            .scan_cf_at(vault.latest_seq(), ColumnFamily::Graph)
            .map_err(|error| calyx_state_error("aster.graph_scan", &error))?;
        Ok(rows
            .into_iter()
            .map(|(key, value)| EditorStateGraphRow {
                key_hex: hex(&key),
                bytes: value.len(),
                sha256: sha256_hex(&value),
            })
            .collect())
    }

    pub fn latest_seq(&self) -> Result<u64, EngineError> {
        Ok(self.open_vault()?.latest_seq())
    }

    fn put_graph(&self, key: Vec<u8>, bytes: &[u8]) -> Result<EditorStateReadback, EngineError> {
        if bytes.is_empty() {
            return invalid_state("payload", "editor state payload must be non-empty");
        }
        let vault = self.open_vault()?;
        let seq = vault
            .write_cf(ColumnFamily::Graph, key.clone(), bytes.to_vec())
            .map_err(|error| calyx_state_error("aster.put", &error))?;
        vault
            .flush()
            .map_err(|error| calyx_state_error("aster.flush", &error))?;
        let value = vault
            .read_cf_at(seq, ColumnFamily::Graph, &key)
            .map_err(|error| calyx_state_error("aster.get", &error))?
            .ok_or_else(|| state_error("aster.get", "editor state row missing after put"))?;
        if value != bytes {
            return invalid_state("aster.readback", "editor state bytes changed after put");
        }
        let graph_rows = vault
            .scan_cf_at(seq, ColumnFamily::Graph)
            .map_err(|error| calyx_state_error("aster.graph_scan", &error))?
            .len();
        Ok(EditorStateReadback {
            vault_dir: self.vault_dir.display().to_string(),
            cf: ColumnFamily::Graph.name().to_string(),
            key_hex: hex(&key),
            seq,
            graph_rows,
            bytes: value.len(),
            sha256: sha256_hex(&value),
        })
    }

    fn read_latest(&self, key: &[u8]) -> Result<Option<Vec<u8>>, EngineError> {
        let vault = self.open_vault()?;
        self.read_at_with_vault(&vault, key, vault.latest_seq())
    }

    fn read_at(&self, key: &[u8], seq: u64) -> Result<Option<Vec<u8>>, EngineError> {
        let vault = self.open_vault()?;
        self.read_at_with_vault(&vault, key, seq)
    }

    fn read_at_with_vault(
        &self,
        vault: &AsterVault<SystemClock>,
        key: &[u8],
        seq: u64,
    ) -> Result<Option<Vec<u8>>, EngineError> {
        vault
            .read_cf_at(seq, ColumnFamily::Graph, key)
            .map_err(|error| calyx_state_error("aster.get", &error))
    }

    fn open_vault(&self) -> Result<AsterVault<SystemClock>, EngineError> {
        let vault_id = EDITOR_STATE_VAULT_ID
            .parse::<VaultId>()
            .map_err(|error| state_error("aster.vault_id", error.to_string()))?;
        AsterVault::open(
            &self.vault_dir,
            vault_id,
            EDITOR_STATE_VAULT_PURPOSE.to_vec(),
            Default::default(),
        )
        .map_err(|error| calyx_state_error("aster.open", &error))
    }
}

fn history_document_key(document_id: &str) -> Result<Vec<u8>, EngineError> {
    if document_id.trim().is_empty() || document_id.contains(['\n', '\r', '\t']) {
        return invalid_state(
            "history.document_id",
            "document id must be non-empty and single-line",
        );
    }
    Ok(format!("{HISTORY_DOCUMENT_PREFIX}{document_id}").into_bytes())
}

fn invalid_state<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(state_error(field, reason.into()))
}

/// Wrap a Calyx/Aster store failure (vault open/read/write/flush/scan) into an EngineError
/// WITHOUT discarding the source error's own PRD-18 remediation (#1531).
///
/// A `CalyxError` carries a stable `code`, a concrete `message`, AND its own `remediation`.
/// The previous code wrapped these with `state_error(field, error.to_string())`, and
/// `CalyxError::to_string()` renders only `"{code}: {message}"` — dropping the source
/// remediation entirely. The wrapping `UiRenderInvalidConfig` variant then contributed a
/// hardcoded, misleading remediation ("Use non-zero viewport and atlas sizes, finite UI
/// rectangles/colors, valid fog grids, and bounded minimap world coordinates.") that is
/// nonsense for a storage/SST-ordering fault. Fold the source code, message, AND remediation
/// into the surfaced reason so the failure names exactly what failed and how to fix it,
/// instead of leaving only the generic UI-geometry guidance.
fn calyx_state_error(field: impl Into<String>, source: &CalyxError) -> EngineError {
    state_error(
        field,
        format!(
            "{}: {} (source remediation: {})",
            source.code, source.message, source.remediation
        ),
    )
}

fn state_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    let field = format!("editor.state.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-EDITOR-STATE-ASTER",
        "editor state store rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The exact hardcoded remediation the UiRenderInvalidConfig variant contributes; it is
    // nonsense for a storage/SST-ordering fault and must not be what a store failure points at.
    const UI_GEOMETRY_REMEDIATION: &str = "Use non-zero viewport and atlas sizes, finite UI rectangles/colors, valid fog grids, and bounded minimap world coordinates.";

    #[test]
    fn calyx_store_failure_propagates_source_remediation_not_ui_geometry() {
        // A real Aster storage fault carries its own PRD-18 code, message, AND remediation.
        // Prove the editor state-store wrapper surfaces THAT remediation (and code + message),
        // rather than silently dropping it and leaving only the misleading generic UI-geometry
        // guidance from the UiRenderInvalidConfig variant (#1531).
        let source = CalyxError {
            code: "CALYX_ASTER_SST_ORDER",
            message: "sst segment 7 precedes committed segment 9".to_string(),
            remediation: "compact the Aster SST segments and retry the durable write",
        };

        let err = calyx_state_error("aster.put", &source);
        let report = err.report();

        // The surfaced failure names exactly what failed: source code + message + remediation.
        assert!(
            report.message.contains("CALYX_ASTER_SST_ORDER"),
            "source code must be surfaced, got: {}",
            report.message
        );
        assert!(
            report
                .message
                .contains("sst segment 7 precedes committed segment 9"),
            "source message must be surfaced, got: {}",
            report.message
        );
        assert!(
            report
                .message
                .contains("compact the Aster SST segments and retry the durable write"),
            "source remediation must be propagated, got: {}",
            report.message
        );
        // The field still pinpoints the exact failing store operation.
        assert!(
            report.message.contains("editor.state.aster.put"),
            "failing store operation must be named, got: {}",
            report.message
        );

        // The misleading UI-geometry guidance must NOT be what the failure text points at.
        assert!(
            !report.message.contains(UI_GEOMETRY_REMEDIATION),
            "storage failure message must not carry the UI-geometry remediation, got: {}",
            report.message
        );
        assert_ne!(report.message, UI_GEOMETRY_REMEDIATION);
    }

    #[test]
    fn non_calyx_state_failure_keeps_scoped_generic_reason_with_message() {
        // A source without its own remediation (here a plain validation reason) keeps a
        // clearly-scoped reason that still includes the concrete failure message (#1531).
        let err = state_error("aster.readback", "editor state bytes changed after put");
        let report = err.report();
        assert!(
            report.message.contains("editor.state.aster.readback"),
            "scoped field must be surfaced, got: {}",
            report.message
        );
        assert!(
            report
                .message
                .contains("editor state bytes changed after put"),
            "concrete failure message must be surfaced, got: {}",
            report.message
        );
    }
}
