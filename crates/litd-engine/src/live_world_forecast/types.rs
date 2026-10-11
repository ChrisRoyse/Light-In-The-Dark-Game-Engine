use std::path::PathBuf;

use calyx_assay::{CusumReport, InterEventHazardReport, TrustTag};
use calyx_loom::{PeriodicFit, RecurrenceReadStats};
use calyx_oracle::TimePrediction;
use serde::{Deserialize, Serialize};

pub const DEFAULT_ENGINE_ORACLE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
pub const DEFAULT_ENGINE_ORACLE_CONFIDENCE_CEILING: f32 = 1.0;
pub const DEFAULT_ENGINE_ORACLE_MIN_HISTORY: usize = 8;
pub const DEFAULT_ENGINE_ORACLE_OVERDUE_ALPHA: f64 = 0.05;
pub const DEFAULT_ENGINE_ORACLE_CUSUM_BASELINE_GAPS: usize = 4;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineOracleServiceConfig {
    pub vault_dir: PathBuf,
    pub vault_id: String,
    pub vault_salt: Vec<u8>,
    pub actor: String,
    pub confidence_ceiling: f32,
    pub min_history: usize,
    pub overdue_alpha: f64,
    pub tz_offset_secs: i32,
    pub cusum_baseline_gaps: usize,
}

impl EngineOracleServiceConfig {
    pub fn for_vault(vault_dir: impl Into<PathBuf>) -> Self {
        Self {
            vault_dir: vault_dir.into(),
            vault_id: DEFAULT_ENGINE_ORACLE_VAULT_ID.to_string(),
            vault_salt: b"litd-engine-live-world-oracle-v1".to_vec(),
            actor: "litd-engine-oracle".to_string(),
            confidence_ceiling: DEFAULT_ENGINE_ORACLE_CONFIDENCE_CEILING,
            min_history: DEFAULT_ENGINE_ORACLE_MIN_HISTORY,
            overdue_alpha: DEFAULT_ENGINE_ORACLE_OVERDUE_ALPHA,
            tz_offset_secs: 0,
            cusum_baseline_gaps: DEFAULT_ENGINE_ORACLE_CUSUM_BASELINE_GAPS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveWorldMetricKind {
    Economy,
    Event,
    Population,
}

impl LiveWorldMetricKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Economy => "economy",
            Self::Event => "event",
            Self::Population => "population",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveWorldObservation {
    pub stream_id: String,
    pub kind: LiveWorldMetricKind,
    pub t_secs: i64,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForecastRequest {
    pub stream_id: String,
    pub kind: LiveWorldMetricKind,
    pub now_secs: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineOracleForecastReport {
    pub record: ForecastRecord,
    pub source_truth: EngineOracleSourceTruth,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineOracleSourceTruth {
    pub source_of_truth: String,
    pub vault_dir: String,
    pub stream_id: String,
    pub stream_cx_id: String,
    pub forecast_key_hex: String,
    pub forecast_row_hash: Option<String>,
    pub forecast_record: Option<ForecastRecord>,
    pub base_row_hash: Option<String>,
    pub recurrence_rows: Vec<RawCfRow>,
    pub ledger_rows: Vec<ForecastLedgerRowReadback>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawCfRow {
    pub key_hex: String,
    pub value_hash: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForecastLedgerRowReadback {
    pub seq: u64,
    pub key_hex: String,
    pub kind: String,
    pub entry_hash: String,
    pub prev_hash: String,
    pub payload_json: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForecastRecord {
    pub schema: String,
    pub stream_id: String,
    pub kind: LiveWorldMetricKind,
    pub stream_cx_id: String,
    pub computed_at_ms: u64,
    pub outcome: ForecastOutcome,
    pub read_stats: Option<RecurrenceReadStats>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ForecastOutcome {
    Predicted(Box<ForecastPrediction>),
    Rejected(ForecastRejection),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForecastPrediction {
    pub source_support: usize,
    pub oracle_time: TimePrediction,
    pub periodic_fit: PeriodicFit,
    pub trend: TrendReport,
    pub validation: RollingValidation,
    pub anomaly: ForecastAnomalyReport,
    pub predicted_value: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForecastRejection {
    pub reason: ForecastRejectionReason,
    pub support: usize,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForecastRejectionReason {
    InsufficientHistory,
    NonStationary,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrendReport {
    pub slope_per_sec: f64,
    pub intercept: f64,
    pub last_value: f64,
    pub last_t_secs: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RollingValidation {
    pub origin_count: usize,
    pub rmse: f64,
    pub max_abs_error: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForecastAnomalyReport {
    pub hazard: ForecastHazardReport,
    pub cusum: CusumReport,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForecastHazardReport {
    pub n_gaps: usize,
    pub mean_gap: f64,
    pub gap_variance: f64,
    pub coefficient_of_variation: f64,
    pub gamma_shape: Option<f64>,
    pub gamma_scale: f64,
    pub deterministic: bool,
    pub elapsed: f64,
    pub survival: f64,
    pub hazard: f64,
    pub empirical_survival: f64,
    pub expected_next: f64,
    pub overdue_threshold_secs: f64,
    pub alpha: f64,
    pub overdue: bool,
    pub trust: TrustTag,
}

impl ForecastHazardReport {
    pub fn from_assay(report: InterEventHazardReport) -> Self {
        Self {
            n_gaps: report.n_gaps,
            mean_gap: report.mean_gap,
            gap_variance: report.gap_variance,
            coefficient_of_variation: report.coefficient_of_variation,
            gamma_shape: report.gamma_shape.is_finite().then_some(report.gamma_shape),
            gamma_scale: report.gamma_scale,
            deterministic: report.deterministic,
            elapsed: report.elapsed,
            survival: report.survival,
            hazard: report.hazard,
            empirical_survival: report.empirical_survival,
            expected_next: report.expected_next,
            overdue_threshold_secs: report.overdue_threshold_secs,
            alpha: report.alpha,
            overdue: report.overdue,
            trust: report.trust,
        }
    }
}
