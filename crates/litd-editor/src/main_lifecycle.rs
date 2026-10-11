use std::{
    env,
    fs::OpenOptions,
    io::Write,
    panic::{self, PanicHookInfo},
    path::{Path, PathBuf},
};

use tracing::{error, info};

use crate::main_build_identity::BuildIdentity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorExitReason {
    Running,
    CloseRequested,
    FsvTargetReached,
    WindowCreateFailed,
    RendererInitFailed,
    ShellEvidenceFailed,
    RuntimeStartFailed,
    RuntimeFrameFailed,
    RenderFrameFailed,
    LoopExiting,
    EventLoopError,
    Panic,
}

impl EditorExitReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::CloseRequested => "close_requested",
            Self::FsvTargetReached => "fsv_target_reached",
            Self::WindowCreateFailed => "window_create_failed",
            Self::RendererInitFailed => "renderer_init_failed",
            Self::ShellEvidenceFailed => "shell_evidence_failed",
            Self::RuntimeStartFailed => "runtime_start_failed",
            Self::RuntimeFrameFailed => "runtime_frame_failed",
            Self::RenderFrameFailed => "render_frame_failed",
            Self::LoopExiting => "loop_exiting",
            Self::EventLoopError => "event_loop_error",
            Self::Panic => "panic",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EditorLifecycleEvidence {
    pub state: &'static str,
    pub last_exit_reason: &'static str,
    pub close_requested_count: u64,
    pub loop_exiting_seen: bool,
    pub shutdown_started_count: u64,
    pub shutdown_finished_count: u64,
    pub process_return_seen: bool,
    pub process_exit_code: &'static str,
}

#[derive(Debug, Clone)]
pub struct EditorLifecycleState {
    state: &'static str,
    last_exit_reason: EditorExitReason,
    close_requested_count: u64,
    loop_exiting_seen: bool,
    shutdown_started_count: u64,
    shutdown_finished_count: u64,
    process_return_seen: bool,
    process_exit_code: &'static str,
}

impl Default for EditorLifecycleState {
    fn default() -> Self {
        Self {
            state: "running",
            last_exit_reason: EditorExitReason::Running,
            close_requested_count: 0,
            loop_exiting_seen: false,
            shutdown_started_count: 0,
            shutdown_finished_count: 0,
            process_return_seen: false,
            process_exit_code: "running",
        }
    }
}

impl EditorLifecycleState {
    pub fn record_exit_request(&mut self, reason: EditorExitReason) {
        if reason == EditorExitReason::CloseRequested {
            self.close_requested_count += 1;
        }
        self.state = "exit_requested";
        if self.last_exit_reason == EditorExitReason::Running {
            self.last_exit_reason = reason;
        }
        info!(
            exit_reason = reason.as_str(),
            close_requested_count = self.close_requested_count,
            "editor lifecycle exit requested"
        );
    }

    pub fn record_loop_exiting(&mut self) {
        if self.last_exit_reason == EditorExitReason::Running {
            self.last_exit_reason = EditorExitReason::LoopExiting;
        }
        self.loop_exiting_seen = true;
        self.state = "loop_exiting";
        info!(
            last_exit_reason = self.last_exit_reason.as_str(),
            close_requested_count = self.close_requested_count,
            "editor event loop exiting"
        );
    }

    pub fn record_shutdown_started(&mut self, reason: EditorExitReason) {
        self.shutdown_started_count += 1;
        info!(
            exit_reason = reason.as_str(),
            shutdown_started_count = self.shutdown_started_count,
            "editor runtime shutdown started"
        );
    }

    pub fn record_shutdown_finished(&mut self, clean_shutdown: bool) {
        self.shutdown_finished_count += 1;
        info!(
            clean_shutdown,
            shutdown_finished_count = self.shutdown_finished_count,
            "editor runtime shutdown finished"
        );
    }

    pub fn record_process_return(&mut self, exit_code: &'static str) {
        self.process_return_seen = true;
        self.process_exit_code = exit_code;
        self.state = "process_returned";
        info!(
            exit_code,
            last_exit_reason = self.last_exit_reason.as_str(),
            loop_exiting_seen = self.loop_exiting_seen,
            "editor process returning"
        );
    }

    pub fn record_event_loop_error(&mut self) {
        self.record_exit_request(EditorExitReason::EventLoopError);
    }

    pub fn evidence(&self) -> EditorLifecycleEvidence {
        EditorLifecycleEvidence {
            state: self.state,
            last_exit_reason: self.last_exit_reason.as_str(),
            close_requested_count: self.close_requested_count,
            loop_exiting_seen: self.loop_exiting_seen,
            shutdown_started_count: self.shutdown_started_count,
            shutdown_finished_count: self.shutdown_finished_count,
            process_return_seen: self.process_return_seen,
            process_exit_code: self.process_exit_code,
        }
    }

    pub fn exit_requested(&self) -> bool {
        self.last_exit_reason != EditorExitReason::Running || self.state != "running"
    }

    pub fn exit_reason(&self) -> EditorExitReason {
        self.last_exit_reason
    }
}

pub fn install_panic_hook(log_path: &Path, window_title: &'static str, build: BuildIdentity) {
    let log_path = log_path.to_path_buf();
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let payload = panic_payload(info);
        let location = panic_location(info);
        error!(
            exit_reason = EditorExitReason::Panic.as_str(),
            panic_payload = %payload,
            panic_location = %location,
            "editor panic hook captured panic"
        );
        append_panic_line(&log_path, &payload, &location);
        write_panic_fsv_stats(window_title, &build, &payload, &location);
        default_hook(info);
    }));
}

pub fn run_panic_probe_from_env() {
    if env::var("LITD_EDITOR_PANIC_PROBE").ok().as_deref() == Some("1") {
        panic!("litd-editor-panic-probe");
    }
}

fn append_panic_line(log_path: &PathBuf, payload: &str, location: &str) {
    // Best-effort by nature (we are inside a panic hook and must never panic), but a
    // silent failure hides the panic evidence the FSV harness depends on. Surface any
    // failure on stderr so the record is not lost without a trace (#1490).
    match OpenOptions::new().create(true).append(true).open(log_path) {
        Ok(mut file) => {
            if let Err(error) = writeln!(
                file,
                "editor_panic_hook=true panic_payload={payload:?} panic_location={location:?}"
            ) {
                eprintln!(
                    "litd-editor panic hook: failed to append panic line to {}: {error}",
                    log_path.display()
                );
            }
        }
        Err(error) => {
            eprintln!(
                "litd-editor panic hook: failed to open panic log {}: {error}",
                log_path.display()
            );
        }
    }
}

fn write_panic_fsv_stats(window_title: &str, build: &BuildIdentity, payload: &str, location: &str) {
    let Ok(path) = env::var("LITD_FSV_STATS") else {
        return;
    };
    let path = PathBuf::from(path);
    if path.as_os_str().is_empty() {
        return;
    }
    if let Some(parent) = path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        eprintln!(
            "litd-editor panic hook: failed to create LITD_FSV_STATS parent {}: {error}",
            parent.display()
        );
    }
    let contents = format!(
        "window_title={window_title}\n\
             fsv_stats_kind=editor_panic_without_frame_snapshot\n\
             fsv_snapshot_available=false\n\
             fsv_snapshot_missing_reason=panic_before_rendered_frame_snapshot\n\
             package_version={}\n\
             build_git_commit={}\n\
             build_git_dirty={}\n\
             build_profile={}\n\
             build_target={}\n\
             build_timestamp_unix_seconds={}\n\
             executable_path={}\n\
             executable_sha256={}\n\
             repo_root={}\n\
             repo_head_commit={}\n\
             repo_head_matches_build={}\n\
             editor_lifecycle_state=panic\n\
             editor_last_exit_reason=panic\n\
             editor_process_return_seen=false\n\
             editor_process_exit_code=panic_unwinding\n\
             panic_payload={payload:?}\n\
             panic_location={location:?}\n",
        build.package_version,
        build.build_git_commit,
        build.build_git_dirty,
        build.build_profile,
        build.build_target,
        build.build_timestamp_unix_seconds,
        build.executable_path.display(),
        build.executable_sha256,
        build
            .repo_root
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "not_found".to_string()),
        build.repo_head_commit.as_deref().unwrap_or("not_found"),
        build
            .repo_head_matches_build
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
    );
    if let Err(error) = std::fs::write(&path, &contents) {
        // The FSV harness reads this file to distinguish "panicked before a frame
        // snapshot" from other exits. If the write fails, do not lose the record:
        // echo the reason and the full stats payload to stderr (#1490).
        eprintln!(
            "litd-editor panic hook: failed to write LITD_FSV_STATS {}: {error}\n{contents}",
            path.display()
        );
    }
}

fn panic_payload(info: &PanicHookInfo<'_>) -> String {
    info.payload()
        .downcast_ref::<&str>()
        .map(|value| (*value).to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".to_string())
}

fn panic_location(info: &PanicHookInfo<'_>) -> String {
    info.location()
        .map(|location| format!("{}:{}", location.file(), location.line()))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::main_test_support::{editor_exe_for_tests, repo_root_for_tests};
    use std::sync::Mutex;

    // LITD_FSV_STATS is process-global; serialize the tests that mutate it so the
    // parallel test runner cannot cross their env-var lifetimes.
    static FSV_STATS_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn close_requested_records_source_of_truth_fields() {
        let mut lifecycle = EditorLifecycleState::default();
        lifecycle.record_exit_request(EditorExitReason::CloseRequested);
        lifecycle.record_loop_exiting();
        lifecycle.record_shutdown_started(EditorExitReason::CloseRequested);
        lifecycle.record_shutdown_finished(true);
        lifecycle.record_process_return("success");

        let evidence = lifecycle.evidence();
        assert_eq!(evidence.state, "process_returned");
        assert_eq!(evidence.last_exit_reason, "close_requested");
        assert_eq!(evidence.close_requested_count, 1);
        assert!(evidence.loop_exiting_seen);
        assert_eq!(evidence.shutdown_started_count, 1);
        assert_eq!(evidence.shutdown_finished_count, 1);
        assert!(evidence.process_return_seen);
        assert_eq!(evidence.process_exit_code, "success");
    }

    #[test]
    fn process_return_does_not_overwrite_causal_exit_reason() {
        let mut lifecycle = EditorLifecycleState::default();
        lifecycle.record_exit_request(EditorExitReason::FsvTargetReached);
        lifecycle.record_process_return("success");

        let evidence = lifecycle.evidence();
        assert_eq!(evidence.last_exit_reason, "fsv_target_reached");
        assert_eq!(evidence.process_exit_code, "success");
    }

    #[test]
    fn exit_requested_tracks_non_running_lifecycle_state() {
        let mut lifecycle = EditorLifecycleState::default();
        assert!(!lifecycle.exit_requested());

        lifecycle.record_exit_request(EditorExitReason::CloseRequested);
        assert!(lifecycle.exit_requested());
    }

    #[test]
    fn panic_fsv_stats_write_source_of_truth_without_frame_snapshot() {
        let _guard = FSV_STATS_ENV_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // #1535: TestTempDir auto-cleans on Drop.
        let _temp_root = litd_engine::test_temp::TestTempDir::new("editor-panic-fsv");
        let dir = _temp_root.path().to_path_buf();
        let path = dir.join("stats.txt");
        let _ = std::fs::remove_file(&path);
        unsafe {
            std::env::set_var("LITD_FSV_STATS", &path);
        }
        let build = BuildIdentity {
            package_version: "0.0.1",
            build_git_commit: "0123456789abcdef0123456789abcdef01234567",
            build_git_dirty: true,
            build_profile: "debug",
            build_target: "x86_64-pc-windows-msvc",
            build_timestamp_unix_seconds: "1782720000",
            executable_path: editor_exe_for_tests(&repo_root_for_tests()),
            executable_sha256: "abc123".to_string(),
            repo_root: Some(repo_root_for_tests()),
            repo_head_commit: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
            repo_head_matches_build: Some(true),
        };

        write_panic_fsv_stats(
            "Light in the Dark Editor - world view",
            &build,
            "litd-editor-panic-probe",
            "probe.rs:1",
        );
        unsafe {
            std::env::remove_var("LITD_FSV_STATS");
        }

        let stats = std::fs::read_to_string(&path).unwrap();
        assert!(stats.contains("fsv_stats_kind=editor_panic_without_frame_snapshot"));
        assert!(stats.contains("fsv_snapshot_available=false"));
        assert!(stats.contains("editor_last_exit_reason=panic"));
        assert!(stats.contains("panic_payload=\"litd-editor-panic-probe\""));

        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir(dir);
    }

    #[test]
    fn panic_fsv_stats_write_failure_surfaces_on_stderr_and_creates_no_file() {
        let _guard = FSV_STATS_ENV_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // Point LITD_FSV_STATS at a path whose parent is a regular FILE, so both the
        // directory creation and the write fail with a real IO error -- the exact case
        // that used to be swallowed by `let _ =`. Run this test with `-- --nocapture`
        // to see the fallback line and the full record echoed on stderr.
        // #1535: TestTempDir auto-cleans on Drop.
        let _temp_root = litd_engine::test_temp::TestTempDir::new("editor-panic-fsv-fail");
        let dir = _temp_root.path().to_path_buf();
        std::fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, b"not-a-directory").unwrap();
        let path = blocker.join("stats.txt");
        assert!(
            blocker.is_file(),
            "parent must be a file so the write cannot succeed"
        );

        unsafe {
            std::env::set_var("LITD_FSV_STATS", &path);
        }
        let build = BuildIdentity {
            package_version: "0.0.1",
            build_git_commit: "0123456789abcdef0123456789abcdef01234567",
            build_git_dirty: true,
            build_profile: "debug",
            build_target: "x86_64-pc-windows-msvc",
            build_timestamp_unix_seconds: "1782720000",
            executable_path: editor_exe_for_tests(&repo_root_for_tests()),
            executable_sha256: "abc123".to_string(),
            repo_root: Some(repo_root_for_tests()),
            repo_head_commit: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
            repo_head_matches_build: Some(true),
        };

        // Must not panic (we are simulating being inside a panic hook).
        write_panic_fsv_stats(
            "Light in the Dark Editor - world view",
            &build,
            "litd-editor-panic-probe",
            "probe.rs:1",
        );
        unsafe {
            std::env::remove_var("LITD_FSV_STATS");
        }

        // Source of truth: no stats file exists, so a downstream reader correctly sees
        // "no record" rather than a partial/corrupt one -- and the failure was surfaced
        // on stderr (visible with --nocapture) instead of being lost silently.
        assert!(
            !path.exists(),
            "no stats file must be created when the write fails: {}",
            path.display()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
