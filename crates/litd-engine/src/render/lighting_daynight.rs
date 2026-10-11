use serde::{Deserialize, Serialize};

use crate::{EngineError, math::Vec3};

use super::{AmbientLight, LightKind, LightingRig, RenderLight};

pub const DEFAULT_TIME_OF_DAY_HOURS: f32 = 14.0;

const DEFAULT_SUN_INTENSITY: f32 = 1.25;
const DEFAULT_MOON_INTENSITY: f32 = 0.28;
const DEFAULT_AMBIENT_INTENSITY: f32 = 0.34;
const DEFAULT_FOG_MAX: f32 = 0.58;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeOfDayPhase {
    Night,
    Dawn,
    Day,
    Dusk,
}

impl TimeOfDayPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Night => "night",
            Self::Dawn => "dawn",
            Self::Day => "day",
            Self::Dusk => "dusk",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeatherLightingState {
    pub cloud_cover: f32,
    pub precipitation: f32,
    pub fog_density: f32,
}

impl WeatherLightingState {
    pub const CLEAR: Self = Self {
        cloud_cover: 0.0,
        precipitation: 0.0,
        fog_density: 0.0,
    };

    pub const AUTHORING_WEATHER: Self = Self {
        cloud_cover: 0.28,
        precipitation: 0.16,
        fog_density: 0.22,
    };

    pub fn validate(self) -> Result<(), EngineError> {
        validate_unit("weather.cloud_cover", self.cloud_cover)?;
        validate_unit("weather.precipitation", self.precipitation)?;
        validate_unit("weather.fog_density", self.fog_density)
    }

    pub fn summary(self) -> String {
        format!(
            "cloud={:.3};precipitation={:.3};fog_density={:.3}",
            self.cloud_cover, self.precipitation, self.fog_density
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DayNightLightingInput {
    pub time_of_day_hours: f32,
    pub weather: WeatherLightingState,
    pub sun_base_intensity: f32,
    pub moon_base_intensity: f32,
    pub ambient_base_intensity: f32,
    pub fog_max_base: f32,
}

impl Default for DayNightLightingInput {
    fn default() -> Self {
        Self {
            time_of_day_hours: DEFAULT_TIME_OF_DAY_HOURS,
            weather: WeatherLightingState::CLEAR,
            sun_base_intensity: DEFAULT_SUN_INTENSITY,
            moon_base_intensity: DEFAULT_MOON_INTENSITY,
            ambient_base_intensity: DEFAULT_AMBIENT_INTENSITY,
            fog_max_base: DEFAULT_FOG_MAX,
        }
    }
}

impl DayNightLightingInput {
    pub fn validate(self) -> Result<(), EngineError> {
        if !self.time_of_day_hours.is_finite() || !(0.0..24.0).contains(&self.time_of_day_hours) {
            return invalid_lighting(
                "day_night.time_of_day_hours",
                "must be finite and inside 0.0 <= hour < 24.0",
            );
        }
        self.weather.validate()?;
        validate_nonnegative("day_night.sun_base_intensity", self.sun_base_intensity)?;
        validate_nonnegative("day_night.moon_base_intensity", self.moon_base_intensity)?;
        validate_nonnegative(
            "day_night.ambient_base_intensity",
            self.ambient_base_intensity,
        )?;
        validate_nonnegative("day_night.fog_max_base", self.fog_max_base)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayNightLightingPlan {
    pub input: DayNightLightingInput,
    pub phase: TimeOfDayPhase,
    pub solar_elevation: f32,
    pub day_factor: f32,
    pub twilight_factor: f32,
    pub weather_attenuation: f32,
    pub sun_direction_to_light: Vec3,
    pub moon_direction_to_light: Vec3,
    pub primary_light_name: String,
    pub primary_direction_to_light: Vec3,
    pub primary_color: Vec3,
    pub primary_intensity: f32,
    pub ambient_light: AmbientLight,
    pub ambient_color: Vec3,
    pub ambient_intensity: f32,
    pub fog_color: Vec3,
    pub fog_max: f32,
    pub lights: Vec<RenderLight>,
}

impl DayNightLightingPlan {
    pub fn summary(&self) -> String {
        format!(
            "time={:.2};phase={};primary={};intensity={:.3};weather_attenuation={:.3};ambient={:.3};fog_max={:.3}",
            self.input.time_of_day_hours,
            self.phase.label(),
            self.primary_light_name,
            self.primary_intensity,
            self.weather_attenuation,
            self.ambient_intensity,
            self.fog_max
        )
    }

    pub fn primary_direction_label(&self) -> String {
        format!(
            "{:.4},{:.4},{:.4}",
            self.primary_direction_to_light.x,
            self.primary_direction_to_light.y,
            self.primary_direction_to_light.z
        )
    }

    pub fn lighting_rig(&self) -> Result<LightingRig, EngineError> {
        LightingRig::new(self.ambient_light.clone(), self.lights.clone())
    }
}

pub fn build_day_night_lighting_plan(
    input: DayNightLightingInput,
    local_lights: &[RenderLight],
) -> Result<DayNightLightingPlan, EngineError> {
    input.validate()?;
    validate_local_lights(local_lights)?;

    let solar_elevation = solar_elevation(input.time_of_day_hours);
    let day_factor = smoothstep(-0.08, 0.22, solar_elevation);
    let twilight_factor = (1.0 - (solar_elevation.abs() / 0.32).clamp(0.0, 1.0)).powi(2);
    let phase = phase_for_hour(input.time_of_day_hours);
    let weather_attenuation = weather_attenuation(input.weather);
    let sun_direction_to_light = celestial_direction(input.time_of_day_hours, solar_elevation);
    let moon_direction_to_light = -sun_direction_to_light;
    let use_sun = day_factor >= 0.08;

    let warm_sun = Vec3::new(1.0, 0.73, 0.46);
    let high_sun = Vec3::new(1.0, 0.88, 0.68);
    let moon = Vec3::new(0.36, 0.48, 0.78);
    let primary_color = if use_sun {
        warm_sun.lerp(high_sun, day_factor)
    } else {
        moon
    };
    let primary_intensity = if use_sun {
        input.sun_base_intensity * (0.20 + 0.80 * day_factor) * weather_attenuation
    } else {
        input.moon_base_intensity
            * (1.0 - day_factor).max(0.0)
            * (1.0 - input.weather.cloud_cover * 0.35).clamp(0.28, 1.0)
    };
    let primary_direction_to_light = if use_sun {
        sun_direction_to_light
    } else {
        moon_direction_to_light
    };
    let primary_light_name = if use_sun { "sun" } else { "moon" }.to_string();
    let ambient_color = Vec3::new(0.050, 0.076, 0.120)
        .lerp(Vec3::new(0.112, 0.124, 0.142), day_factor)
        .lerp(Vec3::new(0.105, 0.090, 0.075), twilight_factor * 0.35);
    let ambient_intensity = input.ambient_base_intensity
        * (0.32 + day_factor * 0.68 + twilight_factor * 0.18)
        * (1.0 - input.weather.cloud_cover * 0.22).clamp(0.45, 1.0);
    let fog_color = Vec3::new(0.018, 0.032, 0.052)
        .lerp(Vec3::new(0.082, 0.112, 0.132), day_factor)
        .lerp(Vec3::new(0.092, 0.080, 0.070), twilight_factor * 0.30);
    let fog_max = (input.fog_max_base
        + input.weather.fog_density * 0.32
        + input.weather.precipitation * 0.10)
        .clamp(0.0, 0.92);
    let ambient_light = AmbientLight::new(
        format!("ambient_{}", phase.label()),
        ambient_color,
        ambient_intensity,
    );
    ambient_light.validate()?;

    let primary = RenderLight::directional(
        &primary_light_name,
        primary_direction_to_light,
        primary_color,
        primary_intensity,
    );
    primary.validate(0)?;
    let mut lights = vec![primary];
    lights.extend_from_slice(local_lights);

    Ok(DayNightLightingPlan {
        input,
        phase,
        solar_elevation,
        day_factor,
        twilight_factor,
        weather_attenuation,
        sun_direction_to_light,
        moon_direction_to_light,
        primary_light_name,
        primary_direction_to_light,
        primary_color,
        primary_intensity,
        ambient_light,
        ambient_color,
        ambient_intensity,
        fog_color,
        fog_max,
        lights,
    })
}

fn validate_local_lights(local_lights: &[RenderLight]) -> Result<(), EngineError> {
    for (index, light) in local_lights.iter().enumerate() {
        light.validate(index + 1)?;
        if matches!(light.kind, LightKind::Directional { .. }) {
            return invalid_lighting(
                format!("local_lights[{index}].kind"),
                "day/night lighting owns the primary directional; local lights must be point or spot",
            );
        }
    }
    Ok(())
}

fn phase_for_hour(hour: f32) -> TimeOfDayPhase {
    if (5.0..7.0).contains(&hour) {
        TimeOfDayPhase::Dawn
    } else if (7.0..17.5).contains(&hour) {
        TimeOfDayPhase::Day
    } else if (17.5..20.0).contains(&hour) {
        TimeOfDayPhase::Dusk
    } else {
        TimeOfDayPhase::Night
    }
}

fn solar_elevation(hour: f32) -> f32 {
    let angle = (hour - 6.0) / 12.0 * std::f32::consts::PI;
    angle.sin().clamp(-1.0, 1.0)
}

fn celestial_direction(hour: f32, elevation: f32) -> Vec3 {
    let y = elevation.clamp(-0.92, 0.92);
    let horizontal = (1.0 - y * y).sqrt();
    let azimuth = hour / 24.0 * std::f32::consts::TAU + 0.42;
    Vec3::new(azimuth.cos() * horizontal, y, azimuth.sin() * horizontal).normalize()
}

fn weather_attenuation(weather: WeatherLightingState) -> f32 {
    (1.0 - weather.cloud_cover * 0.42 - weather.precipitation * 0.22).clamp(0.22, 1.0)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn validate_unit(field: &'static str, value: f32) -> Result<(), EngineError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return invalid_lighting(field, "must be finite and inside 0..=1");
    }
    Ok(())
}

fn validate_nonnegative(field: &'static str, value: f32) -> Result<(), EngineError> {
    if !value.is_finite() || value < 0.0 {
        return invalid_lighting(field, "must be finite and non-negative");
    }
    Ok(())
}

fn invalid_lighting<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::Rendering(
        litd_engine::error::RenderingError::ClusteredLightingInvalidConfig {
            field: field.into(),
            reason: reason.into(),
        },
    ))
}
