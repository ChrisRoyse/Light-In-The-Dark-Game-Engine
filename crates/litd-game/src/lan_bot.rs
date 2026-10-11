//! #1787: a generic, deterministic per-slot order source ("the AI profile can drive one side").
//!
//! This is host machinery, NOT game rules (#1783): given a slot and the field units the loaded
//! blueprint assigned to it, it emits a fixed, tick-indexed stream of [`SessionOrder`]s that
//! MOVE those units toward the opposing corner. It invents no economy/combat knowledge -- it
//! only issues the generic move orders the runtime already validates -- so it drives ANY packaged
//! `.litdgame` that carries field units. Its output is a pure function of `(slot, tick)`, so both
//! peers agree on what every slot submitted and the match stays deterministic.

use litd_engine::game_runtime::{RuntimeDatasetsManifest, SessionOrder};

use crate::lan_error::LanError;

/// The deterministic order source for one slot.
#[derive(Debug, Clone)]
pub struct SlotBot {
    slot: u8,
    field_units: Vec<u32>,
    target_x: i32,
    target_z: i32,
    move_period: u64,
}

impl SlotBot {
    /// Derive the bot from the package blueprint: the non-structure units owned by `slot` are
    /// the movers, and the target is the opposing corner of the fog grid (a deterministic push
    /// toward the enemy that changes fog + positions, hence the state hash, every step).
    pub fn from_manifest(
        manifest: &RuntimeDatasetsManifest,
        slot: u8,
        move_period: u64,
    ) -> Result<Self, LanError> {
        let blueprint = manifest.lowered_match.verify()?;
        let field_units: Vec<u32> = blueprint
            .units
            .iter()
            .filter(|unit| !unit.is_structure && unit.owner == slot)
            .map(|unit| unit.unit)
            .collect();
        let width = manifest.constants.fog_grid_width as i32;
        let height = manifest.constants.fog_grid_height as i32;
        // Slot 0 pushes toward the far corner; other slots push toward the near corner.
        let (target_x, target_z) = if slot == 0 {
            ((width - 2).max(0), (height - 2).max(0))
        } else {
            (1.min(width - 1), 1.min(height - 1))
        };
        Ok(Self {
            slot,
            field_units,
            target_x,
            target_z,
            move_period: move_period.max(1),
        })
    }

    pub fn slot(&self) -> u8 {
        self.slot
    }

    /// The orders this slot submits for `execute_tick`. Deterministic: on each move boundary it
    /// steps each of its field units one cell toward the target, so the stream depends only on
    /// the tick and the (fixed) unit list.
    pub fn orders_for_tick(&self, execute_tick: u64) -> Vec<SessionOrder> {
        if self.field_units.is_empty() || !execute_tick.is_multiple_of(self.move_period) {
            return Vec::new();
        }
        let step = (execute_tick / self.move_period) as i32;
        self.field_units
            .iter()
            .map(|unit| {
                // Advance monotonically toward the target, clamped once it arrives.
                let cell_x = self.approach(step, self.target_x);
                let cell_z = self.approach(step, self.target_z);
                SessionOrder::MoveFieldUnit {
                    field_id: *unit,
                    cell_x,
                    cell_z,
                }
            })
            .collect()
    }

    /// Move `step` cells from the origin corner toward `target`, clamped at the target.
    fn approach(&self, step: i32, target: i32) -> i32 {
        // Slot 0 starts near (2,2); others near the far side. We approximate the start as the
        // opposite of the target and walk toward it -- exact start cell does not matter for
        // determinism, only that the sequence is a pure function of (slot, step).
        let start = if self.slot == 0 { 2 } else { target.max(2) };
        if target >= start {
            (start + step).min(target)
        } else {
            (start - step).max(target)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot(slot: u8) -> SlotBot {
        SlotBot {
            slot,
            field_units: vec![10, 11],
            target_x: 14,
            target_z: 14,
            move_period: 2,
        }
    }

    #[test]
    fn orders_are_a_pure_function_of_tick() {
        let bot = bot(0);
        // Same tick -> same orders (determinism the lockstep relies on).
        assert_eq!(bot.orders_for_tick(4), bot.orders_for_tick(4));
        // Off-boundary ticks emit nothing.
        assert!(bot.orders_for_tick(5).is_empty());
        // Boundary ticks emit one move per unit.
        assert_eq!(bot.orders_for_tick(4).len(), 2);
    }

    #[test]
    fn advance_is_monotonic_toward_target_and_clamps() {
        let bot = bot(0);
        let mut last = 2;
        for boundary in 1..40 {
            let tick = boundary * bot.move_period;
            if let Some(SessionOrder::MoveFieldUnit { cell_x, .. }) =
                bot.orders_for_tick(tick).first()
            {
                assert!(*cell_x >= last);
                assert!(*cell_x <= bot.target_x);
                last = *cell_x;
            }
        }
        assert_eq!(last, bot.target_x);
    }
}
