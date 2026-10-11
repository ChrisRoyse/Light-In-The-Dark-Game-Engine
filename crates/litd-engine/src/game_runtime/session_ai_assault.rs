//! #1858/#1860 assault planning: train → WAVE_SENT → fog-gated ApplyDamage → WAVE_ADVANCE.
//! Split from `session_ai.rs` for the <500-line ratchet.

// session_ai is #[path] under session; this file is a child of session_ai → climb to game_runtime.
use super::super::super::ai_profile::AiWaveTargetKind;
use super::super::super::groups::OrderTargetKind;
use super::AiExecutor;
use super::GameRuntimeError;
use super::{MatchRuntimeSession, SessionOrder};

impl MatchRuntimeSession {
    /// Train to the cumulative wave target, send the wave when massed, strike, then
    /// advance the multi-wave cursor after regroup (#1858).
    pub(super) fn plan_assault<F>(
        &self,
        exec: &mut AiExecutor,
        tick: u64,
        note: &mut F,
        plan: &mut Vec<SessionOrder>,
    ) -> Result<(), GameRuntimeError>
    where
        F: FnMut(&'static str, String),
    {
        let army_target = exec.army_target();
        if exec.wave_active()
            && exec.army_trained < army_target
            && tick >= exec.wave_ready_at_tick()
            && let Some(instance) = exec.army_structure_instance
            && self.economy.structure(instance).is_some_and(|s| s.complete)
        {
            let accounts = self.economy.accounts();
            if accounts.would_exceed_supply(exec.army_food) {
                if !exec.supply_block_noted {
                    exec.supply_block_noted = true;
                    note(
                        "SUPPLY_BLOCKED",
                        format!(
                            "training {} blocked at supply cap {}",
                            exec.profile.army_unit_id,
                            accounts.supply_cap()
                        ),
                    );
                }
            } else if accounts.can_afford("gold", exec.army_gold)
                && accounts.can_afford("lumber", exec.army_lumber)
            {
                plan.push(SessionOrder::TrainUnit {
                    instance_id: instance,
                    unit_id: exec.profile.army_unit_id.clone(),
                });
                exec.army_trained += 1;
                exec.supply_block_noted = false;
                note(
                    "TRAIN",
                    format!("army {} #{}", exec.profile.army_unit_id, exec.army_trained),
                );
            }
        }

        let wave_size = exec.profile.wave_size_at(exec.wave_cursor);
        if exec.wave_active()
            && army_target > 0
            && exec.army_trained >= army_target
            && self.economy.unit_body_count() as u64 >= u64::from(wave_size.max(1))
            && !exec.wave_sent
            && !exec.own_fields.is_empty()
            && tick >= exec.wave_ready_at_tick()
        {
            self.emit_wave(exec, plan);
            exec.wave_sent = true;
            exec.wave_sent_tick = tick;
            note(
                "WAVE_SENT",
                format!(
                    "wave {} size {} massed; {} field units advance to {:?}",
                    exec.wave_cursor,
                    wave_size,
                    exec.own_fields.len(),
                    exec.enemy_cell
                ),
            );
        }

        // #1860: fog-gated ApplyDamage (multi-hit HP).
        if exec.wave_sent {
            match exec.profile.wave_target_kind {
                AiWaveTargetKind::VisibleEnemyStructure => {
                    let cp = exec.profile.controlled_player;
                    if self
                        .fog
                        .is_visible(u32::from(cp), exec.enemy_cell.0, exec.enemy_cell.1)
                    {
                        let damage = self.constants.strike_damage_milli;
                        for target in &exec.enemy_structs {
                            if self.match_state.is_alive(*target)? {
                                let hp_before = self.match_state.hp_milli(*target)?;
                                plan.push(SessionOrder::ApplyDamage {
                                    target: *target,
                                    amount_milli: damage,
                                });
                                exec.strikes_done += 1;
                                let will_kill = damage >= hp_before;
                                note(
                                    "STRIKE",
                                    format!(
                                        "wave {} damages structure {target} for {damage} milli (hp {hp_before}->{}); kill={}",
                                        exec.wave_cursor,
                                        hp_before.saturating_sub(damage),
                                        will_kill
                                    ),
                                );
                                break;
                            }
                        }
                    }
                }
            }
        }

        // #1858: next wave after regroup + next at_tick.
        if exec.profile.has_wave_schedule()
            && exec.wave_sent
            && exec.wave_cursor + 1 < exec.profile.waves.len()
        {
            let regroup = exec.profile.waves[exec.wave_cursor].regroup_ticks;
            let next_at = exec.profile.waves[exec.wave_cursor + 1].at_tick;
            let next_size = exec.profile.waves[exec.wave_cursor + 1].size;
            let regroup_ready = tick >= exec.wave_sent_tick.saturating_add(u64::from(regroup));
            if regroup_ready && tick >= next_at {
                exec.wave_cursor += 1;
                exec.wave_sent = false;
                note(
                    "WAVE_ADVANCE",
                    format!(
                        "advancing to wave {} size {next_size} at_tick {next_at}",
                        exec.wave_cursor
                    ),
                );
            }
        }
        Ok(())
    }

    fn emit_wave(&self, exec: &AiExecutor, plan: &mut Vec<SessionOrder>) {
        let cp = exec.profile.controlled_player;
        for field in &exec.own_fields {
            plan.push(SessionOrder::AddToGroup {
                player: cp,
                unit: *field,
            });
        }
        plan.push(SessionOrder::SelectUnits {
            player: cp,
            units: exec.own_fields.clone(),
        });
        if let Some(ability) = self.first_point_ability(&exec.own_fields) {
            plan.push(SessionOrder::IssueMove {
                player: cp,
                units: exec.own_fields.clone(),
                ability,
                x: exec.enemy_cell.0,
                z: exec.enemy_cell.1,
                queued: false,
            });
        }
        for field in &exec.own_fields {
            plan.push(SessionOrder::MoveFieldUnit {
                field_id: *field,
                cell_x: exec.enemy_cell.0,
                cell_z: exec.enemy_cell.1,
            });
        }
    }

    fn first_point_ability(&self, units: &[u32]) -> Option<u16> {
        let unit = *units.first()?;
        let caps = self.match_state.capabilities(unit).ok()?;
        caps.iter()
            .find(|c| c.target_kind == OrderTargetKind::Point)
            .map(|c| c.ability)
    }
}
