//! Deterministic sim snapshots and replay records with hash verification.

use crate::{
    EngineError,
    sim::{FixedTickSim, SimAttackEvent, SimBodyState, SimConfig},
    sim_order::{SimOrder, SimOrderQueue},
};
use serde::{Deserialize, Serialize};

pub const SIM_REPLAY_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimSnapshot {
    pub schema_version: u32,
    pub config: SimConfig,
    pub tick: u64,
    pub next_body_id: u32,
    pub rng_state: u64,
    pub bodies: Vec<SimBodyState>,
    /// Bounded window of recent attack events (evidence). For a long-running sim
    /// this is a suffix of history — `attack_events_total`, not this list's len,
    /// is the true count.
    pub attack_events: Vec<SimAttackEvent>,
    /// Persistent order-dependent digest of every attack ever recorded — the
    /// determinism source of truth for attacks (#1463).
    pub attack_events_digest: u64,
    /// Total attacks recorded over the session.
    pub attack_events_total: u64,
    pub state_hash: u64,
}

impl SimSnapshot {
    pub fn capture<const MAX_BODIES: usize>(sim: &FixedTickSim<MAX_BODIES>) -> Self {
        let next_body_id = sim
            .body_states()
            .map(|body| body.id.index())
            .max()
            .map_or(0, |index| index + 1);
        Self {
            schema_version: SIM_REPLAY_SCHEMA_VERSION,
            config: sim.config(),
            tick: sim.tick(),
            next_body_id,
            rng_state: sim.rng_state(),
            bodies: sim.body_states().collect(),
            attack_events: sim.attack_events().collect(),
            attack_events_digest: sim.attack_events_digest(),
            attack_events_total: sim.attack_events_total(),
            state_hash: sim.state_hash(),
        }
    }

    pub fn restore<const MAX_BODIES: usize>(
        &self,
    ) -> Result<FixedTickSim<MAX_BODIES>, EngineError> {
        validate_schema(self.schema_version, "SimSnapshot")?;
        let sim = FixedTickSim::from_snapshot_parts_with_attacks(
            self.config,
            self.tick,
            self.next_body_id,
            self.rng_state,
            &self.bodies,
            &self.attack_events,
            self.attack_events_digest,
            self.attack_events_total,
        )?;
        let actual = sim.state_hash();
        if actual != self.state_hash {
            return Err(hash_mismatch(self.state_hash, actual));
        }
        Ok(sim)
    }

    pub fn to_ron_string(&self) -> Result<String, EngineError> {
        ron::ser::to_string_pretty(self, pretty_config()).map_err(serialization_error)
    }

    pub fn from_ron_str(text: &str) -> Result<Self, EngineError> {
        ron::from_str(text).map_err(serialization_error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimReplayRecord {
    pub schema_version: u32,
    pub initial_snapshot: SimSnapshot,
    pub orders: Vec<SimOrder>,
    pub run_ticks: u64,
    pub expected_final_hash: u64,
}

impl SimReplayRecord {
    pub fn replay<const MAX_BODIES: usize, const MAX_ORDERS: usize>(
        &self,
    ) -> Result<SimReplayOutcome, EngineError> {
        validate_schema(self.schema_version, "SimReplayRecord")?;
        let mut sim = self.initial_snapshot.restore::<MAX_BODIES>()?;
        let mut queue = SimOrderQueue::<MAX_ORDERS>::default();
        for order in &self.orders {
            queue.enqueue(*order)?;
        }
        let report = queue.run_ticks(&mut sim, self.run_ticks)?;
        if queue.pending_orders() != 0 {
            return Err(EngineError::SerializationFailed {
                format: "sim-replay",
                reason: format!(
                    "{} orders remained pending after replay",
                    queue.pending_orders()
                ),
            });
        }
        if report.state_hash != self.expected_final_hash {
            return Err(hash_mismatch(self.expected_final_hash, report.state_hash));
        }
        Ok(SimReplayOutcome {
            initial_hash: self.initial_snapshot.state_hash,
            final_hash: report.state_hash,
            final_tick: report.final_tick,
            steps: report.steps,
            orders_applied: queue.applied_total(),
            final_snapshot: SimSnapshot::capture(&sim),
        })
    }

    pub fn to_ron_string(&self) -> Result<String, EngineError> {
        ron::ser::to_string_pretty(self, pretty_config()).map_err(serialization_error)
    }

    pub fn from_ron_str(text: &str) -> Result<Self, EngineError> {
        ron::from_str(text).map_err(serialization_error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimReplayOutcome {
    pub initial_hash: u64,
    pub final_hash: u64,
    pub final_tick: u64,
    pub steps: u64,
    pub orders_applied: u64,
    pub final_snapshot: SimSnapshot,
}

fn validate_schema(version: u32, type_name: &'static str) -> Result<(), EngineError> {
    if version == SIM_REPLAY_SCHEMA_VERSION {
        return Ok(());
    }
    Err(EngineError::SerializedTypeMismatch {
        expected: format!("{type_name}/v{SIM_REPLAY_SCHEMA_VERSION}"),
        actual: format!("{type_name}/v{version}"),
    })
}

fn pretty_config() -> ron::ser::PrettyConfig {
    ron::ser::PrettyConfig::default()
        .new_line("\n".to_string())
        .indentor("  ".to_string())
        .compact_arrays(true)
}

fn serialization_error(error: impl std::error::Error) -> EngineError {
    EngineError::SerializationFailed {
        format: "ron",
        reason: error.to_string(),
    }
}

fn hash_mismatch(expected: u64, actual: u64) -> EngineError {
    EngineError::SerializedFingerprintMismatch {
        expected: format!("{expected:016X}"),
        actual: format!("{actual:016X}"),
    }
}
