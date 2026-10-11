//! Runtime audio foundation backed by Kira.

use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use kira::{
    AudioManager, AudioManagerSettings, Decibels, DefaultBackend, PlaybackRate, Tween,
    sound::static_sound::{StaticSoundData, StaticSoundHandle},
    track::{TrackBuilder, TrackHandle},
};
use serde::{Deserialize, Serialize};

use crate::EngineError;

#[path = "audio/audio_spatial.rs"]
mod audio_spatial;
#[path = "audio/audio_spatial_support.rs"]
mod audio_spatial_support;
#[path = "audio/audio_spatial_types.rs"]
mod audio_spatial_types;
pub use audio_spatial::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioBus {
    Sfx,
    Music,
    Voice,
}

impl AudioBus {
    pub const ALL: [Self; 3] = [Self::Sfx, Self::Music, Self::Voice];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sfx => "sfx",
            Self::Music => "music",
            Self::Voice => "voice",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioTween {
    pub duration_ms: u64,
}

impl AudioTween {
    pub fn immediate() -> Self {
        Self { duration_ms: 0 }
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.duration_ms > 60_000 {
            return audio_invalid(
                "tween.duration_ms",
                "duration must be <= 60000 ms for authored audio changes",
            );
        }
        Ok(())
    }

    fn to_kira(&self) -> Tween {
        Tween {
            duration: Duration::from_millis(self.duration_ms),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioPlaybackRequest {
    pub sound_id: String,
    pub path: String,
    pub bus: AudioBus,
    pub volume_db: f32,
    pub playback_rate: f64,
    pub fade_in: Option<AudioTween>,
}

impl AudioPlaybackRequest {
    pub fn validate(&self) -> Result<(), EngineError> {
        validate_symbol("sound_id", &self.sound_id)?;
        if self.path.trim().is_empty() {
            return audio_invalid("path", "path must be non-empty");
        }
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
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioBusSnapshot {
    pub bus: AudioBus,
    pub volume_db: f32,
    pub active_sounds: usize,
    pub sound_capacity: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioMixerSnapshot {
    pub schema_version: u32,
    pub backend: String,
    pub buses: Vec<AudioBusSnapshot>,
    pub played_sounds: Vec<PlayedSoundSnapshot>,
    pub captured_at_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayedSoundSnapshot {
    pub sound_id: String,
    pub bus: AudioBus,
    pub path: String,
    pub volume_db: f32,
    pub playback_rate: f64,
    pub fade_in_ms: Option<u64>,
}

pub struct EngineAudio {
    _manager: AudioManager<DefaultBackend>,
    buses: AudioBuses,
    played_sounds: Vec<PlayedSoundSnapshot>,
    spatial: SpatialAudioRuntime,
}

struct AudioBuses {
    sfx: AudioBusRuntime,
    music: AudioBusRuntime,
    voice: AudioBusRuntime,
}

struct AudioBusRuntime {
    bus: AudioBus,
    volume_db: f32,
    track: TrackHandle,
}

impl EngineAudio {
    pub fn new() -> Result<Self, EngineError> {
        let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|error| EngineError::AudioBackendUnavailable {
                backend: "kira.default.cpal".to_string(),
                reason: error.to_string(),
            })?;
        let buses = AudioBuses {
            sfx: new_bus(&mut manager, AudioBus::Sfx)?,
            music: new_bus(&mut manager, AudioBus::Music)?,
            voice: new_bus(&mut manager, AudioBus::Voice)?,
        };
        Ok(Self {
            _manager: manager,
            buses,
            played_sounds: Vec::new(),
            spatial: SpatialAudioRuntime::default(),
        })
    }

    pub fn play_static(
        &mut self,
        request: AudioPlaybackRequest,
    ) -> Result<StaticSoundHandle, EngineError> {
        request.validate()?;
        let path = Path::new(&request.path);
        if !path.is_file() {
            return Err(EngineError::AudioOperationFailed {
                op: "load_static_sound".to_string(),
                reason: format!("audio file does not exist: {}", path.display()),
            });
        }
        let mut sound = StaticSoundData::from_file(path).map_err(|error| {
            EngineError::AudioOperationFailed {
                op: "load_static_sound".to_string(),
                reason: error.to_string(),
            }
        })?;
        sound = sound
            .volume(Decibels(request.volume_db))
            .playback_rate(PlaybackRate(request.playback_rate));
        if let Some(tween) = &request.fade_in {
            sound = sound.fade_in_tween(tween.to_kira());
        }
        let handle = self
            .buses
            .bus_mut(request.bus)
            .track
            .play(sound)
            .map_err(|error| EngineError::AudioOperationFailed {
                op: format!("play_static.{}", request.bus.as_str()),
                reason: error.to_string(),
            })?;
        self.played_sounds.push(PlayedSoundSnapshot {
            sound_id: request.sound_id,
            bus: request.bus,
            path: request.path,
            volume_db: request.volume_db,
            playback_rate: request.playback_rate,
            fade_in_ms: request.fade_in.map(|tween| tween.duration_ms),
        });
        Ok(handle)
    }

    pub fn set_bus_volume(
        &mut self,
        bus: AudioBus,
        volume_db: f32,
        tween: AudioTween,
    ) -> Result<(), EngineError> {
        if !volume_db.is_finite() || !(-60.0..=12.0).contains(&volume_db) {
            return audio_invalid("bus.volume_db", "volume must be finite in -60.0..=12.0 dB");
        }
        tween.validate()?;
        let runtime = self.buses.bus_mut(bus);
        runtime
            .track
            .set_volume(Decibels(volume_db), tween.to_kira());
        runtime.volume_db = volume_db;
        Ok(())
    }

    pub fn snapshot(&self) -> AudioMixerSnapshot {
        AudioMixerSnapshot {
            schema_version: 1,
            backend: "kira.default.cpal".to_string(),
            buses: self.buses.snapshots(),
            played_sounds: self.played_sounds.clone(),
            captured_at_unix_ms: unix_ms(),
        }
    }
}

impl AudioBuses {
    fn bus_mut(&mut self, bus: AudioBus) -> &mut AudioBusRuntime {
        match bus {
            AudioBus::Sfx => &mut self.sfx,
            AudioBus::Music => &mut self.music,
            AudioBus::Voice => &mut self.voice,
        }
    }

    fn snapshots(&self) -> Vec<AudioBusSnapshot> {
        [&self.sfx, &self.music, &self.voice]
            .into_iter()
            .map(AudioBusRuntime::snapshot)
            .collect()
    }
}

impl AudioBusRuntime {
    fn snapshot(&self) -> AudioBusSnapshot {
        AudioBusSnapshot {
            bus: self.bus,
            volume_db: self.volume_db,
            active_sounds: self.track.num_sounds(),
            sound_capacity: self.track.sound_capacity(),
        }
    }
}

pub fn write_audio_snapshot(
    path: &Path,
    snapshot: &AudioMixerSnapshot,
) -> Result<AudioMixerSnapshot, EngineError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| EngineError::AudioOperationFailed {
            op: "snapshot.create_parent".to_string(),
            reason: format!("{}: {error}", parent.display()),
        })?;
    }
    let bytes =
        serde_json::to_vec_pretty(snapshot).map_err(|error| EngineError::SerializationFailed {
            format: "audio_snapshot_json",
            reason: error.to_string(),
        })?;
    fs::write(path, bytes).map_err(|error| EngineError::AudioOperationFailed {
        op: "snapshot.write".to_string(),
        reason: format!("{}: {error}", path.display()),
    })?;
    let readback = fs::read(path).map_err(|error| EngineError::AudioOperationFailed {
        op: "snapshot.readback".to_string(),
        reason: format!("{}: {error}", path.display()),
    })?;
    let decoded: AudioMixerSnapshot =
        serde_json::from_slice(&readback).map_err(|error| EngineError::AudioOperationFailed {
            op: "snapshot.decode".to_string(),
            reason: error.to_string(),
        })?;
    if decoded != *snapshot {
        return Err(EngineError::AudioReadbackMismatch {
            field: "snapshot".to_string(),
            expected: format!("{snapshot:?}"),
            actual: format!("{decoded:?}"),
        });
    }
    Ok(decoded)
}

fn new_bus(
    manager: &mut AudioManager<DefaultBackend>,
    bus: AudioBus,
) -> Result<AudioBusRuntime, EngineError> {
    let track = manager
        .add_sub_track(
            TrackBuilder::new()
                .sound_capacity(128)
                .sub_track_capacity(0),
        )
        .map_err(|error| EngineError::AudioOperationFailed {
            op: format!("add_sub_track.{}", bus.as_str()),
            reason: error.to_string(),
        })?;
    Ok(AudioBusRuntime {
        bus,
        volume_db: 0.0,
        track,
    })
}

fn validate_symbol(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() {
        return audio_invalid(field, "symbol cannot be empty");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return audio_invalid(field, format!("{value:?} must be an ASCII identifier"));
    }
    Ok(())
}

fn audio_invalid<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::AudioInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
