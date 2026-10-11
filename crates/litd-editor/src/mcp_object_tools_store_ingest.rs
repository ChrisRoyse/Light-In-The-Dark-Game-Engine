use std::{collections::BTreeSet, sync::Arc};

use calyx_aster::{
    cf::{ColumnFamily, base_key},
    vault::AsterVault,
    vault::encode,
};
use calyx_core::{CxId, LedgerRef, Modality, Panel, SlotId, SlotShape, SlotState, SystemClock};
use calyx_loom::{
    AgreementDriftTracker, ReactiveEngine, ReactiveRowKind, ReactiveSignalSet, TriggerCondition,
    decode_audit_entry, decode_trigger_fired, reactive_row_key,
};
use calyx_registry::{VaultPanelState, code_default, materialize_panel_template};
use litd_engine::EngineError;

use crate::history::EditorDocument;

use super::mcp_object_tools_store_aster::{McpObjectAsterStore, hex};
use super::mcp_object_tools_store_records::object_records;
use super::{McpObjectReactiveNotification, store_error};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StructuredObjectIngestReadback {
    pub(super) cx_ids: Vec<String>,
    pub(super) ledger_ref: Option<LedgerRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AsterBaseRead {
    pub(super) rows: usize,
    pub(super) cx_ids: Vec<String>,
    pub(super) latest_ledger_ref: Option<LedgerRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct AsterReactiveRead {
    pub(super) audit_rows: usize,
    pub(super) fired_rows: usize,
    pub(super) notifications: Vec<McpObjectReactiveNotification>,
}

fn editor_reactive_drift_slot(panel: &Panel) -> Result<SlotId, EngineError> {
    panel
        .slots
        .iter()
        .find(|slot| {
            slot.state == SlotState::Active
                && slot.modality == Modality::Code
                && matches!(slot.shape, SlotShape::Dense(_))
                && !slot.retrieval_only
        })
        .map(|slot| slot.slot_id)
        .ok_or_else(|| {
            store_error(
                "document.reactive.slot",
                "object ingest panel has no active dense code slot for DriftDetected notifications",
            )
        })
}

impl McpObjectAsterStore {
    pub(super) fn ingest_object_constellations(
        &self,
        document: &EditorDocument,
    ) -> Result<StructuredObjectIngestReadback, EngineError> {
        let records = object_records(document)?;
        let template = code_default();
        let materialized = materialize_panel_template(&template, 1_358)
            .map_err(|error| store_error("document.structured_panel", error.to_string()))?;
        let state = VaultPanelState {
            panel: materialized.panel,
            registry: materialized.registry,
            registry_snapshot: None,
        };
        let drift_slot = editor_reactive_drift_slot(&state.panel)?;
        let vault = self.open_vault()?;
        let snapshot = vault.latest_seq();
        let mut document_ids = Vec::with_capacity(records.len());
        let mut staged_ids = Vec::new();
        let mut staged_seen = BTreeSet::new();
        let mut staged = Vec::new();
        for record in records {
            let input = record.to_input()?;
            let cx_id = vault.cx_id_for_input(&input.bytes, state.panel.version);
            let exists = vault
                .read_cf_at(snapshot, ColumnFamily::Base, &base_key(cx_id))
                .map_err(|error| store_error("document.aster.base_get", error.to_string()))?
                .is_some();
            document_ids.push(cx_id);
            if exists || !staged_seen.insert(cx_id) {
                continue;
            }
            let constellation =
                calyx_registry::measure::measure_constellation(&vault, &state, input, 1_358_000)
                    .map_err(|error| {
                        store_error("document.structured_ingest", error.to_string())
                    })?;
            if constellation
                .slots
                .values()
                .all(|vector| vector.is_absent())
            {
                return Err(store_error(
                    "document.structured_ingest",
                    "object ingest panel produced only absent slot vectors",
                ));
            }
            staged_ids.push(cx_id);
            staged.push(constellation);
        }
        if !staged.is_empty() {
            vault
                .put_batch(staged)
                .map_err(|error| store_error("document.structured_ingest", error.to_string()))?;
        }
        let read_seq = vault.latest_seq();
        let ledger_ref = self.latest_ledger_ref_for_cx_ids(&vault, read_seq, &document_ids)?;
        if let Some(ledger_ref) = ledger_ref.clone().filter(|_| !staged_ids.is_empty()) {
            self.evaluate_reactive_notifications(&vault, drift_slot, &staged_ids, ledger_ref)?;
        }
        vault
            .flush()
            .map_err(|error| store_error("document.aster.flush", error.to_string()))?;
        Ok(StructuredObjectIngestReadback {
            cx_ids: document_ids.iter().map(ToString::to_string).collect(),
            ledger_ref,
        })
    }

    fn latest_ledger_ref_for_cx_ids(
        &self,
        vault: &AsterVault<SystemClock>,
        seq: u64,
        cx_ids: &[calyx_core::CxId],
    ) -> Result<Option<LedgerRef>, EngineError> {
        let mut latest_ledger_ref: Option<LedgerRef> = None;
        for cx_id in cx_ids {
            let Some(value) = vault
                .read_cf_at(seq, ColumnFamily::Base, &base_key(*cx_id))
                .map_err(|error| store_error("document.aster.base_get", error.to_string()))?
            else {
                return Err(store_error(
                    "document.aster.base_get",
                    format!("structured object CxId {cx_id} missing from Base CF"),
                ));
            };
            let constellation = encode::decode_constellation_base(&value)
                .map_err(|error| store_error("document.aster.base_decode", error.to_string()))?;
            if latest_ledger_ref
                .as_ref()
                .map(|ledger_ref| constellation.provenance.seq > ledger_ref.seq)
                .unwrap_or(true)
            {
                latest_ledger_ref = Some(constellation.provenance);
            }
        }
        Ok(latest_ledger_ref)
    }

    fn evaluate_reactive_notifications(
        &self,
        vault: &AsterVault<SystemClock>,
        drift_slot: SlotId,
        cx_ids: &[CxId],
        ledger_ref: LedgerRef,
    ) -> Result<(), EngineError> {
        let tracker = AgreementDriftTracker::new();
        let mut engine = ReactiveEngine::new(Arc::new(SystemClock));
        engine
            .register(
                TriggerCondition::DriftDetected {
                    slot: drift_slot,
                    drift_threshold: f32::EPSILON,
                },
                Some("litd-editor-object-tools".to_string()),
            )
            .map_err(|error| store_error("document.reactive.register", error.to_string()))?;
        for cx_id in cx_ids {
            let signals = ReactiveSignalSet::new(vault).with_agreement_drift(*cx_id, &tracker);
            engine
                .evaluate_post_ingest_durable(vault, *cx_id, ledger_ref.clone(), &signals)
                .map_err(|error| store_error("document.reactive.evaluate", error.to_string()))?;
        }
        Ok(())
    }

    pub(super) fn read_base_rows_at(
        &self,
        vault: &AsterVault<SystemClock>,
        seq: u64,
    ) -> Result<AsterBaseRead, EngineError> {
        let mut rows = 0usize;
        let mut cx_ids = Vec::new();
        let mut latest_ledger_ref: Option<LedgerRef> = None;
        for (_, value) in vault
            .scan_cf_at(seq, ColumnFamily::Base)
            .map_err(|error| store_error("document.aster.base_scan", error.to_string()))?
        {
            let constellation = encode::decode_constellation_base(&value)
                .map_err(|error| store_error("document.aster.base_decode", error.to_string()))?;
            rows += 1;
            cx_ids.push(constellation.cx_id.to_string());
            if latest_ledger_ref
                .as_ref()
                .map(|ledger_ref| constellation.provenance.seq > ledger_ref.seq)
                .unwrap_or(true)
            {
                latest_ledger_ref = Some(constellation.provenance);
            }
        }
        cx_ids.sort();
        Ok(AsterBaseRead {
            rows,
            cx_ids,
            latest_ledger_ref,
        })
    }

    pub(super) fn read_reactive_rows_at(
        &self,
        vault: &AsterVault<SystemClock>,
        seq: u64,
    ) -> Result<AsterReactiveRead, EngineError> {
        let mut audit_rows = 0usize;
        let mut notifications = Vec::new();
        for (key, value) in vault
            .scan_cf_at(seq, ColumnFamily::Reactive)
            .map_err(|error| store_error("document.aster.reactive_scan", error.to_string()))?
        {
            match reactive_row_key(&key)
                .map_err(|error| store_error("document.reactive.key", error.to_string()))?
                .kind
            {
                ReactiveRowKind::Audit => {
                    decode_audit_entry(&value).map_err(|error| {
                        store_error("document.reactive.audit", error.to_string())
                    })?;
                    audit_rows += 1;
                }
                ReactiveRowKind::Fired => {
                    let event = decode_trigger_fired(&value).map_err(|error| {
                        store_error("document.reactive.fired", error.to_string())
                    })?;
                    notifications.push(McpObjectReactiveNotification {
                        trigger_id: event.trigger_id.to_string(),
                        cx_id: event.cx_id.to_string(),
                        fired_at: event.fired_at,
                        ledger_seq: event.ledger_ref.seq,
                        ledger_hash: hex(&event.ledger_ref.hash),
                        condition: serde_json::to_value(&event.condition_snapshot).map_err(
                            |error| EngineError::SerializationFailed {
                                format: "mcp-object-reactive-condition",
                                reason: format!("serialize reactive condition: {error}"),
                            },
                        )?,
                    });
                }
            }
        }
        notifications.sort_by(|left, right| {
            left.ledger_seq
                .cmp(&right.ledger_seq)
                .then_with(|| left.trigger_id.cmp(&right.trigger_id))
                .then_with(|| left.cx_id.cmp(&right.cx_id))
        });
        Ok(AsterReactiveRead {
            audit_rows,
            fired_rows: notifications.len(),
            notifications,
        })
    }
}
