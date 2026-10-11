// `GraphicsQualitySettings` carries f32 upscale knobs (#1741): PartialEq, not Eq.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeHostConfig {
    pub runtime_state_path: Option<PathBuf>,
    pub max_frames: Option<u64>,
    pub quality: Option<GraphicsQualitySettings>,
    pub gpu_validation: bool,
    pub window_visible: bool,
    /// #1894 FSV: one-shot ability-slot bitmask (bits 0..=3 = QWER) injected on the
    /// first frame after boot. Production play leaves this `None` — real presses
    /// come from input. When set, the host queues the mask once and clears it so
    /// counters/order_log prove accepted or rejected casts (never silent).
    pub inject_ability_mask: Option<u8>,
    pub replay: Option<litd_game::replay_viewer::ReplayViewerConfig>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeHostFinalReport {
    pub final_state: RuntimeHostState,
    pub state_readback: Option<RuntimeHostStateReadback>,
    pub replay: Option<litd_game::replay_viewer::ReplayViewerSnapshot>,
}

pub fn run_runtime_host(
    package: RuntimePackageReadback,
    config: RuntimeHostConfig,
) -> Result<RuntimeHostFinalReport, EngineError> {
    if matches!(config.max_frames, Some(0)) {
        return Err(runtime_host_error(
            "max_frames",
            "--max-frames must be greater than zero",
        ));
    }
    let event_loop = EventLoop::new()
        .map_err(|error| runtime_host_error("event_loop.create", error.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = RuntimeHostApp::new(package, config)?;
    app.write_state("accepted", None)?;
    event_loop
        .run_app(&mut app)
        .map_err(|error| runtime_host_error("event_loop.run", error.to_string()))?;
    if app.failed {
        return Err(runtime_host_error(
            "event_loop.failed",
            app.failure_reason
                .unwrap_or_else(|| "runtime host failed without a recorded reason".to_string()),
        ));
    }
    app.final_report()
}
