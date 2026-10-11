//! `litd-game` — the game runner. Runs packaged `.litdgame` artifacts only
//! through the LitD runtime.

mod lan_bot;
mod lan_cli;
mod lan_error;
mod lan_lobby;
mod lan_lockstep;
mod lan_protocol;
mod lan_run;
mod lan_transport;
mod runtime_audio;
mod runtime_host;
mod runtime_host_module;
mod runtime_renderer;
mod runtime_report;
mod runtime_state;
mod runtime_state_text;
mod runtime_state_writer;

use std::{
    env, fs,
    path::{Path, PathBuf},
    process,
};

use litd_engine::{
    EngineError,
    game_package::{
        RuntimeGameRunReport, build_runtime_game_run_report, load_runtime_locked_package,
    },
    performance_quality::{GraphicsQualitySettings, GraphicsQualityTier},
};
use litd_game_cli::{QualityTier, ReplaySpeed as CliReplaySpeed};

use litd_game::replay_viewer::{ReplaySpeed, ReplayViewerConfig};
use runtime_host::{RuntimeHostConfig, RuntimeHostFinalReport, run_runtime_host};

// #1532: install the shared counting allocator as the process global allocator so
// `FrameStats.allocations` reflects REAL per-frame heap allocations (was hardcoded 0).
#[global_allocator]
static GLOBAL: litd_engine::frame_alloc::CountingAllocator =
    litd_engine::frame_alloc::CountingAllocator;

fn main() {
    litd_engine::init_logging();
    // #1787: a `--lan-*` invocation routes into the headless LAN match flow (no window / GPU)
    // before the normal packaged-runtime path. Fail loud typed on any LAN error.
    let raw_args: Vec<String> = env::args().skip(1).collect();
    if lan_cli::is_lan_invocation(&raw_args) {
        if let Err(error) = lan_cli::run_lan(&raw_args) {
            tracing::error!(
                error = %error,
                code = error.code(),
                "LitD LAN match flow failed closed"
            );
            eprintln!("litd-game LAN error [{}]: {error}", error.code());
            process::exit(1);
        }
        return;
    }
    match run_from_env() {
        Ok(RunnerOutcome::Validated(report)) => print_validated_report(&report),
        Ok(RunnerOutcome::Ran(report)) => print_runtime_report(&report),
        Err(error) => {
            tracing::error!(
                error = %error,
                code = "LITD-GAME-RUNNER-FAILED",
                "LitD game runner failed closed"
            );
            eprintln!("litd-game error: {error}");
            process::exit(1);
        }
    }
}

fn run_from_env() -> Result<RunnerOutcome, EngineError> {
    run_from_args(env::args().skip(1))
}

fn run_from_args<I, S>(args: I) -> Result<RunnerOutcome, EngineError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let cli = parse_args(args)?;
    let readback = load_runtime_locked_package(&cli.package_path, "litd-game-cli")?;
    let report = build_runtime_game_run_report(&readback);
    if let Some(path) = cli.readback_path {
        write_report_with_readback(&path, &report)?;
    }
    if cli.validate_only {
        return Ok(RunnerOutcome::Validated(Box::new(report)));
    }
    let replay = cli
        .replay_viewer_path
        .map(|replay_path| ReplayViewerConfig {
            replay_path,
            speed: cli.replay_speed,
            perspective: cli.replay_perspective,
            paused: cli.replay_paused,
            start_tick: cli.replay_start_tick,
        });
    let runtime_report = run_runtime_host(
        readback,
        RuntimeHostConfig {
            runtime_state_path: cli.runtime_state_path,
            max_frames: cli.max_frames,
            quality: cli.quality,
            gpu_validation: cli.gpu_validation,
            window_visible: !cli.hidden_window,
            inject_ability_mask: cli.inject_ability_mask,
            replay,
        },
    )?;
    Ok(RunnerOutcome::Ran(Box::new(runtime_report)))
}

#[derive(Debug)]
enum RunnerOutcome {
    Validated(Box<RuntimeGameRunReport>),
    Ran(Box<RuntimeHostFinalReport>),
}

// `GraphicsQualitySettings` carries f32 upscale knobs (#1741), so it is `PartialEq`
// but not `Eq`; this args struct follows suit.
#[derive(Debug, Clone, PartialEq)]
struct CliArgs {
    package_path: PathBuf,
    readback_path: Option<PathBuf>,
    runtime_state_path: Option<PathBuf>,
    validate_only: bool,
    max_frames: Option<u64>,
    quality: Option<GraphicsQualitySettings>,
    gpu_validation: bool,
    hidden_window: bool,
    /// #1894 FSV: one-shot QWER ability-slot bitmask (0..=15).
    inject_ability_mask: Option<u8>,
    replay_viewer_path: Option<PathBuf>,
    replay_speed: ReplaySpeed,
    replay_perspective: u32,
    replay_paused: bool,
    replay_start_tick: Option<u64>,
}

fn parse_args<I, S>(args: I) -> Result<CliArgs, EngineError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let parsed = litd_game_cli::parse_args(args)
        .map_err(|error| cli_error(error.field(), error.reason()))?;
    Ok(CliArgs {
        package_path: parsed.package_path,
        readback_path: parsed.readback_path,
        runtime_state_path: parsed.runtime_state_path,
        validate_only: parsed.validate_only,
        max_frames: parsed.max_frames,
        quality: parsed.quality.map(|tier| match tier {
            QualityTier::Low => GraphicsQualityTier::Low.settings(),
            QualityTier::Medium => GraphicsQualityTier::Medium.settings(),
            QualityTier::High => GraphicsQualityTier::High.settings(),
            QualityTier::Ultra => GraphicsQualityTier::Ultra.settings(),
        }),
        gpu_validation: parsed.gpu_validation,
        hidden_window: parsed.hidden_window,
        inject_ability_mask: parsed.inject_ability_mask,
        replay_viewer_path: parsed.replay_viewer_path,
        replay_speed: match parsed.replay_speed {
            CliReplaySpeed::Half => ReplaySpeed::Half,
            CliReplaySpeed::Normal => ReplaySpeed::Normal,
            CliReplaySpeed::Double => ReplaySpeed::Double,
            CliReplaySpeed::Quad => ReplaySpeed::Quad,
            CliReplaySpeed::Octuple => ReplaySpeed::Octuple,
        },
        replay_perspective: parsed.replay_perspective,
        replay_paused: parsed.replay_paused,
        replay_start_tick: parsed.replay_start_tick,
    })
}

fn write_report_with_readback(
    path: &Path,
    report: &RuntimeGameRunReport,
) -> Result<(), EngineError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            cli_error(
                "readback.parent",
                format!("create {}: {error}", parent.display()),
            )
        })?;
    }
    let bytes =
        serde_json::to_vec_pretty(report).map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize litd-game readback: {error}"),
        })?;
    fs::write(path, &bytes).map_err(|error| {
        cli_error(
            "readback.write",
            format!("write {}: {error}", path.display()),
        )
    })?;
    let readback = fs::read(path)
        .map_err(|error| cli_error("readback.read", format!("read {}: {error}", path.display())))?;
    if readback != bytes {
        return Err(cli_error(
            "readback.verify",
            "readback JSON changed after write",
        ));
    }
    Ok(())
}

fn cli_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    let field = format!("litd_game.cli.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-GAME-CLI-VALIDATION",
        "litd-game CLI rejected invocation"
    );
    EngineError::GamePackageInvalidConfig { field, reason }
}

fn print_validated_report(report: &RuntimeGameRunReport) {
    tracing::info!(
        runtime_id = %report.runtime_id,
        package_id = %report.package_id,
        package_sha256 = %report.package_sha256,
        manifest_integrity_verified = report.manifest_integrity_verified,
        runtime_lock_verified = report.runtime_lock_verified,
        playable_on_litd_runtime = report.playable_on_litd_runtime,
        "LitD package accepted by runtime"
    );
    println!(
        "litd-game accepted package={} sha={} playable={}",
        report.package_id, report.package_sha256, report.playable_on_litd_runtime
    );
}

fn print_runtime_report(report: &RuntimeHostFinalReport) {
    let state = &report.final_state;
    tracing::info!(
        package_id = %state.package_id,
        package_sha256 = %state.package_sha256,
        status = %state.status,
        frame = state.app_frame.as_ref().map(|frame| frame.frame_index).unwrap_or(0),
        fixed_tick_total = state.counters.fixed_tick_total,
        update_total = state.counters.update_total,
        "LitD package ran on runtime host"
    );
    let state_path = report
        .state_readback
        .as_ref()
        .map(|readback| readback.path.as_str())
        .unwrap_or("<not-requested>");
    println!(
        "litd-game ran package={} status={} frames={} fixed_ticks={} quality={} state={}",
        state.package_id,
        state.status,
        state
            .app_frame
            .as_ref()
            .map(|frame| frame.frame_index)
            .unwrap_or(0),
        state.counters.fixed_tick_total,
        state
            .quality
            .as_ref()
            .map(|quality| quality.tier.as_str())
            .unwrap_or("default"),
        state_path
    );
    if let Some(snapshot) = &report.replay {
        runtime_report::print_replay_report(snapshot);
    }
}
