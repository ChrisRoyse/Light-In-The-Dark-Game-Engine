//! Engine-facing Oracle what-if consequence preview.
//!
//! The service composes Calyx Oracle prediction and butterfly expansion, then
//! persists a preview record to Aster Online CF with a Ledger entry. Callers use
//! `read_source_truth` to reopen the persisted result instead of trusting the
//! returned value.

use std::fs;

use calyx_aster::cf::{ColumnFamily, ledger_key};
use calyx_aster::vault::{AsterVault, VaultOptions};
use calyx_core::{Clock, SystemClock, VaultStore};
use calyx_ledger::{ActorId, EntryKind, SubjectId, decode as decode_ledger};
use calyx_oracle::{
    Action, Consequence, DomainId, OracleError, build_tree, oracle_predict, select,
};
use serde_json::json;

use crate::EngineError;

mod analysis;
mod support;
mod types;

use analysis::{raw_cf_rows, tree_diagnostics};
use support::{
    PREVIEW_SCHEMA, calyx_failed, hash_hex, hex_bytes, map_oracle_rejection, oracle_bound,
    oracle_to_engine_error, parse_vault_id, persistence_failed, preview_key, readback_mismatch,
    validate_config, validate_request,
};
pub use types::*;

pub struct WhatIfPreviewService<C = SystemClock>
where
    C: Clock + Copy,
{
    config: WhatIfPreviewServiceConfig,
    clock: C,
    vault: AsterVault<C>,
}

impl WhatIfPreviewService<SystemClock> {
    pub fn open(config: WhatIfPreviewServiceConfig) -> Result<Self, EngineError> {
        Self::open_with_clock(config, SystemClock)
    }
}

impl<C> WhatIfPreviewService<C>
where
    C: Clock + Copy,
{
    pub fn open_with_clock(
        config: WhatIfPreviewServiceConfig,
        clock: C,
    ) -> Result<Self, EngineError> {
        validate_config(&config)?;
        fs::create_dir_all(&config.vault_dir).map_err(|error| {
            persistence_failed(
                "oracle.what_if.vault.create_dir",
                format!("create {}: {error}", config.vault_dir.display()),
            )
        })?;
        let vault_id = parse_vault_id(&config.vault_id)?;
        let vault = AsterVault::new_durable_with_clock(
            &config.vault_dir,
            vault_id,
            config.vault_salt.clone(),
            VaultOptions::default(),
            clock,
        )
        .map_err(|error| calyx_failed("oracle.what_if.vault.open", error))?;
        Ok(Self {
            config,
            clock,
            vault,
        })
    }

    pub fn vault(&self) -> &AsterVault<C> {
        &self.vault
    }

    pub fn preview_change(
        &mut self,
        request: WhatIfPreviewRequest,
    ) -> Result<WhatIfPreviewReport, EngineError> {
        validate_request(&request)?;
        let domain = DomainId::from(request.domain_id.clone());
        let action = Action {
            action_id: request.action_id.clone(),
            panel: request.panel.clone(),
            guard: None,
        };
        let prediction = match oracle_predict(&self.vault, &action, domain.clone(), &self.clock) {
            Ok(prediction) => prediction,
            Err(error) => {
                self.persist_rejection(&request, &error)?;
                return Err(oracle_to_engine_error(
                    &request.domain_id,
                    &request.action_id,
                    error,
                ));
            }
        };
        let root = Consequence {
            action_or_event: request.action_id.clone(),
            domain,
            outcome: prediction.outcome.clone(),
            confidence: prediction.confidence,
            hop: 0,
            provenance: prediction.provenance.clone(),
        };
        let tree = match build_tree(&self.vault, root, &self.clock) {
            Ok(tree) => tree,
            Err(error) => {
                self.persist_rejection(&request, &error)?;
                return Err(oracle_to_engine_error(
                    &request.domain_id,
                    &request.action_id,
                    error,
                ));
            }
        };
        let selected_terminal = request
            .desired_outcome
            .as_ref()
            .and_then(|desired| select(&tree, desired).cloned());
        let diagnostics = tree_diagnostics(&tree);
        let record = WhatIfPreviewRecord {
            schema: PREVIEW_SCHEMA.to_string(),
            domain_id: request.domain_id.clone(),
            action_id: request.action_id.clone(),
            computed_at_ms: self.clock.now(),
            outcome: WhatIfPreviewOutcome::Previewed(Box::new(WhatIfPreviewResult {
                prediction,
                tree,
                selected_terminal,
                diagnostics,
            })),
        };
        self.persist_record(&record)?;
        let source_truth = self.read_source_truth(&request.domain_id, &request.action_id)?;
        Ok(WhatIfPreviewReport {
            record,
            source_truth,
        })
    }

    pub fn read_source_truth(
        &self,
        domain_id: &str,
        action_id: &str,
    ) -> Result<WhatIfPreviewSourceTruth, EngineError> {
        let key = preview_key(domain_id, action_id);
        let preview_bytes = self
            .vault
            .read_cf_at(self.vault.snapshot(), ColumnFamily::Online, &key)
            .map_err(|error| calyx_failed("oracle.what_if.online.read", error))?;
        let preview_record = preview_bytes
            .as_ref()
            .map(|bytes| {
                serde_json::from_slice(bytes).map_err(|error| {
                    persistence_failed("oracle.what_if.online.decode", format!("{error}"))
                })
            })
            .transpose()?;
        let preview_ledger_rows = self.preview_ledger_rows(domain_id, action_id)?;
        if let Some(bytes) = &preview_bytes {
            let expected = hash_hex(bytes);
            let has_ledger = preview_ledger_rows
                .iter()
                .any(|row| row.payload_json["preview_row_hash"] == expected);
            if !has_ledger {
                return Err(readback_mismatch(
                    "what_if_preview_ledger_hash",
                    expected,
                    "<missing>",
                ));
            }
        }
        Ok(WhatIfPreviewSourceTruth {
            source_of_truth:
                "Aster Base CF, Recurrence CF, Assay CF, Online CF preview row, and Ledger CF"
                    .to_string(),
            vault_dir: self.config.vault_dir.display().to_string(),
            domain_id: domain_id.to_string(),
            action_id: action_id.to_string(),
            preview_key_hex: hex_bytes(&key),
            preview_row_hash: preview_bytes.as_ref().map(|bytes| hash_hex(bytes)),
            preview_record,
            base_rows: raw_cf_rows(&self.vault, ColumnFamily::Base)?,
            recurrence_rows: raw_cf_rows(&self.vault, ColumnFamily::Recurrence)?,
            assay_rows: raw_cf_rows(&self.vault, ColumnFamily::Assay)?,
            preview_ledger_rows,
            oracle_ledger_rows: self.oracle_ledger_rows()?,
        })
    }

    fn persist_rejection(
        &self,
        request: &WhatIfPreviewRequest,
        error: &OracleError,
    ) -> Result<(), EngineError> {
        let record = WhatIfPreviewRecord {
            schema: PREVIEW_SCHEMA.to_string(),
            domain_id: request.domain_id.clone(),
            action_id: request.action_id.clone(),
            computed_at_ms: self.clock.now(),
            outcome: WhatIfPreviewOutcome::Rejected(WhatIfPreviewRejection {
                reason: map_oracle_rejection(error),
                oracle_error_code: error.code().to_string(),
                detail: error.to_string(),
                bound: oracle_bound(error),
            }),
        };
        self.persist_record(&record)
    }

    fn persist_record(&self, record: &WhatIfPreviewRecord) -> Result<(), EngineError> {
        let bytes = serde_json::to_vec_pretty(record).map_err(|error| {
            persistence_failed("oracle.what_if.record.serialize", format!("{error}"))
        })?;
        let row_hash = hash_hex(&bytes);
        let key = preview_key(&record.domain_id, &record.action_id);
        let payload = serde_json::to_vec(&json!({
            "tag": "litd_oracle_what_if_preview_v1",
            "domain_id": record.domain_id,
            "action_id": record.action_id,
            "preview_key_hash": hash_hex(&key),
            "preview_row_hash": row_hash,
            "outcome": match &record.outcome {
                WhatIfPreviewOutcome::Previewed(_) => "previewed",
                WhatIfPreviewOutcome::Rejected(_) => "rejected",
            },
        }))
        .map_err(|error| {
            persistence_failed(
                "oracle.what_if.ledger.payload.serialize",
                format!("{error}"),
            )
        })?;
        self.vault
            .write_cf_batch_with_ledger_entry(
                [(ColumnFamily::Online, key, bytes)],
                EntryKind::Answer,
                SubjectId::Query(record.action_id.as_bytes().to_vec()),
                payload,
                ActorId::Service(self.config.actor.clone()),
            )
            .map_err(|error| calyx_failed("oracle.what_if.commit", error))?;
        self.vault
            .flush()
            .map_err(|error| calyx_failed("oracle.what_if.flush", error))
    }

    fn preview_ledger_rows(
        &self,
        domain_id: &str,
        action_id: &str,
    ) -> Result<Vec<PreviewLedgerRowReadback>, EngineError> {
        self.ledger_rows_matching(|payload| {
            payload["tag"] == "litd_oracle_what_if_preview_v1"
                && payload["domain_id"] == domain_id
                && payload["action_id"] == action_id
        })
    }

    fn oracle_ledger_rows(&self) -> Result<Vec<PreviewLedgerRowReadback>, EngineError> {
        self.ledger_rows_matching(|payload| {
            payload["tag"]
                .as_str()
                .is_some_and(|tag| tag.starts_with("oracle_"))
        })
    }

    fn ledger_rows_matching(
        &self,
        mut keep: impl FnMut(&serde_json::Value) -> bool,
    ) -> Result<Vec<PreviewLedgerRowReadback>, EngineError> {
        let rows = self
            .vault
            .scan_cf_at(self.vault.snapshot(), ColumnFamily::Ledger)
            .map_err(|error| calyx_failed("oracle.what_if.ledger.scan", error))?;
        let mut out = Vec::new();
        for (key, bytes) in rows {
            let entry =
                decode_ledger(&bytes).map_err(|error| EngineError::OracleOperationFailed {
                    op: "oracle.what_if.ledger.decode".to_string(),
                    source: Box::new(error),
                })?;
            let payload_json: serde_json::Value =
                serde_json::from_slice(&entry.payload).map_err(|error| {
                    persistence_failed("oracle.what_if.ledger.payload.decode", format!("{error}"))
                })?;
            if !keep(&payload_json) {
                continue;
            }
            let expected_key = ledger_key(entry.seq);
            if key != expected_key {
                return Err(readback_mismatch(
                    "what_if_ledger_key",
                    hex_bytes(&expected_key),
                    hex_bytes(&key),
                ));
            }
            out.push(PreviewLedgerRowReadback {
                seq: entry.seq,
                key_hex: hex_bytes(&key),
                kind: entry.kind.as_str().to_string(),
                entry_hash: hex_bytes(&entry.entry_hash),
                prev_hash: hex_bytes(&entry.prev_hash),
                payload_json,
            });
        }
        out.sort_by_key(|row| row.seq);
        Ok(out)
    }
}
