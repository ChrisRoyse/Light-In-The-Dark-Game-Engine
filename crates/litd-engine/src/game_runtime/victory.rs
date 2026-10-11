//! #838 victory / defeat condition framework + per-player result + game-over.
//!
//! Data-authored conditions evaluated in the sim: the melee default
//! ([`VictoryCondition::AllEnemyStructuresDestroyed`]) and trigger-set custom
//! conditions ([`VictoryCondition::CustomTrigger`]). The evaluator reads a
//! [`StructureLedger`] (per-player live-structure counts) and the fired-trigger
//! set, then transitions each player's [`PlayerResult`]. When results change it
//! emits a typed [`GameOverEvent`] for the runtime host (the game-over screen is
//! host UI; the sim delivers the event + state only). An `observers_after_defeat`
//! flag keeps defeated players in the match as observers instead of removing them.

use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

use crate::{
    EngineError,
    game_runtime::players::{PlayerId, PlayerRoster},
};

fn victory_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::GamePackageInvalidConfig {
        field: field.to_string(),
        reason: reason.into(),
    }
}

/// Per-player match result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerResult {
    Playing,
    Victory,
    Defeat,
}

impl PlayerResult {
    const fn code(self) -> u64 {
        match self {
            Self::Playing => 0,
            Self::Victory => 1,
            Self::Defeat => 2,
        }
    }
}

/// An authored victory/defeat condition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VictoryCondition {
    /// Melee: a player is defeated once all its structures are destroyed; the
    /// last player(s) with an un-defeated, mutually-hostile front wins.
    AllEnemyStructuresDestroyed,
    /// A custom trigger-set condition: `player` wins when `trigger` has fired.
    CustomTrigger { player: PlayerId, trigger: u32 },
}

/// Live-structure counts per player. `counts[player]` is the number of the
/// player's still-standing structures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureLedger {
    counts: Vec<u32>,
}

impl StructureLedger {
    pub fn new(player_count: usize) -> Self {
        Self {
            counts: vec![0; player_count],
        }
    }

    pub fn set(&mut self, player: PlayerId, count: u32) -> Result<(), EngineError> {
        *self
            .counts
            .get_mut(usize::from(player))
            .ok_or_else(|| victory_error("structures.player", format!("no player {player}")))? =
            count;
        Ok(())
    }

    pub fn count(&self, player: PlayerId) -> Result<u32, EngineError> {
        self.counts
            .get(usize::from(player))
            .copied()
            .ok_or_else(|| victory_error("structures.player", format!("no player {player}")))
    }

    /// Record the destruction of one of `player`'s structures. Fails loud if the
    /// player already had zero (a double-count is a caller bug, not a no-op).
    pub fn destroy_one(&mut self, player: PlayerId) -> Result<u32, EngineError> {
        let slot = self
            .counts
            .get_mut(usize::from(player))
            .ok_or_else(|| victory_error("structures.player", format!("no player {player}")))?;
        if *slot == 0 {
            return Err(victory_error(
                "structures.underflow",
                format!("player {player} has no live structures to destroy"),
            ));
        }
        *slot -= 1;
        Ok(*slot)
    }

    pub fn fold_hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(self.counts.len() as u64);
        for count in &self.counts {
            hasher.update_u64_le(u64::from(*count));
        }
    }
}

/// The typed game-over transition surfaced to the runtime host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameOverEvent {
    pub tick: u64,
    pub player: PlayerId,
    pub result: PlayerResult,
}

/// Per-player result state + authored conditions + the observers-after-defeat
/// flag. Evaluated against a roster + structure ledger + fired triggers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VictoryState {
    results: Vec<PlayerResult>,
    conditions: Vec<VictoryCondition>,
    fired_triggers: Vec<u32>,
    observers_after_defeat: bool,
}

impl VictoryState {
    /// Author the victory framework. Validates every condition references a real
    /// player so sabotaged condition data fails loud at authoring, not at play.
    pub fn new(
        roster: &PlayerRoster,
        conditions: Vec<VictoryCondition>,
        observers_after_defeat: bool,
    ) -> Result<Self, EngineError> {
        let n = roster.player_count();
        if conditions.is_empty() {
            return Err(victory_error(
                "victory.conditions",
                "a match must author at least one victory condition",
            ));
        }
        for condition in &conditions {
            if let VictoryCondition::CustomTrigger { player, .. } = condition
                && usize::from(*player) >= n
            {
                return Err(victory_error(
                    "victory.conditions.player",
                    format!("custom condition references unknown player {player}"),
                ));
            }
        }
        Ok(Self {
            results: vec![PlayerResult::Playing; n],
            conditions,
            fired_triggers: Vec::new(),
            observers_after_defeat,
        })
    }

    pub fn result(&self, player: PlayerId) -> Result<PlayerResult, EngineError> {
        self.results
            .get(usize::from(player))
            .copied()
            .ok_or_else(|| victory_error("victory.player", format!("no player {player}")))
    }

    pub fn observers_after_defeat(&self) -> bool {
        self.observers_after_defeat
    }

    /// A defeated player observes the rest of the match when the flag is set.
    pub fn is_observer(&self, player: PlayerId) -> Result<bool, EngineError> {
        Ok(self.observers_after_defeat && matches!(self.result(player)?, PlayerResult::Defeat))
    }

    /// Fire a custom-condition trigger (host/sim event). Deduped + sorted.
    pub fn fire_trigger(&mut self, trigger: u32) {
        if let Err(pos) = self.fired_triggers.binary_search(&trigger) {
            self.fired_triggers.insert(pos, trigger);
        }
    }

    /// #1855/#838: force a player to Defeat (the trigger `defeat_player` action's
    /// counterpart to `fire_trigger` victory). A no-op if already defeated; fails
    /// loud on an unknown player. Returns the direct transition event (empty if no
    /// change); the caller re-evaluates for melee cascades.
    pub fn force_defeat(
        &mut self,
        player: PlayerId,
        tick: u64,
    ) -> Result<Vec<GameOverEvent>, EngineError> {
        let slot = self
            .results
            .get_mut(usize::from(player))
            .ok_or_else(|| victory_error("victory.player", format!("no player {player}")))?;
        if matches!(*slot, PlayerResult::Defeat) {
            return Ok(Vec::new());
        }
        *slot = PlayerResult::Defeat;
        Ok(vec![GameOverEvent {
            tick,
            player,
            result: PlayerResult::Defeat,
        }])
    }

    fn trigger_fired(&self, trigger: u32) -> bool {
        self.fired_triggers.binary_search(&trigger).is_ok()
    }

    /// Evaluate all conditions against current state, mutate per-player results,
    /// and return the game-over events for players whose result changed this
    /// evaluation. Deterministic: players visited in ascending id order.
    // The `player`/`other` indices key into three parallel collections (`next`,
    // the roster, and the structure ledger), so range loops are clearer here than
    // zipped enumerate iterators.
    #[allow(clippy::needless_range_loop)]
    pub fn evaluate(
        &mut self,
        tick: u64,
        roster: &PlayerRoster,
        structures: &StructureLedger,
    ) -> Result<Vec<GameOverEvent>, EngineError> {
        let n = roster.player_count();
        // Melee defeat: a player with a melee condition and zero live structures.
        let melee = self
            .conditions
            .iter()
            .any(|c| matches!(c, VictoryCondition::AllEnemyStructuresDestroyed));
        let mut next = self.results.clone();
        if melee {
            // Only match participants (human/computer) can be defeated or win;
            // neutral controllers (critters, creeps) never do either.
            for player in 0..n {
                let pid = player as PlayerId;
                if matches!(next[player], PlayerResult::Playing)
                    && roster.is_participant(pid)?
                    && structures.count(pid)? == 0
                {
                    next[player] = PlayerResult::Defeat;
                }
            }
            // Melee victory: a still-playing participant with no un-defeated
            // enemy participant wins.
            for player in 0..n {
                let pid = player as PlayerId;
                if !matches!(next[player], PlayerResult::Playing) || !roster.is_participant(pid)? {
                    continue;
                }
                let mut enemy_alive = false;
                for other in 0..n {
                    if other == player {
                        continue;
                    }
                    let oid = other as PlayerId;
                    if roster.is_participant(oid)?
                        && roster.are_enemies(pid, oid)?
                        && !matches!(next[other], PlayerResult::Defeat)
                    {
                        enemy_alive = true;
                        break;
                    }
                }
                if !enemy_alive {
                    next[player] = PlayerResult::Victory;
                }
            }
        }
        // Custom trigger conditions: a fired trigger grants its player victory.
        for condition in &self.conditions {
            if let VictoryCondition::CustomTrigger { player, trigger } = condition {
                let idx = usize::from(*player);
                if matches!(next[idx], PlayerResult::Playing) && self.trigger_fired(*trigger) {
                    next[idx] = PlayerResult::Victory;
                }
            }
        }
        let mut events = Vec::new();
        for player in 0..n {
            if next[player] != self.results[player] {
                events.push(GameOverEvent {
                    tick,
                    player: player as PlayerId,
                    result: next[player],
                });
            }
        }
        self.results = next;
        Ok(events)
    }

    pub fn fold_hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(self.results.len() as u64);
        for result in &self.results {
            hasher.update_u64_le(result.code());
        }
        hasher.update_u8(u8::from(self.observers_after_defeat));
        hasher.update_u64_le(self.fired_triggers.len() as u64);
        for trigger in &self.fired_triggers {
            hasher.update_u64_le(u64::from(*trigger));
        }
    }
}
