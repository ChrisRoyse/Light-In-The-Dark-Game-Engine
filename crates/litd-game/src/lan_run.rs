//! #1787: the headless LAN match run -- host and joiner roles wired over real sockets.
//!
//! This is the top of the LAN flow the `litd-game` binary dispatches into: it performs the join
//! handshake (with same-package enforcement), drives the lobby ready-up, then runs the
//! server-validated lockstep match to completion, printing the state-hash timeline and recording
//! a replay on BOTH ends. It contains ZERO game rules -- every gameplay decision is a
//! [`SessionOrder`] produced by the generic per-slot bot and sequenced by the host -- so it is
//! generic host machinery (#1783).

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use litd_engine::game_runtime::{RuntimeDatasetsManifest, SessionOrder, SessionReplay};

use crate::lan_bot::SlotBot;
use crate::lan_error::{LanError, protocol_err};
use crate::lan_lobby::LanLobby;
use crate::lan_lockstep::{FrameBuffer, LockstepMatch, hash_verdict};
use crate::lan_protocol::{
    DiscoveryBeacon, JoinHello, JoinVerdict, LAN_PROTOCOL_VERSION, LanMessage, MatchStart,
    is_timeout, read_frame, write_frame,
};
use crate::lan_transport;

/// Host-role configuration (all timing/grace/tick values are host settings, NOT hardcoded).
#[derive(Debug, Clone)]
pub struct HostRunConfig {
    pub port: u16,
    pub capacity: u8,
    pub faction: u8,
    pub player_name: String,
    pub total_ticks: u64,
    pub input_delay: u64,
    pub grace_ms: u64,
    pub hash_report_interval: u64,
    pub move_period: u64,
    pub replay_path: PathBuf,
    pub broadcast: bool,
    pub share_id: String,
}

/// Joiner-role configuration.
#[derive(Debug, Clone)]
pub struct JoinRunConfig {
    pub host_addr: String,
    pub faction: u8,
    pub requested_slot: u8,
    pub player_name: String,
    pub move_period: u64,
    pub replay_path: PathBuf,
    /// Optional LAN discovery: find the host via a UDP beacon instead of an explicit address.
    pub discover: bool,
    pub discover_timeout_ms: u64,
    /// Negative control: corrupt the handshake fingerprint so the host REFUSES the join typed.
    pub tamper_fingerprint: bool,
    /// Negative control: at this tick inject a rogue local order so the host's hash check KICKS.
    pub divergence_at: Option<u64>,
    /// Stall test: at this tick suspend sends for `stall_ms` to exercise the grace window.
    pub stall_at: Option<u64>,
    pub stall_ms: u64,
}

/// Re-verify a recorded replay headlessly: rebuild the session from the same manifest, replay the
/// saved order stream (which re-validates its own fingerprint), and return the reproduced final
/// hash. Feeds #83 and closes the "both replays reproduce the identical final state_hash" gate.
pub fn verify_replay(manifest: &RuntimeDatasetsManifest, path: &Path) -> Result<u64, LanError> {
    let replay = SessionReplay::load_from_path(path)?;
    let session = replay.restore(manifest)?;
    Ok(session.combined_state_hash())
}

/// Run the authoritative host: bind, accept one joiner, enforce same-package, ready-up, and drive
/// the lockstep match.
pub fn run_host(manifest: &RuntimeDatasetsManifest, cfg: &HostRunConfig) -> Result<(), LanError> {
    let listener = lan_transport::bind_host(cfg.port)?;
    let port = listener
        .local_addr()
        .map_err(|error| LanError::Io {
            context: "host.local_addr",
            reason: error.to_string(),
        })?
        .port();
    println!("LAN_LISTENING role=host port={port}");

    if cfg.broadcast {
        spawn_beacon(manifest, cfg, port);
    }

    let mut lobby = LanLobby::new(cfg.capacity, &cfg.player_name, cfg.faction)?;
    let mut stream = lan_transport::accept_one(&listener)?;

    let hello: JoinHello = read_frame(&mut stream)?;
    let client_slot = match admit(manifest, &mut lobby, &hello) {
        Ok(slot) => {
            write_frame(
                &mut stream,
                &JoinVerdict::Welcome {
                    assigned_slot: slot,
                    host_slot: lobby.host_slot(),
                    lobby: lobby.snapshot(),
                },
            )?;
            println!("LAN_JOIN_ACCEPTED role=host slot={slot}");
            slot
        }
        Err(error) => {
            let _ = write_frame(
                &mut stream,
                &JoinVerdict::Refused {
                    code: error.code().to_string(),
                    reason: error.to_string(),
                },
            );
            println!("LAN_REFUSED role=host code={} reason={error}", error.code());
            return Err(error);
        }
    };

    lobby.set_ready(lobby.host_slot())?;
    match read_frame::<_, LanMessage>(&mut stream)? {
        LanMessage::Ready => lobby.set_ready(client_slot)?,
        other => {
            return Err(protocol_err(
                "lobby",
                format!("expected Ready, got {other:?}"),
            ));
        }
    }
    write_frame(&mut stream, &LanMessage::Lobby(lobby.snapshot()))?;
    if !lobby.all_ready() {
        return Err(protocol_err(
            "lobby",
            "start attempted before all slots ready",
        ));
    }
    let start = MatchStart {
        first_tick: 0,
        total_ticks: cfg.total_ticks,
        input_delay_ticks: cfg.input_delay.max(1),
        grace_ms: cfg.grace_ms,
        hash_report_interval: cfg.hash_report_interval.max(1),
    };
    write_frame(&mut stream, &LanMessage::Start(start))?;
    println!(
        "LAN_START role=host ticks={} delay={} grace_ms={} interval={}",
        start.total_ticks, start.input_delay_ticks, start.grace_ms, start.hash_report_interval
    );

    lan_transport::arm_stall_timeout(&stream, cfg.grace_ms)?;
    let mut lm = LockstepMatch::new(manifest, start.hash_report_interval)?;
    let mut buffer = FrameBuffer::new();
    let host_bot = SlotBot::from_manifest(manifest, lobby.host_slot(), cfg.move_period)?;
    host_match_loop(
        &mut stream,
        &mut lm,
        &mut buffer,
        &host_bot,
        client_slot,
        &start,
    )?;
    write_frame(
        &mut stream,
        &LanMessage::Finished {
            tick: start.total_ticks,
            final_hash: lm.current_hash(),
        },
    )?;
    finish("host", manifest, lm, &cfg.replay_path)
}

/// Run the joiner: connect, handshake (optionally tampered), ready-up, then apply authoritative
/// frames and report hashes.
pub fn run_join(manifest: &RuntimeDatasetsManifest, cfg: &JoinRunConfig) -> Result<(), LanError> {
    let host_addr = resolve_host_addr(manifest, cfg)?;
    let mut stream = lan_transport::connect(&host_addr)?;
    let mut fingerprint = manifest.datasets_fingerprint.clone();
    if cfg.tamper_fingerprint {
        fingerprint.push_str("-TAMPERED");
    }
    let hello = JoinHello {
        protocol_version: LAN_PROTOCOL_VERSION,
        datasets_fingerprint: fingerprint,
        lockstep_hash: LockstepMatch::lockstep_handshake_hash(manifest)?,
        player_name: cfg.player_name.clone(),
        requested_slot: cfg.requested_slot,
        requested_faction: cfg.faction,
    };
    write_frame(&mut stream, &hello)?;
    let assigned_slot = match read_frame::<_, JoinVerdict>(&mut stream)? {
        JoinVerdict::Welcome { assigned_slot, .. } => {
            println!("LAN_JOIN_ACCEPTED role=join slot={assigned_slot}");
            assigned_slot
        }
        JoinVerdict::Refused { code, reason } => {
            println!("LAN_REFUSED role=join code={code} reason={reason}");
            return Err(protocol_err("join", format!("refused by host: {code}")));
        }
    };

    write_frame(&mut stream, &LanMessage::Ready)?;
    match read_frame::<_, LanMessage>(&mut stream)? {
        LanMessage::Lobby(_) => {}
        other => {
            return Err(protocol_err(
                "lobby",
                format!("expected Lobby, got {other:?}"),
            ));
        }
    }
    let start = match read_frame::<_, LanMessage>(&mut stream)? {
        LanMessage::Start(start) => start,
        other => {
            return Err(protocol_err(
                "lobby",
                format!("expected Start, got {other:?}"),
            ));
        }
    };
    println!(
        "LAN_START role=join ticks={} slot={assigned_slot}",
        start.total_ticks
    );

    let mut lm = LockstepMatch::new(manifest, start.hash_report_interval)?;
    let bot = SlotBot::from_manifest(manifest, assigned_slot, cfg.move_period)?;
    join_match_loop(&mut stream, &mut lm, &bot, &start, cfg)?;
    finish("join", manifest, lm, &cfg.replay_path)
}

/// Same-package + build enforcement, then seat. Any mismatch is a typed error the caller turns
/// into a `Refused` verdict.
fn admit(
    manifest: &RuntimeDatasetsManifest,
    lobby: &mut LanLobby,
    hello: &JoinHello,
) -> Result<u8, LanError> {
    if hello.protocol_version != LAN_PROTOCOL_VERSION {
        return Err(LanError::ProtocolVersion {
            host: LAN_PROTOCOL_VERSION,
            joiner: hello.protocol_version,
        });
    }
    if hello.datasets_fingerprint != manifest.datasets_fingerprint {
        return Err(LanError::PackageMismatch {
            field: "datasets_fingerprint",
            host: manifest.datasets_fingerprint.clone(),
            joiner: hello.datasets_fingerprint.clone(),
        });
    }
    let expected = LockstepMatch::lockstep_handshake_hash(manifest)?;
    if hello.lockstep_hash != expected {
        return Err(LanError::PackageMismatch {
            field: "lockstep_hash",
            host: format!("{expected:016x}"),
            joiner: format!("{:016x}", hello.lockstep_hash),
        });
    }
    lobby.seat_joiner(
        &hello.player_name,
        hello.requested_slot,
        hello.requested_faction,
    )
}

/// Host authoritative loop. `s` walks the pipeline; frame `T = s - delay` is applied once its
/// submissions (buffered `delay` steps earlier) are guaranteed present.
fn host_match_loop(
    stream: &mut TcpStream,
    lm: &mut LockstepMatch,
    buffer: &mut FrameBuffer,
    host_bot: &SlotBot,
    client_slot: u8,
    start: &MatchStart,
) -> Result<(), LanError> {
    let delay = start.input_delay_ticks;
    let total = start.total_ticks;
    for s in 0..(total + delay) {
        if s < total {
            match read_with_grace(stream, start.grace_ms, s)? {
                LanMessage::OrderSubmit {
                    execute_tick,
                    orders,
                } => {
                    if execute_tick != s {
                        return Err(protocol_err("match", "client submit tick out of order"));
                    }
                    buffer.submit(s, client_slot, orders);
                }
                other => {
                    return Err(protocol_err(
                        "match",
                        format!("expected OrderSubmit, got {other:?}"),
                    ));
                }
            }
            buffer.submit(s, host_bot.slot(), host_bot.orders_for_tick(s));
        }
        if s >= delay {
            let tick = s - delay;
            let commands = buffer.drain_frame(tick);
            write_frame(
                stream,
                &LanMessage::Frame {
                    tick,
                    commands: commands.clone(),
                },
            )?;
            let host_hash = lm.apply_frame(tick, &commands)?;
            if lm.is_report_tick(tick) {
                println!("LAN_HASH role=host tick={tick} hash={host_hash:016x}");
                match read_with_grace(stream, start.grace_ms, tick)? {
                    LanMessage::HashReport {
                        tick: reported,
                        hash,
                    } => {
                        if reported != tick {
                            return Err(protocol_err("match", "client hash report tick mismatch"));
                        }
                        if let Err(kick) = hash_verdict(tick, host_hash, hash) {
                            let _ = write_frame(
                                stream,
                                &LanMessage::Kick {
                                    code: kick.code().to_string(),
                                    reason: kick.to_string(),
                                },
                            );
                            println!("LAN_KICK role=host code={} tick={tick}", kick.code());
                            return Err(kick);
                        }
                        println!("LAN_HASH_OK role=host tick={tick}");
                    }
                    other => {
                        return Err(protocol_err(
                            "match",
                            format!("expected HashReport, got {other:?}"),
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Joiner loop: submit this slot's orders for the pipeline tick, apply the authoritative frame,
/// report the resulting hash. Honours the stall and divergence negative controls.
fn join_match_loop(
    stream: &mut TcpStream,
    lm: &mut LockstepMatch,
    bot: &SlotBot,
    start: &MatchStart,
    cfg: &JoinRunConfig,
) -> Result<(), LanError> {
    let delay = start.input_delay_ticks;
    let total = start.total_ticks;
    for s in 0..(total + delay) {
        if s < total {
            if cfg.stall_at == Some(s) {
                println!("LAN_STALL role=join tick={s} ms={}", cfg.stall_ms);
                thread::sleep(Duration::from_millis(cfg.stall_ms));
            }
            write_frame(
                stream,
                &LanMessage::OrderSubmit {
                    execute_tick: s,
                    orders: bot.orders_for_tick(s),
                },
            )?;
        }
        if s >= delay {
            let tick = s - delay;
            match read_frame::<_, LanMessage>(stream)? {
                LanMessage::Frame {
                    tick: framed,
                    commands,
                } => {
                    if framed != tick {
                        return Err(protocol_err("match", "authoritative frame out of order"));
                    }
                    let hash = lm.apply_frame(tick, &commands)?;
                    let mut report_hash = hash;
                    if cfg.divergence_at == Some(tick) {
                        lm.inject_local_divergence(SessionOrder::GrantResource {
                            key: "gold".to_string(),
                            amount: 999_999,
                        })?;
                        report_hash = lm.current_hash();
                        println!("LAN_DIVERGE role=join tick={tick}");
                    }
                    if lm.is_report_tick(tick) {
                        println!("LAN_HASH role=join tick={tick} hash={report_hash:016x}");
                        write_frame(
                            stream,
                            &LanMessage::HashReport {
                                tick,
                                hash: report_hash,
                            },
                        )?;
                    }
                }
                LanMessage::Kick { code, reason } => {
                    println!("LAN_KICK role=join code={code} reason={reason}");
                    return Err(protocol_err("match", format!("kicked by host: {code}")));
                }
                other => {
                    return Err(protocol_err(
                        "match",
                        format!("expected Frame, got {other:?}"),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Read one message, converting a stall-window timeout into a typed [`LanError::StallTimeout`].
fn read_with_grace(
    stream: &mut TcpStream,
    grace_ms: u64,
    tick: u64,
) -> Result<LanMessage, LanError> {
    match read_frame::<_, LanMessage>(stream) {
        Ok(message) => Ok(message),
        Err(error) if is_timeout(&error) => Err(LanError::StallTimeout { tick, grace_ms }),
        Err(error) => Err(error),
    }
}

fn finish(
    role: &'static str,
    manifest: &RuntimeDatasetsManifest,
    lm: LockstepMatch,
    replay_path: &Path,
) -> Result<(), LanError> {
    let replay = lm.capture_replay(manifest)?;
    replay.save_to_path(replay_path)?;
    let final_hash = lm.current_hash();
    println!(
        "LAN_DONE role={role} frames={} final_hash={final_hash:016x} replay={}",
        lm.frames_applied(),
        replay_path.display()
    );
    Ok(())
}

/// Resolve the host address a joiner connects to: the explicit `host_addr` unless discovery is
/// requested, in which case listen for a host beacon carrying our package fingerprint and use
/// its advertised TCP port. Discovery is the optional #1787 convenience; join-by-IP is primary.
fn resolve_host_addr(
    manifest: &RuntimeDatasetsManifest,
    cfg: &JoinRunConfig,
) -> Result<String, LanError> {
    if !cfg.discover {
        return Ok(cfg.host_addr.clone());
    }
    let timeout = Duration::from_millis(cfg.discover_timeout_ms.max(1));
    match lan_transport::listen_for_beacon(timeout)? {
        Some(discovered)
            if discovered.beacon.datasets_fingerprint == manifest.datasets_fingerprint =>
        {
            let addr = discovered.tcp_addr().to_string();
            println!(
                "LAN_DISCOVERED role=join addr={addr} share_id={}",
                discovered.beacon.share_id
            );
            Ok(addr)
        }
        Some(discovered) => Err(LanError::PackageMismatch {
            field: "datasets_fingerprint",
            host: discovered.beacon.datasets_fingerprint,
            joiner: manifest.datasets_fingerprint.clone(),
        }),
        None => Err(protocol_err(
            "discover",
            "no LAN host beacon found before timeout",
        )),
    }
}

fn spawn_beacon(manifest: &RuntimeDatasetsManifest, cfg: &HostRunConfig, port: u16) {
    let beacon = DiscoveryBeacon {
        protocol_version: LAN_PROTOCOL_VERSION,
        share_id: cfg.share_id.clone(),
        tcp_port: port,
        datasets_fingerprint: manifest.datasets_fingerprint.clone(),
    };
    thread::spawn(move || {
        for _ in 0..10 {
            if lan_transport::broadcast_beacon(&beacon).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(200));
        }
    });
}
