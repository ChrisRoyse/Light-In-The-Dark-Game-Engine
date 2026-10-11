//! #848: runtime day/night + weather as DETERMINISTIC sim state.
//!
//! Time-of-day advances one step per sim tick from a cycle length sourced from the
//! lowered #814 lighting record (`cycle_seconds` * `ticks_per_second`) and/or #827
//! gameplay constants. It is integer-only sim state that folds into `state_hash`;
//! the VISUAL application (lerping the #814 keyframes, spawning #815 precipitation
//! VFX) is presentation and lives elsewhere.
//!
//! Day/night alters the SIGHT-RADIUS INPUT deterministically: [`TimeOfDayState::sight_radius_input_mm`]
//! returns an integer milli-metre radius that shrinks toward night. That integer is
//! the SEAM to the fog-of-war system (`render::ui_fog::VisionSource`, being built in
//! parallel): presentation converts it to metres via [`presentation_sight_radius_meters`]
//! when assembling vision sources. Nothing float touches this sim state.

use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

use super::error::{GameRuntimeError, invalid};

pub const TIME_OF_DAY_SCHEMA: &str = "litd.game_runtime.time_of_day.v1";

/// Tuning sourced from lowered authored records -- NOT hardcoded in the host.
/// `day_length_ticks` comes from the #814 lighting cycle; the sight fields come
/// from #827 constants / #818 unit sight. The runtime just executes the numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeOfDayConfig {
    /// Ticks in one full day/night cycle (>0).
    pub day_length_ticks: u64,
    /// Full-daylight sight radius in milli-metres (the unmodified base).
    pub base_sight_radius_mm: u32,
    /// Fraction of the base radius removed at full midnight, per-mille (0..=1000).
    pub night_sight_penalty_permille: u16,
}

impl TimeOfDayConfig {
    fn validate(&self) -> Result<(), GameRuntimeError> {
        if self.day_length_ticks == 0 {
            return invalid("time_of_day.day_length_ticks", "must be positive");
        }
        if self.night_sight_penalty_permille > 1000 {
            return invalid(
                "time_of_day.night_sight_penalty_permille",
                "must be within 0..=1000",
            );
        }
        Ok(())
    }
}

/// A trigger-settable environment order. Applied deterministically at `execute_tick`
/// before the sim steps that tick, exactly like a `SimOrder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeEnvOrderKind {
    SetTimeOfDay {
        tick_in_day: u64,
    },
    SetWeather {
        ordinal: u32,
        intensity_permille: u16,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEnvOrder {
    pub execute_tick: u64,
    pub sequence: u64,
    pub kind: RuntimeEnvOrderKind,
}

impl RuntimeEnvOrder {
    pub const fn new(execute_tick: u64, sequence: u64, kind: RuntimeEnvOrderKind) -> Self {
        Self {
            execute_tick,
            sequence,
            kind,
        }
    }

    pub(crate) fn sort_key(&self) -> (u64, u64) {
        (self.execute_tick, self.sequence)
    }
}

/// The deterministic day/night + weather sim state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeOfDayState {
    pub schema: String,
    pub config: TimeOfDayConfig,
    /// Position within the current day, `0..day_length_ticks`.
    pub tick_in_day: u64,
    /// Completed day count since the runtime booted.
    pub day_count: u64,
    /// Active precipitation kind ordinal (matches #815 `PrecipitationKind::ordinal`).
    pub weather_ordinal: u32,
    /// Active precipitation intensity, per-mille (0..=1000).
    pub weather_intensity_permille: u16,
}

impl TimeOfDayState {
    pub fn new(
        config: TimeOfDayConfig,
        weather_ordinal: u32,
        weather_intensity_permille: u16,
    ) -> Result<Self, GameRuntimeError> {
        config.validate()?;
        if weather_intensity_permille > 1000 {
            return invalid(
                "time_of_day.weather_intensity_permille",
                "must be within 0..=1000",
            );
        }
        Ok(Self {
            schema: TIME_OF_DAY_SCHEMA.to_string(),
            config,
            tick_in_day: 0,
            day_count: 0,
            weather_ordinal,
            weather_intensity_permille,
        })
    }

    /// Advance one sim tick. Wraps at the cycle boundary, incrementing `day_count`.
    pub fn advance_tick(&mut self) -> Result<(), GameRuntimeError> {
        let next = self
            .tick_in_day
            .checked_add(1)
            .ok_or(GameRuntimeError::Sim(crate::EngineError::SimTickOverflow))?;
        if next >= self.config.day_length_ticks {
            self.tick_in_day = 0;
            self.day_count = self
                .day_count
                .checked_add(1)
                .ok_or(GameRuntimeError::Sim(crate::EngineError::SimTickOverflow))?;
        } else {
            self.tick_in_day = next;
        }
        Ok(())
    }

    /// Apply a trigger-set environment order to this state.
    pub fn apply(&mut self, kind: RuntimeEnvOrderKind) -> Result<(), GameRuntimeError> {
        match kind {
            RuntimeEnvOrderKind::SetTimeOfDay { tick_in_day } => {
                if tick_in_day >= self.config.day_length_ticks {
                    return invalid(
                        "time_of_day.set_time.tick_in_day",
                        "must be within 0..day_length_ticks",
                    );
                }
                self.tick_in_day = tick_in_day;
            }
            RuntimeEnvOrderKind::SetWeather {
                ordinal,
                intensity_permille,
            } => {
                if intensity_permille > 1000 {
                    return invalid(
                        "time_of_day.set_weather.intensity_permille",
                        "must be within 0..=1000",
                    );
                }
                self.weather_ordinal = ordinal;
                self.weather_intensity_permille = intensity_permille;
            }
        }
        Ok(())
    }

    /// Cycle position in per-mille, `0..1000` (0 == midnight, 500 == noon).
    pub fn time_permille(&self) -> u16 {
        let permille = self.tick_in_day.saturating_mul(1000) / self.config.day_length_ticks;
        permille.min(999) as u16
    }

    /// Daylight strength in per-mille: triangle peaking at noon (permille 500).
    pub fn brightness_permille(&self) -> u16 {
        let t = u32::from(self.time_permille());
        let brightness = if t <= 500 { t * 2 } else { (1000 - t) * 2 };
        brightness.min(1000) as u16
    }

    /// Darkness strength in per-mille (`1000 - brightness`); drives the sight
    /// penalty.
    pub fn darkness_permille(&self) -> u16 {
        1000 - self.brightness_permille()
    }

    /// True while the sun is below the horizon (brightness under half).
    pub fn is_night(&self) -> bool {
        self.brightness_permille() < 500
    }

    /// The DETERMINISTIC sight-radius input in milli-metres for the current
    /// time-of-day. Full daylight yields the base radius; full midnight removes
    /// `night_sight_penalty_permille` of it. Integer math end to end -- this value
    /// is the seam the fog system consumes (see module docs).
    pub fn sight_radius_input_mm(&self) -> u32 {
        let base = u64::from(self.config.base_sight_radius_mm);
        let penalty = u64::from(self.config.night_sight_penalty_permille);
        let darkness = u64::from(self.darkness_permille());
        // fraction of the base to remove, per-mille (0..=1000)
        let remove_permille = penalty.saturating_mul(darkness) / 1000;
        let reduction = base.saturating_mul(remove_permille) / 1000;
        base.saturating_sub(reduction) as u32
    }

    /// Deterministic digest of the full day/night + weather state.
    pub fn state_hash(&self) -> u64 {
        let mut hasher = Fnv1a64::new();
        hasher.update(self.schema.as_bytes());
        hasher.update_u64_le(self.config.day_length_ticks);
        hasher.update_u64_le(u64::from(self.config.base_sight_radius_mm));
        hasher.update_u64_le(u64::from(self.config.night_sight_penalty_permille));
        hasher.update_u64_le(self.tick_in_day);
        hasher.update_u64_le(self.day_count);
        hasher.update_u64_le(u64::from(self.weather_ordinal));
        hasher.update_u64_le(u64::from(self.weather_intensity_permille));
        hasher.finish()
    }
}

/// SEAM: convert the deterministic integer sight-radius input to the metre-valued
/// float the presentation fog-of-war layer (`render::ui_fog::VisionSource`) expects.
/// PRESENTATION ONLY -- never call this inside the deterministic sim. Isolated here
/// so the fog track (built in parallel) has one documented conversion point.
pub fn presentation_sight_radius_meters(sight_radius_mm: u32) -> f32 {
    sight_radius_mm as f32 / 1000.0
}
