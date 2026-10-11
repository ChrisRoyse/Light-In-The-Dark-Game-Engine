//! Cadence-flushed computer-opponent ledger for the per-AI-decision-tick promotion
//! path (issue #1524 item 3/4).
//!
//! The tool-cadence path (`write_computer_opponent_ledger`) writes then re-reads,
//! re-parses, re-hashes and deep-compares the ledger on **every** invocation — the
//! correct full-state verification for a tool call, and it stays as-is. But if that
//! path is ever promoted to run per AI decision tick, doing a write + full readback +
//! deep-compare every tick is pure hot-loop waste.
//!
//! This module is that promotion-ready path: rows are accumulated in memory and only
//! flushed to the Aster store at a bounded cadence (a count threshold). Each flush is a
//! single durable append (one Aster WAL commit, which internally reads back and
//! byte-compares the row) plus **one** amortized full parse-and-deep-compare
//! verification — instead of one per row. Durability is preserved: every flush is a
//! durable batch boundary, and `finish` reads every flushed batch back and confirms no
//! appended row was lost.
//!
//! This is the group-commit / write-ahead-log pattern (append many records, amortize
//! the durable-sync + verification across the batch), applied to an append-only ledger.

use std::path::{Path, PathBuf};

use litd_engine::{
    EngineError,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
};
use serde::{Deserialize, Serialize};

use crate::history::stable_fingerprint;

use super::computer_opponents_types::{
    COMPUTER_OPPONENT_COLLECTION, COMPUTER_OPPONENT_COLUMN, COMPUTER_OPPONENT_ISSUE,
    COMPUTER_OPPONENT_PURPOSE, COMPUTER_OPPONENT_VAULT_ID, ComputerOpponentRunLedger,
};

/// Schema tag for a flushed batch document.
pub const COMPUTER_OPPONENT_BATCH_SCHEMA: &str = "litd.editor.computer_opponents.batch.v1";

/// One durably-flushed batch of run-ledger rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerOpponentLedgerBatch {
    pub schema: String,
    pub issue: u32,
    pub run_id: String,
    pub batch_index: usize,
    pub row_count: usize,
    pub rows: Vec<ComputerOpponentRunLedger>,
    pub batch_fingerprint: String,
}

/// Summary of a completed batched-ledger run, used for FSV readback.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchedLedgerReadback {
    pub vault_dir: PathBuf,
    pub run_id: String,
    /// Number of durable store writes performed (the fsync/flush-equivalent count).
    pub flush_count: usize,
    /// Total rows appended over the run.
    pub appended: usize,
    /// Every appended row, read back from the store in append order.
    pub rows: Vec<ComputerOpponentRunLedger>,
}

/// In-memory accumulator that flushes run-ledger rows to the Aster store at a cadence.
#[derive(Debug)]
pub struct BatchedComputerOpponentLedger {
    vault_dir: PathBuf,
    run_id: String,
    cadence: usize,
    pending: Vec<ComputerOpponentRunLedger>,
    flush_count: usize,
    appended: usize,
}

impl BatchedComputerOpponentLedger {
    /// Creates an accumulator that flushes once `cadence` rows are buffered. `cadence`
    /// is clamped to at least 1 (1 reproduces the per-row write-and-verify behavior).
    pub fn new(
        vault_dir: impl Into<PathBuf>,
        run_id: impl Into<String>,
        cadence: usize,
    ) -> Result<Self, EngineError> {
        let vault_dir = vault_dir.into();
        let run_id = run_id.into();
        if vault_dir.as_os_str().is_empty() {
            return Err(invalid_batch("vault_dir", "path must be non-empty"));
        }
        if run_id.trim().is_empty() {
            return Err(invalid_batch("run_id", "must be non-empty"));
        }
        Ok(Self {
            vault_dir,
            run_id,
            cadence: cadence.max(1),
            pending: Vec::new(),
            flush_count: 0,
            appended: 0,
        })
    }

    /// Number of durable flushes performed so far (fsync/flush-equivalent count).
    pub fn flush_count(&self) -> usize {
        self.flush_count
    }

    /// Total rows appended so far (whether or not yet flushed).
    pub fn appended(&self) -> usize {
        self.appended
    }

    /// Buffers a row in memory, flushing durably only when the cadence is reached.
    pub fn append(&mut self, row: ComputerOpponentRunLedger) -> Result<(), EngineError> {
        self.pending.push(row);
        self.appended += 1;
        if self.pending.len() >= self.cadence {
            self.flush()?;
        }
        Ok(())
    }

    /// Durably writes any buffered rows as one batch, then verifies that batch once.
    /// Returns the number of rows flushed. A no-op (returns 0) when nothing is pending.
    pub fn flush(&mut self) -> Result<usize, EngineError> {
        if self.pending.is_empty() {
            return Ok(0);
        }
        let batch_index = self.flush_count;
        let rows = std::mem::take(&mut self.pending);
        let flushed = write_batch(&self.vault_dir, &self.run_id, batch_index, rows)?;
        self.flush_count += 1;
        Ok(flushed)
    }

    /// Flushes the remainder and proves durability: reads every flushed batch back and
    /// confirms every appended row survived (no data loss at any batch boundary).
    pub fn finish(mut self) -> Result<BatchedLedgerReadback, EngineError> {
        self.flush()?;
        let mut rows = Vec::with_capacity(self.appended);
        for batch_index in 0..self.flush_count {
            let batch = read_batch(&self.vault_dir, batch_index)?;
            rows.extend(batch.rows);
        }
        if rows.len() != self.appended {
            return Err(invalid_batch(
                "finish.durability",
                "read-back row count did not match appended rows",
            ));
        }
        Ok(BatchedLedgerReadback {
            vault_dir: self.vault_dir,
            run_id: self.run_id,
            flush_count: self.flush_count,
            appended: self.appended,
            rows,
        })
    }
}

/// Reads one flushed batch document back from the store and validates it.
pub fn read_batch(
    vault_dir: &Path,
    batch_index: usize,
) -> Result<ComputerOpponentLedgerBatch, EngineError> {
    let row_key = row_key_for(batch_index);
    let spec = batch_spec(vault_dir, &row_key);
    let readback = read_calyx_json_bytes(&spec)
        .map_err(|error| invalid_batch("batch.read", error))?
        .ok_or_else(|| invalid_batch("batch.read", format!("missing batch {batch_index}")))?;
    let batch = serde_json::from_slice::<ComputerOpponentLedgerBatch>(&readback.bytes).map_err(
        |error| EngineError::SerializationFailed {
            format: "computer-opponent-ledger-batch-json",
            reason: format!("parse computer opponent ledger batch: {error}"),
        },
    )?;
    validate_batch(&batch)?;
    Ok(batch)
}

fn write_batch(
    vault_dir: &Path,
    run_id: &str,
    batch_index: usize,
    rows: Vec<ComputerOpponentRunLedger>,
) -> Result<usize, EngineError> {
    let row_count = rows.len();
    let mut batch = ComputerOpponentLedgerBatch {
        schema: COMPUTER_OPPONENT_BATCH_SCHEMA.to_string(),
        issue: COMPUTER_OPPONENT_ISSUE,
        run_id: run_id.to_string(),
        batch_index,
        row_count,
        rows,
        batch_fingerprint: String::new(),
    };
    batch.batch_fingerprint = batch_fingerprint(&batch)?;
    let bytes =
        serde_json::to_vec_pretty(&batch).map_err(|error| EngineError::SerializationFailed {
            format: "computer-opponent-ledger-batch-json",
            reason: format!("serialize computer opponent ledger batch: {error}"),
        })?;
    let row_key = row_key_for(batch_index);
    let spec = batch_spec(vault_dir, &row_key);
    // One durable append: the store commits a WAL frame and byte-verifies the row.
    let stored = write_calyx_json_bytes(&spec, &bytes)
        .map_err(|error| invalid_batch("batch.write", error))?;
    if stored.bytes != bytes {
        return Err(invalid_batch(
            "batch.write",
            "Aster batch bytes did not match serialized bytes",
        ));
    }
    // One amortized full parse-and-deep-compare verification for the whole batch,
    // instead of one per row.
    let readback = read_batch(vault_dir, batch_index)?;
    if readback != batch {
        return Err(invalid_batch("batch.readback", "batch mismatch"));
    }
    Ok(row_count)
}

fn validate_batch(batch: &ComputerOpponentLedgerBatch) -> Result<(), EngineError> {
    let mut expected = batch.clone();
    let fingerprint = std::mem::take(&mut expected.batch_fingerprint);
    if expected.schema != COMPUTER_OPPONENT_BATCH_SCHEMA
        || expected.issue != COMPUTER_OPPONENT_ISSUE
        || expected.run_id.trim().is_empty()
        || expected.row_count != expected.rows.len()
        || fingerprint.len() != 16
    {
        return Err(invalid_batch(
            "batch",
            "schema, run id, counts, or fingerprint invalid",
        ));
    }
    if stable_fingerprint(&expected)? != fingerprint {
        return Err(invalid_batch("batch.fingerprint", "fingerprint mismatch"));
    }
    Ok(())
}

fn batch_fingerprint(batch: &ComputerOpponentLedgerBatch) -> Result<String, EngineError> {
    let mut expected = batch.clone();
    expected.batch_fingerprint.clear();
    stable_fingerprint(&expected)
}

fn row_key_for(batch_index: usize) -> Vec<u8> {
    format!("computer_opponents/batch/{batch_index}").into_bytes()
}

fn batch_spec<'a>(vault_dir: &'a Path, row_key: &'a [u8]) -> CalyxJsonStoreSpec<'a> {
    CalyxJsonStoreSpec {
        vault_dir,
        vault_id: COMPUTER_OPPONENT_VAULT_ID,
        purpose: COMPUTER_OPPONENT_PURPOSE,
        collection: COMPUTER_OPPONENT_COLLECTION,
        row_key,
        column: COMPUTER_OPPONENT_COLUMN,
    }
}

fn invalid_batch(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::AiInvalidConfig {
        field: format!("computer_opponents.batch.{field}"),
        reason: reason.into(),
    }
}
