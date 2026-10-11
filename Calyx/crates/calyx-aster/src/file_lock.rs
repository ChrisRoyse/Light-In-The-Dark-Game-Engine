use calyx_core::{CalyxError, Result};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock, TryLockError as MutexTryLockError};
use std::time::{Duration, Instant};

/// Env override for the bounded lock-acquisition deadline, in whole seconds.
const FILE_LOCK_TIMEOUT_ENV: &str = "CALYX_FILE_LOCK_TIMEOUT_SECS";
/// Default deadline. Long enough for a legitimate short critical section
/// (WAL append / durable commit) to release, short enough that a wedged or
/// heavily-contended vault fails fast with `CALYX_LOCK_TIMEOUT` instead of
/// blocking forever. Before issue #1712 the acquire path called the blocking
/// `File::lock()`, so a vault held by another agent hung for hours.
const DEFAULT_FILE_LOCK_TIMEOUT: Duration = Duration::from_secs(30);
/// Poll cadence while waiting on a contended lock.
const FILE_LOCK_POLL_INTERVAL: Duration = Duration::from_millis(25);
/// Module-local structured code for a lock that could not be acquired inside
/// the deadline. Built directly (not a PRD 18 catalog entry) per the doctrine
/// in `calyx-core/src/error.rs`; mirrors hazard 14 (`CALYX_LOCK_TIMEOUT`).
const LOCK_TIMEOUT_CODE: &str = "CALYX_LOCK_TIMEOUT";

static PROCESS_LOCKS: OnceLock<Mutex<BTreeMap<PathBuf, &'static Mutex<()>>>> = OnceLock::new();

#[derive(Debug)]
/// Cross-process advisory lock guard with bounded acquisition.
///
/// Callers that need a read/validate/write critical section may use a lock
/// path beneath their durable vault. The guard combines an in-process mutex
/// with the operating-system file lock, so separate processes cannot observe
/// the same stale read concurrently.
pub struct FileLockGuard {
    _process_guard: MutexGuard<'static, ()>,
    _file: File,
}

impl FileLockGuard {
    pub fn acquire(path: &Path) -> Result<Self> {
        Self::acquire_with_timeout(path, file_lock_timeout()?)
    }

    fn acquire_with_timeout(path: &Path, timeout: Duration) -> Result<Self> {
        let key = lock_key(path)?;
        let process_guard = acquire_process_guard(&key, timeout)?;
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)
            .map_err(|error| CalyxError::disk_pressure(format!("open lock file: {error}")))?;
        acquire_file_lock(&file, path, timeout)?;
        Ok(Self {
            _process_guard: process_guard,
            _file: file,
        })
    }
}

/// Serialize same-process acquisitions, bounded so an in-process deadlock (one
/// thread holding the guard while another waits) fails loud instead of hanging.
fn acquire_process_guard(key: &Path, timeout: Duration) -> Result<MutexGuard<'static, ()>> {
    let mutex = process_mutex(key);
    let started = Instant::now();
    loop {
        match mutex.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(MutexTryLockError::WouldBlock) => {
                if started.elapsed() >= timeout {
                    return Err(lock_timeout(key, timeout, "in-process vault lock"));
                }
                std::thread::sleep(poll_interval(timeout));
            }
            Err(MutexTryLockError::Poisoned(_)) => {
                return Err(CalyxError::backpressure("file lock mutex poisoned"));
            }
        }
    }
}

/// Acquire the cross-process advisory file lock with a deadline. This is the
/// site that hung for hours in issue #1712: `File::lock()` blocks with no
/// bound, so N parallel agents on the one canonical vault deadlocked silently.
/// `File::try_lock()` lets us poll and fail loud once the deadline passes.
fn acquire_file_lock(file: &File, path: &Path, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(()),
            Err(TryLockError::WouldBlock) => {
                if started.elapsed() >= timeout {
                    return Err(lock_timeout(path, timeout, "cross-process vault file lock"));
                }
                std::thread::sleep(poll_interval(timeout));
            }
            Err(TryLockError::Error(error)) => {
                return Err(CalyxError::backpressure(format!("lock file: {error}")));
            }
        }
    }
}

fn poll_interval(timeout: Duration) -> Duration {
    // Never sleep longer than the whole budget (matters for a 0s test deadline).
    FILE_LOCK_POLL_INTERVAL.min(timeout.max(Duration::from_millis(1)))
}

fn lock_timeout(path: &Path, timeout: Duration, which: &str) -> CalyxError {
    CalyxError {
        code: LOCK_TIMEOUT_CODE,
        message: format!(
            "{which} {} not acquired within {}s; another process or agent holds it \
             (fail-fast, issue #1712)",
            path.display(),
            timeout.as_secs()
        ),
        remediation: "reduce concurrent writers to the same Calyx vault, or wait for the holder \
                      to release; raise CALYX_FILE_LOCK_TIMEOUT_SECS only for a legitimately \
                      longer critical section",
    }
}

fn file_lock_timeout() -> Result<Duration> {
    match std::env::var(FILE_LOCK_TIMEOUT_ENV) {
        Ok(raw) => {
            let trimmed = raw.trim();
            let secs = trimmed.parse::<u64>().map_err(|error| {
                CalyxError::backpressure(format!(
                    "{FILE_LOCK_TIMEOUT_ENV}={trimmed:?} is not a valid whole-second timeout: \
                     {error}"
                ))
            })?;
            Ok(Duration::from_secs(secs))
        }
        Err(std::env::VarError::NotPresent) => Ok(DEFAULT_FILE_LOCK_TIMEOUT),
        Err(std::env::VarError::NotUnicode(_)) => Err(CalyxError::backpressure(format!(
            "{FILE_LOCK_TIMEOUT_ENV} contains non-unicode data"
        ))),
    }
}

fn process_mutex(path: &Path) -> &'static Mutex<()> {
    let locks = PROCESS_LOCKS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut locks = locks.lock().expect("file lock registry poisoned");
    if let Some(lock) = locks.get(path) {
        return lock;
    }
    let lock = Box::leak(Box::new(Mutex::new(())));
    locks.insert(path.to_path_buf(), lock);
    lock
}

fn lock_key(path: &Path) -> Result<PathBuf> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| CalyxError::disk_pressure(format!("create lock dir: {error}")))?;
        let parent = parent.canonicalize().map_err(|error| {
            CalyxError::disk_pressure(format!("canonicalize lock dir: {error}"))
        })?;
        let name = path
            .file_name()
            .ok_or_else(|| CalyxError::disk_pressure("lock path has no file name"))?;
        return Ok(parent.join(name));
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

    fn test_dir(name: &str) -> PathBuf {
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("calyx-filelock-{name}-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create test dir");
        dir
    }

    #[test]
    fn issue1712_file_lock_fails_fast_when_held_by_another_handle() {
        // Reality: hold the OS advisory lock on the target path via an
        // independent handle (stands in for another process/agent), then prove
        // acquire() does NOT block for hours but returns CALYX_LOCK_TIMEOUT
        // inside the deadline. This exercises the exact cross-process file-lock
        // path that hung in #1712.
        let dir = test_dir("held");
        let lock_path = dir.join(".append.lock");
        let holder = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&lock_path)
            .expect("open holder handle");
        holder.try_lock().expect("holder takes the OS lock");

        let deadline = Duration::from_millis(200);
        let started = Instant::now();
        let error = FileLockGuard::acquire_with_timeout(&lock_path, deadline)
            .expect_err("contended acquire must fail, not hang");
        let elapsed = started.elapsed();

        assert_eq!(error.code, LOCK_TIMEOUT_CODE, "loud structured code");
        assert!(
            error.message.contains(&lock_path.display().to_string()),
            "error names the contended path: {}",
            error.message
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "must fail fast, took {elapsed:?}"
        );

        drop(holder); // release the OS lock
        let guard = FileLockGuard::acquire_with_timeout(&lock_path, deadline)
            .expect("uncontended acquire succeeds once holder releases");
        drop(guard);
        fs::remove_dir_all(&dir).expect("cleanup");

        println!(
            "ISSUE1712_FILE_LOCK_FSV code={} elapsed_ms={} deadline_ms={} path={}",
            error.code,
            elapsed.as_millis(),
            deadline.as_millis(),
            lock_path.display()
        );
    }

    #[test]
    fn issue1712_invalid_timeout_env_fails_loud() {
        // SAFETY: unique env manipulation guarded to this test; restored after.
        let name = FILE_LOCK_TIMEOUT_ENV;
        let previous = std::env::var_os(name);
        unsafe {
            std::env::set_var(name, "not-a-number");
        }
        let error = file_lock_timeout().expect_err("malformed timeout must error, not default");
        unsafe {
            match &previous {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
        assert_eq!(error.code, "CALYX_BACKPRESSURE");
        assert!(error.message.contains(FILE_LOCK_TIMEOUT_ENV));
    }

    #[test]
    fn uncontended_acquire_is_immediate() {
        let dir = test_dir("free");
        let lock_path = dir.join(".append.lock");
        let guard =
            FileLockGuard::acquire_with_timeout(&lock_path, DEFAULT_FILE_LOCK_TIMEOUT).unwrap();
        drop(guard);
        fs::remove_dir_all(&dir).expect("cleanup");
    }
}
