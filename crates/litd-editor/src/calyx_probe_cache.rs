//! Non-blocking, throttled cache around the Calyx availability probe for editor UI panels
//! (#1461 item 1).
//!
//! [`build_calyx_feature_panel`] rebuilds the engine-Calyx surface manifest, reads
//! `CALYX_HOME`, and — when an endpoint is configured — does a BLOCKING
//! `TcpStream::connect_timeout` plus an initialize round-trip. The immediate-mode AI panels
//! called it every frame, so a configured-but-unreachable Calyx stalled the UI for up to
//! `timeout_ms` on EVERY frame — the exact opposite of the graceful degradation #701 promises.
//!
//! [`CalyxProbeCache`] holds the last-known panel and refreshes it on a background thread,
//! throttled to a TTL. The draw path calls [`CalyxProbeCache::poll`], which returns the cached
//! panel immediately (never touching the network) and, only when the panel is stale and no
//! probe is in flight, spawns one background probe. The UI thread therefore never blocks on
//! I/O; the panel updates a frame or two after each background probe finishes. User-initiated
//! actions that must verify live availability use [`CalyxProbeCache::refresh_blocking`] — a
//! deliberate synchronous probe off the hot path, where correctness outranks latency.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use litd_engine::EngineError;

use crate::offline_degradation::{
    CalyxAvailabilityProbe, CalyxFeaturePanel, build_calyx_feature_panel,
};

/// State shared with the background probe thread. The thread writes its result here; the UI
/// thread drains it on the next [`CalyxProbeCache::poll`].
#[derive(Debug)]
struct ProbeShared {
    /// Result of the most recently finished background probe, awaiting pickup by the UI.
    result: Option<Result<CalyxFeaturePanel, String>>,
    /// A background probe is currently running — do not start another.
    in_flight: bool,
    /// When the most recent probe was *started* (throttling anchor).
    last_started: Option<Instant>,
}

/// Non-blocking, TTL-throttled cache of a [`CalyxFeaturePanel`]. `Clone`/`Debug` so it can live
/// inside the (cloneable) editor runtime; cloning shares the same background probe state via the
/// `Arc`, which is harmless because only the live UI copy ever calls [`poll`](Self::poll).
#[derive(Debug, Clone)]
pub struct CalyxProbeCache {
    feature_id: String,
    label: String,
    panel: CalyxFeaturePanel,
    shared: Arc<Mutex<ProbeShared>>,
    ttl: Duration,
}

impl CalyxProbeCache {
    /// Build the cache with one SYNCHRONOUS initial probe so the first frame already shows a
    /// real panel. This single blocking probe happens once at panel construction (editor
    /// startup), never per frame.
    pub fn new(
        feature_id: impl Into<String>,
        label: impl Into<String>,
        probe: &CalyxAvailabilityProbe,
        ttl: Duration,
    ) -> Result<Self, EngineError> {
        let feature_id = feature_id.into();
        let label = label.into();
        let panel = build_calyx_feature_panel(&feature_id, &label, probe)?;
        Ok(Self {
            feature_id,
            label,
            panel,
            shared: Arc::new(Mutex::new(ProbeShared {
                result: None,
                in_flight: false,
                last_started: None,
            })),
            ttl,
        })
    }

    /// The current best-known panel. Never blocks, never touches the network.
    pub fn panel(&self) -> &CalyxFeaturePanel {
        &self.panel
    }

    /// Draw-path refresh: pick up any finished background probe (updating the panel) and, if the
    /// panel is stale and nothing is in flight, spawn a fresh background probe. Never blocks on
    /// I/O. Returns `Some(message)` if the most recent background probe FAILED (so the caller can
    /// surface it), otherwise `None`.
    pub fn poll(&mut self, probe: &CalyxAvailabilityProbe) -> Option<String> {
        let now = Instant::now();
        let mut error = None;
        let mut guard = self
            .shared
            .lock()
            .expect("calyx probe cache mutex poisoned");

        // 1. Absorb a completed background probe.
        if let Some(result) = guard.result.take() {
            match result {
                Ok(panel) => self.panel = panel,
                Err(message) => error = Some(message),
            }
        }

        // 2. Start a fresh probe only when stale AND idle. The `!in_flight` guard means a probe
        //    slower than the TTL never stacks duplicates.
        let stale = match guard.last_started {
            Some(started) => now.duration_since(started) >= self.ttl,
            None => true,
        };
        if stale && !guard.in_flight {
            guard.in_flight = true;
            guard.last_started = Some(now);
            let shared = Arc::clone(&self.shared);
            let feature_id = self.feature_id.clone();
            let label = self.label.clone();
            let probe = probe.clone();
            std::thread::spawn(move || {
                let outcome = build_calyx_feature_panel(&feature_id, &label, &probe)
                    .map_err(|e| e.to_string());
                let mut guard = shared.lock().expect("calyx probe cache mutex poisoned");
                guard.result = Some(outcome);
                guard.in_flight = false;
            });
        }
        error
    }

    /// Synchronous, authoritative refresh for a user-initiated action (e.g. clicking Generate):
    /// probe now, update the cached panel, and return the fresh result. Blocking is acceptable
    /// here — it is a deliberate action, not a draw frame — and correctness (never dispatching
    /// against a Calyx that just went down) outranks latency.
    pub fn refresh_blocking(&mut self, probe: &CalyxAvailabilityProbe) -> Result<(), EngineError> {
        self.panel = build_calyx_feature_panel(&self.feature_id, &self.label, probe)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "calyx_probe_cache_tests.rs"]
mod tests;
