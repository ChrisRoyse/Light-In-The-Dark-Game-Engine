//! #1787: the `--lan-*` command-line surface of the `litd-game` binary.
//!
//! Detecting a LAN mode flag routes the runner into this HEADLESS path (no window, no GPU): it
//! loads the packaged `.litdgame` through the SAME `load_runtime_locked_package` path the normal
//! runner uses, then dispatches to the host / joiner / replay-verify roles in `lan_run`. The
//! two-process FSV drives exactly this surface, so the proof exercises the real binary code path.

use std::path::PathBuf;

use litd_engine::game_package::load_runtime_locked_package;
use litd_engine::game_runtime::RuntimeDatasetsManifest;

use crate::lan_error::LanError;
use crate::lan_run::{self, HostRunConfig, JoinRunConfig};

const LAN_MODE_FLAGS: [&str; 3] = ["--lan-host", "--lan-join", "--lan-verify-replay"];

/// Whether the argument list selects a LAN mode (so `main` can route before the GPU path).
pub fn is_lan_invocation(args: &[String]) -> bool {
    args.iter()
        .any(|arg| LAN_MODE_FLAGS.contains(&arg.as_str()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LanMode {
    Host,
    Join,
    Verify,
}

/// Parse the `--lan-*` args, load the package manifest, and run the selected role. Fail-loud
/// typed on any bad argument or dataset -- never a silent default.
pub fn run_lan(args: &[String]) -> Result<(), LanError> {
    let raw = Raw::parse(args)?;
    let manifest = load_manifest(&raw.package)?;
    match raw.mode()? {
        LanMode::Verify => {
            let path = raw
                .verify_replay
                .clone()
                .ok_or_else(|| arg_err("--lan-verify-replay requires a path"))?;
            let hash = lan_run::verify_replay(&manifest, &path)?;
            println!(
                "LAN_REPLAY_VERIFIED path={} final_hash={hash:016x}",
                path.display()
            );
            Ok(())
        }
        LanMode::Host => {
            let cfg = raw.host_config()?;
            lan_run::run_host(&manifest, &cfg).map(|_| ())
        }
        LanMode::Join => {
            let cfg = raw.join_config()?;
            lan_run::run_join(&manifest, &cfg).map(|_| ())
        }
    }
}

fn load_manifest(package: &Option<PathBuf>) -> Result<RuntimeDatasetsManifest, LanError> {
    let path = package
        .as_ref()
        .ok_or_else(|| arg_err("LAN mode requires --package <path.litdgame>"))?;
    let readback = load_runtime_locked_package(path, "litd-game-lan")?;
    readback
        .runtime
        .ok_or_else(|| arg_err("packaged .litdgame carries no runtime datasets"))
}

/// The raw parsed argument bag; role configs are derived from it.
#[derive(Debug, Default)]
struct Raw {
    mode_set: Option<LanMode>,
    package: Option<PathBuf>,
    join_addr: Option<String>,
    verify_replay: Option<PathBuf>,
    port: u16,
    capacity: u8,
    faction: u8,
    slot: u8,
    name: Option<String>,
    ticks: u64,
    delay: u64,
    grace_ms: u64,
    interval: u64,
    move_period: u64,
    replay: Option<PathBuf>,
    broadcast: bool,
    share_id: Option<String>,
    tamper_fingerprint: bool,
    divergence_at: Option<u64>,
    stall_at: Option<u64>,
    stall_ms: u64,
    discover: bool,
    discover_timeout_ms: u64,
}

impl Raw {
    fn parse(args: &[String]) -> Result<Self, LanError> {
        let mut raw = Raw {
            capacity: 2,
            ticks: 40,
            delay: 1,
            grace_ms: 8000,
            interval: 4,
            move_period: 2,
            stall_ms: 5000,
            discover_timeout_ms: 3000,
            ..Raw::default()
        };
        let mut iter = args.iter().cloned().peekable();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--lan-host" => raw.set_mode(LanMode::Host)?,
                "--lan-join" => {
                    raw.set_mode(LanMode::Join)?;
                    raw.join_addr = Some(next_value(&mut iter, "--lan-join")?);
                }
                "--lan-verify-replay" => {
                    raw.set_mode(LanMode::Verify)?;
                    raw.verify_replay =
                        Some(PathBuf::from(next_value(&mut iter, "--lan-verify-replay")?));
                }
                "--package" => {
                    raw.package = Some(PathBuf::from(next_value(&mut iter, "--package")?))
                }
                "--lan-port" => raw.port = next_parsed(&mut iter, "--lan-port")?,
                "--lan-capacity" => raw.capacity = next_parsed(&mut iter, "--lan-capacity")?,
                "--lan-faction" => raw.faction = next_parsed(&mut iter, "--lan-faction")?,
                "--lan-slot" => raw.slot = next_parsed(&mut iter, "--lan-slot")?,
                "--lan-name" => raw.name = Some(next_value(&mut iter, "--lan-name")?),
                "--lan-ticks" => raw.ticks = next_parsed(&mut iter, "--lan-ticks")?,
                "--lan-delay" => raw.delay = next_parsed(&mut iter, "--lan-delay")?,
                "--lan-grace-ms" => raw.grace_ms = next_parsed(&mut iter, "--lan-grace-ms")?,
                "--lan-interval" => raw.interval = next_parsed(&mut iter, "--lan-interval")?,
                "--lan-move-period" => {
                    raw.move_period = next_parsed(&mut iter, "--lan-move-period")?
                }
                "--lan-replay" => {
                    raw.replay = Some(PathBuf::from(next_value(&mut iter, "--lan-replay")?))
                }
                "--lan-broadcast" => raw.broadcast = true,
                "--lan-share-id" => raw.share_id = Some(next_value(&mut iter, "--lan-share-id")?),
                "--lan-tamper-fingerprint" => raw.tamper_fingerprint = true,
                "--lan-diverge-at" => {
                    raw.divergence_at = Some(next_parsed(&mut iter, "--lan-diverge-at")?)
                }
                "--lan-stall-at" => raw.stall_at = Some(next_parsed(&mut iter, "--lan-stall-at")?),
                "--lan-stall-ms" => raw.stall_ms = next_parsed(&mut iter, "--lan-stall-ms")?,
                "--lan-discover" => raw.discover = true,
                "--lan-discover-timeout-ms" => {
                    raw.discover_timeout_ms = next_parsed(&mut iter, "--lan-discover-timeout-ms")?
                }
                other => return Err(arg_err(format!("unknown LAN argument {other}"))),
            }
        }
        // Ensure a mode was selected (also validated lazily by `mode()`).
        raw.mode()?;
        Ok(raw)
    }

    fn mode(&self) -> Result<LanMode, LanError> {
        self.mode_set.ok_or_else(|| arg_err("no LAN mode selected"))
    }

    fn set_mode(&mut self, mode: LanMode) -> Result<(), LanError> {
        if self.mode_set.is_some() {
            return Err(arg_err("only one LAN mode may be selected"));
        }
        self.mode_set = Some(mode);
        Ok(())
    }

    fn host_config(&self) -> Result<HostRunConfig, LanError> {
        Ok(HostRunConfig {
            port: self.port,
            capacity: self.capacity,
            faction: self.faction,
            player_name: self.name.clone().unwrap_or_else(|| "host".to_string()),
            total_ticks: self.ticks,
            input_delay: self.delay,
            grace_ms: self.grace_ms,
            hash_report_interval: self.interval,
            move_period: self.move_period,
            replay_path: self.replay_or_default("host"),
            broadcast: self.broadcast,
            share_id: self
                .share_id
                .clone()
                .unwrap_or_else(|| "litd-lan".to_string()),
        })
    }

    fn join_config(&self) -> Result<JoinRunConfig, LanError> {
        Ok(JoinRunConfig {
            host_addr: self.join_addr.clone().unwrap_or_default(),
            discover: self.discover,
            discover_timeout_ms: self.discover_timeout_ms,
            faction: self.faction,
            requested_slot: if self.slot == 0 { 1 } else { self.slot },
            player_name: self.name.clone().unwrap_or_else(|| "join".to_string()),
            move_period: self.move_period,
            replay_path: self.replay_or_default("join"),
            tamper_fingerprint: self.tamper_fingerprint,
            divergence_at: self.divergence_at,
            stall_at: self.stall_at,
            stall_ms: self.stall_ms,
        })
    }

    fn replay_or_default(&self, role: &str) -> PathBuf {
        self.replay
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("lan_replay_{role}.ron")))
    }
}

fn next_value(
    iter: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    flag: &str,
) -> Result<String, LanError> {
    iter.next()
        .ok_or_else(|| arg_err(format!("{flag} requires a value")))
}

fn next_parsed<T: std::str::FromStr>(
    iter: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    flag: &str,
) -> Result<T, LanError> {
    let value = next_value(iter, flag)?;
    value
        .parse::<T>()
        .map_err(|_| arg_err(format!("{flag} expects a valid number, got {value:?}")))
}

fn arg_err(reason: impl Into<String>) -> LanError {
    LanError::Protocol {
        phase: "cli",
        reason: reason.into(),
    }
}
