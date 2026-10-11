use std::path::PathBuf;

use litd_engine::{ai::AiControllerAction, nav::NavCellCoord, sim::SimBodyId};
use serde::{Deserialize, Serialize};

pub const COMPUTER_OPPONENT_LEDGER_SCHEMA: &str = "litd.editor.computer_opponents.run.v1";
pub(super) const COMPUTER_OPPONENT_ISSUE: u32 = 852;
pub(super) const COMPUTER_OPPONENT_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FE6";
pub(super) const COMPUTER_OPPONENT_PURPOSE: &[u8] = b"litd-computer-opponents-issue852";
pub(super) const COMPUTER_OPPONENT_COLLECTION: &str = "litd_computer_opponents";
pub(super) const COMPUTER_OPPONENT_ROW: &[u8] = b"computer_opponents/run/latest";
pub(super) const COMPUTER_OPPONENT_COLUMN: &[u8] = b"computer_opponent_run_json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerOpponentDefinition {
    pub player_id: u8,
    pub player_name: String,
    pub agent_id: String,
    pub object_id: String,
    pub display_name: String,
    pub faction: String,
    pub source_id: u32,
    pub region_id: String,
    pub enabled: bool,
    pub script: ComputerOpponentScript,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerOpponentScript {
    pub script_id: String,
    pub behavior_preset: String,
    pub prompt: String,
    pub model_id: String,
    pub personality: String,
    pub allowed_intents: Vec<String>,
    pub forbidden_intents: Vec<String>,
    pub sight_range_cells: i32,
    pub attack_range_cells: i32,
    pub threat_range_cells: i32,
    pub path_max_expansions: usize,
    pub attack_damage_fp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerOpponentObject {
    pub object_id: String,
    pub body_id: SimBodyId,
    pub nav_cell: NavCellCoord,
    pub faction: String,
    pub source_id: u32,
    pub alive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerOpponentTarget {
    pub entity_id: String,
    pub body_id: SimBodyId,
    pub nav_cell: NavCellCoord,
    pub faction: String,
    pub alive: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerOpponentAuthoringArgs {
    pub state_path: PathBuf,
    pub ledger_dir: PathBuf,
    pub state_id: String,
    pub requested_by: String,
    pub opponents: Vec<ComputerOpponentDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerOpponentRuntimeArgs {
    pub run_id: String,
    pub state_path: PathBuf,
    pub controller_run_path: PathBuf,
    pub ledger_path: PathBuf,
    pub opponents: Vec<ComputerOpponentDefinition>,
    pub objects: Vec<ComputerOpponentObject>,
    pub targets: Vec<ComputerOpponentTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputerOpponentToolLedgerSummary {
    pub agent_id: String,
    pub tool: String,
    pub ledger_path: String,
    pub ledger_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerOpponentAuthoringReadback {
    pub state_path: String,
    pub state_sha256: String,
    pub state_fingerprint: String,
    pub binding_count: usize,
    pub prompt_model_count: usize,
    pub ward_envelope_count: usize,
    pub operation_count: usize,
    pub ledgers: Vec<ComputerOpponentToolLedgerSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerOpponentDecisionLedger {
    pub player_id: u8,
    pub agent_id: String,
    pub object_id: String,
    pub source_id: u32,
    pub action: AiControllerAction,
    pub selected_binding: Option<String>,
    pub target_entity_id: Option<String>,
    pub target_body_id: Option<SimBodyId>,
    pub order_count: usize,
    pub order_fingerprints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerOpponentRunLedger {
    pub schema: String,
    pub issue: u32,
    pub run_id: String,
    pub source_of_truth: String,
    pub mcp_agent_state_path: String,
    pub mcp_agent_state_sha256: String,
    pub mcp_agent_state_fingerprint: String,
    pub controller_run_path: String,
    pub controller_sha256: String,
    pub controller_fnv1a64: String,
    pub perception_fnv1a64: String,
    pub navmesh_fnv1a64: String,
    pub tick: u64,
    pub opponent_count: usize,
    pub target_count: usize,
    pub decision_count: usize,
    pub order_count: usize,
    pub decisions: Vec<ComputerOpponentDecisionLedger>,
    pub ledger_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerOpponentRunReadback {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub ledger: ComputerOpponentRunLedger,
}
