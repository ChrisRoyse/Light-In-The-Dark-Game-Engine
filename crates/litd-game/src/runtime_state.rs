use std::path::{Path, PathBuf};

use litd_engine::{
    EngineError,
    app::AppFrameStats,
    calyx_json_store::{CalyxJsonStoreSpec, read_calyx_json_bytes, write_calyx_json_bytes},
    determinism_boundary::{
        DeterminismBoundarySnapshot, snapshot as determinism_boundary_snapshot,
    },
    game_package::{RuntimePackageDocumentSummary, RuntimePackageReadback},
    input::InputStats,
    performance_quality::GraphicsQualitySettings,
    render::{
        FrameStats, RendererInfo,
        editor_world::{EditorWorldCameraState, EditorWorldRenderEvidence},
    },
};
use litd_hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::{runtime_audio::RuntimeAudioState, runtime_state_text::RuntimeTextState};

pub const RUNTIME_HOST_STATE_SCHEMA: &str = "litd.game.runtime_host.state.v1";
const RUNTIME_STATE_VAULT_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FCC";
const RUNTIME_STATE_VAULT_PURPOSE: &[u8] = b"litd-game-runtime-state-issue1363";
const RUNTIME_STATE_COLLECTION: &str = "litd_game_runtime_host";
const RUNTIME_STATE_ROW: &[u8] = b"runtime/host_state/latest";
const RUNTIME_STATE_COLUMN: &[u8] = b"state_json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeHostState {
    pub schema: String,
    pub status: String,
    pub package_path: String,
    pub package_id: String,
    pub package_sha256: String,
    pub runtime_id: String,
    pub document_fingerprint: String,
    pub play_build_fingerprint: String,
    pub document_summary: RuntimePackageDocumentSummary,
    pub app_frame: Option<RuntimeAppFrameState>,
    pub render_frame: Option<RuntimeRenderFrameState>,
    pub renderer: Option<RuntimeRendererState>,
    pub world: Option<RuntimeWorldState>,
    #[serde(default)]
    pub runtime_text: Option<RuntimeTextState>,
    pub audio: Option<RuntimeAudioState>,
    pub input: RuntimeInputState,
    pub camera: Option<RuntimeCameraState>,
    pub quality: Option<GraphicsQualitySettings>,
    pub counters: RuntimeCounterState,
    pub determinism_boundary: DeterminismBoundarySnapshot,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeHostStateReadback {
    pub path: String,
    pub bytes: usize,
    pub sha256: String,
    pub state: RuntimeHostState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCounterState {
    pub runtime_start_calls: u64,
    pub fixed_tick_total: u64,
    pub update_total: u64,
    pub runtime_shutdown_calls: u64,
    /// #1843: true when the package carried lowered runtime datasets and the host
    /// instantiated the composed `MatchRuntimeSession` from them.
    #[serde(default)]
    pub session_active: bool,
    /// #1843: deterministic session ticks the host advanced from package data.
    #[serde(default)]
    pub session_ticks: u64,
    /// #1843: the session `combined_state_hash` after the latest advanced tick, hex.
    /// Empty when no runtime session was instantiated.
    #[serde(default)]
    pub session_state_hash: String,
    /// #1894: accepted MOBA ability-slot casts this process.
    #[serde(default)]
    pub ability_cast_accepted: u64,
    /// #1894: rejected casts (empty selection, no capability, validate fail).
    #[serde(default)]
    pub ability_cast_rejected: u64,
    /// #1894: last ability bar slot pressed (0..=3), or 255 if none.
    #[serde(default = "default_last_ability_slot")]
    pub last_ability_slot: u8,
    /// #1894: last ability id dispatched (or attempted).
    #[serde(default)]
    pub last_ability_id: u16,
}

fn default_last_ability_slot() -> u8 {
    255
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeAppFrameState {
    pub frame_index: u64,
    pub fixed_steps: u32,
    pub dropped_fixed_steps: u32,
    pub module_fixed_updates: u32,
    pub module_updates: u32,
    pub sleep_hint_ns: u128,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeRenderFrameState {
    pub frame_index: u64,
    pub gpu_context_id: u64,
    pub width: u32,
    pub height: u32,
    pub render_graph_passes: u32,
    pub draw_calls: u32,
    pub frame_ns: u128,
    pub clear_color: [f64; 4],
    pub budget_max_draw_calls: u32,
    pub budget_max_allocations: u32,
    pub budget_ok: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeRendererState {
    pub gpu_context_id: u64,
    pub adapter_name: String,
    pub backend: String,
    pub device_type: String,
    pub surface_format: String,
    pub present_mode: String,
    pub width: u32,
    pub height: u32,
    pub internal_render_width: u32,
    pub internal_render_height: u32,
    pub render_graph_passes: u32,
    pub budget_max_draw_calls: u32,
    pub budget_max_allocations: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeWorldState {
    pub renderer_kind: String,
    pub scene_name: String,
    pub scene_source_id: String,
    pub scene_source_path: String,
    pub scene_is_blank: bool,
    pub viewport_rect: String,
    pub triangle_count: u32,
    pub visible_triangle_count: u32,
    pub draw_calls: u32,
    pub visual_preset_id: String,
    pub reference_images: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeInputState {
    pub frame_index: u64,
    pub action_count: usize,
    pub binding_count: usize,
    pub active_binding_count: usize,
    pub events_seen: u64,
    pub cursor_position: Option<(f64, f64)>,
    pub camera_forward_down: bool,
    pub confirm_down: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeCameraState {
    pub target_world: [f32; 3],
    pub distance: f32,
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeInputSnapshot {
    pub stats: InputStats,
    pub camera_forward_down: bool,
    pub confirm_down: bool,
}

pub struct RuntimeHostStateParts<'a> {
    pub status: &'a str,
    pub package: &'a RuntimePackageReadback,
    pub counters: RuntimeCounterState,
    pub input: RuntimeInputSnapshot,
    pub app_frame: Option<&'a AppFrameStats>,
    pub render_frame: Option<&'a FrameStats>,
    pub renderer: Option<&'a RendererInfo>,
    pub world: Option<&'a EditorWorldRenderEvidence>,
    pub runtime_text: Option<&'a RuntimeTextState>,
    pub audio: Option<&'a RuntimeAudioState>,
    pub camera: Option<EditorWorldCameraState>,
    pub quality: Option<GraphicsQualitySettings>,
    pub last_error: Option<String>,
}

pub fn build_runtime_host_state(parts: RuntimeHostStateParts<'_>) -> RuntimeHostState {
    RuntimeHostState {
        schema: RUNTIME_HOST_STATE_SCHEMA.to_string(),
        status: parts.status.to_string(),
        package_path: parts.package.package_path.clone(),
        package_id: parts.package.package_id.clone(),
        package_sha256: parts.package.package_sha256.clone(),
        runtime_id: parts.package.runtime_id.clone(),
        document_fingerprint: parts.package.document_fingerprint.clone(),
        play_build_fingerprint: parts.package.play_build_fingerprint.clone(),
        document_summary: parts.package.document_summary.clone(),
        app_frame: parts.app_frame.map(RuntimeAppFrameState::from),
        render_frame: parts.render_frame.map(RuntimeRenderFrameState::from),
        renderer: parts.renderer.map(RuntimeRendererState::from),
        world: parts.world.map(RuntimeWorldState::from),
        runtime_text: parts.runtime_text.cloned(),
        audio: parts.audio.cloned(),
        input: RuntimeInputState::from(parts.input),
        camera: parts.camera.map(RuntimeCameraState::from),
        quality: parts.quality,
        counters: parts.counters,
        determinism_boundary: determinism_boundary_snapshot(),
        last_error: parts.last_error,
    }
}

pub fn write_runtime_state_with_readback(
    path: &Path,
    state: &RuntimeHostState,
) -> Result<RuntimeHostStateReadback, EngineError> {
    let bytes =
        serde_json::to_vec_pretty(state).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize litd-game runtime state: {error}"),
        })?;
    let stored = write_calyx_json_bytes(&runtime_state_spec(path), &bytes)
        .map_err(|error| runtime_state_error("state.write", error))?;
    if stored.bytes != bytes {
        return Err(runtime_state_error(
            "state.verify",
            "Aster runtime state bytes changed after write",
        ));
    }
    let readback = read_calyx_json_bytes(&runtime_state_spec(path))
        .map_err(|error| runtime_state_error("state.read", error))?
        .ok_or_else(|| {
            runtime_state_error(
                "state.read",
                format!(
                    "missing Calyx Aster runtime state row at {}",
                    path.display()
                ),
            )
        })?;
    if readback.bytes != bytes {
        return Err(runtime_state_error(
            "state.verify",
            "Aster runtime state readback did not match serialized bytes",
        ));
    }
    let parsed = serde_json::from_slice::<RuntimeHostState>(&readback.bytes).map_err(|error| {
        EngineError::SerializationFailed {
            format: "json",
            reason: format!("parse runtime state readback: {error}"),
        }
    })?;
    Ok(RuntimeHostStateReadback {
        path: path.display().to_string(),
        bytes: readback.bytes.len(),
        sha256: sha256_hex(&readback.bytes),
        state: parsed,
    })
}

pub fn normalize_state_path(path: PathBuf) -> Result<PathBuf, EngineError> {
    if path.as_os_str().is_empty() {
        return Err(runtime_state_error(
            "state.path",
            "runtime state path must be non-empty",
        ));
    }
    Ok(path)
}

impl From<&AppFrameStats> for RuntimeAppFrameState {
    fn from(stats: &AppFrameStats) -> Self {
        Self {
            frame_index: stats.frame_index,
            fixed_steps: stats.fixed_steps,
            dropped_fixed_steps: stats.dropped_fixed_steps,
            module_fixed_updates: stats.module_fixed_updates,
            module_updates: stats.module_updates,
            sleep_hint_ns: stats.sleep_hint_ns,
        }
    }
}

impl From<&FrameStats> for RuntimeRenderFrameState {
    fn from(stats: &FrameStats) -> Self {
        Self {
            frame_index: stats.frame_index,
            gpu_context_id: stats.gpu_context_id,
            width: stats.width,
            height: stats.height,
            render_graph_passes: stats.render_graph_passes,
            draw_calls: stats.draw_calls,
            frame_ns: stats.frame_ns,
            clear_color: stats.clear_color,
            budget_max_draw_calls: stats.budget.max_draw_calls,
            budget_max_allocations: stats.budget.max_allocations,
            budget_ok: stats.budget_ok,
        }
    }
}

impl From<&RendererInfo> for RuntimeRendererState {
    fn from(info: &RendererInfo) -> Self {
        Self {
            gpu_context_id: info.gpu_context_id,
            adapter_name: info.adapter.name.clone(),
            backend: info.adapter.backend.clone(),
            device_type: info.adapter.device_type.clone(),
            surface_format: info.surface_format.clone(),
            present_mode: info.present_mode.clone(),
            width: info.width,
            height: info.height,
            internal_render_width: info.internal_render_width,
            internal_render_height: info.internal_render_height,
            render_graph_passes: info.render_graph_passes,
            budget_max_draw_calls: info.budget.max_draw_calls,
            budget_max_allocations: info.budget.max_allocations,
        }
    }
}

impl From<&EditorWorldRenderEvidence> for RuntimeWorldState {
    fn from(world: &EditorWorldRenderEvidence) -> Self {
        Self {
            renderer_kind: world.renderer_kind.to_string(),
            scene_name: world.scene_name.clone(),
            scene_source_id: world.scene_source_id.clone(),
            scene_source_path: world.scene_source_path.clone(),
            scene_is_blank: world.scene_is_blank,
            viewport_rect: world.viewport_rect.clone(),
            triangle_count: world.triangle_count,
            visible_triangle_count: world.visible_triangle_count,
            draw_calls: world.draw_calls,
            visual_preset_id: world.visual_preset_id.to_string(),
            reference_images: world.visual_preset_reference_images.clone(),
        }
    }
}

impl From<RuntimeInputSnapshot> for RuntimeInputState {
    fn from(input: RuntimeInputSnapshot) -> Self {
        Self {
            frame_index: input.stats.frame_index,
            action_count: input.stats.action_count,
            binding_count: input.stats.binding_count,
            active_binding_count: input.stats.active_binding_count,
            events_seen: input.stats.events_seen,
            cursor_position: input.stats.cursor_position,
            camera_forward_down: input.camera_forward_down,
            confirm_down: input.confirm_down,
        }
    }
}

impl From<EditorWorldCameraState> for RuntimeCameraState {
    fn from(camera: EditorWorldCameraState) -> Self {
        Self {
            target_world: camera.target_world.to_array(),
            distance: camera.distance,
            yaw_degrees: camera.yaw_degrees,
            pitch_degrees: camera.pitch_degrees,
        }
    }
}

pub(crate) fn runtime_state_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    let field = format!("litd_game.runtime.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-GAME-RUNTIME-STATE",
        "litd-game runtime state operation failed"
    );
    EngineError::GamePackageInvalidConfig { field, reason }
}

fn runtime_state_spec(path: &Path) -> CalyxJsonStoreSpec<'_> {
    CalyxJsonStoreSpec {
        vault_dir: path,
        vault_id: RUNTIME_STATE_VAULT_ID,
        purpose: RUNTIME_STATE_VAULT_PURPOSE,
        collection: RUNTIME_STATE_COLLECTION,
        row_key: RUNTIME_STATE_ROW,
        column: RUNTIME_STATE_COLUMN,
    }
}
