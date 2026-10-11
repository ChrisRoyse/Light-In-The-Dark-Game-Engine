//! #1787 FSV: LAN match flow in the packaged game -- host/join over REAL loopback sockets in TWO
//! separate OS processes of the real `litd-game` binary, server-validated lockstep, ~5s stall
//! survival, both-ends replay re-verification, and typed negative controls (tampered package
//! REFUSED, rogue local order KICKED). #85 loopback + 2-process gate; the 2-physical-machine run
//! remains #85's exit criterion.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use litd_editor::asset_products::assign_vault_model_to_unit;
use litd_editor::asset_vault::AssetVaultCatalog;
use litd_editor::history::{EditorDocument, EditorObject};
use litd_editor::quick_share::{
    QuickShareBuildRequest, QuickShareEndpoint, write_quick_share_package,
};
use litd_editor::runtime_datasets_lowering::build_runtime_litdgame;
use litd_engine::{EngineError, fsv_artifacts::begin_issue_run, math::Transform, math::Vec3};
use serde_json::json;

const SHARE_ID: &str = "issue1787lan";
const MAX_PACKAGE_BYTES: usize = 4 * 1024 * 1024;
const UNIT_IDS: [&str; 5] = [
    "type_worker",
    "type_town_hall",
    "placed_hall_p0",
    "placed_soldier_p0",
    "placed_soldier_p1",
];
const MATCH_TICKS: u64 = 40;
const HASH_INTERVAL: u64 = 4;
const GRACE_MS: u64 = 8000;
const STALL_TICK: u64 = 6;
const STALL_MS: u64 = 5000;
const DIVERGE_TICK: u64 = 8;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1787_lan_match_flow")?;

    // --- Package a real 1v1 melee through the production path. ---
    let document = author_1v1_melee()?;
    let request = QuickShareBuildRequest::new(
        SHARE_ID,
        "agent.issue1787",
        QuickShareEndpoint::loopback(SHARE_ID, 8497),
        MAX_PACKAGE_BYTES,
    );
    let package = build_runtime_litdgame(&document, &request)?;
    let datasets_fingerprint = package
        .runtime
        .as_ref()
        .ok_or("packaged .litdgame carried no runtime datasets")?
        .datasets_fingerprint
        .clone();
    let package_path = run.run_dir().join("issue1787.litdgame.json");
    write_quick_share_package(&package_path, &package, MAX_PACKAGE_BYTES)?;

    let litd_game = locate_litd_game_binary()?;

    // --- Happy path + stall survival: host + joiner, 1v1, ~5s stall inside an 8s grace. ---
    let host_replay = run.run_dir().join("host_replay.ron");
    let join_replay = run.run_dir().join("join_replay.ron");
    let happy = run_match(
        &litd_game,
        &package_path,
        run.run_dir(),
        "happy",
        &host_extra(&host_replay),
        &join_happy_extra(&join_replay),
    )?;

    let host_final = extract_hash(&happy.host_out, "LAN_DONE role=host", "final_hash")
        .ok_or("host printed no final hash")?;
    let join_final = extract_hash(&happy.join_out, "LAN_DONE role=join", "final_hash")
        .ok_or("joiner printed no final hash")?;
    let host_timeline = extract_timeline(&happy.host_out, "role=host");
    let join_timeline = extract_timeline(&happy.join_out, "role=join");
    let hashes_agree_live = host_final == join_final;
    let periodic_validation = count_marker(&happy.host_out, "LAN_HASH_OK role=host") >= 2;
    let stall_survived = happy.host_out.contains("LAN_DONE role=host")
        && happy.join_out.contains("LAN_STALL role=join")
        && happy.host_ok
        && happy.join_ok;

    // --- Headless replay re-verification through the real binary (feeds #83). ---
    let host_verify = verify_replay(&litd_game, &package_path, &host_replay)?;
    let join_verify = verify_replay(&litd_game, &package_path, &join_replay)?;
    let replays_reverify =
        host_verify == host_final && join_verify == join_final && host_verify == join_verify;

    // --- Negative control 1: tampered package fingerprint REFUSED typed. ---
    let refusal = run_match(
        &litd_game,
        &package_path,
        run.run_dir(),
        "refuse",
        &host_extra(&run.run_dir().join("refuse_host.ron")),
        &join_tamper_extra(&run.run_dir().join("refuse_join.ron")),
    )?;
    let refusal_code = extract_field(&refusal.host_out, "LAN_REFUSED role=host", "code");
    let refusal_typed =
        refusal_code.as_deref() == Some("LITD_LAN_PACKAGE_MISMATCH") && !refusal.host_ok;

    // --- Negative control 2: rogue local order -> divergence DETECTED + KICKED typed. ---
    let divergence = run_match(
        &litd_game,
        &package_path,
        run.run_dir(),
        "diverge",
        &host_extra(&run.run_dir().join("diverge_host.ron")),
        &join_diverge_extra(&run.run_dir().join("diverge_join.ron")),
    )?;
    let kick_code = extract_field(&divergence.host_out, "LAN_KICK role=host", "code");
    let divergence_kicked =
        kick_code.as_deref() == Some("LITD_LAN_DIVERGENCE_KICK") && !divergence.host_ok;

    let verdict = hashes_agree_live
        && periodic_validation
        && stall_survived
        && replays_reverify
        && refusal_typed
        && divergence_kicked
        && !host_timeline.is_empty()
        && !join_timeline.is_empty();

    let fmt_tl = |tl: &[(u64, String)]| {
        tl.iter()
            .map(|(t, h)| format!("t{t}:{h}"))
            .collect::<Vec<_>>()
    };
    let report = json!({
        "schema": "litd.editor.issue1787.lan_match_flow_fsv.v1",
        "issue": 1787, "parents": [1784, 1783], "unblocks": [85, 83],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "sufficiency_argument": "One 1v1 melee packaged through build_runtime_litdgame is host+joined by TWO separate OS processes of the REAL litd-game binary over loopback TCP. Both ready-up and run a deterministic, server-validated lockstep match: orders flow from both processes, periodic client state-hash reports are compared by the authoritative host (LAN_HASH_OK), a ~5s client send-stall survives the 8s grace window and play resumes, and a replay is recorded on both ends. Each replay re-verifies headlessly through the same binary to the identical final state hash. Two typed negative controls -- a tampered package fingerprint REFUSED (LITD_LAN_PACKAGE_MISMATCH) and a rogue local order KICKED (LITD_LAN_DIVERGENCE_KICK) -- cover same-package enforcement and divergence response. Ticks advance on the lockstep schedule, not packet arrival, so the stall changes no hash: the two live final hashes and both replay re-verifications are one value.",
        "datasets_fingerprint": datasets_fingerprint,
        "match_ticks": MATCH_TICKS,
        "grace_ms_vs_stall_ms": format!("{GRACE_MS}/{STALL_MS}"),
        "host_final_state_hash": format!("{host_final:016x}"),
        "join_final_state_hash": format!("{join_final:016x}"),
        "hashes_agree_live": hashes_agree_live,
        "periodic_hash_validation_observed": periodic_validation,
        "stall_survived": stall_survived,
        "host_replay_reverify_hash": format!("{host_verify:016x}"),
        "join_replay_reverify_hash": format!("{join_verify:016x}"),
        "replays_reverify_identical": replays_reverify,
        "neg_refusal_code": refusal_code,
        "neg_refusal_typed": refusal_typed,
        "neg_divergence_kick_code": kick_code,
        "neg_divergence_kicked_typed": divergence_kicked,
        "host_state_hash_timeline": fmt_tl(&host_timeline),
        "join_state_hash_timeline": fmt_tl(&join_timeline),
    });
    let readback_report = run.write_report(&report)?;
    let report_sha256_certutil = certutil_sha256(&readback_report.path);
    println!(
        "ISSUE1787_LAN_MATCH_FLOW_FSV verdict={} report={} report_sha256={} report_sha256_certutil={} host_final={host_final:016x} join_final={join_final:016x} replays_reverify={replays_reverify} stall_survived={stall_survived} refusal={refusal_typed} divergence_kick={divergence_kicked}",
        if verdict { "GREEN" } else { "RED" },
        readback_report.path.display(),
        readback_report.sha256,
        report_sha256_certutil.as_deref().unwrap_or("NA"),
    );
    for (role, timeline) in [("host", &host_timeline), ("join", &join_timeline)] {
        println!("-- {role} state-hash timeline --");
        for (tick, hash) in timeline {
            println!("  {role} t{tick:>3} {hash}");
        }
    }
    if !verdict {
        return Err("ISSUE1787 FSV verdict RED: a LAN acceptance predicate failed".into());
    }
    Ok(())
}

/// The output of one host+join process pair.
struct PairOutput {
    host_out: String,
    join_out: String,
    host_ok: bool,
    join_ok: bool,
}

fn host_extra(replay: &Path) -> Vec<String> {
    vec![
        "--lan-replay".into(),
        replay.display().to_string(),
        "--lan-ticks".into(),
        MATCH_TICKS.to_string(),
        "--lan-interval".into(),
        HASH_INTERVAL.to_string(),
        "--lan-grace-ms".into(),
        GRACE_MS.to_string(),
        "--lan-move-period".into(),
        "2".into(),
    ]
}

fn join_common(replay: &Path) -> Vec<String> {
    vec![
        "--lan-replay".into(),
        replay.display().to_string(),
        "--lan-move-period".into(),
        "2".into(),
    ]
}

fn join_happy_extra(replay: &Path) -> Vec<String> {
    let mut args = join_common(replay);
    args.extend([
        "--lan-stall-at".into(),
        STALL_TICK.to_string(),
        "--lan-stall-ms".into(),
        STALL_MS.to_string(),
    ]);
    args
}

fn join_tamper_extra(replay: &Path) -> Vec<String> {
    let mut args = join_common(replay);
    args.push("--lan-tamper-fingerprint".into());
    args
}

fn join_diverge_extra(replay: &Path) -> Vec<String> {
    let mut args = join_common(replay);
    args.extend(["--lan-diverge-at".into(), DIVERGE_TICK.to_string()]);
    args
}

/// Spawn a real host process, wait until it is listening, spawn the joiner, and collect both
/// processes' combined stdout/stderr and exit status. REAL sockets on loopback (no channels).
fn run_match(
    litd_game: &Path,
    package: &Path,
    run_dir: &Path,
    tag: &str,
    host_extra: &[String],
    join_extra: &[String],
) -> Result<PairOutput, Box<dyn std::error::Error>> {
    let port = free_port()?;
    let host_log = run_dir.join(format!("{tag}_host.log"));
    let join_log = run_dir.join(format!("{tag}_join.log"));

    let mut host_args = vec![
        "--package".to_string(),
        package.display().to_string(),
        "--lan-host".to_string(),
        "--lan-port".to_string(),
        port.to_string(),
    ];
    host_args.extend_from_slice(host_extra);
    let mut host = spawn_logged(litd_game, &host_args, &host_log)?;

    wait_for_marker(&host_log, "LAN_LISTENING", Duration::from_secs(30))?;

    let mut join_args = vec![
        "--package".to_string(),
        package.display().to_string(),
        "--lan-join".to_string(),
        format!("127.0.0.1:{port}"),
    ];
    join_args.extend_from_slice(join_extra);
    let mut join = spawn_logged(litd_game, &join_args, &join_log)?;

    let join_status = join.wait()?;
    let host_status = host.wait()?;
    Ok(PairOutput {
        host_out: std::fs::read_to_string(&host_log).unwrap_or_default(),
        join_out: std::fs::read_to_string(&join_log).unwrap_or_default(),
        host_ok: host_status.success(),
        join_ok: join_status.success(),
    })
}

/// Re-verify a recorded replay headlessly through the real binary; returns the reproduced hash.
fn verify_replay(
    litd_game: &Path,
    package: &Path,
    replay: &Path,
) -> Result<u64, Box<dyn std::error::Error>> {
    let output = Command::new(litd_game)
        .args([
            "--package",
            &package.display().to_string(),
            "--lan-verify-replay",
            &replay.display().to_string(),
        ])
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    extract_hash(&text, "LAN_REPLAY_VERIFIED", "final_hash")
        .ok_or_else(|| format!("replay {} did not re-verify", replay.display()).into())
}

fn spawn_logged(
    bin: &Path,
    args: &[String],
    log: &Path,
) -> Result<std::process::Child, Box<dyn std::error::Error>> {
    let file = std::fs::File::create(log)?;
    let errclone = file.try_clone()?;
    let child = Command::new(bin)
        .args(args)
        .stdout(file)
        .stderr(errclone)
        .spawn()?;
    Ok(child)
}

fn wait_for_marker(
    log: &Path,
    marker: &str,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(log)
            && text.contains(marker)
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(format!("marker {marker:?} never appeared in {}", log.display()).into())
}

/// Bind an ephemeral loopback port, read it, and release it for the host to claim.
fn free_port() -> Result<u16, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Derive the sibling `litd-game` binary from this example's own path
/// (target/<profile>/examples/<this> -> target/<profile>/litd-game[.exe]).
fn locate_litd_game_binary() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let profile_dir = exe
        .parent()
        .and_then(|examples| examples.parent())
        .ok_or("cannot locate target profile dir from example exe")?;
    let name = if cfg!(windows) {
        "litd-game.exe"
    } else {
        "litd-game"
    };
    let candidate = profile_dir.join(name);
    if candidate.exists() {
        return Ok(candidate);
    }
    Err(format!(
        "litd-game binary not found at {}; build it first: cargo build -p litd-game",
        candidate.display()
    )
    .into())
}

fn extract_field(text: &str, line_marker: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    text.lines()
        .find(|line| line.contains(line_marker))
        .and_then(|line| {
            line.split_whitespace()
                .find_map(|token| token.strip_prefix(&needle))
                .map(|value| value.to_string())
        })
}

fn extract_hash(text: &str, line_marker: &str, key: &str) -> Option<u64> {
    let value = extract_field(text, line_marker, key)?;
    u64::from_str_radix(&value, 16).ok()
}

fn count_marker(text: &str, marker: &str) -> usize {
    text.lines().filter(|line| line.contains(marker)).count()
}

fn extract_timeline(text: &str, role_marker: &str) -> Vec<(u64, String)> {
    text.lines()
        .filter(|line| line.contains("LAN_HASH ") && line.contains(role_marker))
        .filter_map(|line| {
            let tick = line
                .split_whitespace()
                .find_map(|t| t.strip_prefix("tick="))?
                .parse::<u64>()
                .ok()?;
            let hash = line
                .split_whitespace()
                .find_map(|t| t.strip_prefix("hash="))?
                .to_string();
            Some((tick, hash))
        })
        .collect()
}

// The `author_1v1_melee` document builder lives in a sibling file textually included here (it
// shares this module's imports + constants). Kept separate so each `.rs` stays under the
// <500-line ratchet (#800 sec.6).
include!("fixtures/issue1787_lan_authoring.rs");

fn certutil_sha256(path: &Path) -> Option<String> {
    let output = Command::new("certutil")
        .args(["-hashfile", &path.display().to_string(), "SHA256"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let candidate: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        if candidate.len() == 64 && candidate.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Some(candidate.to_ascii_lowercase());
        }
    }
    None
}
