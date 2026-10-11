#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    env,
    path::{Path, PathBuf},
};

use litd_editor::{
    editor_commands::{
        EditorCommandBindings, default_editor_command_bindings, default_editor_command_registry,
    },
    editor_state_store::EditorStateStore,
    play::PlayCameraRig,
    shell::{
        EditorMode, EditorShellPersistence, EditorShellPlan, WorldViewCameraBookmark,
        WorldViewCameraState, persist_shell_state, previous_schema_eight_stock_camera,
        restore_shell_state, restore_world_view_camera_bookmark, save_world_view_camera_bookmark,
        switch_editor_mode, validate_world_view_camera_bookmarks,
        world_view_camera_bookmark_summary,
    },
};
use litd_engine::{EngineError, math::Vec3, render::ui::UiRenderSettings};
use serde::{Deserialize, Serialize};
use tracing::info;

use super::main_editor_world_layers::WorldLayerVisibility;
#[cfg(test)]
use litd_editor::editor_commands::{EditorCommandRegistry, EditorKeyChord};

const STATE_SCHEMA_VERSION: u32 = 12;
const SCHEMA_ELEVEN_VERSION: u32 = 11;
const SCHEMA_TEN_VERSION: u32 = 10;
const SCHEMA_NINE_VERSION: u32 = 9;
const SCHEMA_EIGHT_VERSION: u32 = 8;
const SCHEMA_SEVEN_VERSION: u32 = 7;
const SCHEMA_SIX_VERSION: u32 = 6;
const SCHEMA_FIVE_VERSION: u32 = 5;
const SCHEMA_FOUR_VERSION: u32 = 4;
const SCHEMA_THREE_VERSION: u32 = 3;
const SCHEMA_TWO_VERSION: u32 = 2;
const LEGACY_SCHEMA_VERSION: u32 = 1;
const STATE_RECORD_KIND: &str = "litd.editor.shell_state";
const STATE_ENV: &str = "LITD_EDITOR_SHELL_STATE";
const MODE_ENV: &str = "LITD_EDITOR_MODE";
#[cfg(test)]
static TEST_STATE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct ShellStateStore {
    path: PathBuf,
    loaded_from_disk: bool,
    save_count: u32,
    world_layers: WorldLayerVisibility,
    play_camera_rig: PlayCameraRig,
    camera_bookmarks: Vec<WorldViewCameraBookmark>,
    command_keybindings: EditorCommandBindings,
}

#[derive(Debug, Serialize, Deserialize)]
struct ShellStateFile {
    schema_version: u32,
    record_kind: String,
    persistence: EditorShellPersistence,
    #[serde(default)]
    world_layers: Option<WorldLayerVisibility>,
    #[serde(default)]
    play_camera_rig: Option<PlayCameraRig>,
    #[serde(default)]
    camera_bookmarks: Option<Vec<WorldViewCameraBookmark>>,
    #[serde(default)]
    command_keybindings: Option<EditorCommandBindings>,
}

impl ShellStateStore {
    pub fn from_env() -> Result<Self, EngineError> {
        let path = match env::var(STATE_ENV) {
            Ok(value) if value.trim().is_empty() => {
                return invalid_persistence("path.env", "LITD_EDITOR_SHELL_STATE is empty");
            }
            Ok(value) => absolute_or_workspace_relative(PathBuf::from(value)),
            Err(env::VarError::NotPresent) => default_state_path_for_build(),
            Err(err) => {
                return invalid_persistence("path.env", format!("{STATE_ENV}: {err}"));
            }
        };
        Ok(Self::new(path))
    }

    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            loaded_from_disk: false,
            save_count: 0,
            world_layers: WorldLayerVisibility::default(),
            play_camera_rig: PlayCameraRig::default(),
            camera_bookmarks: Vec::new(),
            command_keybindings: default_editor_command_bindings()
                .expect("default editor command bindings must validate"),
        }
    }

    pub fn load_or_initialize(
        &mut self,
        settings: UiRenderSettings,
        default_plan: EditorShellPlan,
    ) -> Result<EditorShellPlan, EngineError> {
        settings.validate()?;
        let mut plan = if self.state_store().shell_state_exists()? {
            self.loaded_from_disk = true;
            self.load_plan(settings, &default_plan)?
        } else {
            self.loaded_from_disk = false;
            default_plan
        };
        if let Some(mode) = mode_override_from_env()? {
            plan = switch_editor_mode(&plan, mode)?;
        }
        self.save_plan(&plan)?;
        Ok(plan)
    }

    pub fn save_plan(&mut self, plan: &EditorShellPlan) -> Result<(), EngineError> {
        self.save_plan_with_world_layers(plan, self.world_layers)
    }

    pub fn save_plan_with_world_layers(
        &mut self,
        plan: &EditorShellPlan,
        world_layers: WorldLayerVisibility,
    ) -> Result<(), EngineError> {
        self.save_plan_with_runtime_state(plan, world_layers, self.play_camera_rig)
    }

    pub fn save_plan_with_runtime_state(
        &mut self,
        plan: &EditorShellPlan,
        world_layers: WorldLayerVisibility,
        play_camera_rig: PlayCameraRig,
    ) -> Result<(), EngineError> {
        // Persist FIRST, commit the in-memory copies only after the durable write
        // succeeds (#1489). Assigning self.world_layers/play_camera_rig before the
        // put_shell_state below meant a failed save left in-memory state ahead of disk
        // with no rollback — the exact silent desync #800 §4 forbids.
        let persistence = persist_shell_state(plan)?;
        let record = ShellStateFile {
            schema_version: STATE_SCHEMA_VERSION,
            record_kind: STATE_RECORD_KIND.to_string(),
            persistence,
            world_layers: Some(world_layers),
            play_camera_rig: Some(play_camera_rig),
            camera_bookmarks: Some(self.camera_bookmarks.clone()),
            command_keybindings: Some(self.command_keybindings.clone()),
        };
        let json = serde_json::to_string_pretty(&record).map_err(|error| {
            EngineError::SerializationFailed {
                format: "json",
                reason: format!("serialize shell state for Aster: {error}"),
            }
        })?;
        self.state_store().put_shell_state(json.as_bytes())?;
        self.world_layers = world_layers;
        self.play_camera_rig = play_camera_rig;
        self.save_count = self.save_count.saturating_add(1);
        Ok(())
    }

    pub fn world_layers(&self) -> WorldLayerVisibility {
        self.world_layers
    }

    pub fn play_camera_rig(&self) -> PlayCameraRig {
        self.play_camera_rig
    }

    pub fn camera_bookmark_count(&self) -> usize {
        self.camera_bookmarks.len()
    }

    pub fn camera_bookmark_summary(&self) -> String {
        world_view_camera_bookmark_summary(&self.camera_bookmarks)
    }

    pub fn command_keybindings(&self) -> &EditorCommandBindings {
        &self.command_keybindings
    }

    #[cfg(test)]
    pub fn replace_command_keybinding(
        &mut self,
        registry: &EditorCommandRegistry,
        command_id: &str,
        key_chord: EditorKeyChord,
    ) -> Result<(), EngineError> {
        let mut next = self.command_keybindings.clone();
        next.replace_binding(registry, command_id, key_chord)?;
        self.command_keybindings = next;
        Ok(())
    }

    pub fn save_camera_bookmark(
        &mut self,
        slot: u8,
        camera: WorldViewCameraState,
    ) -> Result<(), EngineError> {
        save_world_view_camera_bookmark(&mut self.camera_bookmarks, slot, camera)
    }

    /// Snapshot the in-memory bookmark store so a caller can roll back an optimistic
    /// mutation if the subsequent persist fails (#1489). `save_plan` serializes these
    /// bookmarks, so they must be mutated before the save and reverted on failure.
    pub fn camera_bookmarks_snapshot(&self) -> Vec<WorldViewCameraBookmark> {
        self.camera_bookmarks.clone()
    }

    pub fn restore_camera_bookmarks(&mut self, snapshot: Vec<WorldViewCameraBookmark>) {
        self.camera_bookmarks = snapshot;
    }

    pub fn restore_camera_bookmark(&self, slot: u8) -> Result<WorldViewCameraState, EngineError> {
        restore_world_view_camera_bookmark(&self.camera_bookmarks, slot)
    }

    pub fn path_display(&self) -> String {
        self.path.display().to_string()
    }

    pub fn loaded_from_disk(&self) -> bool {
        self.loaded_from_disk
    }

    pub fn save_count(&self) -> u32 {
        self.save_count
    }

    fn load_plan(
        &mut self,
        settings: UiRenderSettings,
        default_plan: &EditorShellPlan,
    ) -> Result<EditorShellPlan, EngineError> {
        let bytes = self
            .state_store()
            .read_shell_state()?
            .ok_or_else(|| persistence_error("aster.read", "shell state row missing"))?;
        let text = String::from_utf8(bytes).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("decode shell state UTF-8 from Aster: {error}"),
        })?;
        let mut record = serde_json::from_str::<ShellStateFile>(&text).map_err(|error| {
            EngineError::SerializationFailed {
                format: "json",
                reason: format!("parse shell state from Aster: {error}"),
            }
        })?;
        if record.record_kind != STATE_RECORD_KIND {
            return invalid_persistence(
                "record_kind",
                format!(
                    "{} has record_kind '{}', expected '{STATE_RECORD_KIND}'",
                    self.path.display(),
                    record.record_kind
                ),
            );
        }
        match record.schema_version {
            STATE_SCHEMA_VERSION => {}
            SCHEMA_ELEVEN_VERSION => migrate_previous_stock_camera(
                &self.path,
                &mut record.persistence,
                schema_eleven_top_down_default_camera(),
                default_plan.viewport.camera,
                "schema 11 ED11 top-down profile",
            )?,
            SCHEMA_TEN_VERSION => {}
            SCHEMA_NINE_VERSION => {}
            SCHEMA_EIGHT_VERSION => migrate_previous_stock_camera(
                &self.path,
                &mut record.persistence,
                previous_schema_eight_stock_camera(),
                default_plan.viewport.camera,
                "schema 8 ED11 pre-bird-eye profile",
            )?,
            SCHEMA_SEVEN_VERSION => {}
            SCHEMA_SIX_VERSION => {}
            SCHEMA_FIVE_VERSION => {}
            SCHEMA_FOUR_VERSION => migrate_previous_stock_camera(
                &self.path,
                &mut record.persistence,
                previous_schema_four_camera(),
                default_plan.viewport.camera,
                "schema 4 ED11 close-up vista",
            )?,
            SCHEMA_THREE_VERSION => migrate_previous_stock_camera(
                &self.path,
                &mut record.persistence,
                previous_schema_three_camera(),
                default_plan.viewport.camera,
                "schema 3 ED11 depth-cluster vista",
            )?,
            SCHEMA_TWO_VERSION => migrate_previous_stock_camera(
                &self.path,
                &mut record.persistence,
                previous_schema_two_camera(),
                default_plan.viewport.camera,
                "schema 2 ED11 close-up",
            )?,
            LEGACY_SCHEMA_VERSION => migrate_legacy_schema_one_camera(
                &self.path,
                &mut record.persistence,
                default_plan.viewport.camera,
            )?,
            version => {
                return invalid_persistence(
                    "schema_version",
                    format!(
                        "{} has schema_version {}, expected {STATE_SCHEMA_VERSION}",
                        self.path.display(),
                        version
                    ),
                );
            }
        }
        self.world_layers = record.world_layers.unwrap_or_default();
        self.play_camera_rig = record.play_camera_rig.unwrap_or_default();
        self.camera_bookmarks = record.camera_bookmarks.unwrap_or_default();
        validate_world_view_camera_bookmarks(&self.camera_bookmarks)?;
        let registry = default_editor_command_registry()?;
        self.command_keybindings = record
            .command_keybindings
            .unwrap_or(default_editor_command_bindings()?);
        self.command_keybindings.validate(&registry)?;
        restore_shell_state(settings, &record.persistence)
    }

    fn state_store(&self) -> EditorStateStore {
        EditorStateStore::new(self.path.clone())
    }
}

fn migrate_legacy_schema_one_camera(
    path: &Path,
    persistence: &mut EditorShellPersistence,
    current_default_camera: WorldViewCameraState,
) -> Result<(), EngineError> {
    migrate_previous_stock_camera(
        path,
        persistence,
        legacy_top_down_default_camera(),
        current_default_camera,
        "schema 1 legacy top-down",
    )
}

fn migrate_previous_stock_camera(
    path: &Path,
    persistence: &mut EditorShellPersistence,
    previous_stock_camera: WorldViewCameraState,
    current_default_camera: WorldViewCameraState,
    previous_label: &'static str,
) -> Result<(), EngineError> {
    persistence.camera.validate()?;
    if camera_approximately_eq(persistence.camera, previous_stock_camera) {
        info!(
            path = %path.display(),
            previous_label,
            from_distance = persistence.camera.distance,
            to_distance = current_default_camera.distance,
            "migrating editor shell previous stock world camera to current ED11 vista default"
        );
        persistence.camera = current_default_camera;
    } else {
        info!(
            path = %path.display(),
            distance = persistence.camera.distance,
            "preserving user-edited editor shell camera while migrating schema 1 shell state"
        );
    }
    Ok(())
}

fn previous_schema_two_camera() -> WorldViewCameraState {
    WorldViewCameraState {
        target_world: Vec3::new(0.0, 0.0, -24.0),
        distance: 128.0,
        yaw_degrees: 38.0,
        pitch_degrees: -44.0,
    }
}

fn previous_schema_three_camera() -> WorldViewCameraState {
    WorldViewCameraState {
        target_world: Vec3::new(0.0, 0.0, -18.0),
        distance: 190.0,
        yaw_degrees: 38.0,
        pitch_degrees: -48.0,
    }
}

fn previous_schema_four_camera() -> WorldViewCameraState {
    WorldViewCameraState {
        target_world: Vec3::new(0.0, 0.0, -24.0),
        distance: 150.0,
        yaw_degrees: 38.0,
        pitch_degrees: -42.0,
    }
}

fn schema_eleven_top_down_default_camera() -> WorldViewCameraState {
    legacy_top_down_default_camera()
}

fn legacy_top_down_default_camera() -> WorldViewCameraState {
    WorldViewCameraState {
        target_world: Vec3::new(0.0, 0.0, -20.0),
        distance: 168.0,
        yaw_degrees: 45.0,
        pitch_degrees: -60.0,
    }
}

fn camera_approximately_eq(left: WorldViewCameraState, right: WorldViewCameraState) -> bool {
    const EPSILON: f32 = 0.001;
    (left.target_world - right.target_world).abs().max_element() <= EPSILON
        && (left.distance - right.distance).abs() <= EPSILON
        && (left.yaw_degrees - right.yaw_degrees).abs() <= EPSILON
        && (left.pitch_degrees - right.pitch_degrees).abs() <= EPSILON
}

#[cfg(not(test))]
fn default_state_path_for_build() -> PathBuf {
    workspace_root()
        .join("tmp")
        .join("litd-editor-shell-state-aster")
}

#[cfg(test)]
fn default_state_path_for_build() -> PathBuf {
    let index = TEST_STATE_COUNTER.fetch_add(1, Ordering::Relaxed);
    workspace_root().join("tmp").join(format!(
        "litd-editor-shell-state-runtime-test-{}-{index}-aster",
        std::process::id()
    ))
}

fn absolute_or_workspace_relative(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        workspace_root().join(path)
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("litd-editor must live under the workspace crates directory")
        .to_path_buf()
}

fn mode_override_from_env() -> Result<Option<EditorMode>, EngineError> {
    match env::var(MODE_ENV) {
        Ok(value) => parse_mode(&value).map(Some),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(err) => invalid_persistence("mode.env", format!("{MODE_ENV}: {err}")),
    }
}

fn parse_mode(value: &str) -> Result<EditorMode, EngineError> {
    let normalized = value.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    match normalized.as_str() {
        "terrain" => Ok(EditorMode::Terrain),
        "objects" => Ok(EditorMode::Objects),
        "triggers" => Ok(EditorMode::Triggers),
        "cinematics" => Ok(EditorMode::Cinematics),
        "vfx" => Ok(EditorMode::Vfx),
        "ai_co_author" | "aicoauthor" | "ai" => Ok(EditorMode::AiCoAuthor),
        _ => invalid_persistence("mode.env", format!("unknown {MODE_ENV} '{value}'")),
    }
}

fn invalid_persistence<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(persistence_error(field, reason))
}

fn persistence_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.shell.persistence.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "main_editor_persistence_tests.rs"]
mod tests;
