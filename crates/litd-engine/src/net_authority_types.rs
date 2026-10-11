use crate::{
    math::FixedVec3,
    sim::{SimBodyId, SimBodyState},
    sim_order::SimOrder,
    sim_replay::SimSnapshot,
};
use serde::{Deserialize, Serialize};

pub const NET_AUTHORITY_SCHEMA: &str = "litd.engine.net.authority.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityServerConfig {
    pub input_delay_ticks: u64,
    pub snapshot_interval_ticks: u64,
    pub max_reconciliation_inputs: usize,
}

impl Default for AuthorityServerConfig {
    fn default() -> Self {
        Self {
            input_delay_ticks: 1,
            snapshot_interval_ticks: 1,
            max_reconciliation_inputs: 32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityClientBinding {
    pub client_id: u32,
    pub body_id: SimBodyId,
    pub hit_points: i64,
    pub inventory_items: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientInput {
    pub client_id: u32,
    pub sequence: u64,
    pub order: SimOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityInputAck {
    pub client_id: u32,
    pub sequence: u64,
    pub accepted_at_server_tick: u64,
    pub order_fingerprint: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotDelta {
    pub from_tick: u64,
    pub to_tick: u64,
    pub from_state_hash: u64,
    pub to_state_hash: u64,
    pub changed_body_count: usize,
    pub changed_bodies: Vec<SimBodyState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityTickReport {
    pub tick: u64,
    pub orders_applied: usize,
    pub bodies_updated: usize,
    pub state_hash: u64,
    pub pending_orders: usize,
    pub snapshot_delta: SnapshotDelta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientStateClaim {
    pub client_id: u32,
    pub acknowledged_sequence: u64,
    pub predicted_snapshot: SimSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationReport {
    pub client_id: u32,
    pub accepted: bool,
    pub authoritative_tick: u64,
    pub expected_state_hash: u64,
    pub claimed_state_hash: u64,
    pub acknowledged_sequence: u64,
    pub replay_from_sequence: u64,
    pub correction: Option<SimSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockstepHashAction {
    Accept,
    RejectDivergentHash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockstepHashVerdict {
    pub client_id: u32,
    pub tick: u64,
    pub accepted: bool,
    pub expected_state_hash: u64,
    pub claimed_state_hash: u64,
    pub action: LockstepHashAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetAuthorityProof {
    pub schema: String,
    pub authority_mode: String,
    pub transport_contract: String,
    pub client_contract: String,
    pub connected_clients: u32,
    pub controlled_client_id: u32,
    pub controlled_body_id: u32,
    pub server_tick: u64,
    pub server_state_hash: u64,
    pub accepted_inputs: u64,
    pub rejected_inputs: u64,
    pub corrected_forged_states: u64,
    pub rejected_forged_positions: u64,
    pub rejected_forged_hit_points: u64,
    pub rejected_forged_items: u64,
    pub snapshot_deltas: u64,
    pub snapshot_delta_changed_bodies: usize,
    pub lockstep_hash_checks: u64,
    pub lockstep_divergence_rejections: u64,
    pub server_truth: Vec<NetAuthorityClientTruth>,
    pub forged_claim_rejections: Vec<ForgedAuthorityRejection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetAuthorityClientTruth {
    pub client_id: u32,
    pub body_id: u32,
    pub position: FixedVec3,
    pub hit_points: i64,
    pub inventory_items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientForgedAuthorityClaim {
    pub client_id: u32,
    pub forged_position: FixedVec3,
    pub forged_hit_points: i64,
    pub forged_item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgedAuthorityRejection {
    pub client_id: u32,
    pub position_rejected: bool,
    pub hit_points_rejected: bool,
    pub item_rejected: bool,
    pub server_position: FixedVec3,
    pub server_hit_points: i64,
    pub server_inventory_items: Vec<String>,
    pub forged_position: FixedVec3,
    pub forged_hit_points: i64,
    pub forged_item_id: String,
}

impl NetAuthorityProof {
    pub fn proves_server_authority(&self) -> bool {
        self.schema == NET_AUTHORITY_SCHEMA
            && self.authority_mode == "server_authoritative"
            && self.client_contract == "inputs_only"
            && self.connected_clients >= 2
            && self.server_state_hash != 0
            && self.accepted_inputs > 0
            && self.rejected_inputs > 0
            && self.corrected_forged_states > 0
            && self.rejected_forged_positions > 0
            && self.rejected_forged_hit_points > 0
            && self.rejected_forged_items > 0
            && self.snapshot_deltas > 0
            && self.snapshot_delta_changed_bodies > 0
            && self.lockstep_hash_checks >= 2
            && self.lockstep_divergence_rejections > 0
            && self.server_truth.len() >= 2
            && self.forged_claim_rejections.iter().any(|rejection| {
                rejection.position_rejected
                    && rejection.hit_points_rejected
                    && rejection.item_rejected
            })
    }
}
