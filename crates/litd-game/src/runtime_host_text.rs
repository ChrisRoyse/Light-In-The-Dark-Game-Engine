use litd_engine::{
    EngineError,
    game_runtime::{
        MatchRuntimeSession,
        hud::{
            HudColor,
            presentation::{floating_text_damage_numbers, world_label},
        },
    },
    render::runtime_text::{DamageNumber, WorldLabel},
};

use super::{RuntimeCounters, runtime_host_error};

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct RuntimeTextSnapshot {
    pub(crate) labels: Vec<WorldLabel>,
    pub(crate) damage_numbers: Vec<DamageNumber>,
    pub(crate) session_tick: u64,
}

impl RuntimeTextSnapshot {
    fn from_session(session: &MatchRuntimeSession) -> Result<Self, EngineError> {
        let match_state = session.match_state();
        let mut labels = Vec::new();
        for player_index in 0..match_state.roster().player_count() {
            let player = u8::try_from(player_index).map_err(|_| {
                runtime_host_error(
                    "runtime_text.player",
                    format!("player index {player_index} does not fit the runtime PlayerId"),
                )
            })?;
            for unit in match_state.alive_owned_units(player) {
                let Some(field) = session.field_unit(unit) else {
                    continue;
                };
                let hp = match_state
                    .hp_milli(unit)
                    .map_err(|error| runtime_host_error("runtime_text.hp", error.to_string()))?;
                let max_hp = match_state
                    .max_hp_milli(unit)
                    .map_err(|error| runtime_host_error("runtime_text.hp", error.to_string()))?;
                let text = unit_label(player, unit, hp, max_hp);
                labels.push(world_label(
                    &format!("unit-label-{unit}"),
                    &text,
                    [
                        i64::from(field.cell_x) * 1_000,
                        0,
                        i64::from(field.cell_z) * 1_000,
                    ],
                    player_color(player),
                ));
            }
        }
        let damage_numbers = floating_text_damage_numbers(
            &session.hud().floating,
            session.hud().tick(),
            session.ticks_per_second(),
        );
        Ok(Self {
            labels,
            damage_numbers,
            session_tick: session.applied_tick(),
        })
    }
}

impl RuntimeCounters {
    pub(super) fn publish_runtime_text(
        &self,
        session: Option<&MatchRuntimeSession>,
    ) -> Result<(), EngineError> {
        let snapshot = match session {
            Some(session) => RuntimeTextSnapshot::from_session(session)?,
            None => RuntimeTextSnapshot::default(),
        };
        let mut guard = self.runtime_text.write().map_err(|_| {
            runtime_host_error(
                "runtime_text.write",
                "runtime text snapshot lock was poisoned",
            )
        })?;
        *guard = snapshot;
        Ok(())
    }

    pub(crate) fn runtime_text_snapshot(&self) -> Result<RuntimeTextSnapshot, EngineError> {
        self.runtime_text
            .read()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| {
                runtime_host_error(
                    "runtime_text.read",
                    "runtime text snapshot lock was poisoned",
                )
            })
    }
}

fn unit_label(player: u8, unit: u32, hp_milli: u32, max_hp_milli: u32) -> String {
    format!(
        "P{} Unit {}  {}/{} HP",
        u16::from(player) + 1,
        unit,
        hp_milli / 1_000,
        max_hp_milli / 1_000
    )
}

fn player_color(player: u8) -> HudColor {
    match player % 4 {
        0 => HudColor::new(244, 210, 112, 255),
        1 => HudColor::new(105, 190, 255, 255),
        2 => HudColor::new(255, 126, 126, 255),
        _ => HudColor::new(137, 224, 151, 255),
    }
}

#[cfg(test)]
#[path = "runtime_host_text_tests.rs"]
mod tests;
