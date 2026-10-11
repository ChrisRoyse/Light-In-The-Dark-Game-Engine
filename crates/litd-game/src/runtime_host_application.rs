use std::{sync::Arc, time::Instant};

use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow},
    window::{Window, WindowId},
};

use super::{DEFAULT_HEIGHT, DEFAULT_WIDTH, MIN_HEIGHT, MIN_WIDTH, RuntimeHostApp, WINDOW_PREFIX};
use crate::runtime_renderer::GameRuntimeRenderer;

impl ApplicationHandler for RuntimeHostApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() {
            return;
        }
        let title = format!("{WINDOW_PREFIX} - {}", self.package.package_id);
        let attrs = Window::default_attributes()
            .with_title(title)
            .with_min_inner_size(LogicalSize::new(MIN_WIDTH, MIN_HEIGHT))
            .with_visible(false);
        let attrs = match self.quality {
            Some(quality) => attrs.with_inner_size(PhysicalSize::new(
                quality.target_width,
                quality.target_height,
            )),
            None => attrs.with_inner_size(LogicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT)),
        };
        let window = match event_loop.create_window(attrs) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.fail_and_exit(event_loop, format!("window creation failed: {error}"));
                return;
            }
        };
        let visible_window = Arc::clone(&window);
        match pollster::block_on(GameRuntimeRenderer::new(
            window,
            &self.package,
            self.quality,
            self.gpu_validation,
        )) {
            Ok(renderer) => {
                self.last_renderer_info = Some(renderer.info());
                if !self.start_runtime(event_loop) {
                    return;
                }
                if self.window_visible {
                    visible_window.set_visible(true);
                    renderer.request_redraw();
                }
                tracing::info!(
                    code = "LITD-RUNTIME-WINDOW-VISIBILITY",
                    visible = self.window_visible,
                    "Runtime window visibility applied"
                );
                self.renderer = Some(renderer);
                self.next_frame_at = Instant::now();
                if let Err(error) = self.write_state("running", None) {
                    self.fail_and_exit(event_loop, error.to_string());
                }
            }
            Err(error) => self.fail_and_exit(event_loop, format!("renderer init failed: {error}")),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.exit_requested {
            return;
        }
        self.input.handle_window_event(&event);
        if let Some(replay_input) = &mut self.replay_input {
            replay_input.handle_window_event(&event);
        }
        match event {
            WindowEvent::CloseRequested => self.exit_cleanly(event_loop),
            WindowEvent::Resized(size) => {
                let Some(renderer) = self.renderer.as_mut() else {
                    return;
                };
                if let Err(error) = renderer.resize(size) {
                    self.fail_and_exit(event_loop, format!("resize failed: {error}"));
                }
            }
            WindowEvent::RedrawRequested if self.window_visible => {
                self.draw_frame(event_loop);
            }
            WindowEvent::RedrawRequested => {}
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.failed || self.exit_requested {
            return;
        }
        // Surface async writer failures loudly even on frames we do not draw.
        if let Some(writer) = &self.state_writer
            && let Some(error) = writer.take_error()
        {
            self.fail_and_exit(event_loop, error.to_string());
            return;
        }
        if self.renderer.is_none() {
            return;
        }
        let now = Instant::now();
        if now >= self.next_frame_at {
            if self.window_visible {
                if let Some(renderer) = &self.renderer {
                    renderer.request_redraw();
                }
            } else if !self.draw_frame(event_loop) {
                return;
            }
            let interval = self.frame_interval(now);
            self.next_frame_at = now + interval;
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_at));
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if self.runtime.lifecycle_report().started {
            self.shutdown_runtime();
        }
        let status = if self.failed { "failed" } else { "exited" };
        // #1464: this is the authoritative terminal-state persist. `exiting()`
        // is a winit trait method returning `()`, so we cannot propagate — but a
        // swallowed error leaves the on-disk runtime state stale (e.g. still
        // "running") while the process claims a clean exit. Surface it loudly so
        // the failure is observable via the log artifact.
        if let Err(error) = self.write_state(status, self.failure_reason.clone()) {
            self.failed = true;
            tracing::error!(
                code = "RUNTIME_HOST_TERMINAL_STATE_WRITE_FAILED",
                status,
                %error,
                "failed to persist terminal runtime state on exit; on-disk state is stale"
            );
        }
        // Flush and stop the writer thread; the final transition above already
        // persisted the terminal state synchronously.
        if let Err(error) = self.shutdown_state_writer() {
            tracing::error!(
                code = "RUNTIME_HOST_STATE_WRITER_SHUTDOWN_FAILED",
                %error,
                "failed to flush/stop the runtime state writer on exit"
            );
        }
    }
}
