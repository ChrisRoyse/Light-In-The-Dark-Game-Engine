//! Shared test scratch-root helper (#1520).
//!
//! Routes every litd-editor test fixture root through the canonical FSV run tree
//! (`<workspace>/target/fsv/<slug>/runs/<run-id>/<label>`) via
//! [`litd_engine::fsv_artifacts::begin_issue_run`] — never package-relative
//! `target/test-data` / `target/test-state`. Building scratch by hand from
//! `CARGO_MANIFEST_DIR`/cwd escaped the one convention the FSV artifact tooling and
//! target cleanup recognize, and those directories accumulated across runs
//! (#1507, #1495). Use this helper instead of hand-building paths so the whole
//! editor test suite stays inside the canonical tree; every call yields a fresh,
//! isolated run directory.

use std::path::PathBuf;

use litd_engine::fsv_artifacts::begin_issue_run;

/// A fresh, unique scratch directory for one test under the canonical FSV run tree.
///
/// `issue_slug` names the owning issue (e.g. `"issue1102_asset_lod"`); `label`
/// distinguishes the specific test within it. Fails loud if the run or artifact
/// directory cannot be created — a broken scratch tree must not be silently
/// swallowed.
pub(crate) fn issue_scratch_dir(issue_slug: &str, label: &str) -> PathBuf {
    begin_issue_run(issue_slug)
        .and_then(|run| run.artifact_dir(label))
        .unwrap_or_else(|error| {
            panic!("create canonical FSV scratch dir for {issue_slug}/{label}: {error}")
        })
}
