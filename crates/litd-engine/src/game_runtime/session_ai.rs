//! #1786 macro-AI order-source: [`AiDirector`] emits typed [`super::SessionOrder`]s each
//! tick from fog-visible state only (no omniscient cheat). Cursors fold into the session
//! hash. #1860 strikes use [`super::SessionOrder::ApplyDamage`] (multi-hit HP), not
//! instant `DestroyUnit`. M1: single economy per slot; blueprint-derived enemy knowledge
//! still gated by `fog.is_visible` before any strike.

use litd_hash::Fnv1a64;

use super::super::ai_profile::{AI_HANDICAP_UNITY_PERMILLE, LoweredAiProfile};
use super::super::datasets::LoweredRuntimeConstants;
use super::super::lowered::LoweredEconomyRuleset;
use super::GameRuntimeError;
use super::{MatchRuntimeSession, SessionOrder};

/// One key AI decision, retained for the FSV/log tick timeline. Deterministic (a pure
/// function of the hashed cursors), so it is not itself hashed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiTimelineEvent {
    pub tick: u64,
    pub player: u8,
    pub kind: &'static str,
    pub detail: String,
}

/// All authored macro-AI opponents composed into one deterministic order-source.
#[derive(Debug, Default, Clone)]
pub struct AiDirector {
    executors: Vec<AiExecutor>,
    timeline: Vec<AiTimelineEvent>,
}

impl AiDirector {
    /// Build the director from the lowered profiles + the verified blueprint units +
    /// economy costs. Ordered by controlled player for a stable iteration/hash.
    /// `owner_forces[slot]` is the force (team) for that dense player slot (#836/#1788 2v2).
    pub(super) fn new(
        profiles: &[LoweredAiProfile],
        blueprint_units: &[(u32, u8, bool)], // (unit, owner, is_structure)
        owner_forces: &[u8],
        economy: &LoweredEconomyRuleset,
        constants: &LoweredRuntimeConstants,
    ) -> Result<Self, GameRuntimeError> {
        let mut executors = Vec::with_capacity(profiles.len());
        for profile in profiles {
            executors.push(AiExecutor::new(
                profile.clone(),
                blueprint_units,
                owner_forces,
                economy,
                constants,
            )?);
        }
        executors.sort_by_key(|e| e.profile.controlled_player);
        Ok(Self {
            executors,
            timeline: Vec::new(),
        })
    }

    pub fn timeline(&self) -> &[AiTimelineEvent] {
        &self.timeline
    }

    pub fn is_active(&self) -> bool {
        !self.executors.is_empty()
    }

    /// Fold every executor's mutable cursors into the session hash. Removing this call is
    /// the "AI state stops being hashed" mutation the determinism test falsifies.
    pub(super) fn fold_hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(self.executors.len() as u64);
        for exec in &self.executors {
            exec.fold_hash(hasher);
        }
    }
}

/// The per-profile runtime executor: immutable authored plan + derived caches + the mutable
/// cursors that are the AI's live state.
#[derive(Debug, Clone)]
struct AiExecutor {
    profile: LoweredAiProfile,
    own_fields: Vec<u32>,
    enemy_structs: Vec<u32>,
    home_cell: (i32, i32),
    enemy_cell: (i32, i32),
    army_gold: u64,
    army_lumber: u64,
    army_food: u32,
    build_costs: Vec<(u64, u64)>,
    build_trains_army: Vec<bool>,
    // Mutable cursors (the hashed AI state):
    build_cursor: usize,
    workers_spawned: u32,
    army_trained: u32,
    army_structure_instance: Option<u32>,
    next_predicted_instance: u32,
    wave_sent: bool,
    /// #1858: index into `profile.waves` (0 when legacy single-wave).
    wave_cursor: usize,
    /// Tick the current wave was WAVE_SENT (for regroup).
    wave_sent_tick: u64,
    strikes_done: u32,
    retreating: bool,
    resume_tick: u64,
    retreat_count: u32,
    retreat_latch: bool,
    supply_block_noted: bool,
}

impl AiExecutor {
    fn new(
        profile: LoweredAiProfile,
        blueprint_units: &[(u32, u8, bool)],
        owner_forces: &[u8],
        economy: &LoweredEconomyRuleset,
        constants: &LoweredRuntimeConstants,
    ) -> Result<Self, GameRuntimeError> {
        let cp = profile.controlled_player;
        let cp_force = owner_forces.get(usize::from(cp)).copied().ok_or_else(|| {
            GameRuntimeError::Invalid {
                field: "ai.controlled_player".to_string(),
                reason: format!("controlled player {cp} has no force entry"),
            }
        })?;
        let is_enemy = |owner: u8| -> bool {
            owner != cp
                && owner_forces
                    .get(usize::from(owner))
                    .is_some_and(|f| *f != cp_force)
        };
        let own_fields: Vec<u32> = blueprint_units
            .iter()
            .filter(|(_, owner, is_struct)| *owner == cp && !*is_struct)
            .map(|(unit, _, _)| *unit)
            .collect();
        // #836/#1788: only cross-force structures are enemies (never strike allies).
        let enemy_structs: Vec<u32> = blueprint_units
            .iter()
            .filter(|(_, owner, is_struct)| is_enemy(*owner) && *is_struct)
            .map(|(unit, _, _)| *unit)
            .collect();
        let enemy_owner = blueprint_units
            .iter()
            .filter(|(_, owner, _)| is_enemy(*owner))
            .map(|(_, owner, _)| *owner)
            .min()
            .unwrap_or(cp);
        let army =
            economy
                .unit(&profile.army_unit_id)
                .ok_or_else(|| GameRuntimeError::Invalid {
                    field: "ai.army_unit".to_string(),
                    reason: format!("army unit {} absent from economy", profile.army_unit_id),
                })?;
        let mut build_costs = Vec::with_capacity(profile.build_order.len());
        let mut build_trains_army = Vec::with_capacity(profile.build_order.len());
        for step in &profile.build_order {
            let def =
                economy
                    .structure(&step.struct_id)
                    .ok_or_else(|| GameRuntimeError::Invalid {
                        field: "ai.build_step".to_string(),
                        reason: format!("structure {} absent from economy", step.struct_id),
                    })?;
            build_costs.push((u64::from(def.gold_cost), u64::from(def.lumber_cost)));
            build_trains_army.push(def.trains.iter().any(|t| t == &profile.army_unit_id));
        }
        Ok(Self {
            own_fields,
            enemy_structs,
            home_cell: super::start_cell(cp, constants),
            enemy_cell: super::start_cell(enemy_owner, constants),
            army_gold: u64::from(army.gold_cost),
            army_lumber: u64::from(army.lumber_cost),
            army_food: army.food_cost,
            build_costs,
            build_trains_army,
            profile,
            build_cursor: 0,
            workers_spawned: 0,
            army_trained: 0,
            army_structure_instance: None,
            next_predicted_instance: 0,
            wave_sent: false,
            wave_cursor: 0,
            wave_sent_tick: 0,
            strikes_done: 0,
            retreating: false,
            resume_tick: 0,
            retreat_count: 0,
            retreat_latch: false,
            supply_block_noted: false,
        })
    }

    fn fold_hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u8(self.profile.controlled_player);
        hasher.update_u64_le(self.build_cursor as u64);
        hasher.update_u64_le(u64::from(self.workers_spawned));
        hasher.update_u64_le(u64::from(self.army_trained));
        hasher.update_u64_le(self.army_structure_instance.map_or(u64::MAX, u64::from));
        hasher.update_u64_le(u64::from(self.next_predicted_instance));
        hasher.update_u8(u8::from(self.wave_sent));
        hasher.update_u64_le(self.wave_cursor as u64);
        hasher.update_u64_le(self.wave_sent_tick);
        hasher.update_u64_le(u64::from(self.strikes_done));
        hasher.update_u8(u8::from(self.retreating));
        hasher.update_u64_le(self.resume_tick);
        hasher.update_u64_le(u64::from(self.retreat_count));
        hasher.update_u8(u8::from(self.retreat_latch));
        hasher.update_u8(u8::from(self.supply_block_noted));
    }

    fn worker_id(&self) -> u32 {
        1_000_000 + u32::from(self.profile.controlled_player) * 1000 + self.workers_spawned
    }

    /// #1858: true when the schedule still has a wave to mass/send at this cursor.
    fn wave_active(&self) -> bool {
        if self.profile.has_wave_schedule() {
            self.wave_cursor < self.profile.waves.len()
        } else {
            true // legacy single wave always "active" until match ends
        }
    }

    fn army_target(&self) -> u32 {
        self.profile.cumulative_army_target(self.wave_cursor)
    }

    fn wave_ready_at_tick(&self) -> u64 {
        if self.profile.has_wave_schedule() {
            self.profile
                .waves
                .get(self.wave_cursor)
                .map(|w| w.at_tick)
                .unwrap_or(u64::MAX)
        } else {
            0
        }
    }
}

impl AiDirector {
    /// Test-only mutation of a live AI cursor, used by the mutation-falsifiable determinism
    /// test: bumping it MUST change `combined_state_hash`. If the AI fold is severed from the
    /// hash, the bump becomes invisible and that test fails -- which is the point.
    #[cfg(test)]
    pub(crate) fn debug_bump_cursor(&mut self) -> bool {
        match self.executors.first_mut() {
            Some(exec) => {
                exec.build_cursor = exec.build_cursor.wrapping_add(1);
                true
            }
            None => false,
        }
    }
}

impl MatchRuntimeSession {
    /// One deterministic macro-AI step, called from `tick_all` after the subsystems have
    /// advanced. Reads live state, emits typed orders, dispatches them internally (they are
    /// NOT logged -- the executor is regenerated on replay). Fail-loud on any rejected order.
    pub(super) fn ai_step(&mut self) -> Result<(), GameRuntimeError> {
        if !self.ai.is_active() {
            return Ok(());
        }
        let mut director = std::mem::take(&mut self.ai);
        let mut plan: Vec<SessionOrder> = Vec::new();
        // Plan each executor from an immutable view of self, recording timeline events.
        let tick = self.applied_tick;
        for exec in &mut director.executors {
            self.plan_executor(exec, tick, &mut director.timeline, &mut plan)?;
        }
        for order in plan {
            self.dispatch_nontransactional(&order)?;
        }
        self.ai = director;
        Ok(())
    }

    fn plan_executor(
        &self,
        exec: &mut AiExecutor,
        tick: u64,
        timeline: &mut Vec<AiTimelineEvent>,
        plan: &mut Vec<SessionOrder>,
    ) -> Result<(), GameRuntimeError> {
        if !tick.is_multiple_of(u64::from(exec.profile.decision_interval_ticks)) {
            return Ok(());
        }
        let cp = exec.profile.controlled_player;
        let mut note = |kind: &'static str, detail: String| {
            timeline.push(AiTimelineEvent {
                tick,
                player: cp,
                kind,
                detail,
            });
        };

        // --- Retreat state machine: react to losing economy (own structures). ---
        if exec.retreating && tick >= exec.resume_tick {
            exec.retreating = false;
            exec.wave_sent = false; // re-form + re-send the assault after regrouping
            note(
                "RESUME",
                format!("regroup complete; resuming assault at tick {tick}"),
            );
        }
        if exec.profile.retreat_when_structures_below > 0 && !exec.retreating && !exec.retreat_latch
        {
            let own = self.match_state.structure_count(cp)?;
            if own < exec.profile.retreat_when_structures_below {
                exec.retreating = true;
                exec.retreat_latch = true;
                exec.retreat_count += 1;
                exec.resume_tick = tick + u64::from(exec.profile.retreat_cooldown_ticks);
                for field in &exec.own_fields {
                    plan.push(SessionOrder::MoveFieldUnit {
                        field_id: *field,
                        cell_x: exec.home_cell.0,
                        cell_z: exec.home_cell.1,
                    });
                }
                note(
                    "RETREAT",
                    format!(
                        "structures {own} < {}; pulling back to base, resume at {}",
                        exec.profile.retreat_when_structures_below, exec.resume_tick
                    ),
                );
                return Ok(()); // skip other actions this tick
            }
        }

        // --- Authorized resource handicap (flat per-decision trickle; 1000 = none). ---
        if exec.profile.handicap_permille > AI_HANDICAP_UNITY_PERMILLE {
            let bonus = u64::from(exec.profile.handicap_permille - AI_HANDICAP_UNITY_PERMILLE);
            plan.push(SessionOrder::GrantResource {
                key: "gold".to_string(),
                amount: bonus,
            });
        }

        if exec.retreating {
            return Ok(());
        }

        // --- Economy: grow workers to target. ---
        if exec.workers_spawned < exec.profile.worker_target {
            let worker_id = exec.worker_id();
            plan.push(SessionOrder::AddWorker {
                worker_id,
                node_id: exec.profile.worker_node_id.clone(),
            });
            exec.workers_spawned += 1;
            note("WORKER", format!("harvester {worker_id} to node"));
        }

        // --- Build order: place next structure when gold-gated + affordable. ---
        if exec.build_cursor < exec.profile.build_order.len() {
            let step = &exec.profile.build_order[exec.build_cursor];
            let (gold_cost, lumber_cost) = exec.build_costs[exec.build_cursor];
            let accounts = self.economy.accounts();
            if accounts.balance("gold") >= step.at_gold
                && accounts.can_afford("gold", gold_cost)
                && accounts.can_afford("lumber", lumber_cost)
            {
                plan.push(SessionOrder::PlaceStructure {
                    struct_id: step.struct_id.clone(),
                    x: step.cell_x,
                    z: step.cell_z,
                });
                if exec.build_trains_army[exec.build_cursor]
                    && exec.army_structure_instance.is_none()
                {
                    exec.army_structure_instance = Some(exec.next_predicted_instance);
                }
                note(
                    "BUILD",
                    format!(
                        "place {} (instance {})",
                        step.struct_id, exec.next_predicted_instance
                    ),
                );
                exec.next_predicted_instance += 1;
                exec.build_cursor += 1;
            }
        }

        // Train / wave / strike / multi-wave advance (#1786/#1858/#1860).
        self.plan_assault(exec, tick, &mut note, plan)?;
        Ok(())
    }
}

#[path = "session_ai_assault.rs"]
mod session_ai_assault;

#[cfg(test)]
#[path = "session_ai_tests.rs"]
mod session_ai_tests;
