use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use calyx_aster::{cf::ColumnFamily, vault::AsterVault};
use calyx_core::{LedgerRef, SystemClock, VaultId};
use litd_engine::EngineError;
use litd_hash::sha256_hex;

use crate::history::{EditorDocument, stable_fingerprint};

use super::mcp_object_tools_store_ingest::StructuredObjectIngestReadback;
use super::mcp_object_tools_store_records::{ledger_row_key, object_row_key};
use super::mcp_object_tools_store_validation::{validate_path, validate_single_line};
use super::{
    DOCUMENT_KEY, LEDGER_LATEST_KEY, McpObjectDocumentReadback, McpObjectReactiveNotification,
    OBJECT_ROW_PREFIX, OBJECT_STORE_VAULT_ID, OBJECT_STORE_VAULT_PURPOSE, store_error,
};

pub(super) fn readback_from_document(
    path: &Path,
    read: &AsterObjectRead,
    document: EditorDocument,
) -> Result<McpObjectDocumentReadback, EngineError> {
    let mut kind_counts = BTreeMap::new();
    for object in document.objects.values() {
        *kind_counts.entry(object.kind.clone()).or_insert(0) += 1;
    }
    Ok(McpObjectDocumentReadback {
        path: path.display().to_string(),
        vault_dir: read.vault_dir.clone(),
        cf: read.cf.clone(),
        key_hex: read.key_hex.clone(),
        seq: read.seq,
        graph_rows: read.graph_rows,
        typed_object_rows: read.typed_object_rows,
        constellation_rows: read.constellation_rows,
        structured_ingest_cx_ids: read.structured_ingest_cx_ids.clone(),
        structured_ingest_ledger_seq: read
            .structured_ingest_ledger_ref
            .as_ref()
            .map(|ledger_ref| ledger_ref.seq),
        structured_ingest_ledger_hash: read
            .structured_ingest_ledger_ref
            .as_ref()
            .map(|ledger_ref| hex(&ledger_ref.hash)),
        reactive_audit_rows: read.reactive_audit_rows,
        reactive_fired_rows: read.reactive_fired_rows,
        reactive_notifications: read.reactive_notifications.clone(),
        bytes: read.bytes.len() as u64,
        sha256: sha256_hex(&read.bytes),
        document_fingerprint: stable_fingerprint(&document)?,
        object_count: document.objects.len(),
        unit_count: count_kind(&kind_counts, "Unit"),
        hero_count: count_kind(&kind_counts, "Hero"),
        ability_count: count_kind(&kind_counts, "Ability"),
        item_count: count_kind(&kind_counts, "Item"),
        buff_count: count_kind(&kind_counts, "Buff"),
        upgrade_count: count_kind(&kind_counts, "Upgrade"),
        tech_count: count_kind(&kind_counts, "Tech"),
        gameplay_table_count: count_kind(&kind_counts, "GameplayTable"),
        kind_counts,
        document,
    })
}

fn count_kind(counts: &BTreeMap<String, usize>, kind: &str) -> usize {
    counts.get(kind).copied().unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct AsterObjectRead {
    pub(super) vault_dir: String,
    pub(super) cf: String,
    pub(super) key_hex: String,
    pub(super) seq: u64,
    pub(super) graph_rows: usize,
    pub(super) typed_object_rows: usize,
    pub(super) constellation_rows: usize,
    pub(super) structured_ingest_cx_ids: Vec<String>,
    pub(super) structured_ingest_ledger_ref: Option<LedgerRef>,
    pub(super) reactive_audit_rows: usize,
    pub(super) reactive_fired_rows: usize,
    pub(super) reactive_notifications: Vec<McpObjectReactiveNotification>,
    pub(super) bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AsterLedgerRead {
    pub(super) vault_dir: String,
    pub(super) cf: String,
    pub(super) key_hex: String,
    pub(super) seq: u64,
    pub(super) graph_rows: usize,
    pub(super) bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct McpObjectAsterStore {
    pub(super) vault_dir: PathBuf,
}

impl McpObjectAsterStore {
    pub(super) fn new(vault_dir: impl Into<PathBuf>) -> Self {
        Self {
            vault_dir: vault_dir.into(),
        }
    }

    pub(super) fn put_document_bytes(
        &self,
        document: &EditorDocument,
        bytes: &[u8],
        structured_ingest: StructuredObjectIngestReadback,
    ) -> Result<AsterObjectRead, EngineError> {
        if bytes.is_empty() {
            return Err(store_error(
                "document.payload",
                "object document payload must be non-empty",
            ));
        }
        let vault = self.open_vault()?;
        let mut rows = Vec::with_capacity(document.objects.len() + 1);
        rows.push((ColumnFamily::Graph, DOCUMENT_KEY.to_vec(), bytes.to_vec()));
        for object in document.objects.values() {
            let row_bytes =
                serde_json::to_vec(object).map_err(|error| EngineError::SerializationFailed {
                    format: "mcp-object-row-aster",
                    reason: format!("serialize typed object row {}: {error}", object.id),
                })?;
            rows.push((ColumnFamily::Graph, object_row_key(object), row_bytes));
        }
        let seq = vault
            .write_cf_batch(rows)
            .map_err(|error| store_error("document.aster.put", error.to_string()))?;
        vault
            .flush()
            .map_err(|error| store_error("document.aster.flush", error.to_string()))?;
        self.read_document_bytes_at(seq, Some(structured_ingest))
    }

    pub(super) fn read_document_bytes(&self) -> Result<AsterObjectRead, EngineError> {
        let vault = self.open_vault()?;
        self.read_document_bytes_at(vault.latest_seq(), None)
    }

    fn read_document_bytes_at(
        &self,
        seq: u64,
        structured_ingest: Option<StructuredObjectIngestReadback>,
    ) -> Result<AsterObjectRead, EngineError> {
        let vault = self.open_vault()?;
        let value = vault
            .read_cf_at(seq, ColumnFamily::Graph, DOCUMENT_KEY)
            .map_err(|error| store_error("document.aster.get", error.to_string()))?
            .ok_or_else(|| {
                store_error(
                    "document.aster.get",
                    "object document row missing from Aster Graph CF",
                )
            })?;
        let graph_rows = vault
            .scan_cf_at(seq, ColumnFamily::Graph)
            .map_err(|error| store_error("document.aster.graph_scan", error.to_string()))?;
        let base_read = self.read_base_rows_at(&vault, seq)?;
        let reactive = self.read_reactive_rows_at(&vault, seq)?;
        let structured_ingest_cx_ids = structured_ingest
            .as_ref()
            .map(|ingest| ingest.cx_ids.clone())
            .unwrap_or(base_read.cx_ids);
        let structured_ingest_ledger_ref = match &structured_ingest {
            Some(ingest) => ingest.ledger_ref.clone(),
            None => base_read.latest_ledger_ref,
        };
        Ok(AsterObjectRead {
            vault_dir: self.vault_dir.display().to_string(),
            cf: ColumnFamily::Graph.name().to_string(),
            key_hex: hex(DOCUMENT_KEY),
            seq,
            graph_rows: graph_rows.len(),
            typed_object_rows: graph_rows
                .iter()
                .filter(|(key, _)| key.starts_with(OBJECT_ROW_PREFIX))
                .count(),
            constellation_rows: base_read.rows,
            structured_ingest_cx_ids,
            structured_ingest_ledger_ref,
            reactive_audit_rows: reactive.audit_rows,
            reactive_fired_rows: reactive.fired_rows,
            reactive_notifications: reactive.notifications,
            bytes: value,
        })
    }

    pub(super) fn put_ledger_bytes(
        &self,
        ledger_id: &str,
        bytes: &[u8],
    ) -> Result<AsterLedgerRead, EngineError> {
        validate_single_line("ledger_id", ledger_id)?;
        if bytes.is_empty() {
            return Err(store_error(
                "ledger.payload",
                "object ledger payload must be non-empty",
            ));
        }
        let vault = self.open_vault()?;
        let seq = vault
            .write_cf_batch([
                (
                    ColumnFamily::Graph,
                    LEDGER_LATEST_KEY.to_vec(),
                    bytes.to_vec(),
                ),
                (
                    ColumnFamily::Graph,
                    ledger_row_key(ledger_id),
                    bytes.to_vec(),
                ),
            ])
            .map_err(|error| store_error("ledger.aster.put", error.to_string()))?;
        vault
            .flush()
            .map_err(|error| store_error("ledger.aster.flush", error.to_string()))?;
        self.read_latest_ledger_bytes_at(seq)
    }

    pub(super) fn read_latest_ledger_bytes(&self) -> Result<AsterLedgerRead, EngineError> {
        let vault = self.open_vault()?;
        self.read_latest_ledger_bytes_at(vault.latest_seq())
    }

    /// Read the latest ledger row, or `Ok(None)` when no ledger has been written yet.
    /// Used by the append path (#1521) so the first write starts a fresh ledger and every
    /// later write accumulates instead of overwriting.
    pub(super) fn read_latest_ledger_bytes_optional(
        &self,
    ) -> Result<Option<AsterLedgerRead>, EngineError> {
        let vault = self.open_vault()?;
        let seq = vault.latest_seq();
        let value = vault
            .read_cf_at(seq, ColumnFamily::Graph, LEDGER_LATEST_KEY)
            .map_err(|error| store_error("ledger.aster.get", error.to_string()))?;
        let Some(value) = value else {
            return Ok(None);
        };
        let graph_rows = vault
            .scan_cf_at(seq, ColumnFamily::Graph)
            .map_err(|error| store_error("ledger.aster.graph_scan", error.to_string()))?;
        Ok(Some(AsterLedgerRead {
            vault_dir: self.vault_dir.display().to_string(),
            cf: ColumnFamily::Graph.name().to_string(),
            key_hex: hex(LEDGER_LATEST_KEY),
            seq,
            graph_rows: graph_rows.len(),
            bytes: value,
        }))
    }

    fn read_latest_ledger_bytes_at(&self, seq: u64) -> Result<AsterLedgerRead, EngineError> {
        let vault = self.open_vault()?;
        let value = vault
            .read_cf_at(seq, ColumnFamily::Graph, LEDGER_LATEST_KEY)
            .map_err(|error| store_error("ledger.aster.get", error.to_string()))?
            .ok_or_else(|| {
                store_error(
                    "ledger.aster.get",
                    "object ledger row missing from Aster Graph CF",
                )
            })?;
        let graph_rows = vault
            .scan_cf_at(seq, ColumnFamily::Graph)
            .map_err(|error| store_error("ledger.aster.graph_scan", error.to_string()))?;
        Ok(AsterLedgerRead {
            vault_dir: self.vault_dir.display().to_string(),
            cf: ColumnFamily::Graph.name().to_string(),
            key_hex: hex(LEDGER_LATEST_KEY),
            seq,
            graph_rows: graph_rows.len(),
            bytes: value,
        })
    }

    pub(super) fn open_vault(&self) -> Result<AsterVault<SystemClock>, EngineError> {
        let vault_id = OBJECT_STORE_VAULT_ID
            .parse::<VaultId>()
            .map_err(|error| store_error("aster.vault_id", error.to_string()))?;
        AsterVault::open(
            &self.vault_dir,
            vault_id,
            OBJECT_STORE_VAULT_PURPOSE.to_vec(),
            Default::default(),
        )
        .map_err(|error| store_error("aster.open", error.to_string()))
    }
}

pub(super) fn object_document_exists(path: &Path) -> Result<bool, EngineError> {
    validate_path("document_path", path)?;
    if !path.exists() {
        return Ok(false);
    }
    let store = McpObjectAsterStore::new(path);
    let vault = store.open_vault()?;
    vault
        .read_cf_at(vault.latest_seq(), ColumnFamily::Graph, DOCUMENT_KEY)
        .map(|value| value.is_some())
        .map_err(|error| store_error("document.aster.get", error.to_string()))
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
