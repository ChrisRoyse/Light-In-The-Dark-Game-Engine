//! WC3-parity runtime UI/HUD systems as deterministic, data-driven state modules
//! (issues #839 quests, #840 leaderboards/multiboards, #841 timers, #842 dialogs,
//! #843 floating text; parent epic #804).
//!
//! Doctrine (#1312/#1783 generic host; #85 lockstep determinism): every
//! sim-affecting HUD quantity lives in an INTEGER-only typed module advanced from
//! the deterministic tick or a typed order in the stream -- NEVER the wall clock.
//! A dialog button click is a [`HudOrder`] in the order stream, so a replay of the
//! same seed + orders reproduces an identical [`GameHudWorld::state_hash`]. Floats
//! appear ONLY in the presentation tier ([`presentation`]) where text is drawn via
//! the #882 runtime-text plan; they never enter sim state.
//!
//! Shared-file discipline (#1809 Wave-3 batch-2): this whole system is new modules;
//! the only edit to a shared core file is the `pub mod game_runtime;` line in
//! `lib.rs`. Typed validation failures reuse [`EngineError::SimInvalidConfig`]
//! (stable code `LITD-SIM-INVALID-CONFIG`) so `error.rs` stays frozen.

use litd_hash::Fnv1a64;

use crate::EngineError;

pub mod boards;
pub mod dialogs;
pub mod floating_text;
pub mod presentation;
pub mod quests;
#[cfg(feature = "full")]
pub mod quests_authoring;
pub mod timers;

pub use boards::{BoardSet, Leaderboard, LeaderboardRow, Multiboard, MultiboardCell, MultiboardId};
pub use dialogs::{ButtonId, Dialog, DialogButton, DialogClickRecord, DialogId, DialogSet};
pub use floating_text::{FloatingText, FloatingTextId, FloatingTextSet};
pub use quests::{FlagId, QuestId, QuestLog, QuestNotification, QuestProgress};
pub use timers::{Timer, TimerFired, TimerId, TimerKind, TimerSet, TimerWindow};

/// A player slot (WC3 supports up to 24 players; a `u64` visibility mask covers
/// up to 64 slots deterministically with no allocation or map-iteration order).
pub type PlayerId = u8;

/// Highest addressable player slot for a [`PlayerMask`].
pub const MAX_PLAYERS: u8 = 64;

/// Length-prefixed string hash so concatenation is unambiguous ("ab"+"c" never
/// collides with "a"+"bc"). Every subsystem hashes text through this helper.
pub(crate) fn hash_str(hasher: &mut Fnv1a64, value: &str) {
    hasher.update_u64_le(value.len() as u64);
    hasher.update(value.as_bytes());
}

/// Construct a typed [`EngineError::SimInvalidConfig`] -- the single fail-loud path
/// for every HUD validation error (keeps `error.rs` frozen).
pub(crate) fn invalid(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::SimInvalidConfig {
        field: field.to_string(),
        reason: reason.into(),
    }
}

/// A deterministic per-player visibility set backed by a `u64` bitmask. Iteration
/// and hashing are slot-ordered, so the mask never introduces map-order nondeterminism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayerMask(u64);

impl PlayerMask {
    pub const EMPTY: Self = Self(0);

    /// Every one of the 64 addressable slots.
    pub const fn all() -> Self {
        Self(u64::MAX)
    }

    /// A mask holding exactly one player. Fails loud if the slot is out of range.
    pub fn single(player: PlayerId) -> Result<Self, EngineError> {
        Self::EMPTY.with(player)
    }

    /// Return a copy with `player` inserted; fails loud on an out-of-range slot.
    pub fn with(self, player: PlayerId) -> Result<Self, EngineError> {
        if player >= MAX_PLAYERS {
            return Err(invalid(
                "player_mask.slot",
                format!("player slot {player} exceeds max {MAX_PLAYERS}"),
            ));
        }
        Ok(Self(self.0 | (1_u64 << player)))
    }

    pub const fn contains(self, player: PlayerId) -> bool {
        player < MAX_PLAYERS && (self.0 & (1_u64 << player)) != 0
    }

    pub const fn bits(self) -> u64 {
        self.0
    }

    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }
}

/// A packed 8-bit-per-channel HUD colour. Explicit integer channels (encode, never
/// embed) so it lowers and hashes with no float precision loss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl HudColor {
    pub const WHITE: Self = Self::new(255, 255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub(crate) fn hash(self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(u64::from(self.r));
        hasher.update_u64_le(u64::from(self.g));
        hasher.update_u64_le(u64::from(self.b));
        hasher.update_u64_le(u64::from(self.a));
    }
}

/// One typed HUD input in the deterministic order stream. A click is scheduled for
/// a future `execute_tick`; the queue applies it in `(execute_tick, sequence)` order
/// so identical streams replay to identical outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudOrderKind {
    /// A player pressed a dialog button (#842). Resolved as a sim order so a replay
    /// with the same click tick reproduces the same dialog outcome + state hash.
    DialogClick {
        dialog: DialogId,
        button: ButtonId,
        player: PlayerId,
    },
}

/// A scheduled HUD order (mirrors `SimOrder`: future-dated, sequenced, deterministic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudOrder {
    pub execute_tick: u64,
    pub sequence: u64,
    pub kind: HudOrderKind,
}

impl HudOrder {
    pub const fn new(execute_tick: u64, sequence: u64, kind: HudOrderKind) -> Self {
        Self {
            execute_tick,
            sequence,
            kind,
        }
    }

    fn sort_key(self) -> (u64, u64) {
        (self.execute_tick, self.sequence)
    }
}

/// What one [`GameHudWorld::step`] produced this tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HudStepReport {
    pub tick: u64,
    pub orders_applied: usize,
    pub timers_fired: usize,
    pub floating_text_expired: usize,
    pub state_hash: u64,
}

/// The aggregate deterministic HUD world: all five WC3 runtime systems advanced by
/// one shared tick + order stream, with a single fold-order state hash. NOTHING here
/// reads the wall clock.
#[derive(Debug, Default)]
pub struct GameHudWorld {
    tick: u64,
    orders: Vec<HudOrder>,
    /// The timer expiries produced by the MOST RECENT [`Self::step`]. #1851: this is
    /// the event stream the session trigger evaluator consumes -- previously the
    /// `TimerFired` records were counted then DISCARDED, which is why no
    /// `countdown_timer_expires` trigger could ever fire. Derived per-tick state
    /// (a pure function of the hashed timers), so it is not itself hashed.
    last_fired: Vec<timers::TimerFired>,
    pub quests: QuestLog,
    pub boards: BoardSet,
    pub timers: TimerSet,
    pub dialogs: DialogSet,
    pub floating: FloatingTextSet,
}

impl GameHudWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn tick(&self) -> u64 {
        self.tick
    }

    pub fn pending_orders(&self) -> usize {
        self.orders.len()
    }

    /// The timer expiries produced by the most recent step, in deterministic
    /// (insertion, tick) order -- the #1851 countdown-timer event stream.
    pub fn fired_this_tick(&self) -> &[timers::TimerFired] {
        &self.last_fired
    }

    /// Schedule a HUD order. Fails loud if the order is not strictly in the future
    /// (a past-due order would diverge peers under lockstep, mirroring #1463).
    pub fn enqueue_order(&mut self, order: HudOrder) -> Result<(), EngineError> {
        if order.execute_tick <= self.tick {
            return Err(invalid(
                "hud_order.execute_tick",
                format!(
                    "order tick {} is not after current tick {}",
                    order.execute_tick, self.tick
                ),
            ));
        }
        self.orders.push(order);
        Ok(())
    }

    /// Advance exactly one tick: apply the orders scheduled for the new tick in
    /// deterministic order, then advance timers and floating-text lifetimes to it.
    pub fn step(&mut self) -> Result<HudStepReport, EngineError> {
        let execute_tick = self
            .tick
            .checked_add(1)
            .ok_or(EngineError::SimTickOverflow)?;
        let orders_applied = self.apply_orders_for(execute_tick)?;
        self.tick = execute_tick;
        // #1851: RETAIN the fired records for the trigger evaluator (they used to be
        // counted and dropped here, orphaning every countdown_timer_expires trigger).
        self.last_fired = self.timers.advance_to(execute_tick)?;
        let timers_fired = self.last_fired.len();
        let floating_text_expired = self.floating.advance_to(execute_tick);
        Ok(HudStepReport {
            tick: self.tick,
            orders_applied,
            timers_fired,
            floating_text_expired,
            state_hash: self.state_hash(),
        })
    }

    /// Advance `ticks` steps, returning the final report.
    pub fn run_ticks(&mut self, ticks: u64) -> Result<HudStepReport, EngineError> {
        let mut report = self.report();
        for _ in 0..ticks {
            report = self.step()?;
        }
        Ok(report)
    }

    fn apply_orders_for(&mut self, execute_tick: u64) -> Result<usize, EngineError> {
        // Deterministic: collect due orders, sort by (execute_tick, sequence), apply
        // in that order, then drop them from the pending set. No HashMap iteration.
        let mut due: Vec<HudOrder> = self
            .orders
            .iter()
            .copied()
            .filter(|order| order.execute_tick == execute_tick)
            .collect();
        due.sort_by_key(|order| order.sort_key());
        for order in &due {
            self.apply_order(*order, execute_tick)?;
        }
        self.orders
            .retain(|order| order.execute_tick != execute_tick);
        Ok(due.len())
    }

    fn apply_order(&mut self, order: HudOrder, execute_tick: u64) -> Result<(), EngineError> {
        match order.kind {
            HudOrderKind::DialogClick {
                dialog,
                button,
                player,
            } => {
                self.dialogs
                    .resolve_click(dialog, button, player, execute_tick)?;
                Ok(())
            }
        }
    }

    fn report(&self) -> HudStepReport {
        HudStepReport {
            tick: self.tick,
            orders_applied: 0,
            timers_fired: 0,
            floating_text_expired: 0,
            state_hash: self.state_hash(),
        }
    }

    /// Deterministic fold-order hash over every subsystem. Identical order streams
    /// replayed from the same start MUST produce an identical value (#85 lockstep).
    pub fn state_hash(&self) -> u64 {
        let mut hasher = Fnv1a64::new();
        hasher.update_u64_le(self.tick);
        hasher.update_u64_le(self.orders.len() as u64);
        self.quests.hash(&mut hasher);
        self.boards.hash(&mut hasher);
        self.timers.hash(&mut hasher);
        self.dialogs.hash(&mut hasher);
        self.floating.hash(&mut hasher);
        hasher.finish()
    }
}

#[cfg(test)]
mod tests;
