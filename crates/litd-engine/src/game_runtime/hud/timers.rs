//! #841 timers + timer windows: sim-tick-driven countdown timers (one-shot or
//! periodic) plus on-screen timer-window display state.
//!
//! Timers are advanced from the deterministic tick ([`TimerSet::advance_to`]), NEVER
//! the wall clock: a period is measured in ticks, and a periodic timer fires exactly
//! `floor(elapsed / period)` times over a tick span. A zero period is rejected with a
//! typed error (the negative control). Remaining time is formatted for the timer
//! window using an integer ticks-per-second, so no float enters sim state.

use litd_hash::Fnv1a64;

use crate::EngineError;

use super::{hash_str, invalid};

/// Stable timer identifier within a [`TimerSet`].
pub type TimerId = u32;

/// Whether a timer fires once then stops, or repeatedly every period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerKind {
    OneShot,
    Periodic,
}

impl TimerKind {
    fn tag(self) -> u64 {
        match self {
            Self::OneShot => 1,
            Self::Periodic => 2,
        }
    }
}

/// A deterministic countdown timer. `next_fire_tick` is the absolute tick of its
/// next expiry; `fire_count` is the total number of expiries so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timer {
    pub id: TimerId,
    pub kind: TimerKind,
    pub period_ticks: u64,
    pub start_tick: u64,
    pub next_fire_tick: u64,
    pub fire_count: u64,
    pub running: bool,
}

impl Timer {
    /// Ticks remaining until the next expiry at `now` (0 if already due / stopped).
    pub fn remaining_ticks(&self, now: u64) -> u64 {
        if !self.running {
            return 0;
        }
        self.next_fire_tick.saturating_sub(now)
    }

    fn hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(u64::from(self.id));
        hasher.update_u64_le(self.kind.tag());
        hasher.update_u64_le(self.period_ticks);
        hasher.update_u64_le(self.start_tick);
        hasher.update_u64_le(self.next_fire_tick);
        hasher.update_u64_le(self.fire_count);
        hasher.update_u64_le(u64::from(self.running));
    }
}

/// One timer expiry produced by [`TimerSet::advance_to`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimerFired {
    pub timer: TimerId,
    pub tick: u64,
    /// The cumulative fire count including this expiry (1 for the first fire).
    pub fire_count: u64,
}

/// An on-screen timer-window display bound to a timer (WC3 `CreateTimerDialog`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerWindow {
    pub timer: TimerId,
    pub title: String,
    pub visible: bool,
}

impl TimerWindow {
    fn hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(u64::from(self.timer));
        hash_str(hasher, &self.title);
        hasher.update_u64_le(u64::from(self.visible));
    }
}

/// The full timer state: timers + their windows + a shared ticks-per-second used
/// only to FORMAT remaining time for display (never for advancing timers).
#[derive(Debug)]
pub struct TimerSet {
    ticks_per_second: u32,
    timers: Vec<Timer>,
    windows: Vec<TimerWindow>,
    next_id: TimerId,
}

impl Default for TimerSet {
    fn default() -> Self {
        Self {
            ticks_per_second: 64,
            timers: Vec::new(),
            windows: Vec::new(),
            next_id: 0,
        }
    }
}

impl TimerSet {
    pub fn new(ticks_per_second: u32) -> Result<Self, EngineError> {
        if ticks_per_second == 0 {
            return Err(invalid(
                "timer_set.ticks_per_second",
                "ticks-per-second must be positive",
            ));
        }
        Ok(Self {
            ticks_per_second,
            ..Self::default()
        })
    }

    pub const fn ticks_per_second(&self) -> u32 {
        self.ticks_per_second
    }

    pub fn timers(&self) -> &[Timer] {
        &self.timers
    }

    pub fn timer(&self, id: TimerId) -> Option<&Timer> {
        self.timers.iter().find(|t| t.id == id)
    }

    pub fn windows(&self) -> &[TimerWindow] {
        &self.windows
    }

    /// Start a timer with a tick period. Fails loud on a zero period -- the negative
    /// control: a periodic timer that never advances would spin forever.
    pub fn start_timer(
        &mut self,
        kind: TimerKind,
        period_ticks: u64,
        now: u64,
    ) -> Result<TimerId, EngineError> {
        if period_ticks == 0 {
            return Err(invalid(
                "timer.period_ticks",
                "timer period must be at least one tick",
            ));
        }
        let next_fire_tick = now
            .checked_add(period_ticks)
            .ok_or_else(|| invalid("timer.next_fire_tick", "next fire tick overflowed"))?;
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| invalid("timer.id", "timer id counter overflowed"))?;
        self.timers.push(Timer {
            id,
            kind,
            period_ticks,
            start_tick: now,
            next_fire_tick,
            fire_count: 0,
            running: true,
        });
        Ok(id)
    }

    pub fn pause_timer(&mut self, id: TimerId) -> Result<(), EngineError> {
        self.timer_mut(id)?.running = false;
        Ok(())
    }

    pub fn resume_timer(&mut self, id: TimerId) -> Result<(), EngineError> {
        self.timer_mut(id)?.running = true;
        Ok(())
    }

    /// Bind an on-screen window to a timer (fails loud if the timer is unknown).
    pub fn create_window(&mut self, timer: TimerId, title: &str) -> Result<(), EngineError> {
        if self.timer(timer).is_none() {
            return Err(invalid(
                "timer_window.timer",
                format!("timer {timer} does not exist"),
            ));
        }
        if let Some(window) = self.windows.iter_mut().find(|w| w.timer == timer) {
            window.title = title.to_string();
        } else {
            self.windows.push(TimerWindow {
                timer,
                title: title.to_string(),
                visible: true,
            });
        }
        Ok(())
    }

    /// Advance every running timer up to and including `tick`, returning every expiry
    /// in deterministic (timer id, fire tick) order. Periodic timers reschedule; one-
    /// shot timers stop after firing.
    pub fn advance_to(&mut self, tick: u64) -> Result<Vec<TimerFired>, EngineError> {
        let mut fired = Vec::new();
        for timer in &mut self.timers {
            if !timer.running {
                continue;
            }
            while timer.running && timer.next_fire_tick <= tick {
                let fire_tick = timer.next_fire_tick;
                timer.fire_count = timer
                    .fire_count
                    .checked_add(1)
                    .ok_or_else(|| invalid("timer.fire_count", "fire count overflowed"))?;
                fired.push(TimerFired {
                    timer: timer.id,
                    tick: fire_tick,
                    fire_count: timer.fire_count,
                });
                match timer.kind {
                    TimerKind::OneShot => timer.running = false,
                    TimerKind::Periodic => {
                        timer.next_fire_tick =
                            fire_tick.checked_add(timer.period_ticks).ok_or_else(|| {
                                invalid("timer.next_fire_tick", "next fire tick overflowed")
                            })?;
                    }
                }
            }
        }
        // Deterministic: timers advance in insertion order, and each timer's expiries
        // are already tick-ordered, so `fired` is already stable across identical runs.
        Ok(fired)
    }

    /// Format a timer's remaining time as `MM:SS` using the integer ticks-per-second.
    /// Presentation-only helper; the value it formats is derived from tick state.
    pub fn remaining_formatted(&self, id: TimerId, now: u64) -> Result<String, EngineError> {
        let timer = self
            .timer(id)
            .ok_or_else(|| invalid("timer.id", format!("timer {id} not found")))?;
        let total_seconds = timer.remaining_ticks(now) / u64::from(self.ticks_per_second);
        Ok(format!(
            "{:02}:{:02}",
            total_seconds / 60,
            total_seconds % 60
        ))
    }

    fn timer_mut(&mut self, id: TimerId) -> Result<&mut Timer, EngineError> {
        self.timers
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| invalid("timer.id", format!("timer {id} not found")))
    }

    pub(crate) fn hash(&self, hasher: &mut Fnv1a64) {
        hasher.update(b"timers");
        hasher.update_u64_le(u64::from(self.ticks_per_second));
        hasher.update_u64_le(self.timers.len() as u64);
        for timer in &self.timers {
            timer.hash(hasher);
        }
        hasher.update_u64_le(self.windows.len() as u64);
        for window in &self.windows {
            window.hash(hasher);
        }
    }
}
