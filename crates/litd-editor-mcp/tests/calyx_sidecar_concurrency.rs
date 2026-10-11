//! Hermetic concurrency/failure-path coverage for the calyx-mcp sidecar transport
//! (#1792). These run under a plain `cargo test -p litd-editor-mcp --features
//! calyx-sidecar` with NO real addon: every test spawns the test-only
//! `calyx_sidecar_stub` binary (built by cargo, located via CARGO_BIN_EXE_) and drives
//! it into a chosen fault mode.
//!
//! Coverage: id correlation, mismatched-id typed error, recv timeout on an unresponsive
//! child, stderr-flood no-deadlock, EOF-on-child-death typed error, and Drop reaping the
//! child (no zombie).
#![cfg(feature = "calyx-sidecar")]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use litd_editor_mcp::calyx_sidecar::{
    CalyxSidecar, CalyxSidecarConfig, SIDECAR_CLOSED, SIDECAR_HANDSHAKE_FAILED,
    SIDECAR_PROTOCOL_ERROR,
};

/// Absolute path to the cargo-built stub binary. `CARGO_BIN_EXE_<name>` is populated by
/// cargo for integration tests and points at the freshly built target, so no separate
/// build step or hardcoded path is needed.
fn stub_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_calyx_sidecar_stub"))
}

fn config(mode: &str, timeout_ms: u64) -> CalyxSidecarConfig {
    CalyxSidecarConfig {
        binary: Some(stub_binary()),
        handshake_timeout_ms: timeout_ms,
        args: vec![mode.to_string()],
    }
}

#[test]
fn id_correlation_holds_across_sequential_requests() {
    let mut sidecar = CalyxSidecar::spawn(&config("echo", 5_000)).expect("stub spawns");
    // Three sequential requests: request() validates that each response id matches the
    // request id it just sent, so a monotonic run proves correlation is not off-by-one.
    for _ in 0..3 {
        let response = sidecar
            .request("ping", None)
            .expect("echo replies in order");
        assert!(response.error.is_none(), "echo mode must not error");
        assert!(response.result.is_some(), "echo mode returns a result");
    }
    // tools/list also decodes through the same framing without desync.
    let tools = sidecar.tools_list().expect("tools/list decodes");
    assert_eq!(tools.len(), 1, "stub advertises exactly one tool");
    assert_eq!(tools[0].name, "calyx.stub.ping");
}

#[test]
fn mismatched_response_id_is_a_typed_protocol_error() {
    let mut sidecar = CalyxSidecar::spawn(&config("mismatched-id", 5_000)).expect("stub spawns");
    let error = sidecar
        .request("ping", None)
        .expect_err("a wrong response id must fail closed, not be accepted");
    assert_eq!(
        error.code, SIDECAR_PROTOCOL_ERROR,
        "id-correlation violation must map to the protocol-error code"
    );
}

#[test]
fn unresponsive_child_hits_recv_timeout() {
    // sleep-forever reads the request and never replies; the bounded recv_timeout must
    // fire so the editor never blocks forever.
    let mut sidecar = CalyxSidecar::spawn(&config("sleep-forever", 300)).expect("stub spawns");
    let started = Instant::now();
    let error = sidecar
        .request("ping", None)
        .expect_err("an unresponsive child must time out");
    assert_eq!(
        error.code, SIDECAR_HANDSHAKE_FAILED,
        "an unresponsive child maps to the handshake/timeout code"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the timeout must bound the wait near handshake_timeout_ms, not block indefinitely"
    );
}

#[test]
fn stderr_flood_does_not_deadlock_the_transport() {
    // The stub floods >64 KiB to stderr before replying. If stderr were not drained on its
    // own thread, the child would block writing stderr and never reach the stdout reply,
    // deadlocking this request. A generous timeout still yields a valid response.
    let mut sidecar = CalyxSidecar::spawn(&config("stderr-flood", 10_000)).expect("stub spawns");
    let response = sidecar
        .request("ping", None)
        .expect("a stderr flood must not deadlock the response path");
    assert!(response.error.is_none());
    assert!(
        response.result.is_some(),
        "the reply must arrive despite the stderr flood"
    );
}

#[test]
fn child_death_mid_request_is_a_typed_closed_error() {
    // exit-mid dies after reading the request without replying; the reader thread sees EOF
    // and the transport must surface a typed closed error rather than hang.
    let mut sidecar = CalyxSidecar::spawn(&config("exit-mid", 5_000)).expect("stub spawns");
    let error = sidecar
        .request("ping", None)
        .expect_err("a child that dies mid-request must fail closed");
    assert_eq!(
        error.code, SIDECAR_CLOSED,
        "EOF on child death maps to the closed code"
    );
}

#[test]
fn drop_reaps_child_leaving_no_zombie() {
    let sidecar = CalyxSidecar::spawn(&config("sleep-forever", 5_000)).expect("stub spawns");
    let pid = sidecar.child_pid();
    assert!(process_alive(pid), "the child must be running before drop");
    drop(sidecar); // Drop kills + waits (reaps) the child.

    // wait() in Drop is synchronous, but poll briefly to absorb OS bookkeeping latency.
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        !process_alive(pid),
        "Drop must kill and reap the child (pid {pid} still present -- zombie/leak)"
    );
}

/// True if a process with `pid` currently exists. No new dependencies: Windows queries
/// `tasklist`, other platforms read `/proc`.
#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output();
    match output {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            !text.contains("No tasks") && text.contains(&format!("\"{pid}\""))
        }
        // If we cannot even query, do not falsely assert liveness.
        Err(_) => false,
    }
}

#[cfg(not(windows))]
fn process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}
