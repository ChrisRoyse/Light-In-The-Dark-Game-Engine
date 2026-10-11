use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::history::EditorDocument;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpObjectDocumentReadback {
    pub path: String,
    pub vault_dir: String,
    pub cf: String,
    pub key_hex: String,
    pub seq: u64,
    pub graph_rows: usize,
    pub typed_object_rows: usize,
    pub constellation_rows: usize,
    pub structured_ingest_cx_ids: Vec<String>,
    pub structured_ingest_ledger_seq: Option<u64>,
    pub structured_ingest_ledger_hash: Option<String>,
    pub reactive_audit_rows: usize,
    pub reactive_fired_rows: usize,
    pub reactive_notifications: Vec<McpObjectReactiveNotification>,
    pub bytes: u64,
    pub sha256: String,
    pub document_fingerprint: String,
    pub document: EditorDocument,
    pub object_count: usize,
    pub kind_counts: BTreeMap<String, usize>,
    pub unit_count: usize,
    pub hero_count: usize,
    pub ability_count: usize,
    pub item_count: usize,
    pub buff_count: usize,
    pub upgrade_count: usize,
    pub tech_count: usize,
    pub gameplay_table_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpObjectReactiveNotification {
    pub trigger_id: String,
    pub cx_id: String,
    pub fired_at: u64,
    pub ledger_seq: u64,
    pub ledger_hash: String,
    pub condition: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpObjectToolLedger {
    pub schema: String,
    pub issue: u32,
    pub ledger_id: String,
    pub source_of_truth: String,
    pub record_count: usize,
    pub records: Vec<McpObjectToolRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpObjectToolRecord {
    pub request_id: String,
    pub requested_by: String,
    pub tool_name: String,
    pub operation_id: String,
    pub document_path: String,
    pub document_sha256: String,
    pub document_bytes: u64,
    pub document_fingerprint: String,
    pub action_summary: String,
    pub affected_object_ids: Vec<String>,
    pub object_count: usize,
    pub unit_count: usize,
    pub hero_count: usize,
    pub ability_count: usize,
    pub item_count: usize,
    pub buff_count: usize,
    pub upgrade_count: usize,
    pub tech_count: usize,
    pub gameplay_table_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpObjectToolLedgerReadback {
    pub path: String,
    pub vault_dir: String,
    pub cf: String,
    pub key_hex: String,
    pub seq: u64,
    pub graph_rows: usize,
    pub bytes: u64,
    pub sha256: String,
    pub ledger: McpObjectToolLedger,
}
