//! Kira-backed spatial audio runtime: listener pose, spatial emitter playback and the
//! snapshot source of truth.
//!
//! Split (#1466) into three cohesive modules; this one owns the runtime:
//!   - `audio_spatial_types.rs`   — request/snapshot value types + their validation
//!   - `audio_spatial_support.rs` — distance/doppler math, vector + mint helpers, validators
//!   - `audio_spatial.rs`         — Kira handles, playback, snapshot compute + persistence
//!
//! The types are re-exported here so `audio.rs`'s `pub use audio_spatial::*` keeps exporting
//! the exact same public surface as before the split.

use std::{fs, path::Path};

use kira::{
    Decibels, Mix, PlaybackRate,
    effect::{filter::FilterBuilder, reverb::ReverbBuilder},
    listener::ListenerHandle,
    sound::static_sound::StaticSoundData,
    track::{SpatialTrackBuilder, SpatialTrackDistances, SpatialTrackHandle},
};

use super::audio_spatial_support::{
    MAX_OCCLUSION_CUTOFF_HZ, distance, distance_attenuation_db, doppler_factor, mint_quat,
    mint_vec3,
};
use super::{AudioTween, EngineAudio, unix_ms};
use crate::EngineError;

pub use super::audio_spatial_types::{
    AudioListenerPose, AudioOcclusion, AudioReverbZone, SpatialAudioPlaybackRequest,
    SpatialAudioSnapshot, SpatialEmitterSnapshot,
};

#[derive(Default)]
pub(crate) struct SpatialAudioRuntime {
    listener: Option<ListenerHandle>,
    listener_pose: Option<AudioListenerPose>,
    emitters: Vec<SpatialEmitterRuntime>,
    snapshots: Vec<SpatialEmitterSnapshot>,
}

struct SpatialEmitterRuntime {
    _track: SpatialTrackHandle,
}

impl EngineAudio {
    pub fn set_listener_pose(&mut self, pose: AudioListenerPose) -> Result<(), EngineError> {
        pose.validate()?;
        let position = mint_vec3(pose.position);
        let orientation = mint_quat(pose.orientation_xyzw);
        if let Some(listener) = &mut self.spatial.listener {
            listener.set_position(position, AudioTween::immediate().to_kira());
            listener.set_orientation(orientation, AudioTween::immediate().to_kira());
        } else {
            let listener = self
                ._manager
                .add_listener(position, orientation)
                .map_err(|error| EngineError::AudioOperationFailed {
                    op: "add_spatial_listener".to_string(),
                    reason: error.to_string(),
                })?;
            self.spatial.listener = Some(listener);
        }
        self.spatial.listener_pose = Some(pose);
        Ok(())
    }

    pub fn play_spatial_static(
        &mut self,
        request: SpatialAudioPlaybackRequest,
    ) -> Result<(), EngineError> {
        request.validate()?;
        let listener_pose =
            self.spatial
                .listener_pose
                .clone()
                .ok_or_else(|| EngineError::AudioInvalidConfig {
                    field: "listener".to_string(),
                    reason: "listener pose must be set before spatial playback".to_string(),
                })?;
        let listener =
            self.spatial
                .listener
                .as_ref()
                .ok_or_else(|| EngineError::AudioInvalidConfig {
                    field: "listener".to_string(),
                    reason: "Kira listener handle is not initialized".to_string(),
                })?;
        let path = Path::new(&request.path);
        if !path.is_file() {
            return Err(EngineError::AudioOperationFailed {
                op: "load_spatial_static_sound".to_string(),
                reason: format!("audio file does not exist: {}", path.display()),
            });
        }

        let computed = compute_emitter_snapshot(&request, &listener_pose)?;
        let mut builder = SpatialTrackBuilder::new()
            .sound_capacity(128)
            .sub_track_capacity(0)
            .distances(SpatialTrackDistances {
                min_distance: request.min_distance_meters,
                max_distance: request.max_distance_meters,
            })
            .spatialization_strength(request.spatialization_strength)
            .volume(Decibels(computed.attenuation_db));
        if request.occlusion.is_some() {
            builder = builder.with_effect(
                FilterBuilder::new()
                    .cutoff(computed.occlusion_cutoff_hz)
                    .mix(Mix::WET),
            );
        }
        if let Some(zone) = &request.reverb_zone {
            builder = builder.with_effect(
                ReverbBuilder::new()
                    .feedback(zone.feedback)
                    .damping(zone.damping)
                    .mix(Mix(computed.reverb_wet_mix)),
            );
        }
        let mut track = self
            ._manager
            .add_spatial_sub_track(listener, mint_vec3(request.position), builder)
            .map_err(|error| EngineError::AudioOperationFailed {
                op: format!("add_spatial_sub_track.{}", request.bus.as_str()),
                reason: error.to_string(),
            })?;
        let mut sound = StaticSoundData::from_file(path).map_err(|error| {
            EngineError::AudioOperationFailed {
                op: "load_spatial_static_sound".to_string(),
                reason: error.to_string(),
            }
        })?;
        sound = sound
            .volume(Decibels(request.volume_db))
            .playback_rate(PlaybackRate(computed.effective_playback_rate));
        if let Some(tween) = &request.fade_in {
            sound = sound.fade_in_tween(tween.to_kira());
        }
        track
            .play(sound)
            .map_err(|error| EngineError::AudioOperationFailed {
                op: format!("play_spatial_static.{}", request.bus.as_str()),
                reason: error.to_string(),
            })?;
        let mut snapshot = computed;
        snapshot.active_sounds = track.num_sounds();
        snapshot.sound_capacity = track.sound_capacity();
        self.spatial.snapshots.push(snapshot);
        self.spatial
            .emitters
            .push(SpatialEmitterRuntime { _track: track });
        Ok(())
    }

    pub fn spatial_snapshot(&self) -> Result<SpatialAudioSnapshot, EngineError> {
        let listener =
            self.spatial
                .listener_pose
                .clone()
                .ok_or_else(|| EngineError::AudioInvalidConfig {
                    field: "listener".to_string(),
                    reason: "listener pose must be set before spatial snapshot".to_string(),
                })?;
        Ok(SpatialAudioSnapshot {
            schema_version: 1,
            backend: "kira.default.cpal".to_string(),
            listener,
            emitters: self.spatial.snapshots.clone(),
            captured_at_unix_ms: unix_ms(),
        })
    }
}

pub fn write_spatial_audio_snapshot(
    path: &Path,
    snapshot: &SpatialAudioSnapshot,
) -> Result<SpatialAudioSnapshot, EngineError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| EngineError::AudioOperationFailed {
            op: "spatial_snapshot.create_parent".to_string(),
            reason: format!("{}: {error}", parent.display()),
        })?;
    }
    let bytes =
        serde_json::to_vec_pretty(snapshot).map_err(|error| EngineError::SerializationFailed {
            format: "spatial_audio_snapshot_json",
            reason: error.to_string(),
        })?;
    fs::write(path, bytes).map_err(|error| EngineError::AudioOperationFailed {
        op: "spatial_snapshot.write".to_string(),
        reason: format!("{}: {error}", path.display()),
    })?;
    let readback = fs::read(path).map_err(|error| EngineError::AudioOperationFailed {
        op: "spatial_snapshot.readback".to_string(),
        reason: format!("{}: {error}", path.display()),
    })?;
    let decoded: SpatialAudioSnapshot =
        serde_json::from_slice(&readback).map_err(|error| EngineError::AudioOperationFailed {
            op: "spatial_snapshot.decode".to_string(),
            reason: error.to_string(),
        })?;
    if decoded != *snapshot {
        return Err(EngineError::AudioReadbackMismatch {
            field: "spatial_snapshot".to_string(),
            expected: format!("{snapshot:?}"),
            actual: format!("{decoded:?}"),
        });
    }
    Ok(decoded)
}

pub fn compute_spatial_emitter_snapshot_for_test(
    request: &SpatialAudioPlaybackRequest,
    listener: &AudioListenerPose,
) -> Result<SpatialEmitterSnapshot, EngineError> {
    request.validate()?;
    listener.validate()?;
    compute_emitter_snapshot(request, listener)
}

fn compute_emitter_snapshot(
    request: &SpatialAudioPlaybackRequest,
    listener: &AudioListenerPose,
) -> Result<SpatialEmitterSnapshot, EngineError> {
    let distance_meters = distance(listener.position, request.position);
    let attenuation_db = distance_attenuation_db(
        distance_meters,
        request.min_distance_meters,
        request.max_distance_meters,
    ) + request
        .occlusion
        .as_ref()
        .map(AudioOcclusion::volume_loss_db)
        .unwrap_or(0.0);
    let doppler_factor = doppler_factor(
        listener.position,
        listener.velocity,
        request.position,
        request.velocity,
    );
    let effective_playback_rate = (request.playback_rate * doppler_factor).clamp(0.125, 4.0);
    let reverb_wet_mix = request
        .reverb_zone
        .as_ref()
        .map(|zone| zone.wet_mix_at(request.position))
        .unwrap_or(0.0);
    let occlusion_factor = request
        .occlusion
        .as_ref()
        .map(|occlusion| occlusion.factor)
        .unwrap_or(0.0);
    let occlusion_cutoff_hz = request
        .occlusion
        .as_ref()
        .map(AudioOcclusion::cutoff_hz)
        .unwrap_or(MAX_OCCLUSION_CUTOFF_HZ);

    Ok(SpatialEmitterSnapshot {
        sound_id: request.sound_id.clone(),
        bus: request.bus,
        path: request.path.clone(),
        position: request.position,
        velocity: request.velocity,
        distance_meters,
        attenuation_db,
        doppler_factor,
        effective_playback_rate,
        reverb_zone_id: request
            .reverb_zone
            .as_ref()
            .map(|zone| zone.zone_id.clone()),
        reverb_wet_mix,
        occlusion_factor,
        occlusion_cutoff_hz,
        spatialization_strength: request.spatialization_strength,
        active_sounds: 0,
        sound_capacity: 128,
    })
}
