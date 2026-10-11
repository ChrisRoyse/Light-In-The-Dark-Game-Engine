//! Append-only audit ledger for the procedural-geometry MCP tool.
//!
//! This ledger is MCP tool-call provenance (who invoked which tool, over which
//! request, producing which content-addressed mesh) -- structured editor data.
//! Per the Calyx single-source-of-truth doctrine (#1348) that class of data is
//! authoritative ONLY inside Calyx, never in a bespoke on-disk JSON store. So
//! the ledger is persisted as a Calyx Aster plain-column row via
//! `litd_engine::calyx_json_store`, exactly like the sibling MCP asset/agent
//! tool ledgers (`mcp_asset_tools_store`, `mcp_agent_tools_store`). The raw mesh
//! geometry (vertex/index bytes) lives separately in the content-addressed store
//! as permitted opaque asset blobs; only that store, plus this Calyx ledger row,
//! are authoritative. Each append writes the row, then reads it straight back
//! out of Aster and re-hashes it, so the returned `sha256` is proof of what
//! actually committed (FSV, #800).

use std::path::Path;

use litd_engine::{
    EngineError,
    calyx_json_store::{
        CalyxJsonStoreSpec, CalyxJsonWalReadback, read_calyx_json_bytes, read_calyx_json_wal_write,
        write_calyx_json_bytes,
    },
};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

pub const PROCGEN_LEDGER_SCHEMA: &str = "litd.procgen_mcp_ledger.v1";

const PROCGEN_LEDGER_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCB";
const PROCGEN_LEDGER_VAULT_PURPOSE: &[u8] = b"litd-mcp-procgen-tools-issue1482";
const PROCGEN_LEDGER_COLLECTION: &str = "litd_mcp_procgen_tools";
const PROCGEN_LEDGER_ROW: &[u8] = b"procgen/ledger/latest";
const PROCGEN_LEDGER_COLUMN: &[u8] = b"ledger_json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcGenToolRecord {
    pub request_id: String,
    pub requested_by: String,
    pub tool_name: String,
    pub operation_id: String,
    pub mesh_id: String,
    pub request_kind: String,
    pub content_hash: String,
    pub vertex_count: usize,
    pub index_count: usize,
    pub triangle_count: usize,
    pub manifest_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcGenLedger {
    pub schema: String,
    pub records: Vec<ProcGenToolRecord>,
}

impl Default for ProcGenLedger {
    fn default() -> Self {
        Self {
            schema: PROCGEN_LEDGER_SCHEMA.to_string(),
            records: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcGenLedgerReadback {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub record_count: usize,
    pub last_content_hash: Option<String>,
}

fn ledger_spec(vault_dir: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir,
        vault_id: PROCGEN_LEDGER_VAULT_ID,
        purpose: PROCGEN_LEDGER_VAULT_PURPOSE,
        collection: PROCGEN_LEDGER_COLLECTION,
        row_key: PROCGEN_LEDGER_ROW,
        column: PROCGEN_LEDGER_COLUMN,
    }
}

/// Append `record` to the Calyx Aster ledger row, write it, then read it back
/// out of Aster and re-hash it.
pub fn append_procgen_ledger(
    vault_dir: &Path,
    record: ProcGenToolRecord,
) -> Result<ProcGenLedgerReadback, EngineError> {
    let mut ledger = load_or_create(vault_dir)?;
    ledger.records.push(record);
    let bytes = serde_json::to_vec_pretty(&ledger)
        .map_err(|error| persistence("ledger.encode", error.to_string()))?;
    let stored = write_calyx_json_bytes(&ledger_spec(vault_dir), &bytes)
        .map_err(|error| persistence("ledger.write", error))?;
    if stored.bytes != bytes {
        return Err(persistence(
            "ledger.write",
            "Aster ledger bytes did not match the serialized ledger".to_string(),
        ));
    }
    read_procgen_ledger(vault_dir)
}

/// Read the ledger row back out of the Calyx Aster vault and re-verify its
/// digest. Fails loud if the vault holds no procgen ledger row.
pub fn read_procgen_ledger(vault_dir: &Path) -> Result<ProcGenLedgerReadback, EngineError> {
    let readback = read_calyx_json_bytes(&ledger_spec(vault_dir))
        .map_err(|error| persistence("ledger.read", error))?
        .ok_or_else(|| {
            persistence(
                "ledger.read",
                format!(
                    "missing Calyx Aster procgen ledger row at {}",
                    vault_dir.display()
                ),
            )
        })?;
    let ledger = decode(&readback.bytes)?;
    Ok(ProcGenLedgerReadback {
        path: vault_dir.display().to_string(),
        sha256: sha256_hex(&readback.bytes),
        bytes: readback.bytes.len() as u64,
        record_count: ledger.records.len(),
        last_content_hash: ledger
            .records
            .last()
            .map(|record| record.content_hash.clone()),
    })
}

/// Read the ledger row's committed bytes straight from the PHYSICAL Aster WAL
/// segment on disk -- not the row-read API, not the tool's return value. The
/// returned `sha256` is over the exact JSON payload replayed from the vault's
/// `wal/` frame, so it is the deepest available proof that the Calyx Aster vault
/// (down to its on-disk write-ahead log) is the ledger's single source of truth.
/// Fails loud if the vault holds no procgen ledger row.
pub fn read_procgen_ledger_wal(vault_dir: &Path) -> Result<CalyxJsonWalReadback, EngineError> {
    let row = read_calyx_json_bytes(&ledger_spec(vault_dir))
        .map_err(|error| persistence("ledger.wal_read", error))?
        .ok_or_else(|| {
            persistence(
                "ledger.wal_read",
                format!(
                    "missing Calyx Aster procgen ledger row at {}",
                    vault_dir.display()
                ),
            )
        })?;
    read_calyx_json_wal_write(vault_dir, row.seq)
        .map_err(|error| persistence("ledger.wal_read", error))
}

fn load_or_create(vault_dir: &Path) -> Result<ProcGenLedger, EngineError> {
    match read_calyx_json_bytes(&ledger_spec(vault_dir))
        .map_err(|error| persistence("ledger.read", error))?
    {
        Some(readback) => decode(&readback.bytes),
        None => Ok(ProcGenLedger::default()),
    }
}

fn decode(bytes: &[u8]) -> Result<ProcGenLedger, EngineError> {
    let ledger: ProcGenLedger = serde_json::from_slice(bytes)
        .map_err(|error| persistence("ledger.decode", error.to_string()))?;
    if ledger.schema != PROCGEN_LEDGER_SCHEMA {
        return Err(EngineError::ProcGenReadbackMismatch {
            field: "ledger.schema".to_string(),
            expected: PROCGEN_LEDGER_SCHEMA.to_string(),
            actual: ledger.schema,
        });
    }
    Ok(ledger)
}

fn persistence(op: &str, reason: String) -> EngineError {
    EngineError::ProcGenPersistenceFailed {
        op: op.to_string(),
        reason,
    }
}
