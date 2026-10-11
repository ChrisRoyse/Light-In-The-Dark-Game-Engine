use std::path::PathBuf;

use calyx_anneal::{ArtifactPtr, MetricSnapshot, ShadowRevertReason, TripwireMetric};
use serde::{Deserialize, Serialize};

pub const DEFAULT_ENGINE_ANNEAL_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
pub const DEFAULT_ENGINE_ANNEAL_REPLAY_QUERIES: usize = 8;
pub const DEFAULT_ENGINE_ANNEAL_BUDGET_TICKS: usize = 64;
pub const DEFAULT_ENGINE_ANNEAL_MIN_SEARCH_GAIN_FRACTION: f64 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineTuneDomain {
    Index,
    Forge,
    Render,
    Ward,
}

impl EngineTuneDomain {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Forge => "forge",
            Self::Render => "render",
            Self::Ward => "ward",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineTuneConfig {
    pub domain: EngineTuneDomain,
    pub profile_id: String,
    pub params: EngineTuneParams,
    pub metrics: EngineTuneMetrics,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineTuneParams {
    pub index_ef_search: u32,
    pub index_quant_bits: u8,
    pub forge_tile_m: u32,
    pub forge_tile_n: u32,
    pub render_max_draws: u32,
    pub render_shadow_cascades: u8,
    pub ward_tau_micros: u32,
}

impl EngineTuneParams {
    pub const fn balanced() -> Self {
        Self {
            index_ef_search: 64,
            index_quant_bits: 8,
            forge_tile_m: 16,
            forge_tile_n: 16,
            render_max_draws: 4096,
            render_shadow_cascades: 4,
            ward_tau_micros: 18_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineTuneMetrics {
    pub recall_at_k: f64,
    pub guard_far: f64,
    pub guard_frr: f64,
    pub search_p99_ms: f64,
    pub ingest_p95_ms: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineAnnealServiceConfig {
    pub vault_dir: PathBuf,
    pub vault_id: String,
    pub vault_salt: Vec<u8>,
    pub actor: String,
    pub rollback_seed: u64,
    pub replay_seed: u64,
    pub replay_query_count: usize,
    pub budget_ticks: usize,
    pub min_search_p99_gain_fraction: f64,
}

impl EngineAnnealServiceConfig {
    pub fn for_vault(vault_dir: impl Into<PathBuf>) -> Self {
        Self {
            vault_dir: vault_dir.into(),
            vault_id: DEFAULT_ENGINE_ANNEAL_VAULT_ID.to_string(),
            vault_salt: b"litd-engine-anneal-optimization-v1".to_vec(),
            actor: "litd-engine-anneal".to_string(),
            rollback_seed: 1_199,
            replay_seed: 1_199,
            replay_query_count: DEFAULT_ENGINE_ANNEAL_REPLAY_QUERIES,
            budget_ticks: DEFAULT_ENGINE_ANNEAL_BUDGET_TICKS,
            min_search_p99_gain_fraction: DEFAULT_ENGINE_ANNEAL_MIN_SEARCH_GAIN_FRACTION,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineAnnealTuningReport {
    pub change_id: u64,
    pub domain: EngineTuneDomain,
    pub profile_id: String,
    pub prior_config_hash: String,
    pub candidate_config_hash: String,
    pub outcome: EngineAnnealOutcome,
    pub metrics: MetricSnapshot,
    pub rollback_readback: EngineRollbackReadback,
    pub source_truth: EngineAnnealSourceTruth,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum EngineAnnealOutcome {
    Promoted,
    Reverted { reason: EngineAnnealRevertReason },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum EngineAnnealRevertReason {
    Shadow {
        shadow_reason: ShadowRevertReason,
    },
    PromotionGate {
        metric: TripwireMetric,
        required_candidate_at_or_below: f64,
        actual_candidate: f64,
        incumbent: f64,
        min_gain_fraction: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineRollbackReadback {
    pub snapshot: calyx_anneal::ArtifactSnapshot,
    pub live_ptr: ArtifactPtr,
    pub snapshot_key_hex: String,
    pub snapshot_bytes_hex: String,
    pub live_key_hex: String,
    pub live_bytes_hex: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineAnnealSourceTruth {
    pub source_of_truth: String,
    pub vault_dir: String,
    pub target_key_hash: String,
    pub live_ptr: Option<ArtifactPtr>,
    pub live_config_hash: Option<String>,
    pub live_config: Option<EngineTuneConfig>,
    pub rollback_rows: Vec<RawCfRow>,
    pub ledger_rows: Vec<AnnealLedgerRowReadback>,
    pub config_files: Vec<EngineConfigFileReadback>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawCfRow {
    pub key_hex: String,
    pub value_hex: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnnealLedgerRowReadback {
    pub seq: u64,
    pub key_hex: String,
    pub kind: String,
    pub entry_hash: String,
    pub prev_hash: String,
    pub payload_json: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineConfigFileReadback {
    pub path: String,
    pub hash: String,
    pub bytes: u64,
    pub config: EngineTuneConfig,
}
