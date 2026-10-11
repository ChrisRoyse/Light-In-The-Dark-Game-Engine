//! Spatial-audio math and field validators (#1466 split of `audio_spatial.rs`).
//!
//! Pure functions: the distance/doppler curves, the `[f32; 3]` vector helpers, the mint
//! interop conversions and the finite/unit-range validators. No Kira handles and no engine
//! state — the runtime lives in `audio_spatial.rs`, the value types in
//! `audio_spatial_types.rs`.

use super::audio_invalid;
use crate::EngineError;

pub(super) const SPEED_OF_SOUND_METERS_PER_SECOND: f32 = 343.0;
pub(super) const MAX_OCCLUSION_LOSS_DB: f32 = 24.0;
pub(super) const MIN_OCCLUSION_CUTOFF_HZ: f64 = 1200.0;
pub(super) const MAX_OCCLUSION_CUTOFF_HZ: f64 = 20_000.0;

pub(super) fn doppler_factor(
    listener_position: [f32; 3],
    listener_velocity: [f32; 3],
    source_position: [f32; 3],
    source_velocity: [f32; 3],
) -> f64 {
    let delta = sub(source_position, listener_position);
    let distance = vec_len(delta);
    if distance <= f32::EPSILON {
        return 1.0;
    }
    let direction = scale(delta, 1.0 / distance);
    let radial_away = dot(sub(source_velocity, listener_velocity), direction);
    let denominator = (SPEED_OF_SOUND_METERS_PER_SECOND + radial_away).max(1.0);
    (f64::from(SPEED_OF_SOUND_METERS_PER_SECOND) / f64::from(denominator)).clamp(0.5, 2.0)
}

pub(super) fn distance_attenuation_db(distance: f32, min_distance: f32, max_distance: f32) -> f32 {
    if distance <= min_distance {
        0.0
    } else if distance >= max_distance {
        -60.0
    } else {
        -60.0 * ((distance - min_distance) / (max_distance - min_distance))
    }
}

pub(super) fn validate_vec3(field: &str, value: [f32; 3]) -> Result<(), EngineError> {
    if value.into_iter().all(f32::is_finite) {
        Ok(())
    } else {
        audio_invalid(field, "vector components must be finite")
    }
}

pub(super) fn validate_unit(field: &str, value: f32) -> Result<(), EngineError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        audio_invalid(field, "value must be finite in 0.0..=1.0")
    }
}

pub(super) fn validate_unit_f64(field: &str, value: f64) -> Result<(), EngineError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        audio_invalid(field, "value must be finite in 0.0..=1.0")
    }
}

pub(super) fn mint_vec3(value: [f32; 3]) -> mint::Vector3<f32> {
    mint::Vector3 {
        x: value[0],
        y: value[1],
        z: value[2],
    }
}

pub(super) fn mint_quat(value: [f32; 4]) -> mint::Quaternion<f32> {
    mint::Quaternion {
        v: mint::Vector3 {
            x: value[0],
            y: value[1],
            z: value[2],
        },
        s: value[3],
    }
}

pub(super) fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    vec_len(sub(a, b))
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(value: [f32; 3], factor: f32) -> [f32; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn vec_len(value: [f32; 3]) -> f32 {
    dot(value, value).sqrt()
}
