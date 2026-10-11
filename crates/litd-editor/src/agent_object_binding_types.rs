use std::path::PathBuf;

use litd_engine::{
    ai::{AiControllerAction, AiControllerActionBindings, BehaviorTree},
    nav::NavCellCoord,
    sim::SimBodyId,
};
use serde::{Deserialize, Serialize};

pub const AGENT_OBJECT_BINDING_LEDGER_SCHEMA: &str = "litd.editor.agent_object_binding_runtime.v1";
pub(super) const AGENT_OBJECT_BINDING_ISSUE: u32 = 784;
pub(super) const AGENT_OBJECT_BINDING_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FE4";
pub(super) const AGENT_OBJECT_BINDING_PURPOSE: &[u8] = b"litd-agent-object-binding-issue784";
pub(super) const AGENT_OBJECT_BINDING_COLLECTION: &str = "litd_agent_object_binding_runtime";
pub(super) const AGENT_OBJECT_BINDING_ROW: &[u8] = b"agent_object_binding/run/latest";
pub(super) const AGENT_OBJECT_BINDING_COLUMN: &[u8] = b"binding_run_json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundAgentObject {
    pub object_id: String,
    pub body_id: SimBodyId,
    pub nav_cell: NavCellCoord,
    pub source_id: u32,
    pub faction: String,
    pub alive: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentObjectBindingRunArgs {
    pub run_id: String,
    pub state_path: PathBuf,
    pub controller_run_path: PathBuf,
    pub ledger_path: PathBuf,
    pub object_bodies: Vec<BoundAgentObject>,
    pub behavior_tree: BehaviorTree,
    pub action_bindings: AiControllerActionBindings,
    pub path_max_expansions: usize,
    pub attack_damage_fp: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentObjectBindingDecisionLedger {
    pub binding_id: String,
    pub agent_id: String,
    pub object_id: String,
    pub deterministic_order_sink: String,
    pub body_id: SimBodyId,
    pub source_id: u32,
    pub action: AiControllerAction,
    pub intent_id: String,
    pub order_count: usize,
    pub order_fingerprints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentObjectBindingRunLedger {
    pub schema: String,
    pub issue: u32,
    pub run_id: String,
    pub source_of_truth: String,
    pub mcp_agent_state_schema: String,
    pub state_path: String,
    pub state_sha256: String,
    pub state_fingerprint: String,
    pub controller_run_path: String,
    pub controller_sha256: String,
    pub controller_fnv1a64: String,
    pub tick: u64,
    pub binding_count: usize,
    pub decision_count: usize,
    pub order_count: usize,
    pub order_fingerprints: Vec<String>,
    pub decisions: Vec<AgentObjectBindingDecisionLedger>,
    pub ledger_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentObjectBindingRunReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub ledger: AgentObjectBindingRunLedger,
}
