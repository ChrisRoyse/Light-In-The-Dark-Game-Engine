//! Deterministic destructible transition machine (#810).
//!
//! The sim consumes the frozen [`DestructibleField`] (as read back from Aster) and
//! applies an ordered script of damage events. Everything is integer arithmetic
//! executed in a canonical order, so a scripted run is reproducible bit-for-bit:
//! two runs of the same field + script produce the identical transition log and
//! fingerprint. This is the hot-path consumption side of the persist->lower valve
//! -- it never queries Calyx.

use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::{
    sim_order::SimOrderSource,
    world_authoring::{WorldAuthoringError, codec_failed, invalid},
    world_destructibles::{DestructibleField, DestructibleKind},
};

pub const DESTRUCTIBLE_SIM_SCHEMA: &str = "litd.world.destructible_sim.v1";

/// The three-valued lifecycle state of a destructible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestructibleState {
    Intact,
    Damaged,
    Destroyed,
}

impl DestructibleState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Intact => "intact",
            Self::Damaged => "damaged",
            Self::Destroyed => "destroyed",
        }
    }
}

/// One scripted attack against a destructible at a given tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DamageEvent {
    pub tick: u64,
    /// Deterministic tie-break within a tick (author/attacker sequence).
    pub sequence: u64,
    pub target_id: String,
    pub amount: u32,
}

/// Live runtime state for one destructible during a sim run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DestructibleRuntime {
    pub id: String,
    pub kind: DestructibleKind,
    pub max_hp: u32,
    pub armor: u32,
    pub current_hp: u32,
    pub state: DestructibleState,
}

impl DestructibleRuntime {
    /// Apply one hit. Armor is flat reduction; a hit that fully penetrates to 0 HP
    /// destroys the object. A `Destroyed` object absorbs no further damage. Returns
    /// the effective (post-armor) damage actually dealt.
    fn apply(&mut self, amount: u32) -> u32 {
        if self.state == DestructibleState::Destroyed {
            return 0;
        }
        let effective = amount.saturating_sub(self.armor).min(self.current_hp);
        self.current_hp -= effective;
        self.state = if self.current_hp == 0 {
            DestructibleState::Destroyed
        } else if self.current_hp < self.max_hp {
            DestructibleState::Damaged
        } else {
            DestructibleState::Intact
        };
        effective
    }
}

/// The recorded outcome of applying one event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DestructibleTransition {
    pub tick: u64,
    pub sequence: u64,
    pub target_id: String,
    pub raw_amount: u32,
    pub effective_damage: u32,
    pub hp_before: u32,
    pub hp_after: u32,
    pub state_before: DestructibleState,
    pub state_after: DestructibleState,
}

/// The full report of a scripted sim run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DestructibleSimReport {
    pub schema: String,
    pub field_id: String,
    pub sim_order_source: SimOrderSource,
    pub event_count: usize,
    pub transitions: Vec<DestructibleTransition>,
    pub final_runtimes: Vec<DestructibleRuntime>,
    pub destroyed_count: usize,
    pub damaged_count: usize,
    pub intact_count: usize,
    pub fingerprint: String,
}

/// Run a scripted destructible sim. Events are executed in canonical order
/// (`tick`, then `sequence`, then `target_id`) so the result is independent of the
/// order the caller supplied them in. Unknown target ids fail closed.
pub fn run_destructible_sim(
    field: &DestructibleField,
    events: &[DamageEvent],
    lowered_agent_id: u32,
) -> Result<DestructibleSimReport, WorldAuthoringError> {
    field.validate()?;
    if lowered_agent_id == 0 {
        return invalid(
            "WORLD_DESTRUCTIBLE_SIM_AGENT",
            "lowered_agent_id",
            "lowered_agent_id must be non-zero (identifies the frozen source)",
        );
    }

    let mut runtimes: Vec<DestructibleRuntime> = field
        .destructibles
        .iter()
        .map(|spec| DestructibleRuntime {
            id: spec.id.clone(),
            kind: spec.kind,
            max_hp: spec.max_hp,
            armor: spec.armor,
            current_hp: spec.max_hp,
            state: DestructibleState::Intact,
        })
        .collect();
    runtimes.sort_by(|a, b| a.id.cmp(&b.id));

    let mut ordered = events.to_vec();
    ordered.sort_by(|a, b| {
        a.tick
            .cmp(&b.tick)
            .then(a.sequence.cmp(&b.sequence))
            .then(a.target_id.cmp(&b.target_id))
    });

    let mut transitions = Vec::with_capacity(ordered.len());
    for event in &ordered {
        if event.amount == 0 {
            return invalid(
                "WORLD_DESTRUCTIBLE_SIM_AMOUNT",
                "events.amount",
                "damage amount must be positive",
            );
        }
        let index = runtimes
            .binary_search_by(|runtime| runtime.id.as_str().cmp(event.target_id.as_str()))
            .map_err(|_| WorldAuthoringError::Invalid {
                code: "WORLD_DESTRUCTIBLE_SIM_TARGET",
                field: "events.target_id".to_string(),
                reason: format!("event targets unknown destructible {}", event.target_id),
            })?;
        let runtime = &mut runtimes[index];
        let hp_before = runtime.current_hp;
        let state_before = runtime.state;
        let effective_damage = runtime.apply(event.amount);
        transitions.push(DestructibleTransition {
            tick: event.tick,
            sequence: event.sequence,
            target_id: event.target_id.clone(),
            raw_amount: event.amount,
            effective_damage,
            hp_before,
            hp_after: runtime.current_hp,
            state_before,
            state_after: runtime.state,
        });
    }

    let destroyed_count = count_state(&runtimes, DestructibleState::Destroyed);
    let damaged_count = count_state(&runtimes, DestructibleState::Damaged);
    let intact_count = count_state(&runtimes, DestructibleState::Intact);

    let mut report = DestructibleSimReport {
        schema: DESTRUCTIBLE_SIM_SCHEMA.to_string(),
        field_id: field.field_id.clone(),
        sim_order_source: SimOrderSource::lowered_agent(lowered_agent_id),
        event_count: ordered.len(),
        transitions,
        final_runtimes: runtimes,
        destroyed_count,
        damaged_count,
        intact_count,
        fingerprint: String::new(),
    };
    report.fingerprint = fingerprint(&report)?;
    Ok(report)
}

fn count_state(runtimes: &[DestructibleRuntime], state: DestructibleState) -> usize {
    runtimes.iter().filter(|item| item.state == state).count()
}

fn fingerprint(report: &DestructibleSimReport) -> Result<String, WorldAuthoringError> {
    let mut canonical = report.clone();
    canonical.fingerprint = String::new();
    let bytes = serde_json::to_vec(&canonical).map_err(codec_failed)?;
    Ok(sha256_hex(&bytes))
}

#[cfg(test)]
#[path = "world_destructibles_sim_tests.rs"]
mod tests;
