//! Play-in-editor session building and hot-reload state.

use serde::{Deserialize, Serialize};

use litd_engine::{
    EngineError,
    math::{FixedVec3, Vec2, Vec3},
    presentation::{PresentationBodyStyle, PresentationEntityKind, PresentationFrame},
    render::batching::ScreenRect,
    sim::{FixedTickSim, SimBodyId, SimConfig},
    sim_replay::SimSnapshot,
};

use crate::{
    asset_products::MODEL_PRODUCT_ID_PROPERTY,
    history::{EditorDocument, EditorObject, stable_fingerprint},
};

#[path = "play_runtime.rs"]
mod play_runtime;

use play_runtime::{apply_runtime_snapshot_to_build, presentation_readback};

/// Preview-sim body capacity for play builds (#1849). Heap-backed (#1295), so this
/// generous bound costs one allocation; a document with more Unit-kind objects fails
/// loud with `EngineError::SimCapacityExceeded` at spawn -- nothing is truncated.
pub const PLAY_SIM_MAX_BODIES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaySessionState {
    Stopped,
    Running,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlayCameraRig {
    #[default]
    TopDown,
    Isometric,
    OverShoulder,
    FirstPerson,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayCameraConfig {
    pub rig: PlayCameraRig,
    pub player_id: String,
    pub target_offset: Vec3,
    pub distance: f32,
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    pub fov_degrees: f32,
    pub follow_lag_seconds: f32,
    pub collision_radius: f32,
}

impl Default for PlayCameraConfig {
    fn default() -> Self {
        Self::preset(PlayCameraRig::default())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayCameraPose {
    pub player_id: String,
    pub target_world: Vec3,
    pub distance: f32,
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    pub fov_degrees: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayBuildArtifact {
    pub calyx_document_id: String,
    pub source_revision_id: String,
    pub source_fingerprint: String,
    pub camera_rig: PlayCameraRig,
    pub camera_config: PlayCameraConfig,
    pub camera_pose: PlayCameraPose,
    pub sim_seed: u64,
    pub spawned_bodies: Vec<PlayBodyBinding>,
    pub snapshot: SimSnapshot,
    pub presentation: PresentationFrame,
    pub visible_instance_ids: Vec<u64>,
    pub draw_batches: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayBodyBinding {
    pub object_id: String,
    pub body_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotReloadPlan {
    pub old_fingerprint: String,
    pub new_fingerprint: String,
    pub needs_reload: bool,
    pub changed_object_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaySession {
    pub state: PlaySessionState,
    pub generation: u64,
    pub source_document: EditorDocument,
    pub build: PlayBuildArtifact,
}

impl PlaySession {
    pub fn start(
        document: &EditorDocument,
        camera_rig: PlayCameraRig,
    ) -> Result<Self, EngineError> {
        Self::start_with_camera_config(document, PlayCameraConfig::preset(camera_rig))
    }

    pub fn start_from_document(document: &EditorDocument) -> Result<Self, EngineError> {
        document.validate()?;
        Ok(Self {
            state: PlaySessionState::Running,
            generation: 0,
            source_document: document.clone(),
            build: build_play_artifact_from_document(document)?,
        })
    }

    pub fn start_with_camera_config(
        document: &EditorDocument,
        camera_config: PlayCameraConfig,
    ) -> Result<Self, EngineError> {
        let document = document_with_camera_config(document, camera_config)?;
        Self::start_from_document(&document)
    }

    pub fn hot_reload(&mut self, document: &EditorDocument) -> Result<HotReloadPlan, EngineError> {
        let document = document_with_camera_config(document, self.build.camera_config.clone())?;
        let plan = plan_hot_reload(
            &self.build.source_fingerprint,
            &self.source_document,
            &document,
        );
        if plan.needs_reload {
            let previous_snapshot = self.build.snapshot.clone();
            let previous_bindings = self.build.spawned_bodies.clone();
            let mut next_build = build_play_artifact_from_document(&document)?;
            if previous_bindings == next_build.spawned_bodies {
                apply_runtime_snapshot_to_build(&mut next_build, &document, previous_snapshot)?;
            }
            self.build = next_build;
            self.source_document = document.clone();
            self.generation = self.generation.saturating_add(1);
        }
        Ok(plan)
    }

    pub fn run_runtime_ticks_with_body_velocity(
        &mut self,
        body_id: SimBodyId,
        velocity: FixedVec3,
        ticks: u64,
    ) -> Result<SimSnapshot, EngineError> {
        let mut sim = self.build.snapshot.restore::<PLAY_SIM_MAX_BODIES>()?;
        sim.set_body_velocity(body_id, velocity)?;
        sim.run_ticks(ticks)?;
        let snapshot = SimSnapshot::capture(&sim);
        apply_runtime_snapshot_to_build(&mut self.build, &self.source_document, snapshot.clone())?;
        Ok(snapshot)
    }

    pub fn require_save_allowed_during_tick(&self, mid_tick: bool) -> Result<(), EngineError> {
        if self.state == PlaySessionState::Running && mid_tick {
            return invalid_play(
                "save.mid_tick",
                "cannot save editor state during an active Playtest tick; pause or stop Playtest first",
            );
        }
        Ok(())
    }

    pub fn set_camera_rig(&mut self, camera_rig: PlayCameraRig) -> Result<(), EngineError> {
        self.set_camera_config(PlayCameraConfig::preset(camera_rig))
    }

    pub fn set_camera_config(
        &mut self,
        camera_config: PlayCameraConfig,
    ) -> Result<(), EngineError> {
        let document = document_with_camera_config(&self.source_document, camera_config)?;
        self.build = build_play_artifact_from_document(&document)?;
        self.source_document = document;
        self.generation = self.generation.saturating_add(1);
        Ok(())
    }
}

pub fn build_play_artifact(
    document: &EditorDocument,
    camera_rig: PlayCameraRig,
) -> Result<PlayBuildArtifact, EngineError> {
    build_play_artifact_with_camera_config(document, PlayCameraConfig::preset(camera_rig))
}

pub fn build_play_artifact_with_camera_config(
    document: &EditorDocument,
    camera_config: PlayCameraConfig,
) -> Result<PlayBuildArtifact, EngineError> {
    let document = document_with_camera_config(document, camera_config)?;
    build_play_artifact_from_document(&document)
}

fn build_play_artifact_from_document(
    document: &EditorDocument,
) -> Result<PlayBuildArtifact, EngineError> {
    document.validate()?;
    let camera_config = document.play_camera_config.clone();
    camera_config.validate()?;
    let source_fingerprint = stable_fingerprint(document)?;
    let mut sim = FixedTickSim::<PLAY_SIM_MAX_BODIES>::new(SimConfig::new(64)?, 0x1141)?;
    let mut styles = Vec::new();
    let mut bindings = Vec::new();

    for object in document
        .objects
        .values()
        .filter(|object| object.kind == "Unit")
    {
        let body_id = sim.spawn_body(
            fixed_from_translation(object),
            FixedVec3::ZERO,
            FixedVec3::ZERO,
        )?;
        styles.push(style_for(body_id, object, document)?);
        bindings.push(PlayBodyBinding {
            object_id: object.id.clone(),
            body_id: body_id.index(),
        });
    }
    if bindings.is_empty() {
        return invalid_play(
            "document.units",
            "play build requires at least one Unit object",
        );
    }
    let camera_pose = resolve_camera_pose(document, &camera_config)?;

    let snapshot = SimSnapshot::capture(&sim);
    let (presentation, visible_instance_ids, draw_batches) =
        presentation_readback(&snapshot, &styles)?;

    Ok(PlayBuildArtifact {
        calyx_document_id: document.calyx_document_id.clone(),
        source_revision_id: document.revision_id.clone(),
        source_fingerprint,
        camera_rig: camera_config.rig,
        camera_config,
        camera_pose,
        sim_seed: 0x1141,
        spawned_bodies: bindings,
        snapshot,
        presentation,
        visible_instance_ids,
        draw_batches,
    })
}

fn document_with_camera_config(
    document: &EditorDocument,
    camera_config: PlayCameraConfig,
) -> Result<EditorDocument, EngineError> {
    let mut document = document.clone();
    document.set_play_camera_config(camera_config)?;
    Ok(document)
}

impl PlayCameraConfig {
    pub fn preset(rig: PlayCameraRig) -> Self {
        match rig {
            PlayCameraRig::TopDown => Self {
                rig,
                player_id: "player_1".to_string(),
                target_offset: Vec3::ZERO,
                distance: 104.0,
                yaw_degrees: 0.0,
                pitch_degrees: -82.0,
                fov_degrees: 42.0,
                follow_lag_seconds: 0.0,
                collision_radius: 0.0,
            },
            PlayCameraRig::Isometric => Self {
                rig,
                player_id: "player_1".to_string(),
                target_offset: Vec3::ZERO,
                distance: 132.0,
                yaw_degrees: 45.0,
                pitch_degrees: -55.0,
                fov_degrees: 50.0,
                follow_lag_seconds: 0.08,
                collision_radius: 0.0,
            },
            PlayCameraRig::OverShoulder => Self {
                rig,
                player_id: "player_1".to_string(),
                target_offset: Vec3::new(0.0, 12.0, 40.0),
                distance: 28.0,
                yaw_degrees: 0.0,
                pitch_degrees: -15.0,
                fov_degrees: 62.0,
                follow_lag_seconds: 0.14,
                collision_radius: 1.25,
            },
            PlayCameraRig::FirstPerson => Self {
                rig,
                player_id: "player_1".to_string(),
                target_offset: Vec3::new(0.0, 7.0, 70.0),
                distance: 1.0,
                yaw_degrees: 0.0,
                pitch_degrees: -2.0,
                fov_degrees: 74.0,
                follow_lag_seconds: 0.0,
                collision_radius: 0.25,
            },
        }
    }

    pub fn for_player(mut self, player_id: impl Into<String>) -> Self {
        self.player_id = player_id.into();
        self
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.player_id.trim().is_empty() {
            return invalid_play("camera.player_id", "player_id must be non-empty");
        }
        if !self.target_offset.is_finite()
            || !self.distance.is_finite()
            || !self.yaw_degrees.is_finite()
            || !self.pitch_degrees.is_finite()
            || !self.fov_degrees.is_finite()
            || !self.follow_lag_seconds.is_finite()
            || !self.collision_radius.is_finite()
        {
            return invalid_play("camera", "camera config values must be finite");
        }
        if self.distance <= 0.0 {
            return invalid_play("camera.distance", "distance must be positive");
        }
        if self.pitch_degrees < -89.0 || self.pitch_degrees > 89.0 {
            return invalid_play("camera.pitch", "pitch must be within -89..89 degrees");
        }
        if self.fov_degrees < 1.0 || self.fov_degrees > 170.0 {
            return invalid_play("camera.fov", "fov_degrees must be within 1..170");
        }
        if self.follow_lag_seconds < 0.0 || self.follow_lag_seconds > 5.0 {
            return invalid_play(
                "camera.follow_lag",
                "follow lag must be within 0..5 seconds",
            );
        }
        if self.collision_radius < 0.0 {
            return invalid_play(
                "camera.collision_radius",
                "collision radius cannot be negative",
            );
        }
        Ok(())
    }
}

impl PlayCameraPose {
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.player_id.trim().is_empty() {
            return invalid_play("camera_pose.player_id", "player_id must be non-empty");
        }
        if !self.target_world.is_finite()
            || !self.distance.is_finite()
            || !self.yaw_degrees.is_finite()
            || !self.pitch_degrees.is_finite()
            || !self.fov_degrees.is_finite()
        {
            return invalid_play("camera_pose", "resolved camera pose values must be finite");
        }
        if self.distance <= 0.0 {
            return invalid_play("camera_pose.distance", "distance must be positive");
        }
        Ok(())
    }
}

fn resolve_camera_pose(
    document: &EditorDocument,
    config: &PlayCameraConfig,
) -> Result<PlayCameraPose, EngineError> {
    let anchor = document
        .objects
        .values()
        .filter(|object| object.kind == "Unit")
        .min_by(|a, b| a.id.cmp(&b.id))
        .map(|object| object.transform.translation)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.play.camera.anchor".to_string(),
            reason: "play camera config requires at least one Unit anchor".to_string(),
        })?;
    let pose = PlayCameraPose {
        player_id: config.player_id.clone(),
        target_world: anchor + config.target_offset,
        distance: config.distance,
        yaw_degrees: config.yaw_degrees,
        pitch_degrees: config.pitch_degrees,
        fov_degrees: config.fov_degrees,
    };
    pose.validate()?;
    Ok(pose)
}

pub fn plan_hot_reload(
    old_fingerprint: &str,
    old_document: &EditorDocument,
    new_document: &EditorDocument,
) -> HotReloadPlan {
    let new_fingerprint = stable_fingerprint(new_document).unwrap_or_default();
    let changed_object_ids = if old_fingerprint == new_fingerprint {
        Vec::new()
    } else {
        diff_changed_object_ids(old_document, new_document)
    };
    HotReloadPlan {
        old_fingerprint: old_fingerprint.to_string(),
        needs_reload: old_fingerprint != new_fingerprint,
        changed_object_ids,
        new_fingerprint,
    }
}

pub fn diff_changed_object_ids(before: &EditorDocument, after: &EditorDocument) -> Vec<String> {
    let mut ids = before
        .objects
        .keys()
        .chain(after.objects.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    ids.retain(|id| before.objects.get(id) != after.objects.get(id));
    ids.into_iter().collect()
}

fn fixed_from_translation(object: &EditorObject) -> FixedVec3 {
    let t = object.transform.translation;
    FixedVec3::from_i32s(t.x.round() as i32, t.y.round() as i32, t.z.round() as i32)
}

fn style_for(
    body_id: SimBodyId,
    object: &EditorObject,
    document: &EditorDocument,
) -> Result<PresentationBodyStyle, EngineError> {
    let product_id = object
        .properties
        .get(MODEL_PRODUCT_ID_PROPERTY)
        .ok_or_else(|| EngineError::UiRenderInvalidConfig {
            field: "editor.play.unit.model_product_id".to_string(),
            reason: format!("unit {} has no assigned model product", object.id),
        })?;
    let product = document.asset_products.get(product_id).ok_or_else(|| {
        EngineError::UiRenderInvalidConfig {
            field: "editor.play.unit.model_product".to_string(),
            reason: format!(
                "unit {} references missing model product {product_id}",
                object.id
            ),
        }
    })?;
    Ok(PresentationBodyStyle {
        body_id,
        entity_kind: PresentationEntityKind::Unit,
        mesh_id: product.product_id.clone(),
        material_id: object
            .properties
            .get("faction")
            .cloned()
            .unwrap_or_else(|| "default".to_string()),
        half_extents: Vec3::splat(0.5),
        screen_rect: ScreenRect::new(Vec2::new(0.42, 0.36), Vec2::new(0.58, 0.58))?,
        nearest_depth: 0.35,
        index_count: 144,
    })
}

fn invalid_play<T>(field: impl Into<String>, reason: impl Into<String>) -> Result<T, EngineError> {
    Err(EngineError::UiRenderInvalidConfig {
        field: format!("editor.play.{}", field.into()),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "play_tests.rs"]
mod tests;
