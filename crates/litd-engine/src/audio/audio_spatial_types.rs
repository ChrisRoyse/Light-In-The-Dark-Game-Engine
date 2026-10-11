//! Spatial-audio value types and their fail-loud validation (#1466 split of
//! `audio_spatial.rs`).
//!
//! Serializable request/snapshot data plus the per-type `validate()` contracts. Kira
//! playback lives in `audio_spatial.rs`; the shared math/validators in
//! `audio_spatial_support.rs`.

use serde::{Deserialize, Serialize};

use super::audio_spatial_support::{
    MAX_OCCLUSION_CUTOFF_HZ, MAX_OCCLUSION_LOSS_DB, MIN_OCCLUSION_CUTOFF_HZ, distance,
    validate_unit, validate_unit_f64, validate_vec3,
};
use super::{AudioBus, AudioTween, audio_invalid, validate_symbol};
use crate::EngineError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioListenerPose {
    pub position: [f32; 3],
    pub orientation_xyzw: [f32; 4],
    pub velocity: [f32; 3],
}

impl Default for AudioListenerPose {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            orientation_xyzw: [0.0, 0.0, 0.0, 1.0],
            velocity: [0.0, 0.0, 0.0],
        }
    }
}

impl AudioListenerPose {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_vec3("listener.position", self.position)?;
        validate_vec3("listener.velocity", self.velocity)?;
        if !self.orientation_xyzw.iter().all(|value| value.is_finite()) {
            return audio_invalid("listener.orientation", "orientation must be finite");
        }
        let len = self
            .orientation_xyzw
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        if !(0.999..=1.001).contains(&len) {
            return audio_invalid("listener.orientation", "orientation must be normalized");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioReverbZone {
    pub zone_id: String,
    pub center: [f32; 3],
    pub radius_meters: f32,
    pub wet_mix: f32,
    pub feedback: f64,
    pub damping: f64,
}

impl AudioReverbZone {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_symbol("reverb.zone_id", &self.zone_id)?;
        validate_vec3("reverb.center", self.center)?;
        if !self.radius_meters.is_finite() || self.radius_meters <= 0.0 {
            return audio_invalid("reverb.radius_meters", "radius must be finite and > 0");
        }
        validate_unit("reverb.wet_mix", self.wet_mix)?;
        validate_unit_f64("reverb.feedback", self.feedback)?;
        validate_unit_f64("reverb.damping", self.damping)?;
        Ok(())
    }

    pub(super) fn wet_mix_at(&self, position: [f32; 3]) -> f32 {
        let distance = distance(self.center, position);
        if distance >= self.radius_meters {
            0.0
        } else {
            self.wet_mix * (1.0 - distance / self.radius_meters)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioOcclusion {
    pub factor: f32,
}

impl AudioOcclusion {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_unit("occlusion.factor", self.factor)
    }

    pub(super) fn volume_loss_db(&self) -> f32 {
        -MAX_OCCLUSION_LOSS_DB * self.factor
    }

    pub(super) fn cutoff_hz(&self) -> f64 {
        MAX_OCCLUSION_CUTOFF_HZ
            - (MAX_OCCLUSION_CUTOFF_HZ - MIN_OCCLUSION_CUTOFF_HZ) * f64::from(self.factor)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialAudioPlaybackRequest {
    pub sound_id: String,
    pub path: String,
    pub bus: AudioBus,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub min_distance_meters: f32,
    pub max_distance_meters: f32,
    pub spatialization_strength: f32,
    pub volume_db: f32,
    pub playback_rate: f64,
    pub fade_in: Option<AudioTween>,
    pub reverb_zone: Option<AudioReverbZone>,
    pub occlusion: Option<AudioOcclusion>,
}

impl SpatialAudioPlaybackRequest {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_symbol("sound_id", &self.sound_id)?;
        if self.path.trim().is_empty() {
            return audio_invalid("path", "path must be non-empty");
        }
        validate_vec3("position", self.position)?;
        validate_vec3("velocity", self.velocity)?;
        if !self.min_distance_meters.is_finite() || self.min_distance_meters < 0.0 {
            return audio_invalid(
                "min_distance_meters",
                "min distance must be finite and >= 0",
            );
        }
        if !self.max_distance_meters.is_finite()
            || self.max_distance_meters <= self.min_distance_meters
        {
            return audio_invalid(
                "max_distance_meters",
                "max distance must be finite and greater than min distance",
            );
        }
        validate_unit("spatialization_strength", self.spatialization_strength)?;
        if !self.volume_db.is_finite() || !(-60.0..=12.0).contains(&self.volume_db) {
            return audio_invalid("volume_db", "volume must be finite in -60.0..=12.0 dB");
        }
        if !self.playback_rate.is_finite() || !(0.125..=4.0).contains(&self.playback_rate) {
            return audio_invalid(
                "playback_rate",
                "playback rate must be finite in 0.125..=4.0",
            );
        }
        if let Some(tween) = &self.fade_in {
            tween.validate()?;
        }
        if let Some(zone) = &self.reverb_zone {
            zone.validate()?;
        }
        if let Some(occlusion) = &self.occlusion {
            occlusion.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialAudioSnapshot {
    pub schema_version: u32,
    pub backend: String,
    pub listener: AudioListenerPose,
    pub emitters: Vec<SpatialEmitterSnapshot>,
    pub captured_at_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialEmitterSnapshot {
    pub sound_id: String,
    pub bus: AudioBus,
    pub path: String,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub distance_meters: f32,
    pub attenuation_db: f32,
    pub doppler_factor: f64,
    pub effective_playback_rate: f64,
    pub reverb_zone_id: Option<String>,
    pub reverb_wet_mix: f32,
    pub occlusion_factor: f32,
    pub occlusion_cutoff_hz: f64,
    pub spatialization_strength: f32,
    pub active_sounds: usize,
    pub sound_capacity: usize,
}
