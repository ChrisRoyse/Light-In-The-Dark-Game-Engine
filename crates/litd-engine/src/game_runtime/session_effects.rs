//! #1855: session dispatch helpers for the runtime-EFFECT orders that reach the
//! WC3-parity HUD/victory subsystems (quests #839, timers #841, dialogs #842,
//! floating text #843, victory/defeat #838).
//!
//! Most effect orders are one-liners handled inline in [`super::MatchRuntimeSession::
//! dispatch`]; the ones needing a small amount of typed construction (floating text)
//! live here so `session_orders.rs` stays under the 500-line ratchet. Every effect
//! mutates a subsystem already folded into `combined_state_hash` (the HUD world or
//! the match state), so no new hash field is required and replay is deterministic.

use super::super::hud::HudColor;
use super::super::hud::floating_text::FloatingTextSpawn;
use super::GameRuntimeError;
use super::{MatchRuntimeSession, SessionOrder};

const DAMAGE_NUMBER_COLOR: HudColor = HudColor::new(255, 96, 80, 255);

impl MatchRuntimeSession {
    /// Apply real combat damage and publish its actual clamped amount into the
    /// deterministic HUD floating-text set consumed by the packaged renderer.
    pub(super) fn apply_damage_with_number(
        &mut self,
        target: u32,
        amount_milli: u32,
    ) -> Result<(), GameRuntimeError> {
        let position_mm = self.damage_target_position_mm(target)?;
        let lifespan_ticks = u64::from(self.constants.tick_rate);
        let now = self.hud.tick();
        let floating = &mut self.hud.floating;
        let match_state = &mut self.match_state;
        floating.apply_then_spawn_damage_number(
            position_mm,
            DAMAGE_NUMBER_COLOR,
            lifespan_ticks,
            now,
            || match_state.apply_damage(target, amount_milli),
            |result| result.amount_applied,
        )?;
        Ok(())
    }

    fn damage_target_position_mm(&self, target: u32) -> Result<[i64; 3], GameRuntimeError> {
        let (cell_x, cell_z) = if let Some(field) = self.field_unit(target) {
            (field.cell_x, field.cell_z)
        } else {
            // Blueprint structures have no field-unit record. Their authoritative
            // combat/base position is the same owner start cell `plan_assault`
            // targets and fog-gates before issuing ApplyDamage.
            let owner = self.match_state.unit_owner(target)?;
            super::start_cell(owner, &self.constants)
        };
        let cell_mm = i64::from(self.constants.fog_cell_size_mm);
        let scale = |cell: i32, axis: &str| {
            i64::from(cell)
                .checked_mul(cell_mm)
                .ok_or_else(|| GameRuntimeError::Invalid {
                    field: format!("session.damage.position.{axis}"),
                    reason: format!("cell {cell} overflows authored fog-cell scale {cell_mm} mm"),
                })
        };
        Ok([scale(cell_x, "x")?, 0, scale(cell_z, "z")?])
    }

    /// Spawn a floating-text tag (WC3 `CreateTextTag`) at the current HUD tick.
    /// Fails loud on a zero lifespan / fade-past-lifespan / empty text (the negative
    /// control the floating-text subsystem enforces).
    pub(super) fn spawn_floating_text(
        &mut self,
        order: &SessionOrder,
    ) -> Result<(), GameRuntimeError> {
        let SessionOrder::SpawnFloatingText {
            text,
            position_mm,
            velocity_mm_per_tick,
            color,
            lifespan_ticks,
            fade_start_ticks,
        } = order
        else {
            return Err(GameRuntimeError::Invalid {
                field: "session.spawn_floating_text".to_string(),
                reason: "dispatched a non-SpawnFloatingText order".to_string(),
            });
        };
        let now = self.hud.tick();
        self.hud.floating.spawn(
            FloatingTextSpawn {
                text: text.clone(),
                position_mm: *position_mm,
                velocity_mm_per_tick: *velocity_mm_per_tick,
                color: HudColor::new(color[0], color[1], color[2], color[3]),
                lifespan_ticks: *lifespan_ticks,
                fade_start_ticks: *fade_start_ticks,
            },
            now,
        )?;
        Ok(())
    }
}
