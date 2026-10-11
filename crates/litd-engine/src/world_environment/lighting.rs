//! #814 lighting + day/night cycle, integrating #1742 probe-GI baked states.
//!
//! A day/night cycle is authored as an ordered set of time-of-day keyframes (sun
//! colour/intensity/angle + ambient), interpolated at runtime -- the standard
//! keyframe-lerp approach. Each authored `BakedGiStateRef` binds a keyframe to a
//! frozen probe-GI volume fingerprint (from `probe_gi_lower::lower_probe_volume`),
//! so the merged #1742 day/night baked states are referenced by content, not
//! re-derived. Angles/intensities are integers; interpolation stays in the sim.

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Rgb8, WorldEnvironmentError, invalid, record::EnvironmentPayload, require_non_empty};

/// Whether the authored map runs a live day/night cycle or is fixed at one state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayNightCyclePolicy {
    Enabled,
    Disabled,
}

impl DayNightCyclePolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

/// One authored lighting keyframe at a fraction of the cycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeOfDayKeyframe {
    /// Position in the cycle, per-mille (0..=1000). 0 == cycle start.
    pub time_permille: u16,
    pub sun_color: Rgb8,
    /// Sun intensity, milli-lux-equivalent (0..). Fixed-point integer.
    pub sun_intensity_milli: u32,
    pub ambient_color: Rgb8,
    /// Sun azimuth in whole degrees (0..360).
    pub sun_azimuth_deg: u16,
    /// Sun elevation in whole degrees (-90..=90); negative == below horizon.
    pub sun_elevation_deg: i16,
}

impl TimeOfDayKeyframe {
    fn validate(&self, index: usize) -> Result<(), WorldEnvironmentError> {
        if self.time_permille > 1000 {
            return invalid(
                format!("lighting.keyframes[{index}].time_permille"),
                "time_permille must be within 0..=1000",
            );
        }
        if self.sun_azimuth_deg >= 360 {
            return invalid(
                format!("lighting.keyframes[{index}].sun_azimuth_deg"),
                "azimuth must be within 0..=359 degrees",
            );
        }
        if self.sun_elevation_deg < -90 || self.sun_elevation_deg > 90 {
            return invalid(
                format!("lighting.keyframes[{index}].sun_elevation_deg"),
                "elevation must be within -90..=90 degrees",
            );
        }
        Ok(())
    }
}

/// Binds a keyframe time to a frozen probe-GI baked volume (#1742).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BakedGiStateRef {
    /// The probe volume's lighting-state label (e.g. "day", "night").
    pub state_label: String,
    /// Cycle position this baked state anchors, per-mille (matches a keyframe).
    pub time_permille: u16,
    /// The lowered probe-volume fingerprint from `probe_gi_lower::lower_probe_volume`.
    pub probe_volume_fingerprint: String,
}

impl BakedGiStateRef {
    fn validate(&self, index: usize) -> Result<(), WorldEnvironmentError> {
        require_non_empty(
            &format!("lighting.baked_gi_states[{index}].state_label"),
            &self.state_label,
        )?;
        if self.time_permille > 1000 {
            return invalid(
                format!("lighting.baked_gi_states[{index}].time_permille"),
                "time_permille must be within 0..=1000",
            );
        }
        // A probe fingerprint is a 64-hex-char sha256 (probe_gi_lower).
        let hex_len = self.probe_volume_fingerprint.len();
        if hex_len != 64
            || !self
                .probe_volume_fingerprint
                .chars()
                .all(|ch| ch.is_ascii_hexdigit())
        {
            return invalid(
                format!("lighting.baked_gi_states[{index}].probe_volume_fingerprint"),
                "must be a 64-hex-char probe volume fingerprint (probe_gi_lower)",
            );
        }
        Ok(())
    }
}

/// The #814 authoring aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LightingEnvironmentAuthoring {
    pub id: String,
    pub display_name: String,
    pub author: String,
    pub cycle: DayNightCyclePolicy,
    /// Full cycle length in seconds (>0 when the cycle is enabled).
    pub cycle_seconds: u32,
    pub keyframes: Vec<TimeOfDayKeyframe>,
    pub baked_gi_states: Vec<BakedGiStateRef>,
}

impl LightingEnvironmentAuthoring {
    pub fn validate(&self) -> Result<(), WorldEnvironmentError> {
        require_non_empty("lighting.id", &self.id)?;
        require_non_empty("lighting.display_name", &self.display_name)?;
        require_non_empty("lighting.author", &self.author)?;
        if self.keyframes.len() < 2 {
            return invalid(
                "lighting.keyframes",
                "a day/night cycle needs at least two keyframes to interpolate",
            );
        }
        if self.cycle == DayNightCyclePolicy::Enabled && self.cycle_seconds == 0 {
            return invalid(
                "lighting.cycle_seconds",
                "an enabled cycle must have a positive length",
            );
        }
        let mut previous: Option<u16> = None;
        for (index, keyframe) in self.keyframes.iter().enumerate() {
            keyframe.validate(index)?;
            if let Some(prev) = previous
                && keyframe.time_permille <= prev
            {
                return invalid(
                    format!("lighting.keyframes[{index}].time_permille"),
                    "keyframes must be strictly increasing in time",
                );
            }
            previous = Some(keyframe.time_permille);
        }
        for (index, state) in self.baked_gi_states.iter().enumerate() {
            state.validate(index)?;
        }
        Ok(())
    }

    pub fn to_payload(&self) -> Result<EnvironmentPayload, WorldEnvironmentError> {
        self.validate()?;
        let keyframes: Vec<_> = self
            .keyframes
            .iter()
            .map(|kf| {
                json!({
                    "ambient_color": kf.ambient_color.to_payload(),
                    "sun_azimuth_deg": kf.sun_azimuth_deg,
                    "sun_color": kf.sun_color.to_payload(),
                    "sun_elevation_deg": kf.sun_elevation_deg,
                    "sun_intensity_milli": kf.sun_intensity_milli,
                    "time_permille": kf.time_permille,
                })
            })
            .collect();
        let baked: Vec<_> = self
            .baked_gi_states
            .iter()
            .map(|state| {
                json!({
                    "probe_volume_fingerprint": state.probe_volume_fingerprint,
                    "state_label": state.state_label,
                    "time_permille": state.time_permille,
                })
            })
            .collect();
        let fields = json!({
            "baked_gi_states": baked,
            "cycle_policy": self.cycle.as_str(),
            "cycle_seconds": self.cycle_seconds,
            "keyframe_count": self.keyframes.len() as u64,
            "keyframes": keyframes,
        });
        EnvironmentPayload::new("lighting_day_night", &self.id, fields)
    }
}
