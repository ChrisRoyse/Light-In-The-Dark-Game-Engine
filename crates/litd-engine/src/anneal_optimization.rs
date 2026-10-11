//! Engine-facing Anneal tuning service.
//!
//! The service persists candidate configs, prepares a Calyx Anneal rollback
//! snapshot, evaluates a held-out replay through tripwires, then promotes only
//! after the ledger write succeeds. Rejected candidates restore the exact prior
//! live pointer and still write an Anneal ledger event.

use std::fs;
use std::path::PathBuf;

use calyx_anneal::{
    AnnealLedger, AnnealLedgerAction, AnnealLedgerEntry, ArtifactPtr, AsterAnnealLedgerStore,
    AsterRollbackStorage, BudgetHandle, MetricSnapshot, RollbackReadback, RollbackStore,
    ShadowExecutor, ShadowVerdict, TripwireMetric, TripwireRegistry,
};
use calyx_aster::cf::{ColumnFamily, full_content_hash};
use calyx_aster::vault::{AsterVault, VaultOptions};
use calyx_core::{Clock, SystemClock};
use calyx_ledger::{ActorId, LedgerAppender};
use serde_json::json;

use crate::EngineError;

mod support;
mod types;

use support::{
    EngineConfigAction, artifact_id, artifact_key, artifact_key_hash, config_hash, hash_hex,
    held_out_replay, hex_bytes, ledger_row_from_bytes, parse_vault_id, persistence_failed,
    ptr_hash, readback_mismatch, reason_details, rollback_json, run_calyx, validate_profile_id,
    validate_service_config, validate_tune_config, validate_tune_pair,
};
pub use types::*;

pub struct EngineAnnealService<C = SystemClock>
where
    C: Clock + Copy,
{
    config: EngineAnnealServiceConfig,
    clock: C,
    vault: AsterVault<C>,
    tripwires: TripwireRegistry,
}

impl EngineAnnealService<SystemClock> {
    pub fn open(config: EngineAnnealServiceConfig) -> Result<Self, EngineError> {
        Self::open_with_clock(config, SystemClock)
    }
}

impl<C> EngineAnnealService<C>
where
    C: Clock + Copy,
{
    pub fn open_with_clock(
        config: EngineAnnealServiceConfig,
        clock: C,
    ) -> Result<Self, EngineError> {
        validate_service_config(&config)?;
        fs::create_dir_all(&config.vault_dir).map_err(|error| {
            persistence_failed(
                "anneal.vault.create_dir",
                format!("create {}: {error}", config.vault_dir.display()),
            )
        })?;
        let vault_id = parse_vault_id(&config.vault_id)?;
        let vault = run_calyx("anneal.vault.open", || {
            AsterVault::new_durable_with_clock(
                &config.vault_dir,
                vault_id,
                config.vault_salt.clone(),
                VaultOptions::default(),
                clock,
            )
        })?;
        let tripwires = run_calyx("anneal.tripwires.load", || {
            TripwireRegistry::load_from_vault(&config.vault_dir)
        })?;
        Ok(Self {
            config,
            clock,
            vault,
            tripwires,
        })
    }

    pub fn config(&self) -> &EngineAnnealServiceConfig {
        &self.config
    }

    pub fn install_live_config(
        &mut self,
        config: &EngineTuneConfig,
    ) -> Result<EngineAnnealSourceTruth, EngineError> {
        validate_tune_config(config)?;
        let hash = self.persist_config(config)?;
        let key = artifact_key(config.domain, &config.profile_id);
        let rollback = run_calyx("anneal.rollback.open", || {
            RollbackStore::open(
                &self.clock,
                self.config.rollback_seed,
                AsterRollbackStorage::new(&self.vault),
            )
        })?;
        run_calyx("anneal.rollback.install_live_ptr", || {
            rollback.install_live_ptr(key, ArtifactPtr::ConfigCacheKeyHash(hash))
        })?;
        run_calyx("anneal.vault.flush", || self.vault.flush())?;
        self.read_source_truth(config.domain, &config.profile_id)
    }

    pub fn run_tuning_pass(
        &mut self,
        incumbent: &EngineTuneConfig,
        candidate: &EngineTuneConfig,
    ) -> Result<EngineAnnealTuningReport, EngineError> {
        validate_tune_pair(incumbent, candidate)?;
        self.assert_live_matches(incumbent)?;
        let key = artifact_key(incumbent.domain, &incumbent.profile_id);
        let candidate_hash = self.persist_config(candidate)?;
        let candidate_ptr = ArtifactPtr::ConfigCacheKeyHash(candidate_hash);
        let description = format!(
            "engine anneal {} profile {}",
            incumbent.domain.as_str(),
            incumbent.profile_id
        );

        let rollback = run_calyx("anneal.rollback.open", || {
            RollbackStore::open(
                &self.clock,
                self.config.rollback_seed,
                AsterRollbackStorage::new(&self.vault),
            )
        })?;
        let change_id = run_calyx("anneal.rollback.prepare", || {
            rollback.prepare_with_description(key.clone(), candidate_ptr, description.clone())
        })?;
        let prepared = run_calyx("anneal.rollback.readback.prepare", || {
            rollback.readback(change_id)
        })?;
        drop(rollback);

        let verdict = self.shadow_verdict(candidate, incumbent);
        let rollback = run_calyx("anneal.rollback.reopen", || {
            RollbackStore::open(
                &self.clock,
                self.config.rollback_seed,
                AsterRollbackStorage::new(&self.vault),
            )
        })?;
        let (outcome, metrics, final_readback) = match verdict {
            ShadowVerdict::Promote { metrics } => {
                if let Some(reason) = self.promotion_gate_reject(&metrics) {
                    run_calyx("anneal.rollback.rollback.gate", || {
                        rollback.rollback(change_id)
                    })?;
                    let readback = run_calyx("anneal.rollback.readback.gate", || {
                        rollback.readback(change_id)
                    })?;
                    self.write_decision_ledger(
                        &readback,
                        AnnealLedgerAction::AutotuneAbandoned,
                        metrics.clone(),
                        description,
                        Some(reason_details(&reason)),
                    )?;
                    (EngineAnnealOutcome::Reverted { reason }, metrics, readback)
                } else {
                    self.write_decision_ledger(
                        &prepared,
                        AnnealLedgerAction::AutotunePromote,
                        metrics.clone(),
                        description,
                        Some(json!({"promotion_gate": "passed"})),
                    )?;
                    run_calyx("anneal.rollback.promote", || rollback.promote(change_id))?;
                    let readback = run_calyx("anneal.rollback.readback.promote", || {
                        rollback.readback(change_id)
                    })?;
                    (EngineAnnealOutcome::Promoted, metrics, readback)
                }
            }
            ShadowVerdict::Revert { reason, metrics } => {
                run_calyx("anneal.rollback.rollback.shadow", || {
                    rollback.rollback(change_id)
                })?;
                let readback = run_calyx("anneal.rollback.readback.shadow", || {
                    rollback.readback(change_id)
                })?;
                let reason = EngineAnnealRevertReason::Shadow {
                    shadow_reason: reason,
                };
                self.write_decision_ledger(
                    &readback,
                    AnnealLedgerAction::AutotuneAbandoned,
                    metrics.clone(),
                    description,
                    Some(reason_details(&reason)),
                )?;
                (EngineAnnealOutcome::Reverted { reason }, metrics, readback)
            }
        };

        run_calyx("anneal.vault.flush", || self.vault.flush())?;
        let source_truth = self.read_source_truth(incumbent.domain, &incumbent.profile_id)?;
        Ok(EngineAnnealTuningReport {
            change_id: change_id.0,
            domain: incumbent.domain,
            profile_id: incumbent.profile_id.clone(),
            prior_config_hash: hash_hex(&config_hash(incumbent)?),
            candidate_config_hash: hash_hex(&candidate_hash),
            outcome,
            metrics,
            rollback_readback: rollback_json(&final_readback),
            source_truth,
        })
    }

    pub fn read_source_truth(
        &self,
        domain: EngineTuneDomain,
        profile_id: &str,
    ) -> Result<EngineAnnealSourceTruth, EngineError> {
        validate_profile_id(profile_id)?;
        let key = artifact_key(domain, profile_id);
        let rollback = run_calyx("anneal.rollback.open.readback", || {
            RollbackStore::open(
                &self.clock,
                self.config.rollback_seed,
                AsterRollbackStorage::new(&self.vault),
            )
        })?;
        let live_ptr = run_calyx("anneal.rollback.live_ptr", || rollback.live_ptr(&key))?;
        let live_hash = live_ptr.as_ref().and_then(ptr_hash);
        let live_config = match live_hash {
            Some(hash) => Some(self.read_config_by_hash(&hash)?),
            None => None,
        };
        Ok(EngineAnnealSourceTruth {
            source_of_truth:
                "Aster AnnealRollback CF, Aster Ledger CF, and .anneal/engine-configs JSON files"
                    .to_string(),
            vault_dir: self.config.vault_dir.display().to_string(),
            target_key_hash: hash_hex(&artifact_key_hash(domain, profile_id)),
            live_ptr,
            live_config_hash: live_hash.map(|hash| hash_hex(&hash)),
            live_config,
            rollback_rows: self.raw_cf_rows(ColumnFamily::AnnealRollback)?,
            ledger_rows: self.ledger_rows()?,
            config_files: self.config_files()?,
        })
    }

    pub fn set_tripwire(
        &mut self,
        metric: TripwireMetric,
        bound: f64,
        hysteresis: f64,
    ) -> Result<(), EngineError> {
        run_calyx("anneal.tripwires.set", || {
            self.tripwires.set_tripwire(metric, bound, hysteresis)
        })
    }

    fn shadow_verdict(
        &mut self,
        candidate: &EngineTuneConfig,
        incumbent: &EngineTuneConfig,
    ) -> ShadowVerdict {
        let replay = held_out_replay(self.config.replay_query_count, self.config.replay_seed);
        let candidate_action = EngineConfigAction::new(candidate.metrics);
        let incumbent_action = EngineConfigAction::new(incumbent.metrics);
        let budget = BudgetHandle::new(self.config.budget_ticks);
        let mut executor = ShadowExecutor::new(self.tripwires.clone(), replay, budget, &self.clock);
        let verdict = executor.run_shadow(&candidate_action, &incumbent_action);
        self.tripwires = executor.registry;
        verdict
    }

    fn promotion_gate_reject(&self, metrics: &MetricSnapshot) -> Option<EngineAnnealRevertReason> {
        let comparison = metrics
            .metrics
            .iter()
            .find(|metric| metric.metric == TripwireMetric::SearchP99)?;
        let required =
            comparison.incumbent_value * (1.0 - self.config.min_search_p99_gain_fraction);
        if comparison.candidate_value <= required + f64::EPSILON {
            return None;
        }
        Some(EngineAnnealRevertReason::PromotionGate {
            metric: TripwireMetric::SearchP99,
            required_candidate_at_or_below: required,
            actual_candidate: comparison.candidate_value,
            incumbent: comparison.incumbent_value,
            min_gain_fraction: self.config.min_search_p99_gain_fraction,
        })
    }

    fn write_decision_ledger(
        &self,
        readback: &RollbackReadback,
        action: AnnealLedgerAction,
        metrics: MetricSnapshot,
        description: String,
        details: Option<serde_json::Value>,
    ) -> Result<(), EngineError> {
        let mut ledger = self.open_ledger()?;
        let entry = AnnealLedgerEntry {
            action,
            change_id: readback.snapshot.change_id,
            artifact_id: artifact_id(&readback.snapshot.key),
            // #1464: never write a zeroed hash into the tamper-evident ledger.
            // `ptr_hash` is total today, but a future ArtifactPtr variant that
            // can't be hashed must fail loud rather than silently corrupt audit
            // integrity with all-zero pointer hashes.
            prior_ptr_hash: ptr_hash(&readback.snapshot.prior_ptr).ok_or_else(|| {
                persistence_failed(
                    "anneal.ledger.prior_ptr_hash",
                    "artifact pointer produced no hash; refusing to write a zeroed ledger hash",
                )
            })?,
            candidate_ptr_hash: ptr_hash(&readback.snapshot.candidate_ptr).ok_or_else(|| {
                persistence_failed(
                    "anneal.ledger.candidate_ptr_hash",
                    "artifact pointer produced no hash; refusing to write a zeroed ledger hash",
                )
            })?,
            metrics,
            ts: readback.snapshot.ts,
            description,
            fault: None,
            proposal: None,
            details,
            prev_hash: None,
        };
        run_calyx("anneal.ledger.write", || ledger.write(entry).map(|_| ()))
    }

    fn open_ledger(&self) -> Result<AnnealLedger<AsterAnnealLedgerStore<'_, C>, C>, EngineError> {
        let store = AsterAnnealLedgerStore::new(&self.vault);
        let appender = run_calyx("anneal.ledger.open", || {
            LedgerAppender::open(store, self.clock)
        })?;
        run_calyx("anneal.ledger.actor", || {
            AnnealLedger::new(appender, ActorId::Service(self.config.actor.clone()))
        })
    }

    fn assert_live_matches(&self, incumbent: &EngineTuneConfig) -> Result<(), EngineError> {
        let state = self.read_source_truth(incumbent.domain, &incumbent.profile_id)?;
        let Some(actual) = state.live_config_hash else {
            return Err(readback_mismatch(
                "live_config_hash",
                hash_hex(&config_hash(incumbent)?),
                "<missing>",
            ));
        };
        let expected = hash_hex(&config_hash(incumbent)?);
        if actual != expected {
            return Err(readback_mismatch("live_config_hash", expected, actual));
        }
        Ok(())
    }

    fn persist_config(&self, config: &EngineTuneConfig) -> Result<[u8; 32], EngineError> {
        let hash = config_hash(config)?;
        let path = self.config_path_for_hash(&hash);
        let bytes = serde_json::to_vec_pretty(config).map_err(|error| {
            persistence_failed(
                "anneal.config.serialize",
                format!("serialize config: {error}"),
            )
        })?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                persistence_failed(
                    "anneal.config.create_dir",
                    format!("create {}: {error}", parent.display()),
                )
            })?;
        }
        fs::write(&path, bytes).map_err(|error| {
            persistence_failed(
                "anneal.config.write",
                format!("write {}: {error}", path.display()),
            )
        })?;
        Ok(hash)
    }

    fn read_config_by_hash(&self, hash: &[u8; 32]) -> Result<EngineTuneConfig, EngineError> {
        let path = self.config_path_for_hash(hash);
        let bytes = fs::read(&path).map_err(|error| {
            persistence_failed(
                "anneal.config.read",
                format!("read {}: {error}", path.display()),
            )
        })?;
        let actual = full_content_hash([bytes.as_slice()]);
        if &actual != hash {
            return Err(readback_mismatch(
                "config_hash",
                hash_hex(hash),
                hash_hex(&actual),
            ));
        }
        serde_json::from_slice(&bytes).map_err(|error| {
            persistence_failed(
                "anneal.config.decode",
                format!("decode {}: {error}", path.display()),
            )
        })
    }

    fn config_files(&self) -> Result<Vec<EngineConfigFileReadback>, EngineError> {
        let dir = self.config_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(&dir).map_err(|error| {
            persistence_failed(
                "anneal.config.scan",
                format!("scan {}: {error}", dir.display()),
            )
        })? {
            let path = entry
                .map_err(|error| persistence_failed("anneal.config.scan", error.to_string()))?
                .path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path).map_err(|error| {
                persistence_failed(
                    "anneal.config.read",
                    format!("read {}: {error}", path.display()),
                )
            })?;
            let config = serde_json::from_slice(&bytes).map_err(|error| {
                persistence_failed(
                    "anneal.config.decode",
                    format!("decode {}: {error}", path.display()),
                )
            })?;
            files.push(EngineConfigFileReadback {
                path: path.display().to_string(),
                hash: hash_hex(&full_content_hash([bytes.as_slice()])),
                bytes: bytes.len() as u64,
                config,
            });
        }
        files.sort_by(|left, right| left.hash.cmp(&right.hash));
        Ok(files)
    }

    fn raw_cf_rows(&self, cf: ColumnFamily) -> Result<Vec<RawCfRow>, EngineError> {
        let rows = run_calyx("anneal.vault.scan_cf", || {
            self.vault.scan_cf_at(self.vault.latest_seq(), cf)
        })?;
        Ok(rows
            .into_iter()
            .map(|(key, value)| RawCfRow {
                key_hex: hex_bytes(&key),
                value_hex: hex_bytes(&value),
            })
            .collect())
    }

    fn ledger_rows(&self) -> Result<Vec<AnnealLedgerRowReadback>, EngineError> {
        let rows = run_calyx("anneal.ledger.scan_cf", || {
            self.vault
                .scan_cf_at(self.vault.latest_seq(), ColumnFamily::Ledger)
        })?;
        rows.into_iter()
            .map(|(key, bytes)| ledger_row_from_bytes(key, bytes))
            .collect()
    }

    fn config_dir(&self) -> PathBuf {
        self.config.vault_dir.join(".anneal").join("engine-configs")
    }

    fn config_path_for_hash(&self, hash: &[u8; 32]) -> PathBuf {
        self.config_dir().join(format!("{}.json", hash_hex(hash)))
    }
}
