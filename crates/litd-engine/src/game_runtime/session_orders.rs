//! #1843: the deterministic order surface for [`super::MatchRuntimeSession`].
//!
//! Every session mutation is a [`SessionOrder`]. Applying one records it in the session's
//! replay log and dispatches it to the owning subsystem. `Tick` is itself an order, so the
//! whole match -- economy ticks, day/night advance, fog recompute, HUD step -- is fully
//! reconstructable by replaying the order stream (the save/load + two-execution replay
//! determinism spine, #85).

use serde::{Deserialize, Serialize};

use crate::math::{Fixed, FixedVec3};

use super::super::groups::UnitOrder;
use super::super::players::AllianceState;
use super::super::structures::RallyPoint;
use super::GameRuntimeError;
use super::MatchRuntimeSession;

/// One deterministic session mutation. Serde-derived so the applied stream persists in a
/// [`super::SessionSave`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionOrder {
    /// Spawn a harvesting worker on a resource node (#1785).
    AddWorker { worker_id: u32, node_id: String },
    /// Reassign an existing worker to an authored resource node.
    AssignWorker { worker_id: u32, node_id: String },
    /// Place a structure at a grid cell; the deterministic instance id is `n`th placed.
    PlaceStructure { struct_id: String, x: i32, z: i32 },
    /// Cancel an incomplete deterministic structure instance and refund its costs.
    CancelStructure { instance_id: u32 },
    /// Point a completed structure's rally at a cell or a live unit body (#1843 rally fix).
    SetRally { instance_id: u32, rally: RallyPoint },
    /// Queue a unit for training on a completed structure (#1785).
    TrainUnit { instance_id: u32, unit_id: String },
    /// Directly grant a resource balance (trigger API, #837).
    GrantResource { key: String, amount: u64 },
    /// Spend an exact resource amount, failing atomically when funds are insufficient.
    SpendResource { key: String, amount: u64 },
    /// Change an alliance (#836). `ally=true` -> `Ally`, else `Enemy`. Shared-vision flips
    /// what the fog system reveals across the pair on the next recompute.
    SetAlliance {
        from: u8,
        to: u8,
        ally: bool,
        shared_vision: bool,
        shared_control: bool,
    },
    /// Move a field unit to a new cell; fog updates for it on the next `Tick` (#847).
    MoveFieldUnit {
        field_id: u32,
        cell_x: i32,
        cell_z: i32,
    },
    /// Add a match unit to a player's persistent named group (#851).
    AddToGroup { player: u8, unit: u32 },
    /// Replace a player's selection (#851).
    SelectUnits { player: u8, units: Vec<u32> },
    /// Issue a point-target move order to a set of units (#851). Validated against each
    /// unit's lowered capabilities + ownership.
    IssueMove {
        player: u8,
        units: Vec<u32>,
        ability: u16,
        x: i32,
        z: i32,
        queued: bool,
    },
    /// #1894: MOBA-style ability bar no-target cast (QWER slots -> ability id).
    /// Validated against selection ownership + unit capability set.
    IssueAbilityNoTarget {
        player: u8,
        units: Vec<u32>,
        ability: u16,
        queued: bool,
    },
    /// Destroy a match unit/structure, flipping victory/defeat in melee (#838).
    /// Scripted / trigger kills; combat uses [`SessionOrder::ApplyDamage`].
    DestroyUnit { unit: u32 },
    /// #1860: apply engagement damage to a live unit. Amount must be > 0. When
    /// HP reaches 0 the unit is destroyed through the same victory path as
    /// `DestroyUnit`. This is the real combat resolution surface (replaces the
    /// AI's prior instant-kill strike scaffold).
    ApplyDamage { target: u32, amount_milli: u32 },
    /// Set the day/night position within the current day (#848 env order).
    SetTimeOfDay { tick_in_day: u64 },
    /// Start a deterministic HUD countdown timer (#841/#1851). Timer ids are
    /// assigned sequentially by the timer set, so identical order streams produce
    /// identical ids. Expiries feed the trigger evaluator's event stream.
    StartTimer { one_shot: bool, period_ticks: u64 },
    /// Fire a victory custom-condition trigger (#838/#1851), then re-evaluate
    /// victory. The `fire_trigger` trigger action dispatches this internally.
    FireTrigger { trigger: u32 },
    /// #1855/#839: create a quest in the runtime quest log. Fails loud on a
    /// duplicate id or empty title/description (sabotaged quest data).
    QuestCreate {
        quest: u32,
        title: String,
        description: String,
        icon: String,
        required: bool,
        required_flags: Vec<u32>,
    },
    /// #1855/#839: mark a quest discovered (WC3 `QuestSetDiscovered`).
    QuestDiscover { quest: u32 },
    /// #1855/#839: record a required quest flag as satisfied.
    QuestSatisfyFlag { flag: u32 },
    /// #1855/#839: complete a quest (only if every required flag is satisfied).
    QuestComplete { quest: u32 },
    /// #1855/#839: fail an active quest (WC3 `QuestSetFailed`).
    QuestFail { quest: u32 },
    /// #1855/#841: pause a running countdown timer.
    PauseTimer { timer: u32 },
    /// #1855/#841: resume a paused countdown timer.
    ResumeTimer { timer: u32 },
    /// #1855/#841: bind/retitle an on-screen timer window (WC3 `CreateTimerDialog`).
    CreateTimerWindow { timer: u32, title: String },
    /// #1855/#842: create a modal dialog. Ids are assigned sequentially by the
    /// dialog set, so identical order streams produce identical ids.
    CreateDialog { title: String },
    /// #1855/#842: add a button to a dialog (WC3 `DialogAddButton`).
    AddDialogButton {
        dialog: u32,
        text: String,
        hotkey: u32,
    },
    /// #1855/#842: show a dialog to a player-mask set (WC3 `DialogDisplay`).
    ShowDialog { dialog: u32, players: u64 },
    /// #1855/#842: hide a dialog from everyone.
    HideDialog { dialog: u32 },
    /// #1855/#843: spawn a world-anchored floating text tag (WC3 `CreateTextTag`).
    SpawnFloatingText {
        text: String,
        position_mm: [i64; 3],
        velocity_mm_per_tick: [i64; 3],
        color: [u8; 4],
        lifespan_ticks: u64,
        fade_start_ticks: u64,
    },
    /// #1855/#838: force a player to Defeat (the `fire_trigger`-victory
    /// counterpart), then re-evaluate for melee cascades.
    DefeatPlayer { player: u8 },
    /// #1857/#847: reveal a rectangular region for a player (WC3 fog modifier).
    /// Adds a deduped session-held modifier, then recomputes fog. Inclusive cells.
    RevealRegion {
        player: u32,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
    },
    /// #1857/#847: conceal a rectangular region for a player. Conceal wins over
    /// reveal on the same cell in the fog recompute.
    ConcealRegion {
        player: u32,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
    },
    /// #1859: spawn a field unit at a cell, validated against a lowered economy
    /// unit record (`unit_key`). Allocates a deterministic unit id.
    SpawnFieldUnit {
        unit_key: String,
        owner: u8,
        cell_x: i32,
        cell_z: i32,
    },
    /// #1854: start a scripted cinematic. Deterministic side records the gate
    /// (`pauses_sim`); presentation host resolves the timeline and drives
    /// `CinematicPlayer`. Empty `timeline_id` fails loud.
    PlayCinematic {
        timeline_id: String,
        pauses_sim: bool,
    },
    /// #1854: player-skip / end the active cinematic gate (idempotent no-op when
    /// none is active).
    SkipCinematic,
    /// #1861: set weather (deterministic env order, folds into time-of-day hash).
    SetWeather {
        ordinal: u32,
        intensity_permille: u16,
    },
    /// #1861: bind the host camera to follow a live field unit (presentation
    /// intent; recorded in the order log for scripted escort framing).
    CameraFollowUnit { unit: u32 },
    /// #1861: clear the camera follow binding.
    CameraClearFollow,
    /// Advance the whole runtime one deterministic tick.
    Tick,
}

/// The per-order outcome the host/FSV observes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionStepReport {
    pub applied_tick: u64,
    pub combined_state_hash: u64,
    pub is_night: bool,
    pub sight_radius_input_mm: u32,
}

impl MatchRuntimeSession {
    /// Apply one order transactionally, then record it in the replay log. A rejected
    /// dispatch is reconstructed from the committed log before its error is returned.
    pub fn apply(&mut self, order: SessionOrder) -> Result<SessionStepReport, GameRuntimeError> {
        let expected_hash = self.combined_state_hash();
        if let Err(error) = self.dispatch_nontransactional(&order) {
            return Err(self.rollback_dispatch_failure(error, expected_hash));
        }
        self.order_log.push(order);
        Ok(self.step_report())
    }

    fn step_report(&self) -> SessionStepReport {
        SessionStepReport {
            applied_tick: self.applied_tick(),
            combined_state_hash: self.combined_state_hash(),
            is_night: self.time_of_day().is_night(),
            sight_radius_input_mm: self.time_of_day().sight_radius_input_mm(),
        }
    }

    pub(super) fn dispatch_nontransactional(
        &mut self,
        order: &SessionOrder,
    ) -> Result<(), GameRuntimeError> {
        match order {
            SessionOrder::AddWorker { worker_id, node_id } => {
                self.economy.add_worker(*worker_id, node_id)?;
            }
            SessionOrder::AssignWorker { worker_id, node_id } => {
                self.economy.reassign_worker(*worker_id, node_id)?;
            }
            SessionOrder::PlaceStructure { struct_id, x, z } => {
                self.economy.place_structure(struct_id, *x, *z)?;
            }
            SessionOrder::CancelStructure { instance_id } => {
                self.economy.cancel_structure(*instance_id)?;
            }
            SessionOrder::SetRally { instance_id, rally } => {
                self.economy.set_rally(*instance_id, *rally)?;
            }
            SessionOrder::TrainUnit {
                instance_id,
                unit_id,
            } => {
                self.economy.train_unit(*instance_id, unit_id)?;
            }
            SessionOrder::GrantResource { key, amount } => {
                self.economy.grant(key, *amount)?;
            }
            SessionOrder::SpendResource { key, amount } => {
                self.economy.spend(key, *amount)?;
            }
            SessionOrder::SetAlliance {
                from,
                to,
                ally,
                shared_vision,
                shared_control,
            } => {
                let state = if *ally {
                    AllianceState::Ally
                } else {
                    AllianceState::Enemy
                };
                self.match_state.set_alliance(
                    *from,
                    *to,
                    state,
                    *shared_vision,
                    *shared_control,
                )?;
            }
            SessionOrder::MoveFieldUnit {
                field_id,
                cell_x,
                cell_z,
            } => {
                self.move_field_unit(*field_id, *cell_x, *cell_z)?;
            }
            SessionOrder::AddToGroup { player, unit } => {
                self.match_state.add_to_group(*player, *unit)?;
            }
            SessionOrder::SelectUnits { player, units } => {
                self.match_state.select(*player, units)?;
            }
            SessionOrder::IssueMove {
                player,
                units,
                ability,
                x,
                z,
                queued,
            } => {
                let target = FixedVec3::new(Fixed::from_i32(*x), Fixed::ZERO, Fixed::from_i32(*z));
                let order = UnitOrder::point(*ability, target, *queued);
                self.match_state.issue_order(*player, units, order)?;
            }
            SessionOrder::IssueAbilityNoTarget {
                player,
                units,
                ability,
                queued,
            } => {
                let order = UnitOrder::no_target(*ability, *queued);
                self.match_state.issue_order(*player, units, order)?;
            }
            SessionOrder::DestroyUnit { unit } => {
                self.match_state.destroy_unit(*unit)?;
            }
            SessionOrder::ApplyDamage {
                target,
                amount_milli,
            } => {
                self.apply_damage_with_number(*target, *amount_milli)?;
            }
            SessionOrder::SetTimeOfDay { tick_in_day } => {
                self.time_of_day.apply(
                    super::super::time_of_day::RuntimeEnvOrderKind::SetTimeOfDay {
                        tick_in_day: *tick_in_day,
                    },
                )?;
            }
            SessionOrder::StartTimer {
                one_shot,
                period_ticks,
            } => {
                let kind = if *one_shot {
                    super::super::hud::TimerKind::OneShot
                } else {
                    super::super::hud::TimerKind::Periodic
                };
                let now = self.hud.tick();
                self.hud.timers.start_timer(kind, *period_ticks, now)?;
            }
            SessionOrder::FireTrigger { trigger } => {
                self.match_state.fire_trigger(*trigger)?;
            }
            SessionOrder::QuestCreate {
                quest,
                title,
                description,
                icon,
                required,
                required_flags,
            } => {
                self.hud.quests.create_quest(
                    *quest,
                    title,
                    description,
                    icon,
                    *required,
                    required_flags,
                )?;
            }
            SessionOrder::QuestDiscover { quest } => {
                let tick = self.hud.tick();
                self.hud.quests.discover_quest(*quest, tick)?;
            }
            SessionOrder::QuestSatisfyFlag { flag } => {
                self.hud.quests.satisfy_flag(*flag);
            }
            SessionOrder::QuestComplete { quest } => {
                let tick = self.hud.tick();
                self.hud.quests.try_complete(*quest, tick)?;
            }
            SessionOrder::QuestFail { quest } => {
                let tick = self.hud.tick();
                self.hud.quests.fail_quest(*quest, tick)?;
            }
            SessionOrder::PauseTimer { timer } => {
                self.hud.timers.pause_timer(*timer)?;
            }
            SessionOrder::ResumeTimer { timer } => {
                self.hud.timers.resume_timer(*timer)?;
            }
            SessionOrder::CreateTimerWindow { timer, title } => {
                self.hud.timers.create_window(*timer, title)?;
            }
            SessionOrder::CreateDialog { title } => {
                self.hud.dialogs.create_dialog(title)?;
            }
            SessionOrder::AddDialogButton {
                dialog,
                text,
                hotkey,
            } => {
                self.hud.dialogs.add_button(*dialog, text, *hotkey)?;
            }
            SessionOrder::ShowDialog { dialog, players } => {
                self.hud
                    .dialogs
                    .show(*dialog, super::super::hud::PlayerMask::from_bits(*players))?;
            }
            SessionOrder::HideDialog { dialog } => {
                self.hud.dialogs.hide(*dialog)?;
            }
            SessionOrder::SpawnFloatingText { .. } => {
                self.spawn_floating_text(order)?;
            }
            SessionOrder::DefeatPlayer { player } => {
                self.match_state.defeat_player(*player)?;
            }
            SessionOrder::RevealRegion { .. } | SessionOrder::ConcealRegion { .. } => {
                self.apply_visibility_modifier(order)?;
            }
            SessionOrder::SpawnFieldUnit { .. } => {
                self.spawn_field_unit(order)?;
            }
            SessionOrder::PlayCinematic { .. } | SessionOrder::SkipCinematic => {
                self.apply_cinematic_order(order)?;
            }
            SessionOrder::SetWeather {
                ordinal,
                intensity_permille,
            } => {
                self.time_of_day.apply(
                    super::super::time_of_day::RuntimeEnvOrderKind::SetWeather {
                        ordinal: *ordinal,
                        intensity_permille: *intensity_permille,
                    },
                )?;
            }
            SessionOrder::CameraFollowUnit { unit } => {
                self.set_camera_follow(Some(*unit))?;
            }
            SessionOrder::CameraClearFollow => {
                self.set_camera_follow(None)?;
            }
            SessionOrder::Tick => {
                self.tick_all()?;
            }
        }
        Ok(())
    }

    /// One deterministic tick of every subsystem, in fixed order.
    fn tick_all(&mut self) -> Result<(), GameRuntimeError> {
        self.economy.tick()?;
        self.match_state.advance_tick()?;
        self.time_of_day.advance_tick()?;
        self.hud.run_ticks(1)?;
        self.applied_tick = self
            .applied_tick
            .checked_add(1)
            .ok_or(GameRuntimeError::Sim(crate::EngineError::SimTickOverflow))?;
        // #1851: the ECA trigger evaluator runs AFTER the subsystems advance (so it
        // observes this tick's timer expiries and unit deaths) and BEFORE the macro-AI
        // and fog recompute, a fixed position in the deterministic tick order.
        self.trigger_step()?;
        // #1786: the macro-AI acts as a deterministic order-source AFTER the subsystems
        // advance (so it observes fresh state) and BEFORE fog recompute (so its field
        // moves are reflected in vision this tick).
        self.ai_step()?;
        self.recompute_fog()?;
        Ok(())
    }

    fn move_field_unit(
        &mut self,
        field_id: u32,
        cell_x: i32,
        cell_z: i32,
    ) -> Result<(), GameRuntimeError> {
        let unit = self
            .field_units
            .iter_mut()
            .find(|u| u.id == field_id)
            .ok_or_else(|| GameRuntimeError::Invalid {
                field: "session.move_field_unit".to_string(),
                reason: format!("no field unit {field_id}"),
            })?;
        unit.cell_x = cell_x;
        unit.cell_z = cell_z;
        Ok(())
    }
}
