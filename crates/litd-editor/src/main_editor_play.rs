use litd_editor::{
    asset_products::assign_vault_model_to_unit,
    asset_vault::AssetVaultCatalog,
    history::{EditorDocument, EditorObject},
    play::{PlayCameraConfig, PlayCameraPose, PlayCameraRig, PlaySession, PlaySessionState},
    shell::WorldViewCameraState,
};
use litd_engine::math::{Transform, Vec3};

use super::*;

const PLAYTEST_UNIT_MODEL_RELATIVE_PATH: &str =
    "sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf";

pub(super) const PLAY_CAMERA_RIGS: [PlayCameraRig; 4] = [
    PlayCameraRig::TopDown,
    PlayCameraRig::Isometric,
    PlayCameraRig::OverShoulder,
    PlayCameraRig::FirstPerson,
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PlayEditSnapshot {
    pub camera: WorldViewCameraState,
    pub selected_asset: usize,
}

impl EditorShellRuntime {
    pub(crate) fn toggle_playtest(&mut self) -> Result<(), EngineError> {
        if self.play_session.is_some() {
            self.stop_playtest()
        } else {
            self.start_playtest()
        }
    }

    pub(crate) fn set_play_camera_rig(&mut self, rig: PlayCameraRig) -> Result<(), EngineError> {
        if self.play_camera_rig == rig {
            return Ok(());
        }
        self.play_camera_rig = rig;
        let Some(session) = self.play_session.as_mut() else {
            self.shell_state.save_plan_with_runtime_state(
                &self.plan,
                self.world_layers,
                self.play_camera_rig,
            )?;
            return Ok(());
        };
        session.set_camera_rig(rig)?;
        let camera = play_camera_from_pose(&session.build.camera_pose)?;
        let mut next = self.plan.clone();
        next.viewport.camera = camera;
        next.validate()?;
        self.plan = next;
        self.shell_state.save_plan_with_runtime_state(
            &self.plan,
            self.world_layers,
            self.play_camera_rig,
        )?;
        self.play_rig_switch_count = self.play_rig_switch_count.saturating_add(1);
        tracing::info!(
            rig = ?rig,
            camera = %camera_summary(camera),
            generation = session.generation,
            "playtest camera rig switched"
        );
        Ok(())
    }

    pub(crate) fn play_camera_rig(&self) -> PlayCameraRig {
        self.play_camera_rig
    }

    pub(crate) fn play_running(&self) -> bool {
        self.play_session
            .as_ref()
            .map(|session| session.state == PlaySessionState::Running)
            .unwrap_or(false)
    }

    pub(crate) fn play_state_label(&self) -> &'static str {
        if self.play_running() {
            "Running"
        } else {
            "Stopped"
        }
    }

    pub(crate) fn play_session_summary(&self) -> String {
        self.play_session
            .as_ref()
            .map(|session| {
                format!(
                    "generation={};bodies={};visible_instances={};draw_batches={};rig={:?};player={};camera={};document={};revision={}",
                    session.generation,
                    session.build.spawned_bodies.len(),
                    session.build.visible_instance_ids.len(),
                    session.build.draw_batches,
                    session.build.camera_rig,
                    session.build.camera_pose.player_id,
                    play_camera_pose_summary(&session.build.camera_pose),
                    session.build.calyx_document_id,
                    session.build.source_revision_id
                )
            })
            .unwrap_or_else(|| "none".to_string())
    }

    pub(crate) fn saved_edit_snapshot_summary(&self) -> String {
        self.saved_edit_snapshot
            .map(|snapshot| {
                format!(
                    "camera={};selected_asset={}",
                    camera_summary(snapshot.camera),
                    snapshot.selected_asset
                )
            })
            .unwrap_or_else(|| "none".to_string())
    }

    pub(crate) fn selected_asset_index(&self) -> usize {
        self.selected_asset
    }

    pub(crate) fn snap_to_game_camera(&mut self) -> Result<(), EngineError> {
        let document = self.play_document_from_vault()?;
        let session = PlaySession::start_from_document(&document)?;
        let camera = play_camera_from_pose(&session.build.camera_pose)?;
        let mut next = self.plan.clone();
        next.viewport.camera = camera;
        next.validate()?;
        self.plan = next;
        self.shell_state.save_plan(&self.plan)?;
        tracing::info!(
            rig = ?self.play_camera_rig,
            camera = %camera_summary(camera),
            generation = session.generation,
            "viewport snapped to game camera"
        );
        Ok(())
    }

    fn start_playtest(&mut self) -> Result<(), EngineError> {
        let document = self.play_document_from_vault()?;
        self.start_playtest_from_document(document)
    }

    fn start_playtest_from_document(
        &mut self,
        document: EditorDocument,
    ) -> Result<(), EngineError> {
        if self.play_session.is_some() || self.saved_edit_snapshot.is_some() {
            return invalid_shell_runtime(
                "play.start",
                "cannot start Playtest while an existing session snapshot is active",
            );
        }
        self.require_world_validation(
            &document,
            litd_editor::world_validation::WorldValidationUseCase::Play,
        )?;
        let snapshot = PlayEditSnapshot {
            camera: self.plan.viewport.camera,
            selected_asset: self.selected_asset,
        };
        let session = PlaySession::start_from_document(&document)?;
        let camera = play_camera_from_pose(&session.build.camera_pose)?;
        let mut next = self.plan.clone();
        next.viewport.camera = camera;
        next.validate()?;

        self.plan = next;
        self.saved_edit_snapshot = Some(snapshot);
        self.play_session = Some(session);
        self.play_transition_count = self.play_transition_count.saturating_add(1);
        let selected_asset = self
            .selected_asset_entry()
            .map(|asset| asset.label.clone())
            .unwrap_or_else(|| "not_loaded_for_objects_mode".to_string());
        tracing::info!(
            rig = ?self.play_camera_rig,
            camera = %camera_summary(camera),
            selected_asset = %selected_asset,
            "playtest started and viewport snapped to runtime camera"
        );
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn start_playtest_with_document_for_test(
        &mut self,
        document: EditorDocument,
    ) -> Result<(), EngineError> {
        self.start_playtest_from_document(document)
    }

    fn stop_playtest(&mut self) -> Result<(), EngineError> {
        let Some(snapshot) = self.saved_edit_snapshot.take() else {
            return invalid_shell_runtime("play.stop", "missing saved edit camera snapshot");
        };
        let Some(session) = self.play_session.take() else {
            self.saved_edit_snapshot = Some(snapshot);
            return invalid_shell_runtime("play.stop", "missing running Playtest session");
        };
        if snapshot.selected_asset >= self.asset_entries.len() {
            self.play_session = Some(session);
            self.saved_edit_snapshot = Some(snapshot);
            return invalid_shell_runtime(
                "play.stop.selected_asset",
                format!(
                    "saved selected asset index {} exceeds asset entry count {}",
                    snapshot.selected_asset,
                    self.asset_entries.len()
                ),
            );
        }
        let mut next = self.plan.clone();
        next.viewport.camera = snapshot.camera;
        next.validate()?;
        self.plan = next;
        self.selected_asset = snapshot.selected_asset;
        self.shell_state.save_plan(&self.plan)?;
        self.play_transition_count = self.play_transition_count.saturating_add(1);
        tracing::info!(
            restored_camera = %camera_summary(snapshot.camera),
            restored_selected_asset = self.selected_asset,
            final_generation = session.generation,
            "playtest stopped and exact edit view restored"
        );
        Ok(())
    }

    pub(super) fn play_document_from_vault(&self) -> Result<EditorDocument, EngineError> {
        let catalog = AssetVaultCatalog::load_default_cached()?;
        let record = catalog
            .records()
            .iter()
            .find(|record| {
                record.source == "quaternius"
                    && record.relative_path == PLAYTEST_UNIT_MODEL_RELATIVE_PATH
            })
            .ok_or_else(|| EngineError::UiRenderInvalidConfig {
                field: "editor.shell.runtime.play.asset".to_string(),
                reason: format!(
                    "required Playtest unit model {} is absent from the real asset vault",
                    PLAYTEST_UNIT_MODEL_RELATIVE_PATH
                ),
            })?;
        let mut document = EditorDocument::new(
            "calyx://world/editor-shell-playtest",
            "editor-playtest-rev-1",
        )?;
        document.set_play_camera_config(PlayCameraConfig::preset(self.play_camera_rig))?;
        document.upsert_object(EditorObject::new(
            "world",
            "World",
            None,
            Transform::IDENTITY,
        ))?;
        document.upsert_object(
            EditorObject::new(
                "unit_playtest_hero",
                "Unit",
                Some("world".to_string()),
                Transform::from_translation(Vec3::new(3.0, 0.0, -6.0)),
            )
            .with_property("faction", "Dawnward"),
        )?;
        assign_vault_model_to_unit(&mut document, "unit_playtest_hero", record)?;
        Ok(document)
    }
}

pub(super) fn play_camera_rig_label(rig: PlayCameraRig) -> &'static str {
    match rig {
        PlayCameraRig::TopDown => "Top-down",
        PlayCameraRig::Isometric => "Isometric",
        PlayCameraRig::OverShoulder => "Over-shoulder",
        PlayCameraRig::FirstPerson => "First-person",
    }
}

fn play_camera_from_pose(pose: &PlayCameraPose) -> Result<WorldViewCameraState, EngineError> {
    let camera = WorldViewCameraState {
        target_world: pose.target_world,
        distance: pose.distance,
        yaw_degrees: pose.yaw_degrees,
        pitch_degrees: pose.pitch_degrees,
    };
    camera.validate()?;
    Ok(camera)
}

fn play_camera_pose_summary(pose: &PlayCameraPose) -> String {
    format!(
        "target={:.1},{:.1},{:.1};distance={:.1};yaw={:.1};pitch={:.1};fov={:.1}",
        pose.target_world.x,
        pose.target_world.y,
        pose.target_world.z,
        pose.distance,
        pose.yaw_degrees,
        pose.pitch_degrees,
        pose.fov_degrees
    )
}

fn camera_summary(camera: WorldViewCameraState) -> String {
    format!(
        "target={:.1},{:.1},{:.1};distance={:.1};yaw={:.1};pitch={:.1}",
        camera.target_world.x,
        camera.target_world.y,
        camera.target_world.z,
        camera.distance,
        camera.yaw_degrees,
        camera.pitch_degrees
    )
}
