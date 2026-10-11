//! Typed match orders + a deterministic order queue over [`MatchState`].
//!
//! The order-issuing surface for the match layer (#836/#851/#838): alliance
//! changes, selection, unit orders (point / unit-target / no-target, queued or
//! not), trigger fires, and unit destruction. [`MatchOrderQueue`] applies the
//! orders scheduled for each tick in a fixed total order — `(execute_tick,
//! player, sequence)` — so replaying an identical order stream reproduces an
//! identical [`MatchState::state_hash`] (#85 lockstep). Past-due orders fail
//! loud rather than being silently dropped (a divergence risk under lockstep).

use crate::{
    EngineError,
    game_runtime::{
        groups::{UnitId, UnitOrder},
        match_state::MatchState,
        players::{AllianceState, PlayerId},
        victory::GameOverEvent,
    },
};

/// What a match order does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchOrderKind {
    SetAlliance {
        from: PlayerId,
        to: PlayerId,
        state: AllianceState,
        shared_vision: bool,
        shared_control: bool,
    },
    Select {
        units: Vec<UnitId>,
    },
    IssueOrder {
        units: Vec<UnitId>,
        order: UnitOrder,
    },
    FireTrigger {
        trigger: u32,
    },
    DestroyUnit {
        unit: UnitId,
    },
}

/// One scheduled match order. `player` is the issuing source (part of the total
/// order for determinism); `sequence` breaks ties within a player+tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchOrder {
    pub execute_tick: u64,
    pub player: PlayerId,
    pub sequence: u64,
    pub kind: MatchOrderKind,
}

impl MatchOrder {
    pub fn new(execute_tick: u64, player: PlayerId, sequence: u64, kind: MatchOrderKind) -> Self {
        Self {
            execute_tick,
            player,
            sequence,
            kind,
        }
    }

    fn sort_key(&self) -> (u64, PlayerId, u64) {
        (self.execute_tick, self.player, self.sequence)
    }
}

/// Report from stepping the match one tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchStepReport {
    pub tick: u64,
    pub orders_applied: usize,
    pub game_over_events: Vec<GameOverEvent>,
    pub state_hash: u64,
    pub pending_orders: usize,
}

/// A deterministic queue of match orders applied against a [`MatchState`].
#[derive(Debug, Clone, Default)]
pub struct MatchOrderQueue {
    orders: Vec<MatchOrder>,
    applied_total: u64,
}

impl MatchOrderQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending_orders(&self) -> usize {
        self.orders.len()
    }

    pub fn applied_total(&self) -> u64 {
        self.applied_total
    }

    /// Schedule an order. Must be for a future or current-not-yet-stepped tick.
    pub fn enqueue(&mut self, order: MatchOrder) -> Result<(), EngineError> {
        self.orders.push(order);
        Ok(())
    }

    /// Advance the match one tick and apply every order scheduled for it, in the
    /// fixed total order. A leftover order for an already-elapsed tick fails loud.
    pub fn step(&mut self, state: &mut MatchState) -> Result<MatchStepReport, EngineError> {
        let execute_tick = state.advance_tick()?;
        // Reject past-due orders: under lockstep, dropping one peer's late order
        // while another applied it on time diverges their hashes.
        if let Some(order) = self.orders.iter().find(|o| o.execute_tick < execute_tick) {
            return Err(EngineError::SimInvalidConfig {
                field: "match_order.past_due".to_string(),
                reason: format!(
                    "order for tick {} is past due at tick {execute_tick}",
                    order.execute_tick
                ),
            });
        }
        let mut due: Vec<MatchOrder> = self
            .orders
            .iter()
            .filter(|o| o.execute_tick == execute_tick)
            .cloned()
            .collect();
        self.orders.retain(|o| o.execute_tick != execute_tick);
        due.sort_by_key(|o| o.sort_key());
        let mut game_over_events = Vec::new();
        let mut applied = 0;
        for order in due {
            let events = apply_order(state, order)?;
            game_over_events.extend(events);
            self.applied_total += 1;
            applied += 1;
        }
        Ok(MatchStepReport {
            tick: execute_tick,
            orders_applied: applied,
            game_over_events,
            state_hash: state.state_hash(),
            pending_orders: self.orders.len(),
        })
    }

    /// Step `ticks` times, accumulating the final state hash.
    pub fn run_ticks(&mut self, state: &mut MatchState, ticks: u64) -> Result<u64, EngineError> {
        for _ in 0..ticks {
            self.step(state)?;
        }
        Ok(state.state_hash())
    }
}

fn apply_order(
    state: &mut MatchState,
    order: MatchOrder,
) -> Result<Vec<GameOverEvent>, EngineError> {
    match order.kind {
        MatchOrderKind::SetAlliance {
            from,
            to,
            state: alliance,
            shared_vision,
            shared_control,
        } => {
            state.set_alliance(from, to, alliance, shared_vision, shared_control)?;
            Ok(Vec::new())
        }
        MatchOrderKind::Select { units } => {
            state.select(order.player, &units)?;
            Ok(Vec::new())
        }
        MatchOrderKind::IssueOrder {
            units,
            order: unit_order,
        } => {
            state.issue_order(order.player, &units, unit_order)?;
            Ok(Vec::new())
        }
        MatchOrderKind::FireTrigger { trigger } => state.fire_trigger(trigger),
        MatchOrderKind::DestroyUnit { unit } => state.destroy_unit(unit),
    }
}
