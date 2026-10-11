use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use litd_engine::{
    EngineError,
    app::{AppFrameStats, AppRuntime},
    game_package::RuntimePackageReadback,
    input::{ABILITY_SLOT_ACTIONS, ACTION_CAMERA_FORWARD, ACTION_CONFIRM, InputState},
    performance_quality::GraphicsQualitySettings,
    render::{FrameStats, RendererInfo},
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};

use litd_game::{
    replay_viewer::{ReplayViewer, ReplayViewerSnapshot},
    replay_viewer_input::ReplayViewerInput,
};

use crate::{
    runtime_audio::GameRuntimeAudio,
    runtime_host_module::{GameRuntimeModule, RuntimeCounters, runtime_host_error},
    runtime_renderer::GameRuntimeRenderer,
    runtime_state::{
        RuntimeHostState, RuntimeHostStateParts, RuntimeHostStateReadback, RuntimeInputSnapshot,
        build_runtime_host_state, normalize_state_path,
    },
    runtime_state_writer::RuntimeStateWriter,
};

const WINDOW_PREFIX: &str = "Light in the Dark Runtime";
const DEFAULT_WIDTH: f64 = 1280.0;
const DEFAULT_HEIGHT: f64 = 720.0;
const MIN_WIDTH: f64 = 640.0;
const MIN_HEIGHT: f64 = 360.0;
/// Off-thread running-state heartbeat cadence; transitions persist immediately (#1460).
const STATE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);

include!("runtime_host_entry.rs");
include!("runtime_host_finalize.rs");
include!("runtime_host_replay.rs");
#[path = "runtime_host_application.rs"]
mod application;

struct RuntimeHostApp {
    package: RuntimePackageReadback,
    max_frames: Option<u64>,
    runtime: AppRuntime,
    input: InputState,
    counters: Arc<RuntimeCounters>,
    renderer: Option<GameRuntimeRenderer>,
    audio: GameRuntimeAudio,
    quality: Option<GraphicsQualitySettings>,
    last_app_frame: Option<AppFrameStats>,
    last_render_frame: Option<FrameStats>,
    last_renderer_info: Option<RendererInfo>,
    last_state: Option<RuntimeHostState>,
    last_state_readback: Option<RuntimeHostStateReadback>,
    state_writer: Option<RuntimeStateWriter>,
    last_heartbeat: Instant,
    next_frame_at: Instant,
    gpu_validation: bool,
    window_visible: bool,
    replay_viewer: Option<ReplayViewer>,
    replay_input: Option<ReplayViewerInput>,
    last_replay_snapshot: Option<ReplayViewerSnapshot>,
    /// One-shot #1894 ability-slot FSV injection.
    inject_ability_mask: Option<u8>,
    exit_requested: bool,
    failed: bool,
    failure_reason: Option<String>,
}

impl RuntimeHostApp {
    fn new(
        package: RuntimePackageReadback,
        config: RuntimeHostConfig,
    ) -> Result<Self, EngineError> {
        let counters = Arc::new(RuntimeCounters::default());
        let mut runtime = AppRuntime::default();
        let replay_viewer = build_replay_viewer(&package, config.replay.as_ref())?;
        if replay_viewer.is_none() {
            runtime
                .modules_mut()
                .register(GameRuntimeModule::from_package(
                    Arc::clone(&counters),
                    &package,
                )?)?;
        }
        let replay_input = replay_viewer.as_ref().map(|_| ReplayViewerInput::new());
        let audio = GameRuntimeAudio::from_package(&package)?;
        let runtime_state_path = config
            .runtime_state_path
            .map(normalize_state_path)
            .transpose()?;
        if let Some(quality) = config.quality {
            quality.validate()?;
        }
        let state_writer = match &runtime_state_path {
            Some(path) => Some(RuntimeStateWriter::spawn(path.clone())?),
            None => None,
        };
        let now = Instant::now();
        Ok(Self {
            package,
            max_frames: config.max_frames,
            runtime,
            // #1892: RTS play defaults (edge-scroll/MMB/wheel) — not editor WASD.
            input: InputState::with_rts_play_defaults(),
            counters,
            renderer: None,
            audio,
            quality: config.quality,
            last_app_frame: None,
            last_render_frame: None,
            last_renderer_info: None,
            last_state: None,
            last_state_readback: None,
            state_writer,
            last_heartbeat: now,
            next_frame_at: now,
            gpu_validation: config.gpu_validation,
            window_visible: config.window_visible,
            replay_viewer,
            replay_input,
            last_replay_snapshot: None,
            inject_ability_mask: config.inject_ability_mask,
            exit_requested: false,
            failed: false,
            failure_reason: None,
        })
    }

    fn start_runtime(&mut self, event_loop: &ActiveEventLoop) -> bool {
        match self.runtime.start() {
            Ok(report) => {
                tracing::info!(
                    module_count = report.module_count,
                    module_start_calls = report.module_start_calls,
                    package_id = %self.package.package_id,
                    "litd-game runtime app started"
                );
                if let Err(error) = self.audio.start() {
                    self.fail_and_exit(event_loop, format!("audio start failed: {error}"));
                    return false;
                }
                true
            }
            Err(error) => {
                self.fail_and_exit(event_loop, format!("runtime start failed: {error}"));
                false
            }
        }
    }

    fn shutdown_runtime(&mut self) {
        match self.runtime.shutdown() {
            Ok(report) => {
                tracing::info!(
                    clean_shutdown = report.clean_shutdown,
                    module_shutdown_calls = report.module_shutdown_calls,
                    package_id = %self.package.package_id,
                    "litd-game runtime app stopped"
                );
            }
            Err(error) => {
                tracing::error!(
                    error = %error,
                    code = "LITD-GAME-RUNTIME-SHUTDOWN",
                    "litd-game runtime shutdown failed"
                );
                self.failed = true;
                self.failure_reason = Some(format!("runtime shutdown failed: {error}"));
            }
        }
    }

    fn exit_cleanly(&mut self, event_loop: &ActiveEventLoop) {
        if self.exit_requested {
            return;
        }
        self.exit_requested = true;
        self.shutdown_runtime();
        if let Err(error) = self.write_state("closed", None) {
            self.failed = true;
            self.failure_reason = Some(error.to_string());
        }
        // The writer is flushed and stopped in `exiting`, after the terminal
        // "exited" transition is persisted.
        event_loop.exit();
    }

    fn fail_and_exit(&mut self, event_loop: &ActiveEventLoop, reason: String) {
        if self.exit_requested {
            return;
        }
        self.exit_requested = true;
        tracing::error!(
            package_id = %self.package.package_id,
            reason = %reason,
            code = "LITD-GAME-RUNTIME-FAILED",
            "litd-game runtime host failed closed"
        );
        self.failed = true;
        self.failure_reason = Some(reason.clone());
        self.shutdown_runtime();
        let _ = self.write_state("failed", Some(reason));
        event_loop.exit();
    }

    fn build_state(&self, status: &'static str, last_error: Option<String>) -> RuntimeHostState {
        let input = RuntimeInputSnapshot {
            stats: self.input.stats(),
            camera_forward_down: self.input.action_state(ACTION_CAMERA_FORWARD).down,
            confirm_down: self.input.action_state(ACTION_CONFIRM).down,
        };
        let camera = self.renderer.as_ref().map(GameRuntimeRenderer::camera);
        // #1485 finding 3: build world evidence ON DEMAND (heartbeat/transition cadence), not per frame.
        let world_evidence = self.renderer.as_ref().and_then(|r| r.frame_evidence());
        build_runtime_host_state(RuntimeHostStateParts {
            status,
            package: &self.package,
            counters: self.counters.snapshot(),
            input,
            app_frame: self.last_app_frame.as_ref(),
            render_frame: self.last_render_frame.as_ref(),
            renderer: self.last_renderer_info.as_ref(),
            world: world_evidence.as_ref(),
            runtime_text: self
                .renderer
                .as_ref()
                .map(GameRuntimeRenderer::runtime_text_state),
            audio: Some(self.audio.state()),
            camera,
            quality: self.quality,
            last_error,
        })
    }

    /// Persist a state TRANSITION synchronously (through the writer thread), with
    /// the full write→readback→hash discipline, and capture the verified readback.
    /// Transitions are rare (accepted/running/closed/failed/exited), so blocking
    /// here does not touch the per-frame path.
    fn write_state(
        &mut self,
        status: &'static str,
        last_error: Option<String>,
    ) -> Result<(), EngineError> {
        let state = self.build_state(status, last_error);
        if let Some(writer) = &self.state_writer {
            let readback = writer.persist_blocking(state.clone())?;
            self.last_state_readback = Some(readback);
        }
        self.last_state = Some(state);
        self.last_heartbeat = Instant::now();
        Ok(())
    }

    /// Per-frame heartbeat: surface any async-writer failure loudly, and persist a
    /// "running" snapshot at most once per [`STATE_HEARTBEAT_INTERVAL`], off the
    /// render thread. When no state path is configured this is a no-op.
    fn heartbeat(&mut self) -> Result<(), EngineError> {
        if let Some(writer) = &self.state_writer
            && let Some(error) = writer.take_error()
        {
            return Err(error);
        }
        if self.state_writer.is_some() && self.last_heartbeat.elapsed() >= STATE_HEARTBEAT_INTERVAL
        {
            let state = self.build_state("running", None);
            if let Some(writer) = &self.state_writer {
                writer.persist_async(state.clone())?;
            }
            self.last_state = Some(state);
            self.last_heartbeat = Instant::now();
        }
        Ok(())
    }

    fn frame_interval(&self, now: Instant) -> Duration {
        self.runtime
            .sleep_hint(now)
            .min(GameRuntimeRenderer::frame_duration_hint())
    }

    fn draw_frame(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let replay_frame = match self.advance_replay_frame() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.fail_and_exit(event_loop, error.to_string());
                return false;
            }
        };
        if replay_frame.is_none() {
            self.queue_ability_slot_presses();
            if let Some(mask) = self.inject_ability_mask.take() {
                tracing::info!(
                    code = "LITD-ABILITY-SLOT-INJECT",
                    mask,
                    "FSV inject of ability-slot mask for #1894 cast path"
                );
                self.counters.queue_ability_slots(mask);
            }
        }
        let app_frame = match self.runtime.frame(Instant::now()) {
            Ok(stats) => stats,
            Err(error) => {
                self.fail_and_exit(event_loop, format!("runtime frame failed: {error}"));
                return false;
            }
        };
        let replay_grid = match self
            .replay_viewer
            .as_ref()
            .map(ReplayViewer::perspective_grid)
            .transpose()
        {
            Ok(grid) => grid,
            Err(error) => {
                self.fail_and_exit(event_loop, error.to_string());
                return false;
            }
        };
        let Some(renderer) = self.renderer.as_mut() else {
            self.fail_and_exit(event_loop, "renderer missing during redraw".to_string());
            return false;
        };
        if let Err(error) = renderer.set_replay_presentation(replay_frame.as_ref(), replay_grid) {
            self.fail_and_exit(event_loop, format!("replay presentation failed: {error}"));
            return false;
        }
        let (render_frame, renderer_info) =
            match renderer.render_frame(&mut self.input, &self.counters) {
                Ok(frame) => frame,
                Err(error) => {
                    self.fail_and_exit(event_loop, format!("render frame failed: {error}"));
                    return false;
                }
            };
        self.last_app_frame = Some(app_frame);
        self.last_render_frame = Some(render_frame);
        self.last_renderer_info = Some(renderer_info);
        if let Some(before_render) = replay_frame.as_ref()
            && let Err(error) = self.verify_replay_presentation_unchanged(before_render)
        {
            self.fail_and_exit(event_loop, error.to_string());
            return false;
        }
        if let Err(error) = self.heartbeat() {
            self.fail_and_exit(event_loop, error.to_string());
            return false;
        }
        if self
            .max_frames
            .is_some_and(|max_frames| app_frame.frame_index >= max_frames)
        {
            self.exit_cleanly(event_loop);
            return false;
        }
        self.input.finish_frame();
        self.finish_replay_input();
        if self.replay_completed_hidden() {
            self.exit_cleanly(event_loop);
            return false;
        }
        true
    }
}
