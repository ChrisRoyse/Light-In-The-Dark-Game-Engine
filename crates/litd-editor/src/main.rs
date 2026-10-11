#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![cfg_attr(test, allow(dead_code))]

//! `litd-editor` — the Light in the Dark editor entry point.
//!
//! The editor opens a real egui/wgpu shell so FSV can verify the visible UI and
//! the persisted shell state directly.

mod fail_loud;
#[cfg(not(test))]
mod main_app_resume;
mod main_build_identity;
mod main_editor_assets;
mod main_editor_thumbnails;
mod main_editor_ui;
#[cfg(not(test))]
mod main_egui_gpu_validation;
mod main_egui_minimap;
#[cfg(not(test))]
mod main_egui_renderer;
#[cfg(not(test))]
mod main_egui_renderer_foliage;
#[cfg(not(test))]
mod main_egui_renderer_info;
mod main_egui_scale;
#[cfg(not(test))]
mod main_egui_style;
#[cfg(not(test))]
mod main_egui_surface;
mod main_egui_window;
mod main_egui_world_profile;
mod main_fsv;
#[cfg(not(test))]
mod main_fsv_app;
#[cfg(test)]
mod main_fsv_tests;
mod main_lifecycle;
#[cfg(test)]
mod main_test_support;

// #1532: install the shared counting allocator as the process global allocator so
// `FrameStats.allocations` reflects REAL per-frame heap allocations (was hardcoded 0).
#[global_allocator]
static GLOBAL: litd_engine::frame_alloc::CountingAllocator =
    litd_engine::frame_alloc::CountingAllocator;

#[cfg(not(test))]
use std::{process::ExitCode, time::Instant};

#[cfg(not(test))]
use litd_engine::{
    EngineError,
    app::{AppModule, AppRuntime, LifecycleReport},
    diagnostics::log_engine_error,
    input::{ACTION_CAMERA_FORWARD, ACTION_CONFIRM, InputState},
};
#[cfg(not(test))]
use main_build_identity::BuildIdentity;
#[cfg(not(test))]
use main_egui_renderer::{EguiEditorRenderer, SurfaceResizeState};
#[cfg(not(test))]
use main_fsv::{FsvConfig, InputFsvSnapshot, OwnedEditorFsvSnapshot};
#[cfg(not(test))]
use main_lifecycle::{
    EditorExitReason, EditorLifecycleState, install_panic_hook, run_panic_probe_from_env,
};
#[cfg(not(test))]
use tracing::{debug, error, info, warn};
#[cfg(not(test))]
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

#[cfg(not(test))]
const WINDOW_TITLE: &str = "Light in the Dark Editor - world view";

#[cfg(not(test))]
struct EditorApp {
    renderer: Option<EguiEditorRenderer>,
    runtime: AppRuntime,
    input: InputState,
    fsv: FsvConfig,
    build_identity: BuildIdentity,
    lifecycle: EditorLifecycleState,
    last_snapshot: Option<OwnedEditorFsvSnapshot>,
    failed: bool,
    // #1485 finding 2: next scheduled redraw time for ControlFlow::WaitUntil pacing. The editor
    // previously blocked the UI thread with thread::sleep each wait, queuing input behind it
    // (~16 ms added latency, stutter under load). Mirrors litd-game runtime_host (#1460).
    next_frame_at: Instant,
}

#[cfg(not(test))]
impl EditorApp {
    fn new(build_identity: BuildIdentity) -> Result<Self, EngineError> {
        let mut runtime = AppRuntime::default();
        runtime.modules_mut().register(EditorRuntimeModule)?;
        Ok(Self {
            renderer: None,
            runtime,
            input: InputState::with_editor_defaults(),
            fsv: FsvConfig::from_env(),
            build_identity,
            lifecycle: EditorLifecycleState::default(),
            last_snapshot: None,
            failed: false,
            next_frame_at: Instant::now(),
        })
    }

    fn start_runtime(&mut self, event_loop: &ActiveEventLoop) -> bool {
        match self.runtime.start() {
            Ok(report) => {
                info!(
                    module_count = report.module_count,
                    module_start_calls = report.module_start_calls,
                    started = report.started,
                    "app runtime started"
                );
                litd_editor::mcp_content_tools::load_and_log_authored_race_content_at_boot();
                true
            }
            Err(err) => {
                log_engine_error("editor_app_runtime_start", &err);
                self.fail_and_exit(event_loop, EditorExitReason::RuntimeStartFailed);
                false
            }
        }
    }

    fn shutdown_runtime(&mut self, reason: EditorExitReason) -> LifecycleReport {
        self.lifecycle.record_shutdown_started(reason);
        match self.runtime.shutdown() {
            Ok(report) => {
                self.lifecycle
                    .record_shutdown_finished(report.clean_shutdown);
                info!(
                    module_count = report.module_count,
                    clean_shutdown = report.clean_shutdown,
                    module_shutdown_calls = report.module_shutdown_calls,
                    "app runtime stopped"
                );
                report
            }
            Err(err) => {
                log_engine_error("editor_app_runtime_shutdown", &err);
                let report = self.runtime.lifecycle_report();
                self.lifecycle.record_shutdown_finished(false);
                report
            }
        }
    }

    fn exit_cleanly(&mut self, event_loop: &ActiveEventLoop) {
        if self.lifecycle.exit_requested() {
            event_loop.exit();
            return;
        }
        self.lifecycle
            .record_exit_request(EditorExitReason::CloseRequested);
        let report = self.shutdown_runtime(EditorExitReason::CloseRequested);
        self.write_latest_fsv_stats(&report);
        event_loop.exit();
    }

    fn fail_and_exit(&mut self, event_loop: &ActiveEventLoop, reason: EditorExitReason) {
        if self.lifecycle.exit_requested() {
            event_loop.exit();
            return;
        }
        self.failed = true;
        self.lifecycle.record_exit_request(reason);
        let report = self.shutdown_runtime(reason);
        self.write_latest_fsv_stats(&report);
        event_loop.exit();
    }
}

#[cfg(not(test))]
struct EditorRuntimeModule;

#[cfg(not(test))]
impl AppModule for EditorRuntimeModule {
    fn name(&self) -> &'static str {
        "editor.runtime"
    }
}

#[cfg(not(test))]
impl ApplicationHandler for EditorApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.handle_resumed(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.lifecycle.exit_requested() {
            event_loop.exit();
            return;
        }
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.handle_window_event(&event);
        }
        self.input.handle_window_event(&event);
        match event {
            WindowEvent::CloseRequested => self.exit_cleanly(event_loop),
            WindowEvent::Resized(size) => {
                let Some(renderer) = self.renderer.as_mut() else {
                    return;
                };
                match renderer.resize(size) {
                    Ok(SurfaceResizeState::Drawable { width, height }) => {
                        debug!(width, height, "editor surface drawable after resize");
                    }
                    Ok(SurfaceResizeState::Suspended { width, height }) => {
                        info!(
                            width,
                            height, "editor surface suspended until a non-zero resize arrives"
                        );
                    }
                    Err(err) => {
                        warn!(error = %err, width = size.width, height = size.height, "resize ignored");
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = self.renderer.as_ref()
                    && let SurfaceResizeState::Suspended { width, height } =
                        renderer.surface_resize_state()
                {
                    debug!(
                        width,
                        height, "editor redraw skipped while surface has no drawable area"
                    );
                    return;
                }
                let app_stats = match self.runtime.frame(Instant::now()) {
                    Ok(stats) => stats,
                    Err(err) => {
                        log_engine_error("editor_app_frame", &err);
                        self.fail_and_exit(event_loop, EditorExitReason::RuntimeFrameFailed);
                        return;
                    }
                };
                // #1485 finding 3: full world evidence rebuilt on demand at snapshot cadence (not per frame).
                let (stats, renderer_info, shell_evidence, _world_metrics) = {
                    let Some(renderer) = self.renderer.as_mut() else {
                        return;
                    };
                    match renderer.render_frame() {
                        Ok((stats, renderer_info, shell_evidence, world_metrics)) => {
                            debug!(
                                app_frame = app_stats.frame_index,
                                variable_delta_ns = app_stats.variable_delta_ns,
                                fixed_delta_ns = app_stats.fixed_delta_ns,
                                fixed_steps = app_stats.fixed_steps,
                                dropped_fixed_steps = app_stats.dropped_fixed_steps,
                                module_fixed_updates = app_stats.module_fixed_updates,
                                module_updates = app_stats.module_updates,
                                sleep_hint_ns = app_stats.sleep_hint_ns,
                                "app frame stepped"
                            );
                            debug!(
                                frame = stats.frame_index,
                                width = stats.width,
                                height = stats.height,
                                frame_ns = stats.frame_ns,
                                render_graph_passes = stats.render_graph_passes,
                                draw_calls = stats.draw_calls,
                                allocations = stats.allocations,
                                budget_ok = stats.budget_ok,
                                clear_color = ?stats.clear_color,
                                shell_frontend = shell_evidence.frontend_kind,
                                shell_active_mode = ?shell_evidence.active_mode,
                                world_renderer = world_metrics.renderer_kind,
                                world_triangles = world_metrics.triangle_count,
                                world_viewport = ?world_metrics.viewport,
                                "presented editor shell frame"
                            );
                            (stats, renderer_info, shell_evidence, world_metrics)
                        }
                        Err(err) => {
                            log_engine_error("editor_render_frame", &err);
                            self.fail_and_exit(event_loop, EditorExitReason::RenderFrameFailed);
                            return;
                        }
                    }
                };
                let input_stats = self.input.stats();
                let input = InputFsvSnapshot {
                    stats: input_stats,
                    camera_forward_down: self.input.action_state(ACTION_CAMERA_FORWARD).down,
                    confirm_down: self.input.action_state(ACTION_CONFIRM).down,
                };
                // #1485 finding 5b: MOVE the snapshot into `last_snapshot` instead of cloning
                // it in — the old `snapshot.clone()` deep-copied RendererInfo + shell + world
                // evidence (dozens of heap strings) every frame. `last_snapshot` still holds the
                // latest rendered frame, so the periodic/exit writers below (which now read
                // `self.last_snapshot`) see current data; `handle_fsv_exit` only inspects the
                // frame index, so exit detection is unaffected.
                self.last_snapshot = Some(OwnedEditorFsvSnapshot {
                    info: renderer_info,
                    render_stats: stats,
                    app_stats,
                    input,
                    shell: shell_evidence,
                });
                self.write_periodic_fsv_stats();
                if self.handle_fsv_exit(event_loop) {
                    return;
                }
                self.input.finish_frame();
                if let Some(renderer) = &self.renderer {
                    renderer.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.lifecycle.exit_requested() {
            event_loop.exit();
            return;
        }
        // #1485 finding 2: pace with ControlFlow::WaitUntil instead of blocking the UI thread on
        // thread::sleep. winit sleeps until next_frame_at but wakes IMMEDIATELY on input, so
        // events are handled without queuing behind a sleep (removes ~16 ms input latency and
        // stutter). Redraw cadence is unchanged — the same sleep_hint/frame_duration_hint bound
        // now sets the wake deadline. Mirrors litd-game runtime_host (#1460).
        if let Some(renderer) = &self.renderer {
            let now = Instant::now();
            if now >= self.next_frame_at {
                renderer.request_redraw();
                let interval = self
                    .runtime
                    .sleep_hint(now)
                    .min(EguiEditorRenderer::frame_duration_hint());
                self.next_frame_at = now + interval;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_at));
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.lifecycle.record_loop_exiting();
        let report = if self.runtime.lifecycle_report().started {
            self.shutdown_runtime(self.lifecycle.exit_reason())
        } else {
            self.runtime.lifecycle_report()
        };
        self.write_latest_fsv_stats(&report);
    }
}

#[cfg(not(test))]
fn main() -> ExitCode {
    let log_path = match litd_engine::init_file_logging("litd-editor") {
        Ok(path) => path,
        Err(error) => {
            // File logging is not up yet, so the tracing subscriber cannot carry this;
            // stderr is the only channel left. Exiting silently here left operators with
            // a FAILURE code and no cause (#1489, #800 §4).
            eprintln!("litd-editor: failed to initialize file logging: {error}");
            return ExitCode::FAILURE;
        }
    };
    let build_identity = match BuildIdentity::current() {
        Ok(identity) => identity,
        Err(err) => {
            log_engine_error("editor_build_identity", &err);
            return ExitCode::FAILURE;
        }
    };
    build_identity.log_startup(&log_path);
    install_panic_hook(&log_path, WINDOW_TITLE, build_identity.clone());
    run_panic_probe_from_env();
    if let Some(exit_code) = fail_loud::run_from_env() {
        return exit_code;
    }

    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(err) => {
            error!(error = %err, "failed to create winit event loop");
            return ExitCode::FAILURE;
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = match EditorApp::new(build_identity) {
        Ok(app) => app,
        Err(err) => {
            log_engine_error("editor_app_init", &err);
            return ExitCode::FAILURE;
        }
    };
    match event_loop.run_app(&mut app) {
        Ok(()) if app.failed => {
            app.lifecycle.record_process_return("failure");
            let report = app.runtime.lifecycle_report();
            app.write_latest_fsv_stats(&report);
            ExitCode::FAILURE
        }
        Ok(()) => {
            app.lifecycle.record_process_return("success");
            let report = app.runtime.lifecycle_report();
            app.write_latest_fsv_stats(&report);
            ExitCode::SUCCESS
        }
        Err(err) => {
            app.lifecycle.record_event_loop_error();
            error!(error = %err, "editor event loop failed");
            let report = if app.runtime.lifecycle_report().started {
                app.shutdown_runtime(EditorExitReason::EventLoopError)
            } else {
                app.runtime.lifecycle_report()
            };
            app.lifecycle.record_process_return("failure");
            app.write_latest_fsv_stats(&report);
            ExitCode::FAILURE
        }
    }
}
