use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::CalyxProbeCache;
use crate::offline_degradation::{CalyxAvailabilityProbe, CalyxEndpointProbe};

/// Empty dir under the canonical GameEditor Calyx root — a valid, readable `CALYX_HOME` that
/// passes `validate_gameeditor_calyx_home`, so the probe reaches the (blocking) endpoint step.
fn valid_home(name: &str) -> std::path::PathBuf {
    let home = litd_engine::calyx_paths::canonical_gameeditor_calyx_root()
        .unwrap()
        .join("tmp")
        .join(format!(
            "litd-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    std::fs::create_dir_all(&home).unwrap();
    home
}

/// A listener that accepts every connection and then HOLDS it open without answering the
/// initialize round-trip, so each probe blocks for the full `timeout_ms` (CALYX_ENDPOINT_TIMEOUT)
/// — a real, deterministic "configured but unresponsive Calyx". Returns the address plus a stop
/// flag + join handle to shut it down.
fn spawn_unresponsive_endpoint() -> (String, Arc<AtomicBool>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        let mut held = Vec::new();
        while !stop_thread.load(Ordering::Relaxed) {
            match listener.accept() {
                // Hold the accepted stream so the client's initialize read blocks until timeout.
                Ok((stream, _)) => held.push(stream),
                Err(_) => thread::sleep(Duration::from_millis(5)),
            }
        }
    });
    (address, stop, handle)
}

fn unresponsive_probe(
    home: &std::path::Path,
    address: &str,
    timeout_ms: u64,
) -> CalyxAvailabilityProbe {
    let mut probe = CalyxAvailabilityProbe::local_home("issue1461-nonblocking", home);
    probe.endpoint = Some(CalyxEndpointProbe {
        address: address.to_string(),
        require_initialize: true,
    });
    probe.timeout_ms = timeout_ms;
    probe
}

#[test]
fn poll_never_blocks_the_ui_thread_on_an_unreachable_endpoint() {
    // #1461 item 1: the draw-path poll() must never block on the network. Proven against a REAL
    // unresponsive TCP endpoint (no mocks): a synchronous probe blocks ~timeout_ms, but poll()
    // returns in a small fraction of that even while a background probe is in flight.
    const TIMEOUT_MS: u64 = 400;
    let home = valid_home("poll-nonblock");
    let (address, stop, listener_handle) = spawn_unresponsive_endpoint();
    let probe = unresponsive_probe(&home, &address, TIMEOUT_MS);

    // Baseline: the initial SYNCHRONOUS probe (done once at construction) genuinely blocks for
    // ~timeout_ms — this is exactly the per-frame stall we are eliminating.
    let build_start = Instant::now();
    let mut cache = CalyxProbeCache::new(
        "ai_coauthor",
        "AI co-author",
        &probe,
        Duration::from_millis(50),
    )
    .unwrap();
    let sync_probe_ms = build_start.elapsed().as_millis();
    assert!(
        sync_probe_ms >= (TIMEOUT_MS as u128) / 2,
        "a synchronous probe against an unresponsive endpoint must block ~timeout_ms; got {sync_probe_ms} ms"
    );
    // Correctness: the probe result is a disabled panel (endpoint unreachable).
    assert!(
        !cache.panel().enabled,
        "unreachable endpoint must yield a disabled Calyx panel"
    );

    // Now hammer poll() the way the immediate-mode UI would (many frames), with the endpoint
    // still down. If poll() blocked like the old per-frame probe (~timeout_ms each), 20 polls
    // would take ~20 * 400 ms = 8 s. Async, they finish in well under a quarter of that.
    const FRAMES: usize = 20;
    let mut max_single_poll = Duration::ZERO;
    let loop_start = Instant::now();
    for _ in 0..FRAMES {
        let poll_start = Instant::now();
        cache.poll(&probe);
        let elapsed = poll_start.elapsed();
        if elapsed > max_single_poll {
            max_single_poll = elapsed;
        }
        // Simulate ~60 fps frame spacing; the background probe runs across many of these frames.
        thread::sleep(Duration::from_millis(16));
    }
    let loop_total = loop_start.elapsed();

    let blocking_budget = Duration::from_millis((FRAMES as u64) * TIMEOUT_MS);
    assert!(
        loop_total < blocking_budget / 4,
        "poll loop must not block per frame: {FRAMES} polls took {loop_total:?} (blocking would be ~{blocking_budget:?})"
    );
    assert!(
        max_single_poll < Duration::from_millis(TIMEOUT_MS / 2),
        "no single poll may approach the probe timeout; worst was {max_single_poll:?}"
    );
    // Still correctly reports the endpoint as down after async refreshes.
    assert!(!cache.panel().enabled);

    println!(
        "issue1461 calyx probe: sync_probe={sync_probe_ms}ms, {FRAMES} async polls total={loop_total:?} max_single={max_single_poll:?} (blocking budget ~{blocking_budget:?})"
    );

    stop.store(true, Ordering::Relaxed);
    listener_handle.join().unwrap();
    std::fs::remove_dir_all(home).ok();
}
