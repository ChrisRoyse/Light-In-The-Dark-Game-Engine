use std::path::PathBuf;

use calyx_core::{AnchorValue, LedgerRef, Panel};
use calyx_oracle::{ConsequenceTree, Prediction, SufficiencyBound};
use serde::{Deserialize, Serialize};

pub const DEFAULT_WHAT_IF_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WhatIfPreviewServiceConfig {
    pub vault_dir: PathBuf,
    pub vault_id: String,
    pub vault_salt: Vec<u8>,
    pub actor: String,
}

impl WhatIfPreviewServiceConfig {
    pub fn for_vault(vault_dir: impl Into<PathBuf>) -> Self {
        Self {
            vault_dir: vault_dir.into(),
            vault_id: DEFAULT_WHAT_IF_VAULT_ID.to_string(),
            vault_salt: b"litd-engine-what-if-preview-v1".to_vec(),
            actor: "litd-engine-what-if-oracle".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WhatIfPreviewRequest {
    pub domain_id: String,
    pub action_id: String,
    pub panel: Panel,
    pub desired_outcome: Option<AnchorValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WhatIfPreviewReport {
    pub record: WhatIfPreviewRecord,
    pub source_truth: WhatIfPreviewSourceTruth,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WhatIfPreviewSourceTruth {
    pub source_of_truth: String,
    pub vault_dir: String,
    pub domain_id: String,
    pub action_id: String,
    pub preview_key_hex: String,
    pub preview_row_hash: Option<String>,
    pub preview_record: Option<WhatIfPreviewRecord>,
    pub base_rows: Vec<PreviewRawCfRow>,
    pub recurrence_rows: Vec<PreviewRawCfRow>,
    pub assay_rows: Vec<PreviewRawCfRow>,
    pub preview_ledger_rows: Vec<PreviewLedgerRowReadback>,
    pub oracle_ledger_rows: Vec<PreviewLedgerRowReadback>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewRawCfRow {
    pub key_hex: String,
    pub value_hash: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreviewLedgerRowReadback {
    pub seq: u64,
    pub key_hex: String,
    pub kind: String,
    pub entry_hash: String,
    pub prev_hash: String,
    pub payload_json: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WhatIfPreviewRecord {
    pub schema: String,
    pub domain_id: String,
    pub action_id: String,
    pub computed_at_ms: u64,
    pub outcome: WhatIfPreviewOutcome,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum WhatIfPreviewOutcome {
    Previewed(Box<WhatIfPreviewResult>),
    Rejected(WhatIfPreviewRejection),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WhatIfPreviewResult {
    pub prediction: Prediction,
    pub tree: ConsequenceTree,
    pub selected_terminal: Option<ConsequenceTree>,
    pub diagnostics: WhatIfTreeDiagnostics,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WhatIfPreviewRejection {
    pub reason: WhatIfPreviewRejectionReason,
    pub oracle_error_code: String,
    pub detail: String,
    pub bound: Option<SufficiencyBound>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhatIfPreviewRejectionReason {
    Insufficient,
    NoRecurrence,
    DomainNotFound,
    LedgerWriteFailure,
    OracleOperationFailed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhatIfTreeDiagnostics {
    pub nodes: u64,
    pub leaves: u64,
    pub max_hop: u8,
    pub repeated_action_paths: u64,
    pub cycle_guard_satisfied: bool,
    pub low_confidence_pruned: bool,
    pub provisional_nodes: u64,
    pub grounded_nodes: u64,
    pub prediction_ledger: LedgerRef,
}
