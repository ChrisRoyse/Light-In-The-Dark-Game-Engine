//! RAII scratch directory for tests (#1495).
//!
//! Test suites created a uniquely-named `%TEMP%/litd-*` directory per run but never
//! removed it, leaking thousands of directories (~2 GB) over time. [`TestTempDir`] is a
//! guard whose `Drop` removes the directory and its contents, so cleanup happens at the
//! end of the test scope — including during panic unwinding, where destructors still run.
//!
//! The one case a destructor cannot cover is a hard process kill (SIGKILL /
//! `Stop-Process -Force`, `std::process::exit`, or an `abort` panic), which bypasses
//! `Drop` entirely; the `scripts/prune-build-artifacts.ps1` backstop sweep of aged
//! `litd-*` dirs handles that.
//!
//! The path embeds pid + nanoseconds so it never collides across concurrent runs, and
//! the directory is NOT created eagerly — matching the historical `unique_vault_dir`
//! contract where the caller (or an Aster vault open) creates it. `Drop` tolerates a
//! never-created dir (`NotFound` is ignored) and surfaces any real removal failure on
//! stderr rather than swallowing it.

use std::{
    fs, io,
    ops::Deref,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

/// A uniquely-named scratch directory that removes itself on `Drop`.
#[derive(Debug)]
pub struct TestTempDir {
    path: PathBuf,
}

impl TestTempDir {
    /// Reserve a fresh, unique scratch path under the system temp dir. `label`
    /// identifies the owning test for debuggability and backstop matching. The path is
    /// `litd-<label>-<pid>-<nanos>`; the directory is created lazily by the caller.
    pub fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("litd-{label}-{}-{nanos}", process::id()));
        Self { path }
    }

    /// Borrow the scratch path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Consume the guard WITHOUT deleting the directory (e.g. to inspect it after a
    /// failure). Returns the owned path; the dir then leaks by design, so use sparingly.
    pub fn leak(self) -> PathBuf {
        let path = self.path.clone();
        std::mem::forget(self);
        path
    }
}

impl Deref for TestTempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TestTempDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl From<&TestTempDir> for PathBuf {
    fn from(dir: &TestTempDir) -> Self {
        dir.path.clone()
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        match fs::remove_dir_all(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                eprintln!(
                    "TestTempDir: failed to remove scratch dir {}: {error}",
                    self.path.display()
                );
            }
        }
    }
}
