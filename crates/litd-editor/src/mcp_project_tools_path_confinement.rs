//! Filesystem-path confinement for the editor MCP tool surface (#1487/#1518).
//!
//! Extracted from `mcp_project_tools_validation.rs` to keep both files under the #800 §6
//! `<500`-line doctrine (#1466). This is the single confinement policy every MCP tool family
//! shares: resolve the trusted project root, confine each tool-supplied path within it before
//! any filesystem effect, and re-verify immediately before the store write to close the
//! symlink/junction TOCTOU. Re-exported through `mcp_project_tools_validation` so existing
//! `mcp_project_tools_validation::{confine_tool_paths, confine_path, …}` call sites are unchanged.

use std::path::{Component, Path, PathBuf};

use litd_engine::EngineError;

use super::super::invalid_mcp_project;

/// Environment variable naming the trusted project/workspace root that every
/// filesystem-path argument to editor MCP tools must resolve within (#1487).
///
/// The MCP surface is a stateless local-loopback deputy: `litd-editor-mcp-stdio`
/// reads line-delimited JSON-RPC and dispatches with no per-session state, and the
/// OS-process boundary is the intended trust boundary. The agent driving the editor
/// is the deputy — poisoned tool arguments (from a malicious catalog/package it was
/// asked to open) must NOT become an arbitrary-filesystem primitive. The confinement
/// anchor therefore cannot be derived from the (untrusted) request; it is supplied
/// out-of-band by whoever launches the editor / stdio MCP server, via this variable.
pub const PROJECT_ROOT_ENV: &str = "LITD_EDITOR_PROJECT_ROOT";

/// Resolve the canonicalized trusted project root from [`PROJECT_ROOT_ENV`].
///
/// Fails loud (no fallback) when the variable is unset or does not name an existing
/// directory. An unconfigured root must never silently degrade into "any path is
/// allowed" — that is exactly the confused-deputy hole #1487 closes (#800 §4).
pub fn resolve_project_root() -> Result<PathBuf, EngineError> {
    let Some(raw) = std::env::var_os(PROJECT_ROOT_ENV) else {
        return Err(invalid_mcp_project(
            PROJECT_ROOT_ENV,
            format!(
                "{PROJECT_ROOT_ENV} is not set; editor MCP filesystem tools require a configured project root to confine tool-supplied paths"
            ),
        ));
    };
    let raw = PathBuf::from(raw);
    let canonical = std::fs::canonicalize(&raw).map_err(|error| {
        invalid_mcp_project(
            PROJECT_ROOT_ENV,
            format!(
                "canonicalize project root {} ({PROJECT_ROOT_ENV}): {error}",
                raw.display()
            ),
        )
    })?;
    if !canonical.is_dir() {
        return Err(invalid_mcp_project(
            PROJECT_ROOT_ENV,
            format!(
                "project root {} ({PROJECT_ROOT_ENV}) is not a directory",
                canonical.display()
            ),
        ));
    }
    Ok(canonical)
}

/// Confine a tool-supplied path within `root`, returning the resolved absolute path.
///
/// Standard directory-traversal defense (verified best practice, #1487):
///  1. Reject any parent-directory (`..`) component outright — legitimate editor
///     paths never need `..`, and removing it eliminates every traversal escape
///     before resolution.
///  2. Resolve a relative candidate against the process working directory, matching
///     how the filesystem sinks (`fs::read`/`fs::write`) interpret it.
///  3. Canonicalize the deepest EXISTING ancestor (the target file/dir may be created
///     later) and prove it stays within the canonicalized `root`. Because no `..`
///     remains, the not-yet-existing suffix cannot escape either.
///
/// `root` must already be canonical (see [`resolve_project_root`]). Note: a symlink
/// created between this check and a later write is a classic TOCTOU that this string
/// gate does not close; hardening the sinks against reparse points is tracked
/// separately.
pub fn confine_path(field: &str, candidate: &Path, root: &Path) -> Result<PathBuf, EngineError> {
    for component in candidate.components() {
        // Reject a real ParentDir component, and also a literal ".." NORMAL component:
        // on Windows a verbatim (\\?\) prefix disables path normalization, so `..`
        // surfaces as Component::Normal("..") rather than Component::ParentDir. ".." is
        // never a legitimate filename, so rejecting both is safe and closes the gap.
        let is_parent = matches!(component, Component::ParentDir)
            || matches!(component, Component::Normal(part) if part == "..");
        if is_parent {
            return Err(invalid_mcp_project(
                field,
                format!(
                    "path {} contains a parent-directory (..) component; traversal outside the project root is not allowed",
                    candidate.display()
                ),
            ));
        }
    }
    let absolute = absolutize(field, candidate)?;
    let existing = deepest_existing_ancestor(&absolute);
    let canonical_existing = std::fs::canonicalize(&existing).map_err(|error| {
        invalid_mcp_project(
            field,
            format!("canonicalize {}: {error}", existing.display()),
        )
    })?;
    if !canonical_existing.starts_with(root) {
        return Err(invalid_mcp_project(
            field,
            format!(
                "path {} resolves to {} which is outside the project root {}",
                candidate.display(),
                canonical_existing.display(),
                root.display()
            ),
        ));
    }
    let suffix = absolute.strip_prefix(&existing).unwrap_or(Path::new(""));
    Ok(canonical_existing.join(suffix))
}

/// Confine a batch of tool-supplied paths within the configured project root before any
/// filesystem effect (#1487/#1518).
///
/// Resolves [`resolve_project_root`] once and rejects the whole tool call if ANY path
/// escapes. Every editor MCP tool family that persists arg-supplied paths calls this at
/// its mutation-path validation choke point, so the entire surface shares one confinement
/// policy instead of each family re-implementing it (and some silently forgetting — the
/// gap #1518 closes). A family with no path arguments never calls it and so never forces
/// the root to be configured.
pub fn confine_tool_paths(fields: &[(&str, &Path)]) -> Result<(), EngineError> {
    let root = resolve_project_root()?;
    for (field, path) in fields {
        confine_path(field, path, &root)?;
    }
    Ok(())
}

/// Resolve `candidate` to an absolute path the way the filesystem sinks interpret it:
/// a relative candidate is joined onto the process working directory. Shared by
/// [`confine_path`] and [`reverify_confined_before_write`] so both observe the same
/// absolute form.
fn absolutize(field: &str, candidate: &Path) -> Result<PathBuf, EngineError> {
    if candidate.is_absolute() {
        Ok(candidate.to_path_buf())
    } else {
        let cwd = std::env::current_dir().map_err(|error| {
            invalid_mcp_project(field, format!("resolve working directory: {error}"))
        })?;
        Ok(cwd.join(candidate))
    }
}

/// True when `path` (which must exist) is a reparse point: a POSIX symlink, or a Windows
/// symlink OR junction (mount point). `symlink_metadata` inspects the entry itself with
/// no target follow.
///
/// Rust's `FileType::is_symlink()` reports POSIX symlinks and Windows symlinks but NOT
/// Windows junctions (`IO_REPARSE_TAG_MOUNT_POINT`) — the more common (non-privileged)
/// escape primitive — so on Windows we test `FILE_ATTRIBUTE_REPARSE_POINT` (0x400)
/// directly, which covers symlinks and junctions alike.
fn is_reparse_point(path: &Path) -> Result<bool, EngineError> {
    let meta = std::fs::symlink_metadata(path).map_err(|error| {
        invalid_mcp_project(
            "reparse_check",
            format!("inspect {} for reparse point: {error}", path.display()),
        )
    })?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        Ok(meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
    }
    #[cfg(not(windows))]
    {
        Ok(meta.file_type().is_symlink())
    }
}

/// Re-verify path confinement IMMEDIATELY before a store filesystem effect (#1518,
/// part 3 bullet 3 — symlink/reparse-point TOCTOU).
///
/// The handler-layer [`confine_tool_paths`] runs once, early in the tool call, then the
/// handler does more work (identity/metadata validation, record building, state load)
/// before the store finally opens the vault. A symlink or junction planted in that gap
/// can still redirect the write outside the project root — the classic time-of-check /
/// time-of-use race called out in #1487's `confine_path` doc. Each store write function
/// calls this right before its `AsterVault::open` / archive write, so the LAST
/// observation of the path is the one the write acts on: the window shrinks to the few
/// lines between here and the open, and
///  - a reparse point swapped onto the target's existing parent is rejected outright, and
///  - `confine_path` re-canonicalizes, so a reparse point pointing OUT of the root is
///    caught even if it is planted between the two checks.
///
/// Active only when a trusted root is configured (`PROJECT_ROOT_ENV`). Every editor MCP
/// dispatch sets it, so confined tools always re-verify; a non-MCP embedder that never
/// configures a root keeps the store's prior contract. This is not a silent downgrade of
/// the mandatory handler gate (which still errors loudly when a confined tool runs with
/// no root) — it is a second, tighter gate that only engages once confinement is in use.
pub fn reverify_confined_before_write(field: &str, path: &Path) -> Result<(), EngineError> {
    if std::env::var_os(PROJECT_ROOT_ENV).is_none() {
        return Ok(());
    }
    let root = resolve_project_root()?;
    confine_path(field, path, &root)?;
    // Reject a reparse point at the exact directory the write attaches to — the deepest
    // real component before the not-yet-existing suffix (e.g. the vault dir's parent).
    // confine_path's canonicalize already rejects one pointing OUT of root; this also
    // rejects an in-root symlink/junction so confined tool writes never traverse a
    // reparse point at all.
    let existing = deepest_existing_ancestor(&absolutize(field, path)?);
    if is_reparse_point(&existing)? {
        return Err(invalid_mcp_project(
            field,
            format!(
                "path {} attaches to {}, which is a symlink/reparse point; confined \
                 editor tools reject reparse points to close symlink/junction TOCTOU",
                path.display(),
                existing.display()
            ),
        ));
    }
    Ok(())
}

/// Walk up from `path` (always absolute here) until an existing ancestor is found.
/// On Windows the drive root always exists, so this always terminates with a real
/// directory to canonicalize.
fn deepest_existing_ancestor(path: &Path) -> PathBuf {
    let mut current = path;
    loop {
        if current.exists() {
            return current.to_path_buf();
        }
        match current.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => current = parent,
            _ => return current.to_path_buf(),
        }
    }
}

/// Set [`PROJECT_ROOT_ENV`] to the canonical workspace root exactly once for the test
/// process. Delivery fixtures write under the cwd-relative `target/` and the social
/// fixtures write under the workspace `target/fsv/` (via `fsv_artifacts::issue_root`);
/// both live under the workspace root, so a single broad root confines every fixture
/// while the rejection logic is proven separately by pure unit tests over
/// [`confine_path`]. `get_or_init` serializes the one `set_var`, so delivery tests
/// never race to write different values.
#[cfg(test)]
pub(crate) fn ensure_project_root_env_for_tests() {
    use std::sync::OnceLock;
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        // Workspace root = <crate>/../.. (crates/litd-editor -> workspace). Matches the
        // crate's other workspace_root() helpers and fsv_artifacts::issue_root.
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("litd-editor must live under the workspace crates directory")
            .to_path_buf();
        let canonical = std::fs::canonicalize(&workspace_root).unwrap_or(workspace_root);
        // SAFETY: run exactly once via OnceLock before any delivery tool dispatch;
        // consistent with the crate's existing env-var test convention (EnvGuard).
        unsafe {
            std::env::set_var(PROJECT_ROOT_ENV, &canonical);
        }
    });
}

#[cfg(test)]
mod confinement_tests {
    use super::*;
    use std::fs;

    // Confinement roots must be canonical (resolve_project_root canonicalizes), so canonicalize
    // the created directory before use. #1535: returns a TestTempDir guard (auto-cleans on Drop);
    // callers MUST bind it (`let (_g, root) = temp_root(..)`) so the dir survives the test.
    fn temp_root(label: &str) -> (litd_engine::test_temp::TestTempDir, PathBuf) {
        let guard = litd_engine::test_temp::TestTempDir::new(&format!("confine-{label}"));
        fs::create_dir_all(guard.path()).unwrap();
        let canonical = fs::canonicalize(guard.path()).unwrap();
        (guard, canonical)
    }

    #[test]
    fn issue1487_in_root_existing_path_accepted() {
        let (_temp_root, root) = temp_root("in-root-existing");
        let target = root.join("project-state.json");
        fs::write(&target, b"{}").unwrap();
        let confined = confine_path("state_path", &target, &root).unwrap();
        assert!(
            confined.starts_with(&root),
            "confined {} must stay within root {}",
            confined.display(),
            root.display()
        );
    }

    #[test]
    fn issue1487_in_root_not_yet_existing_descendant_accepted() {
        let (_temp_root, root) = temp_root("in-root-new");
        // install_root_path/publish_id/pkg.json — the leaf and its parents do not
        // exist yet; confinement must still accept it because no `..` escapes root.
        let target = root
            .join("installed")
            .join("pub-1")
            .join("game.litdgame.json");
        let confined = confine_path("install_root_path", &target, &root).unwrap();
        assert!(confined.starts_with(&root));
    }

    #[test]
    fn issue1487_parent_dir_traversal_rejected() {
        let (_temp_root, root) = temp_root("traversal");
        // A tool argument carrying `..` as a plain (non-verbatim) string — the shape a
        // poisoned catalog/agent actually sends. The `..` component must be rejected
        // before any resolution.
        let target = Path::new("installed/../../escape.json");
        let error = confine_path("install_root_path", target, &root)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("parent-directory") && error.contains(".."),
            "expected traversal rejection, got: {error}"
        );
        // Prove no side effect: nothing was created at any escape target.
        assert!(!root.join("escape.json").exists());
    }

    #[test]
    fn issue1487_absolute_path_outside_root_rejected() {
        let (_temp_root, root) = temp_root("abs-escape");
        let (_temp_sibling, sibling) = temp_root("abs-escape-sibling");
        let target = sibling.join("startup.litdgame.json");
        let error = confine_path("install_root_path", &target, &root)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("outside the project root"),
            "expected outside-root rejection, got: {error}"
        );
    }
}
