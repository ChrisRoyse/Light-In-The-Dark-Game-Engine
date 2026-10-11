use litd_hash::sha256_hex;
use std::path::Path;

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::EngineError;

use super::{StructuredCapabilityCoverageAudit, validate_structured_capability_coverage_audit};

const COVERAGE_ASTER_COLLECTION: &str = "litd_structured_capability_coverage";
const COVERAGE_AUDIT_COLUMN: &[u8] = b"structured_capability_coverage_audit_json";
const COVERAGE_ROW_PREFIX: &str = "structured_capability_coverage/audit/";
const COVERAGE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FB7";
const COVERAGE_VAULT_PURPOSE: &[u8] = b"litd-structured-capability-coverage-issue1421";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredCapabilityCoverageWriteback {
    pub bytes: Vec<u8>,
    pub seq: u64,
    pub graph_rows: usize,
    pub cell_key: String,
    pub index_key: String,
    pub sha256: String,
    pub mapped_tool_count: usize,
    pub covered_subsystem_count: usize,
}

pub fn write_structured_capability_coverage_audit(
    _state_path: &Path,
    aster_dir: &Path,
    audit: &StructuredCapabilityCoverageAudit,
) -> Result<StructuredCapabilityCoverageWriteback, EngineError> {
    validate_structured_capability_coverage_audit(audit)?;
    let bytes = serialize_audit(audit)?;
    let readback = write_aster_audit(aster_dir, audit, &bytes)?;
    if readback.bytes != bytes {
        return invalid_store(
            "aster.readback",
            "Aster structured capability coverage bytes did not match serialized audit",
        );
    }
    Ok(readback)
}

pub fn read_structured_capability_coverage_audit(
    aster_dir: &Path,
    audit_id: &str,
) -> Result<Option<StructuredCapabilityCoverageWriteback>, EngineError> {
    validate_audit_id(audit_id)?;
    let vault = open_aster(aster_dir)?;
    let column = PlainColumn::new(&vault, COVERAGE_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let seq = vault.latest_seq();
    let row = audit_row(audit_id);
    let Some(bytes) = column
        .get(seq, &row, COVERAGE_AUDIT_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
    else {
        return Ok(None);
    };
    let audit = parse_structured_capability_coverage_audit(&bytes)?;
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    Ok(Some(StructuredCapabilityCoverageWriteback {
        sha256: sha256_hex(&bytes),
        bytes,
        seq,
        graph_rows: raw.len(),
        cell_key: "loaded".to_string(),
        index_key: "loaded".to_string(),
        mapped_tool_count: audit.mapped_tool_count,
        covered_subsystem_count: audit.covered_subsystem_count,
    }))
}

pub fn parse_structured_capability_coverage_audit(
    bytes: &[u8],
) -> Result<StructuredCapabilityCoverageAudit, EngineError> {
    let audit =
        serde_json::from_slice::<StructuredCapabilityCoverageAudit>(bytes).map_err(|error| {
            EngineError::SerializationFailed {
                format: "json",
                reason: format!("parse structured capability coverage audit: {error}"),
            }
        })?;
    validate_structured_capability_coverage_audit(&audit)?;
    Ok(audit)
}

fn write_aster_audit(
    dir: &Path,
    audit: &StructuredCapabilityCoverageAudit,
    bytes: &[u8],
) -> Result<StructuredCapabilityCoverageWriteback, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, COVERAGE_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let row = audit_row(&audit.audit_id);
    let commit = column
        .put(&row, COVERAGE_AUDIT_COLUMN, bytes)
        .map_err(|error| store_error("aster.put", error))?;
    let value = column
        .get(commit.seq, &row, COVERAGE_AUDIT_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
        .ok_or_else(|| store_error("aster.get", "audit missing immediately after put"))?;
    if value != bytes {
        return invalid_store(
            "aster.readback",
            "audit bytes differed immediately after put",
        );
    }
    let raw = vault
        .scan_cf_at(commit.seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    let cell_ok = raw
        .iter()
        .any(|(key, value)| key == &commit.cell_key && value.as_slice() == bytes);
    let index_ok = raw
        .iter()
        .any(|(key, value)| key == &commit.index_key && value.as_slice() == bytes);
    if !cell_ok || !index_ok {
        return invalid_store(
            "aster.graph_cf",
            "Aster Graph CF did not contain both structured capability coverage rows",
        );
    }
    Ok(StructuredCapabilityCoverageWriteback {
        bytes: value,
        seq: commit.seq,
        graph_rows: raw.len(),
        cell_key: hex_preview(&commit.cell_key),
        index_key: hex_preview(&commit.index_key),
        sha256: sha256_hex(bytes),
        mapped_tool_count: audit.mapped_tool_count,
        covered_subsystem_count: audit.covered_subsystem_count,
    })
}

fn serialize_audit(audit: &StructuredCapabilityCoverageAudit) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(audit).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize structured capability coverage audit: {error}"),
    })
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = COVERAGE_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| store_error("aster.vault_id", error))?;
    AsterVault::open(
        dir,
        vault_id,
        COVERAGE_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| store_error("aster.open", error))
}

fn audit_row(audit_id: &str) -> Vec<u8> {
    format!("{COVERAGE_ROW_PREFIX}{audit_id}").into_bytes()
}

fn validate_audit_id(audit_id: &str) -> Result<(), EngineError> {
    if !crate::row_safe_ledger_id::is_row_safe_ledger_id(audit_id) {
        return invalid_store("row", "audit id must be non-empty and single-line");
    }
    Ok(())
}

fn invalid_store<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(store_error(field, reason.into()))
}

fn store_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!(
            "editor.structured_capability_coverage.store.{}",
            field.into()
        ),
        reason: reason.to_string(),
    }
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(18)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}
