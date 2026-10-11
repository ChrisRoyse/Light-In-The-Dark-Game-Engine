//! Dependency-light command-line parsing for `litd-game`.
//!
//! This package intentionally contains no engine or graphics dependency.  The
//! parser is production code used by the runner, while its focused tests can
//! compile without rebuilding the runtime/Calyx graph (#1895).

use std::{path::PathBuf, str::FromStr};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{field}: {reason}")]
pub struct CliError {
    field: String,
    reason: String,
}

impl CliError {
    fn new(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            reason: reason.into(),
        }
    }

    pub fn field(&self) -> &str {
        &self.field
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityTier {
    Low,
    Medium,
    High,
    Ultra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaySpeed {
    Half,
    Normal,
    Double,
    Quad,
    Octuple,
}

impl FromStr for ReplaySpeed {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "0.5" => Ok(Self::Half),
            "1" => Ok(Self::Normal),
            "2" => Ok(Self::Double),
            "4" => Ok(Self::Quad),
            "8" => Ok(Self::Octuple),
            other => Err(format!(
                "invalid replay speed {other}; expected 0.5, 1, 2, 4, or 8"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Fully validated values consumed by the engine-backed runner.
pub struct CliArgs {
    pub package_path: PathBuf,
    pub readback_path: Option<PathBuf>,
    pub runtime_state_path: Option<PathBuf>,
    pub validate_only: bool,
    pub max_frames: Option<u64>,
    pub quality: Option<QualityTier>,
    pub gpu_validation: bool,
    pub hidden_window: bool,
    pub inject_ability_mask: Option<u8>,
    pub replay_viewer_path: Option<PathBuf>,
    pub replay_speed: ReplaySpeed,
    pub replay_perspective: u32,
    pub replay_paused: bool,
    pub replay_start_tick: Option<u64>,
}

/// Parse the runner's dependency-light command line contract.
///
/// Keeping this boundary free of engine dependencies is intentional: parser edits can use the
/// targeted CLI gate without recompiling the renderer, runtime, or Calyx graph (#1895).
pub fn parse_args<I, S>(args: I) -> Result<CliArgs, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = args.into_iter().map(Into::into).peekable();
    let mut package_path = None;
    let mut readback_path = None;
    let mut runtime_state_path = None;
    let mut validate_only = false;
    let mut max_frames = None;
    let mut quality = None;
    let mut gpu_validation = false;
    let mut hidden_window = false;
    let mut inject_ability_mask = None;
    let mut replay_viewer_path = None;
    let mut replay_speed = ReplaySpeed::Normal;
    let mut replay_perspective = 0;
    let mut replay_paused = false;
    let mut replay_start_tick = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--package" => {
                if package_path.is_some() {
                    return Err(CliError::new(
                        "package",
                        "package path was provided more than once",
                    ));
                }
                package_path =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        CliError::new("package", "--package requires a path")
                    })?));
            }
            "--readback" => {
                if readback_path.is_some() {
                    return Err(CliError::new(
                        "readback",
                        "readback path was provided more than once",
                    ));
                }
                readback_path =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        CliError::new("readback", "--readback requires a path")
                    })?));
            }
            "--runtime-state" => {
                if runtime_state_path.is_some() {
                    return Err(CliError::new(
                        "runtime_state",
                        "runtime state path was provided more than once",
                    ));
                }
                runtime_state_path = Some(PathBuf::from(args.next().ok_or_else(|| {
                    CliError::new("runtime_state", "--runtime-state requires a path")
                })?));
            }
            "--validate-only" => {
                if validate_only {
                    return Err(CliError::new(
                        "validate_only",
                        "--validate-only was provided twice",
                    ));
                }
                validate_only = true;
            }
            "--gpu-validation" => {
                if gpu_validation {
                    return Err(CliError::new(
                        "gpu_validation",
                        "--gpu-validation was provided twice",
                    ));
                }
                gpu_validation = true;
            }
            "--hidden-window" => {
                if hidden_window {
                    return Err(CliError::new(
                        "hidden_window",
                        "--hidden-window was provided twice",
                    ));
                }
                hidden_window = true;
            }
            "--max-frames" => {
                if max_frames.is_some() {
                    return Err(CliError::new(
                        "max_frames",
                        "--max-frames was provided more than once",
                    ));
                }
                let value = args
                    .next()
                    .ok_or_else(|| CliError::new("max_frames", "--max-frames requires a number"))?;
                let parsed = value.parse::<u64>().map_err(|error| {
                    CliError::new(
                        "max_frames",
                        format!("--max-frames must be an integer: {error}"),
                    )
                })?;
                if parsed == 0 {
                    return Err(CliError::new(
                        "max_frames",
                        "--max-frames must be greater than zero",
                    ));
                }
                max_frames = Some(parsed);
            }
            "--quality" => {
                if quality.is_some() {
                    return Err(CliError::new(
                        "quality",
                        "--quality was provided more than once",
                    ));
                }
                let value = args
                    .next()
                    .ok_or_else(|| CliError::new("quality", "--quality requires a tier"))?;
                quality = Some(parse_quality_tier(&value)?);
            }
            "--inject-ability-mask" => {
                if inject_ability_mask.is_some() {
                    return Err(CliError::new(
                        "inject_ability_mask",
                        "--inject-ability-mask was provided more than once",
                    ));
                }
                let value = args.next().ok_or_else(|| {
                    CliError::new(
                        "inject_ability_mask",
                        "--inject-ability-mask requires an integer 1..=15",
                    )
                })?;
                let parsed = value.parse::<u8>().map_err(|error| {
                    CliError::new(
                        "inject_ability_mask",
                        format!("--inject-ability-mask must be an integer: {error}"),
                    )
                })?;
                if !(1..=15).contains(&parsed) {
                    return Err(CliError::new(
                        "inject_ability_mask",
                        "--inject-ability-mask must be 1..=15 (QWER bits 0..=3)",
                    ));
                }
                inject_ability_mask = Some(parsed);
            }
            "--replay-viewer" => {
                replay_viewer_path = Some(PathBuf::from(args.next().ok_or_else(|| {
                    CliError::new("replay_viewer", "--replay-viewer requires a replay path")
                })?));
            }
            "--replay-speed" => {
                let value = args.next().ok_or_else(|| {
                    CliError::new("replay_speed", "--replay-speed requires a value")
                })?;
                replay_speed = value
                    .parse()
                    .map_err(|error: String| CliError::new("replay_speed", error))?;
            }
            "--replay-perspective" => {
                let value = args.next().ok_or_else(|| {
                    CliError::new(
                        "replay_perspective",
                        "--replay-perspective requires a player id",
                    )
                })?;
                replay_perspective = value.parse::<u32>().map_err(|error| {
                    CliError::new(
                        "replay_perspective",
                        format!("player id must be a u32: {error}"),
                    )
                })?;
            }
            "--replay-paused" => replay_paused = true,
            "--replay-start-tick" => {
                if replay_start_tick.is_some() {
                    return Err(CliError::new(
                        "replay_start_tick",
                        "--replay-start-tick was provided more than once",
                    ));
                }
                let value = args.next().ok_or_else(|| {
                    CliError::new("replay_start_tick", "--replay-start-tick requires a number")
                })?;
                replay_start_tick = Some(value.parse::<u64>().map_err(|error| {
                    CliError::new(
                        "replay_start_tick",
                        format!("--replay-start-tick must be an integer: {error}"),
                    )
                })?);
            }
            "--help" | "-h" => return Err(CliError::new("help", usage())),
            other => {
                return Err(CliError::new(
                    "argument",
                    format!("unknown argument {other}; {}", usage()),
                ));
            }
        }
    }
    let package_path = package_path
        .ok_or_else(|| CliError::new("package", format!("missing --package; {}", usage())))?;
    if replay_viewer_path.is_none()
        && (replay_speed != ReplaySpeed::Normal
            || replay_perspective != 0
            || replay_paused
            || replay_start_tick.is_some())
    {
        return Err(CliError::new(
            "replay_viewer",
            "replay controls require --replay-viewer <save.ron>",
        ));
    }
    if replay_viewer_path.is_some() && inject_ability_mask.is_some() {
        return Err(CliError::new(
            "inject_ability_mask",
            "simulation ability injection is forbidden in replay-viewer mode",
        ));
    }
    Ok(CliArgs {
        package_path,
        readback_path,
        runtime_state_path,
        validate_only,
        max_frames,
        quality,
        gpu_validation,
        hidden_window,
        inject_ability_mask,
        replay_viewer_path,
        replay_speed,
        replay_perspective,
        replay_paused,
        replay_start_tick,
    })
}

fn parse_quality_tier(value: &str) -> Result<QualityTier, CliError> {
    match value {
        "low" => Ok(QualityTier::Low),
        "medium" => Ok(QualityTier::Medium),
        "high" => Ok(QualityTier::High),
        "ultra" => Ok(QualityTier::Ultra),
        other => Err(CliError::new(
            "quality",
            format!("unknown quality tier {other}; expected low, medium, high, or ultra"),
        )),
    }
}

fn usage() -> &'static str {
    "usage: litd-game --package <path> [--readback <path>] [--validate-only]"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_package() {
        let error = parse_args(Vec::<String>::new()).unwrap_err().to_string();
        assert!(error.contains("missing --package"));
    }

    #[test]
    fn accepts_replay_controls_without_runtime_dependencies() {
        let cli = parse_args([
            "--package",
            "game.litdgame",
            "--replay-viewer",
            "match.ron",
            "--replay-speed",
            "4",
            "--replay-perspective",
            "1",
            "--replay-start-tick",
            "1000",
            "--replay-paused",
        ])
        .unwrap();
        assert_eq!(cli.replay_speed, ReplaySpeed::Quad);
        assert_eq!(cli.replay_start_tick, Some(1000));
        assert!(cli.replay_paused);
    }

    #[test]
    fn accepts_all_runtime_controls() {
        let cli = parse_args([
            "--package",
            "game.litdgame",
            "--readback",
            "report.json",
            "--runtime-state",
            "state.json",
            "--validate-only",
            "--max-frames",
            "42",
            "--quality",
            "high",
            "--gpu-validation",
            "--hidden-window",
            "--inject-ability-mask",
            "15",
        ])
        .unwrap();
        assert_eq!(cli.package_path, PathBuf::from("game.litdgame"));
        assert_eq!(cli.readback_path, Some(PathBuf::from("report.json")));
        assert_eq!(cli.runtime_state_path, Some(PathBuf::from("state.json")));
        assert!(cli.validate_only);
        assert_eq!(cli.max_frames, Some(42));
        assert_eq!(cli.quality, Some(QualityTier::High));
        assert!(cli.gpu_validation);
        assert!(cli.hidden_window);
        assert_eq!(cli.inject_ability_mask, Some(15));
    }

    #[test]
    fn rejects_invalid_replay_controls() {
        let speed = parse_args(["--package", "game.litdgame", "--replay-speed", "turbo"])
            .unwrap_err()
            .to_string();
        assert!(speed.contains("expected 0.5, 1, 2, 4, or 8"));
        let perspective = parse_args(["--package", "game.litdgame", "--replay-perspective", "-1"])
            .unwrap_err()
            .to_string();
        assert!(perspective.contains("player id must be a u32"));
    }
}
