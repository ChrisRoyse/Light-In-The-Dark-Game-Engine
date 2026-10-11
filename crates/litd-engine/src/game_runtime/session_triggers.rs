//! #1851: the deterministic ECA trigger evaluator that runs INSIDE the session tick.
//!
//! A [`TriggerRuntime`] holds the lowered trigger graph plus the mutable trigger
//! state -- the typed variable store, per-trigger fire counts, and the per-unit
//! alive latches that detect `unit_dies` transitions. On each `Tick`,
//! `MatchRuntimeSession::trigger_step` collects this tick's event occurrences (map
//! init, elapsed time, HUD timer expiries via [`super::super::hud::GameHudWorld::
//! fired_this_tick`], unit-death transitions), evaluates every trigger in lowered
//! order, and dispatches its unit-affecting actions as typed [`super::SessionOrder`]s
//! (the same internal-dispatch idiom as the #1786 macro-AI): replaying the `Tick`
//! stream re-runs the evaluator identically, and all mutable trigger state folds
//! into `combined_state_hash`, so an unhashed variable or latch is a determinism
//! hole a mutation test detects (#85 lockstep).
//!
//! The evaluator is a pure function of (tick, sim state, trigger graph): NO wall
//! clock, NO floats, NO map-iteration order (variables live in a name-sorted vec,
//! triggers evaluate in lowered order, timer expiries arrive in the deterministic
//! order the timer set produced them).

use litd_hash::Fnv1a64;

use super::super::regions::LoweredRegionSet;
use super::super::trigger_graph::{
    CompareOp, FlagOperand, IntOperand, LoweredTriggerGraph, TriggerAction, TriggerCondition,
    TriggerEvent, TriggerValue,
};
use super::GameRuntimeError;
use super::{MatchRuntimeSession, SessionOrder};

// #1855+: plan effect/fog/cinematic/spawn/weather/camera trigger actions
// (kept modular so this evaluator stays under the 500-line ratchet).
#[path = "session_trigger_plan.rs"]
mod session_trigger_plan;
#[path = "session_trigger_unit_orders.rs"]
mod session_trigger_unit_orders;
// #1856: region enter/leave transition collection.
#[path = "session_region_triggers.rs"]
mod session_region_triggers;

/// One trigger firing, retained for the FSV/log timeline. Deterministic (a pure
/// function of the hashed state), so it is not itself hashed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerFiring {
    pub tick: u64,
    pub trigger_id: String,
    pub event: &'static str,
}

/// The live trigger state: lowered graph + variable store + latches + fire counts.
#[derive(Debug, Default, Clone)]
pub struct TriggerRuntime {
    graph: Option<LoweredTriggerGraph>,
    /// Name-sorted typed variable store (parallel to the graph's variable table).
    variables: Vec<(String, TriggerValue)>,
    /// Per-trigger cumulative fire count, in lowered trigger order.
    fire_counts: Vec<u64>,
    /// (unit, last_known_alive) for every unit a `unit_dies` event watches, sorted
    /// by unit id. A true->false transition is one death occurrence.
    unit_latches: Vec<(u32, bool)>,
    /// #1856: (unit, region, was_inside) membership latches for enter/leave events.
    pub(super) region_latches: Vec<(u32, u32, bool)>,
    timeline: Vec<TriggerFiring>,
}

impl TriggerRuntime {
    /// Build the runtime trigger state from the (already dataset-validated) graph
    /// and optional #1856 region set (used to seed enter/leave latches).
    pub(super) fn new(
        graph: Option<&LoweredTriggerGraph>,
        regions: Option<&LoweredRegionSet>,
    ) -> Self {
        let Some(graph) = graph else {
            return Self::default();
        };
        let variables = graph
            .variables
            .iter()
            .map(|init| (init.name.clone(), init.value.clone()))
            .collect();
        let fire_counts = vec![0_u64; graph.triggers.len()];
        let mut watched: Vec<u32> = graph
            .triggers
            .iter()
            .flat_map(|t| t.events.iter())
            .filter_map(|event| match event {
                TriggerEvent::UnitDies { unit } => Some(*unit),
                _ => None,
            })
            .collect();
        watched.sort_unstable();
        watched.dedup();
        // Region latches: every unit x every region that enter/leave events watch.
        let mut region_ids: Vec<u32> = graph
            .triggers
            .iter()
            .flat_map(|t| t.events.iter())
            .filter_map(|event| match event {
                TriggerEvent::UnitEntersRegion { region }
                | TriggerEvent::UnitLeavesRegion { region } => Some(*region),
                _ => None,
            })
            .collect();
        region_ids.sort_unstable();
        region_ids.dedup();
        // Pre-seed region-only sentinels (unit_id=0); collect_region_transitions expands
        // to every live field unit on first evaluate.
        let mut region_latches = Vec::new();
        let _ = regions; // validated at package load; latches only need region ids
        for region_id in region_ids {
            region_latches.push((0, region_id, false));
        }
        Self {
            graph: Some(graph.clone()),
            variables,
            fire_counts,
            unit_latches: watched.into_iter().map(|unit| (unit, true)).collect(),
            region_latches,
            timeline: Vec::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.graph.is_some()
    }

    /// The trigger firings recorded so far (FSV/log observability).
    pub fn timeline(&self) -> &[TriggerFiring] {
        &self.timeline
    }

    /// Current value of a runtime variable, if declared.
    pub fn variable(&self, name: &str) -> Option<&TriggerValue> {
        self.variables
            .binary_search_by(|(n, _)| n.as_str().cmp(name))
            .ok()
            .map(|index| &self.variables[index].1)
    }

    /// Cumulative fire count of the trigger with the given id.
    pub fn fire_count(&self, trigger_id: &str) -> Option<u64> {
        let graph = self.graph.as_ref()?;
        graph
            .triggers
            .iter()
            .position(|t| t.trigger_id == trigger_id)
            .map(|index| self.fire_counts[index])
    }

    fn set_variable(&mut self, name: &str, value: &TriggerValue) -> Result<(), GameRuntimeError> {
        let index = self
            .variables
            .binary_search_by(|(n, _)| n.as_str().cmp(name))
            .map_err(|_| GameRuntimeError::Invalid {
                field: "session.trigger.set_variable".to_string(),
                reason: format!("undeclared variable {name} (lowering must reject this)"),
            })?;
        self.variables[index].1 = value.clone();
        Ok(())
    }

    fn int_operand(&self, operand: &IntOperand) -> Result<i64, GameRuntimeError> {
        match operand {
            IntOperand::Literal(value) => Ok(*value),
            IntOperand::Variable(name) => match self.variable(name) {
                Some(TriggerValue::Integer(value)) => Ok(*value),
                other => Err(GameRuntimeError::Invalid {
                    field: "session.trigger.int_operand".to_string(),
                    reason: format!("variable {name} is not an integer ({other:?})"),
                }),
            },
        }
    }

    fn flag_operand(&self, operand: &FlagOperand) -> Result<bool, GameRuntimeError> {
        match operand {
            FlagOperand::Literal(value) => Ok(*value),
            FlagOperand::Variable(name) => match self.variable(name) {
                Some(TriggerValue::Flag(value)) => Ok(*value),
                other => Err(GameRuntimeError::Invalid {
                    field: "session.trigger.flag_operand".to_string(),
                    reason: format!("variable {name} is not a flag ({other:?})"),
                }),
            },
        }
    }

    /// Fold every mutable trigger cursor into the session hash. Removing this call
    /// is the "trigger state stops being hashed" determinism hole the
    /// mutation-falsifiable test detects.
    pub(super) fn fold_hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(u64::from(self.graph.is_some()));
        if let Some(graph) = &self.graph {
            hasher.update(graph.graph_fingerprint.as_bytes());
        }
        hasher.update_u64_le(self.variables.len() as u64);
        for (name, value) in &self.variables {
            hasher.update_u64_le(name.len() as u64);
            hasher.update(name.as_bytes());
            value.fold_hash(hasher);
        }
        hasher.update_u64_le(self.fire_counts.len() as u64);
        for count in &self.fire_counts {
            hasher.update_u64_le(*count);
        }
        hasher.update_u64_le(self.unit_latches.len() as u64);
        for (unit, alive) in &self.unit_latches {
            hasher.update_u64_le(u64::from(*unit));
            hasher.update_u64_le(u64::from(*alive));
        }
        hasher.update_u64_le(self.region_latches.len() as u64);
        for (unit, region, inside) in &self.region_latches {
            hasher.update_u64_le(u64::from(*unit));
            hasher.update_u64_le(u64::from(*region));
            hasher.update_u64_le(u64::from(*inside));
        }
    }
}

/// The event occurrences one tick produced, collected before any trigger runs so
/// every trigger observes the SAME occurrence set regardless of its own actions.
struct TickOccurrences {
    map_init: bool,
    tick: u64,
    timers: Vec<u32>,
    deaths: Vec<u32>,
    /// Region ids that saw at least one outside->inside transition this tick.
    region_enters: Vec<u32>,
    /// Region ids that saw at least one inside->outside transition this tick.
    region_leaves: Vec<u32>,
}

impl TickOccurrences {
    fn matches(&self, event: &TriggerEvent) -> Option<&'static str> {
        match event {
            TriggerEvent::MapInitialization => self.map_init.then_some("map_initialization"),
            TriggerEvent::ElapsedGameTime { elapsed_ticks } => {
                (self.tick == *elapsed_ticks).then_some("elapsed_game_time")
            }
            TriggerEvent::CountdownTimerExpires { timer } => self
                .timers
                .contains(timer)
                .then_some("countdown_timer_expires"),
            TriggerEvent::UnitDies { unit } => self.deaths.contains(unit).then_some("unit_dies"),
            TriggerEvent::UnitEntersRegion { region } => self
                .region_enters
                .contains(region)
                .then_some("unit_enters_region"),
            TriggerEvent::UnitLeavesRegion { region } => self
                .region_leaves
                .contains(region)
                .then_some("unit_leaves_region"),
        }
    }
}

impl MatchRuntimeSession {
    /// Read surface for the live trigger state (variables, fire counts, timeline).
    pub fn triggers(&self) -> &TriggerRuntime {
        &self.triggers
    }

    /// One deterministic trigger-evaluator step, called from `tick_all` after the
    /// subsystems have advanced. Reads live state, runs every armed trigger whose
    /// events matched this tick's occurrences and whose conditions all hold, and
    /// dispatches its unit-affecting actions internally as typed orders (NOT
    /// re-logged -- the evaluator is regenerated on replay, like the #1786 AI).
    pub(super) fn trigger_step(&mut self) -> Result<(), GameRuntimeError> {
        if !self.triggers.is_active() {
            return Ok(());
        }
        let mut state = std::mem::take(&mut self.triggers);
        let result = self.trigger_step_inner(&mut state);
        self.triggers = state;
        result
    }

    fn trigger_step_inner(&mut self, state: &mut TriggerRuntime) -> Result<(), GameRuntimeError> {
        let tick = self.applied_tick;
        // --- Collect this tick's event occurrences (deterministic order). ---
        let timers: Vec<u32> = self.hud.fired_this_tick().iter().map(|f| f.timer).collect();
        let mut deaths: Vec<u32> = Vec::new();
        for (unit, last_alive) in &mut state.unit_latches {
            if *last_alive && !self.match_state.is_alive(*unit)? {
                *last_alive = false;
                deaths.push(*unit);
            }
        }
        // #1856: expand region latches to every live field unit x watched region,
        // then detect enter/leave transitions against current cell membership.
        let (region_enters, region_leaves) = self.collect_region_transitions(state)?;
        let occurrences = TickOccurrences {
            map_init: tick == 1,
            tick,
            timers,
            deaths,
            region_enters,
            region_leaves,
        };
        // --- Evaluate every trigger in lowered order. ---
        let graph = state
            .graph
            .take()
            .expect("active trigger runtime has graph");
        let mut plan: Vec<SessionOrder> = Vec::new();
        for (index, trigger) in graph.triggers.iter().enumerate() {
            if !trigger.enabled || !trigger.initially_on {
                continue;
            }
            let Some(event_name) = trigger
                .events
                .iter()
                .find_map(|event| occurrences.matches(event))
            else {
                continue;
            };
            if !self.trigger_conditions_hold(state, &trigger.conditions)? {
                continue;
            }
            state.fire_counts[index] += 1;
            state.timeline.push(TriggerFiring {
                tick,
                trigger_id: trigger.trigger_id.clone(),
                event: event_name,
            });
            for action in &trigger.actions {
                self.plan_trigger_action(state, action, &mut plan)?;
            }
        }
        state.graph = Some(graph);
        for order in plan {
            self.dispatch_nontransactional(&order)?;
        }
        Ok(())
    }

    fn trigger_conditions_hold(
        &self,
        state: &TriggerRuntime,
        conditions: &[TriggerCondition],
    ) -> Result<bool, GameRuntimeError> {
        for condition in conditions {
            let holds = match condition {
                TriggerCondition::UnitAlive { unit } => self.match_state.is_alive(*unit)?,
                TriggerCondition::IntegerCompare { op, left, right } => {
                    let left = state.int_operand(left)?;
                    let right = state.int_operand(right)?;
                    match op {
                        CompareOp::Equal => left == right,
                        CompareOp::GreaterThan => left > right,
                        CompareOp::GreaterEqual => left >= right,
                    }
                }
                TriggerCondition::FlagEquals { left, right } => {
                    state.flag_operand(left)? == state.flag_operand(right)?
                }
                TriggerCondition::UnitInRegion { unit, region } => {
                    self.unit_in_region(*unit, *region)?
                }
            };
            if !holds {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Translate one trigger action into its session mutation: variable writes hit
    /// the store directly; unit/victory effects become typed [`SessionOrder`]s.
    /// `kill_unit` on an already-dead unit is a deterministic no-op (WC3 parity:
    /// the unit may legitimately have died first), never a mid-match abort.
    fn plan_trigger_action(
        &self,
        state: &mut TriggerRuntime,
        action: &TriggerAction,
        plan: &mut Vec<SessionOrder>,
    ) -> Result<(), GameRuntimeError> {
        // #1855+ effect/fog/cinematic/spawn/weather/camera actions.
        if self.plan_effect_trigger_action(action, plan)? {
            return Ok(());
        }
        if self.plan_unit_order_trigger_action(action, plan)? {
            return Ok(());
        }
        match action {
            TriggerAction::KillUnit { unit } => {
                let already_planned = plan
                    .iter()
                    .any(|o| matches!(o, SessionOrder::DestroyUnit { unit: u } if u == unit));
                if self.match_state.is_alive(*unit)? && !already_planned {
                    plan.push(SessionOrder::DestroyUnit { unit: *unit });
                }
            }
            TriggerAction::MoveUnit {
                unit,
                cell_x,
                cell_z,
            } => {
                plan.push(SessionOrder::MoveFieldUnit {
                    field_id: *unit,
                    cell_x: *cell_x,
                    cell_z: *cell_z,
                });
            }
            TriggerAction::SetVariable { name, value } => {
                state.set_variable(name, value)?;
            }
            TriggerAction::FireTrigger { trigger } => {
                plan.push(SessionOrder::FireTrigger { trigger: *trigger });
            }
            other => {
                return Err(GameRuntimeError::Invalid {
                    field: "session.trigger.action".to_string(),
                    reason: format!("unhandled trigger action {other:?}"),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "session_trigger_tests.rs"]
mod session_trigger_tests;

#[cfg(test)]
#[path = "session_trigger_resource_tests.rs"]
mod session_trigger_resource_tests;

#[cfg(test)]
#[path = "session_trigger_worker_assignment_tests.rs"]
mod session_trigger_worker_assignment_tests;

#[cfg(test)]
#[path = "session_trigger_economy_tests.rs"]
mod session_trigger_economy_tests;

#[cfg(test)]
#[path = "session_trigger_production_tests.rs"]
mod session_trigger_production_tests;

#[cfg(test)]
#[path = "session_trigger_supply_tests.rs"]
mod session_trigger_supply_tests;

#[cfg(test)]
#[path = "session_trigger_harvest_tests.rs"]
mod session_trigger_harvest_tests;

#[cfg(test)]
#[path = "session_trigger_transaction_tests.rs"]
mod session_trigger_transaction_tests;

#[cfg(test)]
#[path = "session_trigger_cancel_tests.rs"]
mod session_trigger_cancel_tests;

#[cfg(test)]
#[path = "session_trigger_alliance_tests.rs"]
mod session_trigger_alliance_tests;

#[cfg(test)]
#[path = "session_trigger_defeat_tests.rs"]
mod session_trigger_defeat_tests;

#[cfg(test)]
#[path = "session_trigger_ability_tests.rs"]
mod session_trigger_ability_tests;

#[cfg(test)]
#[path = "session_trigger_damage_tests.rs"]
mod session_trigger_damage_tests;

#[cfg(test)]
#[path = "session_trigger_group_tests.rs"]
mod session_trigger_group_tests;

#[cfg(test)]
#[path = "session_trigger_selection_tests.rs"]
mod session_trigger_selection_tests;

#[cfg(test)]
#[path = "session_trigger_spawn_tests.rs"]
mod session_trigger_spawn_tests;

#[cfg(test)]
#[path = "session_trigger_dialog_tests.rs"]
mod session_trigger_dialog_tests;

#[cfg(test)]
#[path = "session_trigger_timer_tests.rs"]
mod session_trigger_timer_tests;
