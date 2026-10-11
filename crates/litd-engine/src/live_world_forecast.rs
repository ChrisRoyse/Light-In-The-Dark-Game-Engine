//! Engine-facing Oracle forecasting for live-world economy, event, and population streams.
//!
//! The service writes real Aster Base/Recurrence rows for observations, then
//! persists the latest forecast in the Online CF with an append-only Ledger
//! entry in the same commit. Readback reopens the source of truth instead of
//! trusting returned values.

use std::collections::BTreeMap;
use std::fs;

use calyx_assay::{CusumConfig, inter_event_hazard_from_series, recurrence_rate_cusum_from_series};
use calyx_aster::cf::{ColumnFamily, base_key, ledger_key, recurrence_prefix_range};
use calyx_aster::dedup::EpochSecs;
use calyx_aster::vault::{AsterVault, VaultOptions};
use calyx_core::{
    Clock, Constellation, CxFlags, CxId, InputRef, LedgerRef, Modality, SystemClock, VaultStore,
};
use calyx_ledger::{ActorId, EntryKind, SubjectId, decode as decode_ledger};
use calyx_loom::{OccurrenceContext, SeriesStore};
use calyx_oracle::predict_next_occurrence_from_series_with_tz_offset;
use serde_json::json;

use crate::EngineError;

mod analysis;
mod support;
mod types;

use analysis::{
    ObservationContext, raw_cf_range_rows, rolling_validation, stream_input, stream_panel_version,
    trend_report,
};
use support::{
    FORECAST_SCHEMA, forecast_key, hash_hex, hex_bytes, insufficient_history, non_stationary,
    parse_vault_id, persistence_failed, readback_mismatch, run_calyx, validate_observation,
    validate_service_config, validate_stream_id,
};
pub use types::*;

pub struct EngineOracleService<C = SystemClock>
where
    C: Clock + Copy,
{
    config: EngineOracleServiceConfig,
    clock: C,
    vault: AsterVault<C>,
}

impl EngineOracleService<SystemClock> {
    pub fn open(config: EngineOracleServiceConfig) -> Result<Self, EngineError> {
        Self::open_with_clock(config, SystemClock)
    }
}

impl<C> EngineOracleService<C>
where
    C: Clock + Copy,
{
    pub fn open_with_clock(
        config: EngineOracleServiceConfig,
        clock: C,
    ) -> Result<Self, EngineError> {
        validate_service_config(&config)?;
        fs::create_dir_all(&config.vault_dir).map_err(|error| {
            persistence_failed(
                "oracle.vault.create_dir",
                format!("create {}: {error}", config.vault_dir.display()),
            )
        })?;
        let vault_id = parse_vault_id(&config.vault_id)?;
        let vault = run_calyx("oracle.vault.open", || {
            AsterVault::new_durable_with_clock(
                &config.vault_dir,
                vault_id,
                config.vault_salt.clone(),
                VaultOptions::default(),
                clock,
            )
        })?;
        Ok(Self {
            config,
            clock,
            vault,
        })
    }

    pub fn ingest_observation(
        &mut self,
        observation: LiveWorldObservation,
    ) -> Result<EngineOracleSourceTruth, EngineError> {
        validate_observation(&observation)?;
        let cx_id = self.ensure_stream_base(&observation.stream_id, observation.kind)?;
        let context = serde_json::to_vec(&ObservationContext {
            value: observation.value,
            kind: observation.kind,
        })
        .map_err(|error| {
            persistence_failed("oracle.observation.serialize", format!("context: {error}"))
        })?;
        let store = SeriesStore::new(&self.vault);
        run_calyx("oracle.recurrence.append", || {
            store
                .append_occurrence(
                    cx_id,
                    EpochSecs(observation.t_secs),
                    OccurrenceContext::new(context)?,
                )
                .map(|_| ())
        })?;
        run_calyx("oracle.vault.flush", || self.vault.flush())?;
        self.read_source_truth(&observation.stream_id, observation.kind)
    }

    pub fn forecast_stream(
        &mut self,
        request: ForecastRequest,
    ) -> Result<EngineOracleForecastReport, EngineError> {
        validate_stream_id(&request.stream_id)?;
        if request.now_secs < 0 {
            return Err(support::invalid_config(
                "now_secs",
                format!("timestamp must be non-negative, got {}", request.now_secs),
            ));
        }
        let cx_id = self.stream_cx_id(&request.stream_id, request.kind);
        let store = SeriesStore::new(&self.vault);
        let read = run_calyx("oracle.loom.recurrence_readback", || {
            store.recurrence_series_with_tz_offset(cx_id, self.config.tz_offset_secs)
        })?;
        let support = read.series.occurrences.len();
        if support < self.config.min_history {
            let detail = format!("support={support} min={}", self.config.min_history);
            self.persist_rejection(
                &request,
                cx_id,
                support,
                ForecastRejectionReason::InsufficientHistory,
                detail,
                Some(read.read_stats),
            )?;
            return Err(insufficient_history(
                request.stream_id,
                support,
                self.config.min_history,
            ));
        }

        let cusum = run_calyx("oracle.assay.cusum", || {
            recurrence_rate_cusum_from_series(
                &read.series,
                &CusumConfig::with_baseline(self.config.cusum_baseline_gaps),
            )
        })?;
        if let Some(change) = cusum.change_point {
            let detail = format!(
                "direction={:?} occurrence_index={}",
                change.direction, change.occurrence_index
            );
            self.persist_rejection(
                &request,
                cx_id,
                support,
                ForecastRejectionReason::NonStationary,
                detail,
                Some(read.read_stats),
            )?;
            return Err(non_stationary(
                request.stream_id,
                format!("{:?}", change.direction),
                change.occurrence_index,
            ));
        }

        let time_prediction = run_calyx("oracle.time_prediction", || {
            predict_next_occurrence_from_series_with_tz_offset(
                &read.series,
                self.config.confidence_ceiling,
                self.config.tz_offset_secs,
            )
        })?;
        let trend = trend_report(&read.series)?;
        let validation = rolling_validation(&read.series)?;
        let hazard = run_calyx("oracle.assay.hazard", || {
            inter_event_hazard_from_series(
                &read.series,
                EpochSecs(request.now_secs),
                self.config.overdue_alpha,
            )
        })?;
        let predicted_value =
            trend.intercept + trend.slope_per_sec * time_prediction.t_hat.0 as f64;
        let record = ForecastRecord {
            schema: FORECAST_SCHEMA.to_string(),
            stream_id: request.stream_id.clone(),
            kind: request.kind,
            stream_cx_id: cx_id.to_string(),
            computed_at_ms: self.clock.now(),
            outcome: ForecastOutcome::Predicted(Box::new(ForecastPrediction {
                source_support: support,
                oracle_time: time_prediction,
                periodic_fit: read.periodic_fit,
                trend,
                validation,
                anomaly: ForecastAnomalyReport {
                    hazard: ForecastHazardReport::from_assay(hazard),
                    cusum,
                },
                predicted_value,
            })),
            read_stats: Some(read.read_stats),
        };
        self.persist_record(&record)?;
        let source_truth = self.read_source_truth(&request.stream_id, request.kind)?;
        Ok(EngineOracleForecastReport {
            record,
            source_truth,
        })
    }

    pub fn read_source_truth(
        &self,
        stream_id: &str,
        kind: LiveWorldMetricKind,
    ) -> Result<EngineOracleSourceTruth, EngineError> {
        validate_stream_id(stream_id)?;
        let cx_id = self.stream_cx_id(stream_id, kind);
        let forecast_key = forecast_key(stream_id, kind);
        let forecast_bytes = run_calyx("oracle.online.read", || {
            self.vault
                .read_cf_at(self.vault.snapshot(), ColumnFamily::Online, &forecast_key)
        })?;
        let forecast_record = forecast_bytes
            .as_ref()
            .map(|bytes| {
                serde_json::from_slice(bytes).map_err(|error| {
                    persistence_failed("oracle.online.decode", format!("decode forecast: {error}"))
                })
            })
            .transpose()?;
        let base_row = run_calyx("oracle.base.read", || {
            self.vault
                .read_cf_at(self.vault.snapshot(), ColumnFamily::Base, &base_key(cx_id))
        })?;
        let recurrence_rows = raw_cf_range_rows(
            &self.vault,
            ColumnFamily::Recurrence,
            &recurrence_prefix_range(cx_id),
        )?;
        let ledger_rows = self.ledger_rows_for_stream(stream_id, kind)?;
        if let Some(bytes) = &forecast_bytes {
            let expected = hash_hex(bytes);
            let has_ledger = ledger_rows
                .iter()
                .any(|row| row.payload_json["forecast_row_hash"] == expected);
            if !has_ledger {
                return Err(readback_mismatch(
                    "forecast_ledger_hash",
                    expected,
                    "<missing>",
                ));
            }
        }
        Ok(EngineOracleSourceTruth {
            source_of_truth: "Aster Base CF, Recurrence CF, Online CF forecast row, and Ledger CF"
                .to_string(),
            vault_dir: self.config.vault_dir.display().to_string(),
            stream_id: stream_id.to_string(),
            stream_cx_id: cx_id.to_string(),
            forecast_key_hex: hex_bytes(&forecast_key),
            forecast_row_hash: forecast_bytes.as_ref().map(|bytes| hash_hex(bytes)),
            forecast_record,
            base_row_hash: base_row.as_ref().map(|bytes| hash_hex(bytes)),
            recurrence_rows,
            ledger_rows,
        })
    }

    fn persist_rejection(
        &self,
        request: &ForecastRequest,
        cx_id: CxId,
        support: usize,
        reason: ForecastRejectionReason,
        detail: String,
        read_stats: Option<calyx_aster::recurrence::RecurrenceReadStats>,
    ) -> Result<(), EngineError> {
        let record = ForecastRecord {
            schema: FORECAST_SCHEMA.to_string(),
            stream_id: request.stream_id.clone(),
            kind: request.kind,
            stream_cx_id: cx_id.to_string(),
            computed_at_ms: self.clock.now(),
            outcome: ForecastOutcome::Rejected(ForecastRejection {
                reason,
                support,
                detail,
            }),
            read_stats,
        };
        self.persist_record(&record)
    }

    fn persist_record(&self, record: &ForecastRecord) -> Result<(), EngineError> {
        let bytes = serde_json::to_vec_pretty(record).map_err(|error| {
            persistence_failed("oracle.forecast.serialize", format!("record: {error}"))
        })?;
        let row_hash = hash_hex(&bytes);
        let key = forecast_key(&record.stream_id, record.kind);
        let payload = serde_json::to_vec(&json!({
            "tag": "litd_oracle_live_world_forecast_v1",
            "stream_id": record.stream_id,
            "kind": record.kind.as_str(),
            "stream_cx_id": record.stream_cx_id,
            "forecast_key_hash": hash_hex(&key),
            "forecast_row_hash": row_hash,
            "outcome": match &record.outcome {
                ForecastOutcome::Predicted(_) => "predicted",
                ForecastOutcome::Rejected(_) => "rejected",
            },
        }))
        .map_err(|error| {
            persistence_failed(
                "oracle.ledger.payload.serialize",
                format!("payload: {error}"),
            )
        })?;
        run_calyx("oracle.forecast.commit", || {
            self.vault
                .write_cf_batch_with_ledger_entry(
                    [(ColumnFamily::Online, key, bytes)],
                    EntryKind::Answer,
                    SubjectId::Query(record.stream_cx_id.as_bytes().to_vec()),
                    payload,
                    ActorId::Service(self.config.actor.clone()),
                )
                .map(|_| ())
        })?;
        run_calyx("oracle.vault.flush", || self.vault.flush())
    }

    fn ensure_stream_base(
        &self,
        stream_id: &str,
        kind: LiveWorldMetricKind,
    ) -> Result<CxId, EngineError> {
        let cx_id = self.stream_cx_id(stream_id, kind);
        let exists = run_calyx("oracle.base.exists", || {
            self.vault
                .read_cf_at(self.vault.snapshot(), ColumnFamily::Base, &base_key(cx_id))
        })?
        .is_some();
        if exists {
            return Ok(cx_id);
        }
        let input = stream_input(stream_id, kind);
        let input_hash = calyx_aster::cf::full_content_hash([input.as_slice()]);
        let mut metadata = BTreeMap::new();
        metadata.insert("litd.oracle.stream_id".to_string(), stream_id.to_string());
        metadata.insert("litd.oracle.kind".to_string(), kind.as_str().to_string());
        let cx = Constellation {
            cx_id,
            vault_id: self.vault.vault_id(),
            panel_version: stream_panel_version(),
            created_at: self.clock.now(),
            input_ref: InputRef {
                hash: input_hash,
                pointer: None,
                redacted: false,
            },
            modality: Modality::Structured,
            slots: BTreeMap::new(),
            scalars: BTreeMap::new(),
            metadata,
            anchors: Vec::new(),
            provenance: LedgerRef {
                seq: 0,
                hash: [0; 32],
            },
            flags: CxFlags::default(),
        };
        run_calyx("oracle.base.put", || self.vault.put(cx).map(|_| cx_id))
    }

    fn stream_cx_id(&self, stream_id: &str, kind: LiveWorldMetricKind) -> CxId {
        self.vault
            .cx_id_for_input(&stream_input(stream_id, kind), stream_panel_version())
    }

    fn ledger_rows_for_stream(
        &self,
        stream_id: &str,
        kind: LiveWorldMetricKind,
    ) -> Result<Vec<ForecastLedgerRowReadback>, EngineError> {
        let rows = run_calyx("oracle.ledger.scan", || {
            self.vault
                .scan_cf_at(self.vault.snapshot(), ColumnFamily::Ledger)
        })?;
        let mut out = Vec::new();
        for (key, bytes) in rows {
            let entry =
                decode_ledger(&bytes).map_err(|error| EngineError::OracleOperationFailed {
                    op: "oracle.ledger.decode".to_string(),
                    source: Box::new(error),
                })?;
            let payload_json: serde_json::Value =
                serde_json::from_slice(&entry.payload).map_err(|error| {
                    persistence_failed("oracle.ledger.payload.decode", format!("{error}"))
                })?;
            if payload_json["tag"] != "litd_oracle_live_world_forecast_v1"
                || payload_json["stream_id"] != stream_id
                || payload_json["kind"] != kind.as_str()
            {
                continue;
            }
            let expected_key = ledger_key(entry.seq);
            if key != expected_key {
                return Err(readback_mismatch(
                    "ledger_key",
                    hex_bytes(&expected_key),
                    hex_bytes(&key),
                ));
            }
            out.push(ForecastLedgerRowReadback {
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
