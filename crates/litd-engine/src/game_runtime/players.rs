//! #836 players / forces / teams + alliance matrix + shared vision/control.
//!
//! The match-structure player model: fixed-size player slots (human / computer /
//! neutral-hostile / neutral-passive), each bound to a force (team), an N*N
//! alliance-state matrix (ally / enemy / neutral per ordered pair) carrying
//! shared-vision and shared-unit-control flags, and per-player color + handicap.
//!
//! All state lives in `Vec`s indexed by a dense `PlayerId` (no `HashMap`), so
//! iteration order is fixed and the deterministic `fold_hash` below is stable
//! across runs and machines (#85 lockstep). Alliances may be authored *locked*
//! (fixed) or *editable*; a locked pair rejects a runtime alliance-change order
//! with a typed error rather than silently ignoring it.

use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

use crate::EngineError;

/// Dense player-slot index (0-based). A match has at most [`MAX_PLAYERS`] slots.
pub type PlayerId = u8;
/// Dense force (team) index.
pub type ForceId = u8;

/// Upper bound on player slots in a single match (WC3-style 12 + neutrals).
pub const MAX_PLAYERS: usize = 24;

fn players_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::GamePackageInvalidConfig {
        field: field.to_string(),
        reason: reason.into(),
    }
}

/// Who drives a player slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerController {
    Human,
    Computer,
    NeutralHostile,
    NeutralPassive,
}

impl PlayerController {
    /// Whether this controller is a match participant that can win or be defeated.
    /// Neutral controllers (passive critters, hostile creeps) never do either.
    pub const fn is_participant(self) -> bool {
        matches!(self, Self::Human | Self::Computer)
    }

    const fn code(self) -> u64 {
        match self {
            Self::Human => 0,
            Self::Computer => 1,
            Self::NeutralHostile => 2,
            Self::NeutralPassive => 3,
        }
    }
}

/// Alliance state of one ordered player pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllianceState {
    Ally,
    Neutral,
    Enemy,
}

impl AllianceState {
    const fn code(self) -> u64 {
        match self {
            Self::Ally => 0,
            Self::Neutral => 1,
            Self::Enemy => 2,
        }
    }
}

/// One authored player slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerSlot {
    pub id: PlayerId,
    pub controller: PlayerController,
    pub force: ForceId,
    /// Presentation color index (never affects sim outcome, but authored + lowered).
    pub color: u8,
    /// Integer handicap percent applied to authored quantities (100 = full).
    pub handicap_pct: u16,
}

/// One directed alliance-matrix cell: how `from` regards `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllianceCell {
    pub state: AllianceState,
    pub shared_vision: bool,
    pub shared_control: bool,
    /// Fixed alliance: a runtime alliance-change order on this pair fails loud.
    pub locked: bool,
}

impl AllianceCell {
    fn fold(self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(self.state.code());
        hasher.update_u8(u8::from(self.shared_vision));
        hasher.update_u8(u8::from(self.shared_control));
        hasher.update_u8(u8::from(self.locked));
    }
}

/// The full player roster + alliance matrix. Row-major `matrix[from*n + to]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerRoster {
    slots: Vec<PlayerSlot>,
    matrix: Vec<AllianceCell>,
    force_count: u8,
}

impl PlayerRoster {
    /// Build a roster from authored slots. Slots must be dense (`id == index`),
    /// carry a force below `force_count`, and there must be at least two.
    pub fn new(slots: Vec<PlayerSlot>, force_count: u8) -> Result<Self, EngineError> {
        if slots.len() < 2 {
            return Err(players_error(
                "roster.slots",
                "a match needs at least two player slots",
            ));
        }
        if slots.len() > MAX_PLAYERS {
            return Err(players_error(
                "roster.slots",
                format!("at most {MAX_PLAYERS} player slots"),
            ));
        }
        if force_count == 0 {
            return Err(players_error("roster.force_count", "must be non-zero"));
        }
        for (index, slot) in slots.iter().enumerate() {
            if usize::from(slot.id) != index {
                return Err(players_error(
                    "roster.slots.id",
                    format!("slot {index} has non-dense id {}", slot.id),
                ));
            }
            if slot.force >= force_count {
                return Err(players_error(
                    "roster.slots.force",
                    format!(
                        "slot {index} force {} >= force_count {force_count}",
                        slot.force
                    ),
                ));
            }
            if slot.handicap_pct == 0 || slot.handicap_pct > 100 {
                return Err(players_error(
                    "roster.slots.handicap_pct",
                    format!("slot {index} handicap {} out of 1..=100", slot.handicap_pct),
                ));
            }
        }
        let n = slots.len();
        // Default matrix: same-force players are allied with shared vision,
        // cross-force players are enemies. Neutral-passive players are neutral to
        // everyone by default. Authoring can override any cell afterward.
        let mut matrix = Vec::with_capacity(n * n);
        for from in &slots {
            for to in &slots {
                let state = if from.id == to.id {
                    AllianceState::Ally
                } else if matches!(from.controller, PlayerController::NeutralPassive)
                    || matches!(to.controller, PlayerController::NeutralPassive)
                {
                    AllianceState::Neutral
                } else if from.force == to.force {
                    AllianceState::Ally
                } else {
                    AllianceState::Enemy
                };
                let allied = matches!(state, AllianceState::Ally);
                matrix.push(AllianceCell {
                    state,
                    shared_vision: allied,
                    shared_control: false,
                    locked: false,
                });
            }
        }
        Ok(Self {
            slots,
            matrix,
            force_count,
        })
    }

    pub fn player_count(&self) -> usize {
        self.slots.len()
    }

    pub fn force_count(&self) -> u8 {
        self.force_count
    }

    pub fn slots(&self) -> &[PlayerSlot] {
        &self.slots
    }

    pub fn slot(&self, id: PlayerId) -> Result<&PlayerSlot, EngineError> {
        self.slots
            .get(usize::from(id))
            .ok_or_else(|| players_error("roster.slot", format!("no player slot {id}")))
    }

    fn cell_index(&self, from: PlayerId, to: PlayerId) -> Result<usize, EngineError> {
        let n = self.slots.len();
        let (f, t) = (usize::from(from), usize::from(to));
        if f >= n || t >= n {
            return Err(players_error(
                "alliance.pair",
                format!("pair ({from},{to}) out of range for {n} players"),
            ));
        }
        Ok(f * n + t)
    }

    pub fn alliance(&self, from: PlayerId, to: PlayerId) -> Result<AllianceCell, EngineError> {
        let index = self.cell_index(from, to)?;
        Ok(self.matrix[index])
    }

    /// Author an alliance cell (map-authoring path — may set `locked`).
    pub fn author_alliance(
        &mut self,
        from: PlayerId,
        to: PlayerId,
        cell: AllianceCell,
    ) -> Result<(), EngineError> {
        if from == to {
            return Err(players_error(
                "alliance.pair",
                "a player's self-alliance cell is fixed and cannot be authored",
            ));
        }
        let index = self.cell_index(from, to)?;
        self.matrix[index] = cell;
        Ok(())
    }

    /// Runtime alliance change (the typed alliance-change sim order). Fails loud
    /// on a locked (fixed) pair — under lockstep a silent no-op would diverge
    /// peers whose data disagreed. Shared vision/control follow the new state
    /// unless the state stays the same.
    pub fn set_alliance_state(
        &mut self,
        from: PlayerId,
        to: PlayerId,
        state: AllianceState,
        shared_vision: bool,
        shared_control: bool,
    ) -> Result<AllianceCell, EngineError> {
        if from == to {
            return Err(players_error(
                "alliance.pair",
                "cannot change a player's alliance with itself",
            ));
        }
        let index = self.cell_index(from, to)?;
        if self.matrix[index].locked {
            return Err(players_error(
                "alliance.locked",
                format!("alliance ({from},{to}) is fixed and cannot change at runtime"),
            ));
        }
        let cell = AllianceCell {
            state,
            shared_vision,
            shared_control,
            locked: false,
        };
        self.matrix[index] = cell;
        Ok(cell)
    }

    /// Whether the player is a match participant (can win / be defeated).
    pub fn is_participant(&self, id: PlayerId) -> Result<bool, EngineError> {
        Ok(self.slot(id)?.controller.is_participant())
    }

    pub fn are_enemies(&self, from: PlayerId, to: PlayerId) -> Result<bool, EngineError> {
        Ok(matches!(
            self.alliance(from, to)?.state,
            AllianceState::Enemy
        ))
    }

    /// Fold the roster + matrix into a deterministic digest for the match hash.
    pub fn fold_hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(self.slots.len() as u64);
        hasher.update_u8(self.force_count);
        for slot in &self.slots {
            hasher.update_u8(slot.id);
            hasher.update_u64_le(slot.controller.code());
            hasher.update_u8(slot.force);
            hasher.update_u8(slot.color);
            hasher.update_u64_le(u64::from(slot.handicap_pct));
        }
        for cell in &self.matrix {
            cell.fold(hasher);
        }
    }
}
