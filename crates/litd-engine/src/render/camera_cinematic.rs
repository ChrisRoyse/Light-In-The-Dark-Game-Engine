use serde::{Deserialize, Serialize};

use super::{RtsCameraConfig, RtsCameraState, invalid_camera};
use crate::EngineError;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CinematicKeyframe {
    pub time_seconds: f32,
    pub state: RtsCameraState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CinematicCameraPath {
    pub keyframes: Vec<CinematicKeyframe>,
}

impl CinematicCameraPath {
    pub fn validate(&self, config: &RtsCameraConfig) -> Result<(), EngineError> {
        if self.keyframes.len() < 2 {
            return invalid_camera("cinematic.keyframes", "requires at least two keyframes");
        }
        let mut previous = f32::NEG_INFINITY;
        for keyframe in &self.keyframes {
            if !keyframe.time_seconds.is_finite() || keyframe.time_seconds <= previous {
                return invalid_camera(
                    "cinematic.time",
                    "keyframe times must be finite and sorted",
                );
            }
            keyframe.state.validate(config)?;
            previous = keyframe.time_seconds;
        }
        Ok(())
    }

    pub fn sample(
        &self,
        config: &RtsCameraConfig,
        time_seconds: f32,
    ) -> Result<RtsCameraState, EngineError> {
        self.validate(config)?;
        if !time_seconds.is_finite() {
            return invalid_camera("cinematic.sample_time", "must be finite");
        }
        let first = self.keyframes.first().unwrap();
        if time_seconds <= first.time_seconds {
            return Ok(first.state);
        }
        for window in self.keyframes.windows(2) {
            let a = window[0];
            let b = window[1];
            if time_seconds <= b.time_seconds {
                let t = (time_seconds - a.time_seconds) / (b.time_seconds - a.time_seconds);
                return Ok(RtsCameraState {
                    target: a.state.target.lerp(b.state.target, t),
                    yaw_rad: lerp_angle(a.state.yaw_rad, b.state.yaw_rad, t),
                    pitch_rad: lerp_scalar(a.state.pitch_rad, b.state.pitch_rad, t),
                    distance: lerp_scalar(a.state.distance, b.state.distance, t),
                });
            }
        }
        Ok(self.keyframes.last().unwrap().state)
    }
}

fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let delta =
        (b - a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    a + delta * t
}

fn lerp_scalar(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
