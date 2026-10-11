//! #815 weather effects + global fog / visibility distance.
//!
//! Global exponential fog matching the standard engine model: a fog colour, a
//! density (fixed-point), and an explicit visibility/cutoff distance beyond which
//! geometry is fully obscured (the Z-cull horizon). A weather layer names the
//! precipitation kind and its intensity. Both are authored, persisted, and
//! lowered so the fog renderer and weather VFX consume frozen data, not live
//! Calyx queries.

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Rgb8, WorldEnvironmentError, invalid, record::EnvironmentPayload, require_non_empty};

/// The authored precipitation/weather kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrecipitationKind {
    Clear,
    Rain,
    Snow,
    Ash,
    Sandstorm,
}

impl PrecipitationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Rain => "rain",
            Self::Snow => "snow",
            Self::Ash => "ash",
            Self::Sandstorm => "sandstorm",
        }
    }

    pub const fn ordinal(self) -> u32 {
        match self {
            Self::Clear => 0,
            Self::Rain => 1,
            Self::Snow => 2,
            Self::Ash => 3,
            Self::Sandstorm => 4,
        }
    }
}

/// Global exponential fog with an explicit visibility horizon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalFog {
    pub enabled: bool,
    pub color: Rgb8,
    /// Fog density coefficient, fixed-point milli (density * 1000).
    pub density_milli: u32,
    /// Distance (milli-metres) at/after which fog fully obscures geometry.
    pub visibility_distance_mm: u32,
    /// Distance (milli-metres) before which there is no fog (near plane).
    pub start_distance_mm: u32,
}

impl GlobalFog {
    fn validate(&self) -> Result<(), WorldEnvironmentError> {
        if !self.enabled {
            return Ok(());
        }
        if self.visibility_distance_mm == 0 {
            return invalid(
                "weather.fog.visibility_distance_mm",
                "enabled fog must have a positive visibility distance",
            );
        }
        if self.start_distance_mm >= self.visibility_distance_mm {
            return invalid(
                "weather.fog.start_distance_mm",
                "fog start must be nearer than the visibility horizon",
            );
        }
        if self.density_milli == 0 {
            return invalid(
                "weather.fog.density_milli",
                "enabled fog must have a positive density",
            );
        }
        Ok(())
    }
}

/// The #815 authoring aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeatherEnvironmentAuthoring {
    pub id: String,
    pub display_name: String,
    pub author: String,
    pub precipitation: PrecipitationKind,
    /// Precipitation intensity, per-mille (0..=1000). 0 == none.
    pub intensity_permille: u16,
    /// Wind bearing driving the precipitation, whole degrees (0..360).
    pub wind_bearing_deg: u16,
    pub fog: GlobalFog,
}

impl WeatherEnvironmentAuthoring {
    pub fn validate(&self) -> Result<(), WorldEnvironmentError> {
        require_non_empty("weather.id", &self.id)?;
        require_non_empty("weather.display_name", &self.display_name)?;
        require_non_empty("weather.author", &self.author)?;
        if self.intensity_permille > 1000 {
            return invalid(
                "weather.intensity_permille",
                "intensity must be within 0..=1000",
            );
        }
        if self.precipitation == PrecipitationKind::Clear && self.intensity_permille != 0 {
            return invalid(
                "weather.intensity_permille",
                "clear weather must have zero intensity",
            );
        }
        if self.precipitation != PrecipitationKind::Clear && self.intensity_permille == 0 {
            return invalid(
                "weather.intensity_permille",
                "active precipitation must have positive intensity",
            );
        }
        if self.wind_bearing_deg >= 360 {
            return invalid(
                "weather.wind_bearing_deg",
                "wind bearing must be within 0..=359 degrees",
            );
        }
        self.fog.validate()?;
        Ok(())
    }

    pub fn to_payload(&self) -> Result<EnvironmentPayload, WorldEnvironmentError> {
        self.validate()?;
        let fields = json!({
            "fog_color": self.fog.color.to_payload(),
            "fog_density_milli": self.fog.density_milli,
            "fog_enabled": self.fog.enabled,
            "fog_start_distance_mm": self.fog.start_distance_mm,
            "fog_visibility_distance_mm": self.fog.visibility_distance_mm,
            "precipitation_intensity_permille": self.intensity_permille,
            "precipitation_kind": self.precipitation.as_str(),
            "precipitation_ordinal": self.precipitation.ordinal(),
            "wind_bearing_deg": self.wind_bearing_deg,
        });
        EnvironmentPayload::new("weather_fog", &self.id, fields)
    }
}
