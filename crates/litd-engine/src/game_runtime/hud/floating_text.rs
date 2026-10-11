//! #843 floating text / text tags: world-anchored floating text (damage numbers,
//! labels) with position, velocity, fade, lifespan, and colour.
//!
//! Sim state is INTEGER-only: positions are fixed-point milli-units, velocity is
//! milli-units per tick, and lifetimes are counted in ticks. A tag's screen position
//! and fade alpha at any tick are DERIVED deterministically from its spawn params, so
//! the presentation tier (floats, [`super::presentation`]) never feeds back into the
//! hash. Lifetime is managed per tick by [`FloatingTextSet::advance_to`], which culls
//! tags whose lifespan has elapsed with exact-tick precision.

use litd_hash::Fnv1a64;

use crate::EngineError;

use super::{HudColor, invalid};

/// Stable floating-text identifier within a [`FloatingTextSet`].
pub type FloatingTextId = u32;

/// A world-anchored floating text tag. All spatial quantities are integer
/// milli-units; `position_at`/`alpha_at` derive presentation values on demand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatingText {
    pub id: FloatingTextId,
    pub text: String,
    /// Spawn position, fixed-point milli-units (x, y, z).
    pub position_mm: [i64; 3],
    /// Constant velocity, milli-units per tick.
    pub velocity_mm_per_tick: [i64; 3],
    pub color: HudColor,
    pub spawn_tick: u64,
    /// Total lifetime in ticks (>= 1). The tag is culled once this many ticks elapse.
    pub lifespan_ticks: u64,
    /// Tick offset after which the tag begins fading (<= lifespan).
    pub fade_start_ticks: u64,
}

impl FloatingText {
    /// The tag's position at `tick` (spawn + velocity * elapsed), integer milli-units.
    pub fn position_at(&self, tick: u64) -> [i64; 3] {
        let elapsed = tick.saturating_sub(self.spawn_tick) as i64;
        [
            self.position_mm[0] + self.velocity_mm_per_tick[0] * elapsed,
            self.position_mm[1] + self.velocity_mm_per_tick[1] * elapsed,
            self.position_mm[2] + self.velocity_mm_per_tick[2] * elapsed,
        ]
    }

    /// The tag's fade alpha at `tick`, as an integer 0..=255. Full until
    /// `fade_start_ticks`, then linearly ramps to 0 at `lifespan_ticks`. Pure integer
    /// math so it is identical on every peer.
    pub fn alpha_at(&self, tick: u64) -> u8 {
        let elapsed = tick.saturating_sub(self.spawn_tick);
        if elapsed >= self.lifespan_ticks {
            return 0;
        }
        if elapsed < self.fade_start_ticks {
            return self.color.a;
        }
        let fade_span = self.lifespan_ticks.saturating_sub(self.fade_start_ticks);
        if fade_span == 0 {
            return 0;
        }
        let faded = elapsed - self.fade_start_ticks;
        let remaining = fade_span - faded;
        ((u64::from(self.color.a) * remaining) / fade_span) as u8
    }

    /// Whether the tag has expired at `tick` (exact-tick lifetime boundary).
    pub fn is_expired(&self, tick: u64) -> bool {
        tick.saturating_sub(self.spawn_tick) >= self.lifespan_ticks
    }

    fn hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(u64::from(self.id));
        hasher.update_u64_le(self.text.len() as u64);
        hasher.update(self.text.as_bytes());
        for axis in self.position_mm {
            hasher.update_i64_le(axis);
        }
        for axis in self.velocity_mm_per_tick {
            hasher.update_i64_le(axis);
        }
        self.color.hash(hasher);
        hasher.update_u64_le(self.spawn_tick);
        hasher.update_u64_le(self.lifespan_ticks);
        hasher.update_u64_le(self.fade_start_ticks);
    }
}

/// Parameters for spawning a floating text tag (WC3 `CreateTextTag`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatingTextSpawn {
    pub text: String,
    pub position_mm: [i64; 3],
    pub velocity_mm_per_tick: [i64; 3],
    pub color: HudColor,
    pub lifespan_ticks: u64,
    pub fade_start_ticks: u64,
}

/// The floating-text subsystem: live tags plus a monotonic id allocator.
#[derive(Debug, Default)]
pub struct FloatingTextSet {
    texts: Vec<FloatingText>,
    next_id: FloatingTextId,
}

impl FloatingTextSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn texts(&self) -> &[FloatingText] {
        &self.texts
    }

    pub fn active_count(&self) -> usize {
        self.texts.len()
    }

    pub fn text(&self, id: FloatingTextId) -> Option<&FloatingText> {
        self.texts.iter().find(|t| t.id == id)
    }

    fn validate_spawn(spawn: &FloatingTextSpawn) -> Result<(), EngineError> {
        if spawn.lifespan_ticks == 0 {
            return Err(invalid(
                "floating_text.lifespan_ticks",
                "lifespan must be at least one tick",
            ));
        }
        if spawn.fade_start_ticks > spawn.lifespan_ticks {
            return Err(invalid(
                "floating_text.fade_start_ticks",
                "fade start cannot exceed the lifespan",
            ));
        }
        if spawn.text.trim().is_empty() {
            return Err(invalid(
                "floating_text.text",
                "floating text must be non-empty",
            ));
        }
        Ok(())
    }

    fn next_slot(&self) -> Result<(FloatingTextId, FloatingTextId), EngineError> {
        let id = self.next_id;
        let next_id = id
            .checked_add(1)
            .ok_or_else(|| invalid("floating_text.id", "text id counter overflowed"))?;
        Ok((id, next_id))
    }

    fn commit_spawn(
        &mut self,
        id: FloatingTextId,
        next_id: FloatingTextId,
        spawn: FloatingTextSpawn,
        now: u64,
    ) -> FloatingTextId {
        self.next_id = next_id;
        self.texts.push(FloatingText {
            id,
            text: spawn.text,
            position_mm: spawn.position_mm,
            velocity_mm_per_tick: spawn.velocity_mm_per_tick,
            color: spawn.color,
            spawn_tick: now,
            lifespan_ticks: spawn.lifespan_ticks,
            fade_start_ticks: spawn.fade_start_ticks,
        });
        id
    }

    /// Spawn a floating text tag at `now`. Fails loud on a zero lifespan or a fade
    /// start beyond the lifespan (a tag that never dies or fades incoherently).
    pub fn spawn(
        &mut self,
        spawn: FloatingTextSpawn,
        now: u64,
    ) -> Result<FloatingTextId, EngineError> {
        Self::validate_spawn(&spawn)?;
        let (id, next_id) = self.next_slot()?;
        Ok(self.commit_spawn(id, next_id, spawn, now))
    }

    /// Convenience for combat damage numbers: a white-to-fade rising number.
    pub fn spawn_damage_number(
        &mut self,
        amount: i64,
        position_mm: [i64; 3],
        color: HudColor,
        lifespan_ticks: u64,
        now: u64,
    ) -> Result<FloatingTextId, EngineError> {
        self.spawn(
            FloatingTextSpawn {
                text: amount.to_string(),
                position_mm,
                // Rise on +z at 40 milli-units/tick, a deterministic damage-number drift.
                velocity_mm_per_tick: [0, 0, 40],
                color,
                lifespan_ticks,
                fade_start_ticks: lifespan_ticks / 2,
            },
            now,
        )
    }

    /// Preflight one damage-number slot, apply the real damage mutation, then
    /// commit exactly one tag from the returned actual amount. The closure cannot
    /// observe or mutate this set, so a rejected damage leaves both the id cursor
    /// and text list unchanged; after success the commit path is infallible.
    pub(crate) fn apply_then_spawn_damage_number<T>(
        &mut self,
        position_mm: [i64; 3],
        color: HudColor,
        lifespan_ticks: u64,
        now: u64,
        apply: impl FnOnce() -> Result<T, EngineError>,
        amount_applied: impl FnOnce(&T) -> u32,
    ) -> Result<T, EngineError> {
        if lifespan_ticks == 0 {
            return Err(invalid(
                "floating_text.lifespan_ticks",
                "damage-number lifespan must be at least one tick",
            ));
        }
        let (id, next_id) = self.next_slot()?;
        let result = apply()?;
        let spawn = FloatingTextSpawn {
            text: amount_applied(&result).to_string(),
            position_mm,
            velocity_mm_per_tick: [0, 0, 40],
            color,
            lifespan_ticks,
            fade_start_ticks: lifespan_ticks / 2,
        };
        self.commit_spawn(id, next_id, spawn, now);
        Ok(result)
    }

    /// Cull every tag whose lifespan elapsed at `tick`, returning how many expired.
    /// Exact-tick lifetime: a tag spawned at S with lifespan L is removed at S + L.
    pub fn advance_to(&mut self, tick: u64) -> usize {
        let before = self.texts.len();
        self.texts.retain(|text| !text.is_expired(tick));
        before - self.texts.len()
    }

    pub(crate) fn hash(&self, hasher: &mut Fnv1a64) {
        hasher.update(b"floating_text");
        hasher.update_u64_le(self.texts.len() as u64);
        for text in &self.texts {
            text.hash(hasher);
        }
    }
}
