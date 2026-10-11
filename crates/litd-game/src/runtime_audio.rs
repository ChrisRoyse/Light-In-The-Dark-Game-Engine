use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use litd_engine::{
    EngineError,
    audio::{
        AudioBus, AudioListenerPose, AudioOcclusion, AudioReverbZone, EngineAudio,
        SpatialAudioPlaybackRequest, SpatialAudioSnapshot,
    },
    game_package::{RuntimePackageAssetRef, RuntimePackageReadback},
};
use serde::{Deserialize, Serialize};

#[path = "runtime_audio_editor_emitters.rs"]
mod runtime_audio_editor_emitters;
use runtime_audio_editor_emitters::sound_emitters_from_editor_document;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeAudioState {
    pub schema_version: u32,
    pub status: RuntimeAudioStatus,
    pub cue_count: usize,
    pub triggered_count: usize,
    pub listener: Option<AudioListenerPose>,
    pub cues: Vec<RuntimeAudioCueState>,
    pub spatial_snapshot: Option<SpatialAudioSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAudioStatus {
    NoAudioCues,
    Ready,
    Playing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeAudioCueState {
    pub trigger_id: String,
    pub cue_id: String,
    pub kind: RuntimeAudioCueKind,
    pub asset_product_id: String,
    pub resolved_path: String,
    pub bus: AudioBus,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub direction: [f32; 3],
    pub cone_inner_degrees: f32,
    pub cone_outer_degrees: f32,
    pub reverb_zone_id: Option<String>,
    pub occlusion_factor: f32,
    pub triggered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAudioCueKind {
    PositionalSound,
    Music,
    Ambience,
}

pub struct GameRuntimeAudio {
    plan: RuntimeAudioPlan,
    engine: Option<EngineAudio>,
    state: RuntimeAudioState,
}

#[derive(Debug, Clone)]
struct RuntimeAudioPlan {
    listener: AudioListenerPose,
    cues: Vec<RuntimeAudioCuePlan>,
}

#[derive(Debug, Clone)]
struct RuntimeAudioCuePlan {
    trigger_id: String,
    cue_id: String,
    kind: RuntimeAudioCueKind,
    asset_product_id: String,
    resolved_path: PathBuf,
    bus: AudioBus,
    position: [f32; 3],
    velocity: [f32; 3],
    direction: [f32; 3],
    cone_inner_degrees: f32,
    cone_outer_degrees: f32,
    min_distance_meters: f32,
    max_distance_meters: f32,
    spatialization_strength: f32,
    volume_db: f32,
    playback_rate: f64,
    reverb_zone: Option<AudioReverbZone>,
    occlusion: Option<AudioOcclusion>,
}

#[derive(Debug, Clone, Deserialize)]
struct RuntimeAudioDocument {
    #[serde(default)]
    listener: Option<AudioListenerPose>,
    #[serde(default)]
    cues: Vec<RuntimeAudioCueDocument>,
}

#[derive(Debug, Clone, Deserialize)]
struct RuntimeAudioCueDocument {
    trigger_id: String,
    cue_id: String,
    kind: RuntimeAudioCueKind,
    asset_product_id: String,
    #[serde(default)]
    bus: Option<AudioBus>,
    position: [f32; 3],
    #[serde(default)]
    velocity: [f32; 3],
    #[serde(default = "default_direction")]
    direction: [f32; 3],
    #[serde(default = "default_cone_inner_degrees")]
    cone_inner_degrees: f32,
    #[serde(default = "default_cone_outer_degrees")]
    cone_outer_degrees: f32,
    min_distance_meters: f32,
    max_distance_meters: f32,
    spatialization_strength: f32,
    volume_db: f32,
    playback_rate: f64,
    #[serde(default)]
    reverb_zone: Option<AudioReverbZone>,
    #[serde(default)]
    occlusion: Option<AudioOcclusion>,
}

impl GameRuntimeAudio {
    pub fn from_package(package: &RuntimePackageReadback) -> Result<Self, EngineError> {
        let plan = build_runtime_audio_plan(package)?;
        let state = RuntimeAudioState {
            schema_version: 1,
            status: if plan.cues.is_empty() {
                RuntimeAudioStatus::NoAudioCues
            } else {
                RuntimeAudioStatus::Ready
            },
            cue_count: plan.cues.len(),
            triggered_count: 0,
            listener: if plan.cues.is_empty() {
                None
            } else {
                Some(plan.listener.clone())
            },
            cues: plan.cues.iter().map(RuntimeAudioCueState::from).collect(),
            spatial_snapshot: None,
        };
        Ok(Self {
            plan,
            engine: None,
            state,
        })
    }

    pub fn start(&mut self) -> Result<(), EngineError> {
        if self.plan.cues.is_empty() {
            return Ok(());
        }
        let mut engine = EngineAudio::new()?;
        engine.set_listener_pose(self.plan.listener.clone())?;
        for cue in &self.plan.cues {
            engine.play_spatial_static(cue.request())?;
        }
        let snapshot = engine.spatial_snapshot()?;
        for cue in &mut self.state.cues {
            cue.triggered = true;
        }
        self.state.status = RuntimeAudioStatus::Playing;
        self.state.triggered_count = self.plan.cues.len();
        self.state.spatial_snapshot = Some(snapshot);
        self.engine = Some(engine);
        Ok(())
    }

    pub fn state(&self) -> &RuntimeAudioState {
        &self.state
    }
}

fn build_runtime_audio_plan(
    package: &RuntimePackageReadback,
) -> Result<RuntimeAudioPlan, EngineError> {
    let mut listener = AudioListenerPose::default();
    let mut documents = Vec::new();
    if let Some(value) = package.document.get("runtime_audio") {
        let document: RuntimeAudioDocument =
            serde_json::from_value(value.clone()).map_err(|error| {
                audio_error(
                    "document.runtime_audio",
                    format!("invalid runtime audio block: {error}"),
                )
            })?;
        if let Some(pose) = document.listener.clone() {
            listener = pose;
        }
        documents.extend(document.cues);
    }
    documents.extend(sound_emitters_from_editor_document(&package.document)?);
    if documents.is_empty() {
        return Ok(RuntimeAudioPlan {
            listener,
            cues: Vec::new(),
        });
    }
    listener.validate()?;
    let package_root = Path::new(&package.package_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let assets = package
        .assets
        .iter()
        .map(|asset| (asset.product_id.as_str(), asset))
        .collect::<BTreeMap<_, _>>();
    let mut cues = Vec::new();
    for cue in documents {
        cues.push(resolve_cue(cue, &assets, package_root)?);
    }
    Ok(RuntimeAudioPlan { listener, cues })
}

fn resolve_cue(
    cue: RuntimeAudioCueDocument,
    assets: &BTreeMap<&str, &RuntimePackageAssetRef>,
    package_root: &Path,
) -> Result<RuntimeAudioCuePlan, EngineError> {
    validate_symbol("trigger_id", &cue.trigger_id)?;
    validate_symbol("cue_id", &cue.cue_id)?;
    let asset = assets.get(cue.asset_product_id.as_str()).ok_or_else(|| {
        audio_error(
            "cue.asset_product_id",
            format!("missing package audio asset {}", cue.asset_product_id),
        )
    })?;
    let resolved_path = package_root.join(&asset.source_relative_path);
    let bus = cue.bus.unwrap_or_else(|| cue.kind.default_bus());
    let plan = RuntimeAudioCuePlan {
        trigger_id: cue.trigger_id,
        cue_id: cue.cue_id,
        kind: cue.kind,
        asset_product_id: cue.asset_product_id,
        resolved_path,
        bus,
        position: cue.position,
        velocity: cue.velocity,
        direction: cue.direction,
        cone_inner_degrees: cue.cone_inner_degrees,
        cone_outer_degrees: cue.cone_outer_degrees,
        min_distance_meters: cue.min_distance_meters,
        max_distance_meters: cue.max_distance_meters,
        spatialization_strength: cue.spatialization_strength,
        volume_db: cue.volume_db,
        playback_rate: cue.playback_rate,
        reverb_zone: cue.reverb_zone,
        occlusion: cue.occlusion,
    };
    plan.request().validate()?;
    Ok(plan)
}

impl RuntimeAudioCuePlan {
    fn request(&self) -> SpatialAudioPlaybackRequest {
        SpatialAudioPlaybackRequest {
            sound_id: self.cue_id.clone(),
            path: self.resolved_path.display().to_string(),
            bus: self.bus,
            position: self.position,
            velocity: self.velocity,
            min_distance_meters: self.min_distance_meters,
            max_distance_meters: self.max_distance_meters,
            spatialization_strength: self.spatialization_strength,
            volume_db: self.volume_db,
            playback_rate: self.playback_rate,
            fade_in: None,
            reverb_zone: self.reverb_zone.clone(),
            occlusion: self.occlusion.clone(),
        }
    }
}

impl RuntimeAudioCueKind {
    fn default_bus(self) -> AudioBus {
        match self {
            Self::PositionalSound => AudioBus::Sfx,
            Self::Music => AudioBus::Music,
            Self::Ambience => AudioBus::Sfx,
        }
    }
}

impl From<&RuntimeAudioCuePlan> for RuntimeAudioCueState {
    fn from(cue: &RuntimeAudioCuePlan) -> Self {
        Self {
            trigger_id: cue.trigger_id.clone(),
            cue_id: cue.cue_id.clone(),
            kind: cue.kind,
            asset_product_id: cue.asset_product_id.clone(),
            resolved_path: cue.resolved_path.display().to_string(),
            bus: cue.bus,
            position: cue.position,
            velocity: cue.velocity,
            direction: cue.direction,
            cone_inner_degrees: cue.cone_inner_degrees,
            cone_outer_degrees: cue.cone_outer_degrees,
            reverb_zone_id: cue.reverb_zone.as_ref().map(|zone| zone.zone_id.clone()),
            occlusion_factor: cue
                .occlusion
                .as_ref()
                .map(|occlusion| occlusion.factor)
                .unwrap_or(0.0),
            triggered: false,
        }
    }
}

fn default_direction() -> [f32; 3] {
    [0.0, 0.0, -1.0]
}

fn default_cone_inner_degrees() -> f32 {
    360.0
}

fn default_cone_outer_degrees() -> f32 {
    360.0
}

fn validate_symbol(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() {
        return Err(audio_error(field, "symbol cannot be empty"));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(audio_error(
            field,
            format!("{value:?} must be an ASCII identifier"),
        ));
    }
    Ok(())
}

fn audio_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let field = format!("litd_game.runtime_audio.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-GAME-RUNTIME-AUDIO",
        "litd-game runtime audio failed closed"
    );
    EngineError::GamePackageInvalidConfig { field, reason }
}
