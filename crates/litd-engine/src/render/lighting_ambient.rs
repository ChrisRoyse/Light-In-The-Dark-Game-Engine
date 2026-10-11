use serde::{Deserialize, Serialize};

use crate::{EngineError, math::Vec3};

use super::RenderLight;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AmbientLight {
    pub name: String,
    pub color: Vec3,
    pub intensity: f32,
}

impl AmbientLight {
    pub fn new(name: impl Into<String>, color: Vec3, intensity: f32) -> Self {
        Self {
            name: name.into(),
            color,
            intensity,
        }
    }

    pub fn default_fill() -> Self {
        Self::new("default_ambient_fill", Vec3::new(0.012, 0.014, 0.022), 1.0)
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.name.trim().is_empty() || self.name.contains(['\n', '\r', '\t']) {
            return invalid_ambient("ambient.name", "must be non-empty single-line text");
        }
        if !self.color.is_finite() || self.color.cmplt(Vec3::ZERO).any() {
            return invalid_ambient(
                "ambient.color",
                "RGB values must be finite and non-negative",
            );
        }
        if !self.intensity.is_finite() || self.intensity < 0.0 {
            return invalid_ambient("ambient.intensity", "must be finite and non-negative");
        }
        Ok(())
    }

    pub fn contribution(&self) -> Vec3 {
        self.color * self.intensity
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LightingRig {
    pub ambient: AmbientLight,
    pub lights: Vec<RenderLight>,
}

impl LightingRig {
    pub fn new(ambient: AmbientLight, lights: Vec<RenderLight>) -> Result<Self, EngineError> {
        let rig = Self { ambient, lights };
        rig.validate()?;
        Ok(rig)
    }

    pub fn from_lights(lights: &[RenderLight]) -> Result<Self, EngineError> {
        Self::new(AmbientLight::default_fill(), lights.to_vec())
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        self.ambient.validate()?;
        for (index, light) in self.lights.iter().enumerate() {
            light.validate(index)?;
        }
        Ok(())
    }
}

fn invalid_ambient<T>(
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
