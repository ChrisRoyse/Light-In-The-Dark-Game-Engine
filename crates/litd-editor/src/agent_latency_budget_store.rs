use litd_hash::sha256_hex;
use std::path::Path;

use calyx_aster::{cf::ColumnFamily, plain_column::PlainColumn, vault::AsterVault};
use calyx_core::{SystemClock, VaultId};
use litd_engine::EngineError;

use super::{AgentLatencyBudgetLedger, validate_agent_latency_budget_ledger};

const AGENT_LATENCY_ASTER_COLLECTION: &str = "litd_agent_latency_budget";
const AGENT_LATENCY_LEDGER_COLUMN: &[u8] = b"agent_latency_budget_ledger_json";
const AGENT_LATENCY_ROW_PREFIX: &str = "play/agent-latency/ledger/";
const AGENT_LATENCY_VAULT_ID: &str = "01J2X3Y4Z5A6B7C8D9E0F1G2J9";
const AGENT_LATENCY_VAULT_PURPOSE: &[u8] = b"litd-agent-latency-issue1179";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLatencyBudgetAsterReadback {
    pub bytes: Vec<u8>,
    pub seq: u64,
    pub graph_rows: usize,
    pub cell_key: String,
    pub index_key: String,
    pub sha256: String,
    pub tier_count: usize,
    pub all_tiers_within_budget: bool,
    pub cold_and_warm_measured: bool,
    pub slowest_stage_name: String,
}

pub fn write_agent_latency_budget_ledger(
    _state_path: &Path,
    aster_dir: &Path,
    ledger: &AgentLatencyBudgetLedger,
) -> Result<AgentLatencyBudgetAsterReadback, EngineError> {
    validate_agent_latency_budget_ledger(ledger)?;
    let bytes = serialize_ledger(ledger)?;
    let readback = write_aster_ledger(aster_dir, ledger, &bytes)?;
    if readback.bytes != bytes {
        return invalid_store(
            "aster.readback",
            "Aster agent-latency bytes did not match serialized ledger",
        );
    }
    Ok(readback)
}

pub fn read_agent_latency_budget_ledger(
    aster_dir: &Path,
    ledger_id: &str,
) -> Result<Option<AgentLatencyBudgetAsterReadback>, EngineError> {
    validate_ledger_id(ledger_id)?;
    let vault = open_aster(aster_dir)?;
    let column = PlainColumn::new(&vault, AGENT_LATENCY_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let seq = vault.latest_seq();
    let row = ledger_row(ledger_id);
    let Some(bytes) = column
        .get(seq, &row, AGENT_LATENCY_LEDGER_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
    else {
        return Ok(None);
    };
    let ledger = parse_agent_latency_budget_ledger(&bytes)?;
    let raw = vault
        .scan_cf_at(seq, ColumnFamily::Graph)
        .map_err(|error| store_error("aster.graph_scan", error))?;
    Ok(Some(readback_from_ledger(bytes, seq, raw.len(), ledger)))
}

pub fn parse_agent_latency_budget_ledger(
    bytes: &[u8],
) -> Result<AgentLatencyBudgetLedger, EngineError> {
    let ledger = serde_json::from_slice::<AgentLatencyBudgetLedger>(bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse agent-latency ledger: {error}"),
        }
    })?;
    validate_agent_latency_budget_ledger(&ledger)?;
    Ok(ledger)
}

fn write_aster_ledger(
    dir: &Path,
    ledger: &AgentLatencyBudgetLedger,
    bytes: &[u8],
) -> Result<AgentLatencyBudgetAsterReadback, EngineError> {
    let vault = open_aster(dir)?;
    let column = PlainColumn::new(&vault, AGENT_LATENCY_ASTER_COLLECTION)
        .map_err(|error| store_error("aster.column", error))?;
    let row = ledger_row(&ledger.ledger_id);
    let commit = column
        .put(&row, AGENT_LATENCY_LEDGER_COLUMN, bytes)
        .map_err(|error| store_error("aster.put", error))?;
    let value = column
        .get(commit.seq, &row, AGENT_LATENCY_LEDGER_COLUMN)
        .map_err(|error| store_error("aster.get", error))?
        .ok_or_else(|| store_error("aster.get", "ledger missing immediately after put"))?;
    if value != bytes {
        return invalid_store(
            "aster.readback",
            "ledger bytes differed immediately after put",
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
            "Aster Graph CF did not contain both agent-latency ledger rows",
        );
    }
    let mut readback = readback_from_ledger(value, commit.seq, raw.len(), ledger.clone());
    readback.cell_key = hex_preview(&commit.cell_key);
    readback.index_key = hex_preview(&commit.index_key);
    Ok(readback)
}

fn readback_from_ledger(
    bytes: Vec<u8>,
    seq: u64,
    graph_rows: usize,
    ledger: AgentLatencyBudgetLedger,
) -> AgentLatencyBudgetAsterReadback {
    AgentLatencyBudgetAsterReadback {
        sha256: sha256_hex(&bytes),
        bytes,
        seq,
        graph_rows,
        cell_key: "loaded".to_string(),
        index_key: "loaded".to_string(),
        tier_count: ledger.readback.tier_count,
        all_tiers_within_budget: ledger.readback.all_tiers_within_budget,
        cold_and_warm_measured: ledger.readback.cold_and_warm_measured,
        slowest_stage_name: ledger.readback.slowest_stage_name,
    }
}

fn serialize_ledger(ledger: &AgentLatencyBudgetLedger) -> Result<Vec<u8>, EngineError> {
    serde_json::to_vec_pretty(ledger).map_err(|error| EngineError::SerializationFailed {
        format: "json",
        reason: format!("serialize agent-latency ledger: {error}"),
    })
}

fn open_aster(dir: &Path) -> Result<AsterVault<SystemClock>, EngineError> {
    let vault_id = AGENT_LATENCY_VAULT_ID
        .parse::<VaultId>()
        .map_err(|error| store_error("aster.vault_id", error))?;
    AsterVault::open(
        dir,
        vault_id,
        AGENT_LATENCY_VAULT_PURPOSE.to_vec(),
        Default::default(),
    )
    .map_err(|error| store_error("aster.open", error))
}

fn ledger_row(ledger_id: &str) -> Vec<u8> {
    format!("{AGENT_LATENCY_ROW_PREFIX}{ledger_id}").into_bytes()
}

fn validate_ledger_id(ledger_id: &str) -> Result<(), EngineError> {
    if !crate::row_safe_ledger_id::is_row_safe_ledger_id(ledger_id) {
        return invalid_store("row", "ledger id must be non-empty and single-line");
    }
    Ok(())
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

fn invalid_store<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(store_error(field, reason.into()))
}

fn store_error(field: impl Into<String>, reason: impl ToString) -> EngineError {
    let field = format!("editor.agent_latency_budget.store.{}", field.into());
    let reason = reason.to_string();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-AGENT-LATENCY-BUDGET-STORE",
        "agent-latency store rejected"
    );
    EngineError::UiRenderInvalidConfig { field, reason }
}
