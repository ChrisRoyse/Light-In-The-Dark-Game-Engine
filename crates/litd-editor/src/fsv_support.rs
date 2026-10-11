use std::{env, fs, io, path::PathBuf, sync::OnceLock};

use litd_engine::fsv_artifacts::{FsvRun, begin_issue_run};

use crate::mcp_project_tools::mcp_project_tools_validation::PROJECT_ROOT_ENV;

/// Point [`PROJECT_ROOT_ENV`] at the workspace root for FSV example harnesses that drive
/// confined editor MCP tools (delivery/agents/opponents/bindings — #1487, #1518).
///
/// Those tools call `resolve_project_root()` and confine every tool-supplied path within
/// the trusted root before any filesystem effect; an example that never sets the root
/// fails loud at the first confined call. Example fixtures are all written under
/// `<workspace>/target/fsv/...` (via [`begin_issue_run`]), so the workspace root is the
/// correct confinement anchor. Idempotent — the one `set_var` runs once per process, so a
/// harness that calls several confined tools stays consistent.
pub fn ensure_project_root_env() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        // env!("CARGO_MANIFEST_DIR") is <workspace>/crates/litd-editor at build time;
        // ../.. is the workspace root. Matches ensure_project_root_env_for_tests and
        // fsv_artifacts::issue_root, so the anchor is identical across tests and examples.
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("litd-editor must live under <workspace>/crates/litd-editor")
            .to_path_buf();
        let canonical = fs::canonicalize(&workspace_root).unwrap_or(workspace_root);
        // SAFETY: run exactly once via OnceLock at the top of an example main(), before any
        // confined tool dispatch and before any thread that reads the variable is spawned.
        unsafe {
            env::set_var(PROJECT_ROOT_ENV, &canonical);
        }
    });
}

pub fn artifact_path_from_env(
    env_var: &str,
    issue_slug: &str,
    default_relative_path: &str,
) -> io::Result<(FsvRun, PathBuf)> {
    let run = begin_issue_run(issue_slug)?;
    let relative_path = env::var(env_var).unwrap_or_else(|_| default_relative_path.to_string());
    let path = run.artifact_path(&relative_path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok((run, path))
}

pub fn artifact_dir(issue_slug: &str, relative_path: &str) -> io::Result<(FsvRun, PathBuf)> {
    let run = begin_issue_run(issue_slug)?;
    let path = run.artifact_dir(relative_path)?;
    Ok((run, path))
}

pub fn artifact_dir_from_env(
    env_var: &str,
    issue_slug: &str,
    default_relative_path: &str,
) -> io::Result<(FsvRun, PathBuf)> {
    let run = begin_issue_run(issue_slug)?;
    let relative_path = env::var(env_var).unwrap_or_else(|_| default_relative_path.to_string());
    let path = run.artifact_dir(&relative_path)?;
    Ok((run, path))
}
