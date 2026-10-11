//! Wall-clock self-abort guard for FSV example harnesses (issue #1712).
//!
//! Even with a bounded lock timeout on the Calyx vault path, an FSV example can
//! in principle block on some other unbounded operation (an ONNX/CUDA init, a
//! wedged daemon). The project forbids a silent multi-hour hang: "if something
//! doesn't work it should error out with robust logging". This guard is the
//! belt-and-suspenders that converts ANY hang into a loud, bounded failure.
//!
//! An example calls [`FsvDeadlineGuard::install`] at the top of `main`. A
//! background watchdog thread wakes after the deadline; if the guard has not
//! been dropped (i.e. the example is still running) it prints a structured
//! diagnostic naming the harness, the elapsed time, and the deadline, then
//! aborts the process with a non-zero exit code. On the happy path the guard is
//! dropped at the end of `main`, which cancels the watchdog before it fires.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Env override for the FSV wall-clock deadline, in whole seconds.
pub const FSV_DEADLINE_ENV: &str = "LITD_FSV_DEADLINE_SECS";
/// Default deadline. A healthy content FSV example (author -> persist -> lower
/// -> readback) completes in well under a minute even cold; five minutes leaves
/// generous headroom while still failing fast versus the 5-hour hang in #1712.
pub const DEFAULT_FSV_DEADLINE: Duration = Duration::from_secs(300);
/// How often the watchdog checks for cancellation while counting down.
const WATCHDOG_POLL: Duration = Duration::from_millis(100);
/// Exit code used when the guard fires. Matches the Rust default panic/abort
/// convention so callers see a clear non-zero status.
const DEADLINE_EXIT_CODE: i32 = 101;

/// Resolve the configured deadline, falling back to [`DEFAULT_FSV_DEADLINE`]
/// when the env var is unset. A set-but-malformed value is treated as unset and
/// the fallback is logged loudly to stderr rather than silently applied.
pub fn deadline_from_env() -> Duration {
    match std::env::var(FSV_DEADLINE_ENV) {
        Ok(raw) => match raw.trim().parse::<u64>() {
            Ok(0) => {
                eprintln!(
                    "FSV_DEADLINE_GUARD warning: {FSV_DEADLINE_ENV}=0 is invalid; using default \
                     {}s",
                    DEFAULT_FSV_DEADLINE.as_secs()
                );
                DEFAULT_FSV_DEADLINE
            }
            Ok(secs) => Duration::from_secs(secs),
            Err(error) => {
                eprintln!(
                    "FSV_DEADLINE_GUARD warning: {FSV_DEADLINE_ENV}={raw:?} is not a whole-second \
                     count ({error}); using default {}s",
                    DEFAULT_FSV_DEADLINE.as_secs()
                );
                DEFAULT_FSV_DEADLINE
            }
        },
        Err(_) => DEFAULT_FSV_DEADLINE,
    }
}

/// Build the structured diagnostic emitted when the deadline is exceeded.
pub fn deadline_diagnostic(label: &str, deadline: Duration) -> String {
    format!(
        "FSV_DEADLINE_EXCEEDED harness={label} deadline_secs={} elapsed_secs>={} \
         cause=blocking_operation_never_returned \
         remediation=source Calyx runtime env (scripts/Set-GameEditorCalyxRuntimeEnv.ps1), reduce \
         parallel agents on the shared vault, or raise {FSV_DEADLINE_ENV}",
        deadline.as_secs(),
        deadline.as_secs()
    )
}

/// RAII watchdog. Drop it (end of `main`) to cancel; leave it running past the
/// deadline to trip the abort.
pub struct FsvDeadlineGuard {
    cancel: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl FsvDeadlineGuard {
    /// Install a guard using the env-configured deadline and the real
    /// process-aborting action.
    pub fn install(label: impl Into<String>) -> Self {
        Self::with_action(label.into(), deadline_from_env(), abort_process)
    }

    /// Install a guard with an explicit deadline and action. The action is only
    /// invoked if the deadline elapses before the guard is dropped. Exposed for
    /// deterministic tests that assert firing without killing the test runner.
    pub fn with_action(label: String, deadline: Duration, action: fn(&str, Duration)) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let watch_cancel = Arc::clone(&cancel);
        let handle = thread::spawn(move || {
            let started = Instant::now();
            while started.elapsed() < deadline {
                if watch_cancel.load(Ordering::Acquire) {
                    return;
                }
                let remaining = deadline.saturating_sub(started.elapsed());
                thread::sleep(WATCHDOG_POLL.min(remaining.max(Duration::from_millis(1))));
            }
            if watch_cancel.load(Ordering::Acquire) {
                return;
            }
            action(&label, deadline);
        });
        Self {
            cancel,
            handle: Some(handle),
        }
    }
}

impl Drop for FsvDeadlineGuard {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Default fired-action: log the structured diagnostic loudly and abort.
fn abort_process(label: &str, deadline: Duration) {
    let diagnostic = deadline_diagnostic(label, deadline);
    eprintln!("{diagnostic}");
    tracing::error!(target: "fsv_deadline", "{diagnostic}");
    std::process::exit(DEADLINE_EXIT_CODE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

    #[test]
    fn guard_fires_when_deadline_elapses() {
        // Reality: a guard whose deadline passes while still "running" invokes
        // its action exactly once with the harness label and deadline. Proves
        // the watchdog trips instead of letting the harness hang forever.
        let (tx, rx) = mpsc::channel::<String>();
        // Static bridge so the fn-pointer action can reach the channel.
        static SENDER: std::sync::OnceLock<std::sync::Mutex<Option<mpsc::Sender<String>>>> =
            std::sync::OnceLock::new();
        *SENDER
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .unwrap() = Some(tx);
        fn fire(label: &str, deadline: Duration) {
            let msg = super::deadline_diagnostic(label, deadline);
            if let Some(sender) = SENDER.get().unwrap().lock().unwrap().as_ref() {
                sender.send(msg).unwrap();
            }
        }
        let started = Instant::now();
        let guard = FsvDeadlineGuard::with_action(
            "unit_fires".to_string(),
            Duration::from_millis(80),
            fire,
        );
        let fired = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("watchdog must fire within the window");
        let elapsed = started.elapsed();
        drop(guard);
        assert!(fired.contains("FSV_DEADLINE_EXCEEDED"));
        assert!(fired.contains("unit_fires"));
        assert!(elapsed >= Duration::from_millis(80));
        assert!(elapsed < Duration::from_secs(5), "fired fast: {elapsed:?}");
        println!(
            "FSV_DEADLINE_FIRE_FSV elapsed_ms={} msg={fired}",
            elapsed.as_millis()
        );
    }

    #[test]
    fn guard_does_not_fire_when_dropped_before_deadline() {
        // Reality: the happy path. Dropping the guard before the deadline must
        // cancel the watchdog so it never invokes the action.
        use std::sync::atomic::{AtomicU32, Ordering};
        static FIRE_COUNT: AtomicU32 = AtomicU32::new(0);
        fn fire(_label: &str, _deadline: Duration) {
            FIRE_COUNT.fetch_add(1, Ordering::SeqCst);
        }
        FIRE_COUNT.store(0, Ordering::SeqCst);
        {
            let _guard = FsvDeadlineGuard::with_action(
                "unit_cancel".to_string(),
                Duration::from_secs(30),
                fire,
            );
            // Drop immediately (end of scope).
        }
        // Give any errant watchdog time to (wrongly) fire.
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            FIRE_COUNT.load(Ordering::SeqCst),
            0,
            "must not fire after cancel"
        );
    }

    #[test]
    fn malformed_env_falls_back_to_default() {
        let name = FSV_DEADLINE_ENV;
        let previous = std::env::var_os(name);
        // SAFETY: unique test-scoped env manipulation, restored below.
        unsafe {
            std::env::set_var(name, "banana");
        }
        let resolved = deadline_from_env();
        unsafe {
            match &previous {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
        assert_eq!(resolved, DEFAULT_FSV_DEADLINE);
    }
}
