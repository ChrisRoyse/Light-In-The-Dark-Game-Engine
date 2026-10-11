//! Serializable value types for the MCP trigger-tools store (#1466 split of
//! `mcp_trigger_tools_store.rs`).
//!
//! Pure serde data: runtime state, sound record/runtime readbacks and the tool ledger. The
//! store I/O, validation and the `McpTriggerRuntimeState` impl stay in
//! `mcp_trigger_tools_store.rs`, which re-exports these so the public paths are unchanged.

use serde::{Deserialize, Serialize};

use super::super::{EcaGraph, RuntimeCue, TriggerVariable};
use crate::sound_editor::{SoundEditorRecord, SoundEditorRuntime};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpTriggerRuntimeState {
    pub schema: String,
    pub issue: u32,
    pub state_id: String,
    pub source_of_truth: String,
    pub update_count: u64,
    pub variables: Vec<TriggerVariable>,
    pub graphs: Vec<EcaGraph>,
    pub runtime_cues: Vec<RuntimeCue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpTriggerStateReadback {
    pub path: String,
    pub bytes: u64,
    pub seq: u64,
    pub sha256: String,
    pub state_fingerprint: String,
    pub state: McpTriggerRuntimeState,
    pub variable_count: usize,
    pub graph_count: usize,
    pub enabled_graph_count: usize,
    pub runtime_cue_count: usize,
    pub wasm_escape_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpSoundRecordReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub record_fingerprint: String,
    pub record: SoundEditorRecord,
    pub library_count: usize,
    pub named_sound_count: usize,
    pub trigger_binding_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpSoundRuntimeReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub runtime: SoundEditorRuntime,
    pub library_row_count: usize,
    pub resolved_trigger_binding_count: usize,
    pub preview_active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpTriggerToolLedger {
    pub schema: String,
    pub issue: u32,
    pub ledger_id: String,
    pub source_of_truth: String,
    pub record_count: usize,
    pub records: Vec<McpTriggerToolRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpTriggerToolRecord {
    pub request_id: String,
    pub requested_by: String,
    pub tool_name: String,
    pub operation_id: String,
    pub primary_path: String,
    pub primary_sha256: String,
    pub primary_bytes: u64,
    pub action_summary: String,
    pub affected_ids: Vec<String>,
    pub variable_count: usize,
    pub graph_count: usize,
    pub runtime_cue_count: usize,
    pub sound_library_count: usize,
    pub named_sound_count: usize,
    pub sound_binding_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpTriggerToolLedgerReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub ledger: McpTriggerToolLedger,
}
