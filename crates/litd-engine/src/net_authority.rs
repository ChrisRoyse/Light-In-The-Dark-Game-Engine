//! Server-authoritative netcode core over the deterministic fixed-tick sim.

use crate::{
    EngineError,
    sim::{FixedTickSim, SimBodyId},
    sim_order::{SimOrderQueue, SimOrderSourceKind},
    sim_replay::SimSnapshot,
};

#[path = "net_authority_proof.rs"]
mod net_authority_proof;
#[path = "net_authority_types.rs"]
mod net_authority_types;
pub use self::net_authority_proof::*;
pub use self::net_authority_types::*;

pub struct AuthoritativeSimServer<const MAX_BODIES: usize, const MAX_ORDERS: usize> {
    config: AuthorityServerConfig,
    sim: FixedTickSim<MAX_BODIES>,
    queue: SimOrderQueue<MAX_ORDERS>,
    bindings: Vec<AuthorityClientBinding>,
    last_sequences: Vec<(u32, u64)>,
    snapshot_history: Vec<SimSnapshot>,
    accepted_input_count: u64,
    rejected_input_count: u64,
    corrected_state_count: u64,
    snapshot_delta_count: u64,
    lockstep_hash_checks: u64,
    lockstep_divergence_rejections: u64,
    rejected_authority_claim_count: u64,
    rejected_forged_position_count: u64,
    rejected_forged_hit_points_count: u64,
    rejected_forged_item_count: u64,
}

impl<const MAX_BODIES: usize, const MAX_ORDERS: usize>
    AuthoritativeSimServer<MAX_BODIES, MAX_ORDERS>
{
    pub fn new(
        config: AuthorityServerConfig,
        sim: FixedTickSim<MAX_BODIES>,
        bindings: Vec<AuthorityClientBinding>,
    ) -> Result<Self, EngineError> {
        validate_config(config)?;
        validate_bindings(&sim, &bindings)?;
        let last_sequences = bindings
            .iter()
            .map(|binding| (binding.client_id, 0))
            .collect();
        let snapshot_history = vec![SimSnapshot::capture(&sim)];
        Ok(Self {
            config,
            sim,
            queue: SimOrderQueue::default(),
            bindings,
            last_sequences,
            snapshot_history,
            accepted_input_count: 0,
            rejected_input_count: 0,
            corrected_state_count: 0,
            snapshot_delta_count: 0,
            lockstep_hash_checks: 0,
            lockstep_divergence_rejections: 0,
            rejected_authority_claim_count: 0,
            rejected_forged_position_count: 0,
            rejected_forged_hit_points_count: 0,
            rejected_forged_item_count: 0,
        })
    }

    pub const fn tick(&self) -> u64 {
        self.sim.tick()
    }

    pub fn state_hash(&self) -> u64 {
        self.sim.state_hash()
    }

    pub fn snapshot(&self) -> SimSnapshot {
        SimSnapshot::capture(&self.sim)
    }

    pub fn snapshot_history_len(&self) -> usize {
        self.snapshot_history.len()
    }

    pub const fn accepted_input_count(&self) -> u64 {
        self.accepted_input_count
    }

    pub const fn rejected_input_count(&self) -> u64 {
        self.rejected_input_count
    }

    pub const fn corrected_state_count(&self) -> u64 {
        self.corrected_state_count
    }

    pub const fn snapshot_delta_count(&self) -> u64 {
        self.snapshot_delta_count
    }

    pub const fn lockstep_hash_checks(&self) -> u64 {
        self.lockstep_hash_checks
    }

    pub const fn lockstep_divergence_rejections(&self) -> u64 {
        self.lockstep_divergence_rejections
    }

    pub fn connected_client_count(&self) -> usize {
        self.bindings.len()
    }

    pub const fn rejected_authority_claim_count(&self) -> u64 {
        self.rejected_authority_claim_count
    }

    pub const fn rejected_forged_position_count(&self) -> u64 {
        self.rejected_forged_position_count
    }

    pub const fn rejected_forged_hit_points_count(&self) -> u64 {
        self.rejected_forged_hit_points_count
    }

    pub const fn rejected_forged_item_count(&self) -> u64 {
        self.rejected_forged_item_count
    }

    pub fn submit_input(&mut self, input: ClientInput) -> Result<AuthorityInputAck, EngineError> {
        let binding = self.binding(input.client_id)?.clone();
        if input.order.source.kind != SimOrderSourceKind::Human
            || input.order.source.id != input.client_id
        {
            return self.reject("input.source", "client input source must match client_id");
        }
        if input.order.target != binding.body_id {
            return self.reject(
                "input.target",
                "client input can target only the server-bound body",
            );
        }
        let min_tick = self
            .sim
            .tick()
            .saturating_add(self.config.input_delay_ticks);
        if input.order.execute_tick < min_tick {
            return self.reject(
                "input.execute_tick",
                format!("input must execute at or after authoritative tick {min_tick}"),
            );
        }
        let last_index = self.last_sequence_index(input.client_id)?;
        let last_sequence = self.last_sequences[last_index].1;
        if input.sequence <= last_sequence {
            return self.reject("input.sequence", "client input sequence must increase");
        }
        let fingerprint = match self.queue.enqueue(input.order) {
            Ok(fingerprint) => fingerprint,
            Err(error) => {
                self.rejected_input_count = self.rejected_input_count.saturating_add(1);
                return Err(error);
            }
        };
        self.last_sequences[last_index].1 = input.sequence;
        self.accepted_input_count = self.accepted_input_count.saturating_add(1);
        Ok(AuthorityInputAck {
            client_id: input.client_id,
            sequence: input.sequence,
            accepted_at_server_tick: self.sim.tick(),
            order_fingerprint: fingerprint,
        })
    }

    pub fn tick_once(&mut self) -> Result<AuthorityTickReport, EngineError> {
        let before = SimSnapshot::capture(&self.sim);
        let step = self.queue.step(&mut self.sim)?;
        let after = SimSnapshot::capture(&self.sim);
        let delta = SnapshotDelta::between(&before, &after)?;
        if after
            .tick
            .is_multiple_of(self.config.snapshot_interval_ticks)
        {
            self.snapshot_history.push(after);
            self.snapshot_delta_count = self.snapshot_delta_count.saturating_add(1);
        }
        Ok(AuthorityTickReport {
            tick: step.tick,
            orders_applied: step.orders_applied,
            bodies_updated: step.bodies_updated,
            state_hash: step.state_hash,
            pending_orders: step.pending_orders,
            snapshot_delta: delta,
        })
    }

    pub fn reconcile(
        &mut self,
        claim: ClientStateClaim,
    ) -> Result<ReconciliationReport, EngineError> {
        self.binding(claim.client_id)?;
        let authoritative = self.snapshot();
        if claim.predicted_snapshot.tick != authoritative.tick {
            return Err(net_error(
                "reconcile.tick",
                "client prediction tick must match the authoritative correction tick",
            ));
        }
        let accepted = claim.predicted_snapshot == authoritative;
        let correction = if accepted {
            None
        } else {
            self.corrected_state_count = self.corrected_state_count.saturating_add(1);
            Some(authoritative.clone())
        };
        Ok(ReconciliationReport {
            client_id: claim.client_id,
            accepted,
            authoritative_tick: authoritative.tick,
            expected_state_hash: authoritative.state_hash,
            claimed_state_hash: claim.predicted_snapshot.state_hash,
            acknowledged_sequence: claim.acknowledged_sequence,
            replay_from_sequence: claim.acknowledged_sequence.saturating_add(1),
            correction,
        })
    }

    pub fn validate_lockstep_hash(
        &mut self,
        client_id: u32,
        tick: u64,
        claimed_state_hash: u64,
    ) -> Result<LockstepHashVerdict, EngineError> {
        self.binding(client_id)?;
        if tick != self.sim.tick() {
            return Err(net_error(
                "lockstep.tick",
                "lockstep hash must be reported for the current authoritative tick",
            ));
        }
        let expected = self.sim.state_hash();
        let accepted = expected == claimed_state_hash;
        self.lockstep_hash_checks = self.lockstep_hash_checks.saturating_add(1);
        if !accepted {
            self.lockstep_divergence_rejections =
                self.lockstep_divergence_rejections.saturating_add(1);
            tracing::warn!(
                client_id,
                tick,
                expected_state_hash = expected,
                claimed_state_hash,
                code = "LITD-NET-AUTHORITY-LOCKSTEP-DIVERGED",
                "server rejected divergent client lockstep hash"
            );
        }
        Ok(LockstepHashVerdict {
            client_id,
            tick,
            accepted,
            expected_state_hash: expected,
            claimed_state_hash,
            action: if accepted {
                LockstepHashAction::Accept
            } else {
                LockstepHashAction::RejectDivergentHash
            },
        })
    }

    pub fn client_truth(&self, client_id: u32) -> Result<NetAuthorityClientTruth, EngineError> {
        let binding = self.binding(client_id)?;
        let snapshot = self.snapshot();
        let body = snapshot
            .bodies
            .iter()
            .find(|body| body.id == binding.body_id)
            .ok_or_else(|| net_error("truth.body", "bound body missing from server snapshot"))?;
        Ok(NetAuthorityClientTruth {
            client_id,
            body_id: binding.body_id.index(),
            position: body.position,
            hit_points: binding.hit_points,
            inventory_items: binding.inventory_items.clone(),
        })
    }

    pub fn reject_authority_claim(
        &mut self,
        claim: ClientForgedAuthorityClaim,
    ) -> Result<ForgedAuthorityRejection, EngineError> {
        let truth = self.client_truth(claim.client_id)?;
        self.rejected_authority_claim_count = self.rejected_authority_claim_count.saturating_add(1);
        self.rejected_forged_position_count = self.rejected_forged_position_count.saturating_add(1);
        self.rejected_forged_hit_points_count =
            self.rejected_forged_hit_points_count.saturating_add(1);
        self.rejected_forged_item_count = self.rejected_forged_item_count.saturating_add(1);
        tracing::warn!(
            client_id = claim.client_id,
            code = "LITD-NET-AUTHORITY-CLAIM-REJECTED",
            "server rejected client-authored position, hit points, and inventory authority claim"
        );
        Ok(ForgedAuthorityRejection {
            client_id: claim.client_id,
            position_rejected: true,
            hit_points_rejected: true,
            item_rejected: true,
            server_position: truth.position,
            server_hit_points: truth.hit_points,
            server_inventory_items: truth.inventory_items,
            forged_position: claim.forged_position,
            forged_hit_points: claim.forged_hit_points,
            forged_item_id: claim.forged_item_id,
        })
    }

    fn binding(&self, client_id: u32) -> Result<&AuthorityClientBinding, EngineError> {
        self.bindings
            .iter()
            .find(|binding| binding.client_id == client_id)
            .ok_or_else(|| net_error("client_id", "client is not bound to this server"))
    }

    fn last_sequence_index(&self, client_id: u32) -> Result<usize, EngineError> {
        self.last_sequences
            .iter()
            .position(|(id, _)| *id == client_id)
            .ok_or_else(|| net_error("client_id", "client sequence ledger missing"))
    }

    fn reject<T>(
        &mut self,
        field: impl Into<String>,
        reason: impl std::fmt::Display,
    ) -> Result<T, EngineError> {
        let field = field.into();
        let reason = reason.to_string();
        self.rejected_input_count = self.rejected_input_count.saturating_add(1);
        tracing::warn!(
            field = %field,
            reason = %reason,
            code = "LITD-NET-AUTHORITY-INPUT-REJECTED",
            "server rejected non-authoritative client input"
        );
        Err(net_error(field, reason))
    }
}

impl SnapshotDelta {
    pub fn between(from: &SimSnapshot, to: &SimSnapshot) -> Result<Self, EngineError> {
        if to.tick < from.tick {
            return Err(net_error(
                "snapshot.tick",
                "snapshot delta cannot move backwards in authoritative time",
            ));
        }
        let changed_bodies = to
            .bodies
            .iter()
            .copied()
            .filter(|body| from.bodies.iter().find(|previous| previous.id == body.id) != Some(body))
            .collect::<Vec<_>>();
        Ok(Self {
            from_tick: from.tick,
            to_tick: to.tick,
            from_state_hash: from.state_hash,
            to_state_hash: to.state_hash,
            changed_body_count: changed_bodies.len(),
            changed_bodies,
        })
    }
}

fn validate_config(config: AuthorityServerConfig) -> Result<(), EngineError> {
    if config.input_delay_ticks == 0 {
        return Err(net_error("input_delay_ticks", "must be non-zero"));
    }
    if config.snapshot_interval_ticks == 0 {
        return Err(net_error("snapshot_interval_ticks", "must be non-zero"));
    }
    if config.max_reconciliation_inputs == 0 {
        return Err(net_error("max_reconciliation_inputs", "must be non-zero"));
    }
    Ok(())
}

fn validate_bindings<const MAX_BODIES: usize>(
    sim: &FixedTickSim<MAX_BODIES>,
    bindings: &[AuthorityClientBinding],
) -> Result<(), EngineError> {
    if bindings.is_empty() {
        return Err(net_error(
            "bindings",
            "at least one client binding is required",
        ));
    }
    for (index, binding) in bindings.iter().enumerate() {
        if binding.client_id == 0 {
            return Err(net_error(
                "bindings.client_id",
                "client_id must be non-zero",
            ));
        }
        if sim.body_state(binding.body_id).is_none() {
            return Err(net_error(
                "bindings.body_id",
                format!(
                    "body {} is not present in authoritative sim",
                    binding.body_id.index()
                ),
            ));
        }
        if binding.hit_points <= 0 {
            return Err(net_error(
                "bindings.hit_points",
                "server-owned hit points must be positive",
            ));
        }
        validate_inventory_items(&binding.inventory_items)?;
        for previous in &bindings[..index] {
            if previous.client_id == binding.client_id {
                return Err(net_error("bindings.client_id", "duplicate client_id"));
            }
            if previous.body_id == binding.body_id {
                return Err(net_error(
                    "bindings.body_id",
                    "a sim body cannot be controlled by multiple clients",
                ));
            }
        }
    }
    Ok(())
}

fn validate_inventory_items(items: &[String]) -> Result<(), EngineError> {
    if items.is_empty() {
        return Err(net_error(
            "bindings.inventory_items",
            "server-owned inventory item list must be non-empty",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for item in items {
        if item.trim().is_empty() || item.bytes().any(|byte| byte.is_ascii_control()) {
            return Err(net_error(
                "bindings.inventory_items",
                "server-owned inventory item ids must be non-empty single-line strings",
            ));
        }
        if !seen.insert(item.as_str()) {
            return Err(net_error(
                "bindings.inventory_items",
                "server-owned inventory item ids must be unique per binding",
            ));
        }
    }
    Ok(())
}

pub(crate) fn missing_body_id(snapshot: &SimSnapshot) -> SimBodyId {
    let next = snapshot
        .bodies
        .iter()
        .map(|body| body.id.index())
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    SimBodyId::from_raw(next)
}

pub(crate) fn net_error(field: impl Into<String>, reason: impl std::fmt::Display) -> EngineError {
    EngineError::SimInvalidConfig {
        field: format!("net_authority.{}", field.into()),
        reason: reason.to_string(),
    }
}
