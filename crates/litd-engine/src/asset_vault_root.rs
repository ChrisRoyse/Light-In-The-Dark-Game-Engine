//! #1543: the ONE authoritative resolver for the out-of-git-tree `local-asset-vault`.
//!
//! `local-asset-vault/` is a large, gitignored directory that lives BESIDE the primary
//! checkout (`<primary>/../local-asset-vault` is wrong; it is `<primary>/local-asset-vault`,
//! i.e. inside the primary working tree but not tracked by git). Because it is untracked,
//! a linked git worktree does NOT get a copy of it.
//!
//! The bug this module kills: every call site used to build the path from
//! `env!("CARGO_MANIFEST_DIR")`, which is a COMPILE-TIME literal. In a linked worktree that
//! literal is the worktree's own path, so the vault resolved to a directory that does not
//! exist and 10 tests died with `os error 3`. Worse, because worktrees share one
//! `CARGO_TARGET_DIR`, cargo's fingerprint deliberately ignores absolute paths, so a cached
//! artifact compiled in worktree A can be reused in worktree B with A's path baked in.
//! Anything resolved at compile time (`env!`, `build.rs` + `cargo::rustc-env`) is therefore
//! unsound here. Resolution MUST happen at RUNTIME.
//!
//! Resolution order (first hit wins, and every step is runtime state):
//!   1. `LITD_LOCAL_ASSET_VAULT` — explicit override. If set, it is authoritative: if it does
//!      not name a real directory we FAIL, we do not fall through to a guess.
//!   2. The PRIMARY checkout, via `git rev-parse --path-format=absolute --git-common-dir`.
//!      In both the main checkout and a linked worktree this yields `<primary>/.git`, whose
//!      parent is the primary working-tree root. This is the step that fixes worktrees.
//!   3. The workspace root derived from the RUNTIME `CARGO_MANIFEST_DIR` (cargo sets it for
//!      the test process), for the no-git / exported-source case.
//!   4. An ancestor walk from the current directory, for the no-git no-cargo case.
//!
//! If none resolve, we emit ONE structured `GATE_FAILED` line naming every path searched and
//! the exact fix — never a scatter of confusing `os error 3` import failures.

use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

/// Explicit override. Set this to an absolute path to the `local-asset-vault` directory.
pub const LOCAL_ASSET_VAULT_ENV: &str = "LITD_LOCAL_ASSET_VAULT";
/// The vault directory name as it sits beside the workspace manifest in the primary checkout.
pub const LOCAL_ASSET_VAULT_DIR: &str = "local-asset-vault";
/// Gate step name carried in the structured failure line.
pub const ASSET_VAULT_RESOLVE_STEP: &str = "asset-vault-resolve";

/// #1569: the top-level roots a *materialized* `local-asset-vault` is expected to contain. The
/// vault is a large gitignored directory populated out-of-band (asset-gen pipeline #782); a real
/// one holds at least one of these. If the vault directory exists but has NONE of them, it is
/// present-but-empty — the #1569 data-loss shape (a `robocopy /MIR` through a worktree junction
/// purged it to 0 files) — which is a categorically different failure from "absent".
pub const EXPECTED_VAULT_ROOTS: [&str; 3] = ["sources", "manifests", "extracted"];
/// Reason tag carried by the [`AssetVaultRootError`] emitted for a present-but-empty vault.
pub const ASSET_VAULT_EMPTY_REASON: &str = "vault-empty";

/// Env override for the primary checkout root, symmetric with [`LOCAL_ASSET_VAULT_ENV`]. Lets an
/// exported (no-git) checkout, or an unusual layout, name its own root without hardcoding one.
pub const REPO_ROOT_ENV: &str = "LITD_REPO_ROOT";
/// Gate step name carried in the repo-root failure line.
pub const REPO_ROOT_RESOLVE_STEP: &str = "repo-root-resolve";

/// A single, precise, actionable failure. Its `Display` is the whole `GATE_FAILED` line.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct AssetVaultRootError {
    /// Machine-readable reason tag, e.g. `override-not-a-directory`, `no-candidate-exists`.
    pub reason: &'static str,
    /// Every absolute path this resolver actually looked at, in order.
    pub searched: Vec<PathBuf>,
    message: String,
}

impl AssetVaultRootError {
    fn new(reason: &'static str, detail: impl AsRef<str>, searched: Vec<PathBuf>) -> Self {
        let rendered = searched
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let message = format!(
            "GATE_FAILED: step={ASSET_VAULT_RESOLVE_STEP} reason={reason} detail={} \
             searched=[{rendered}] fix: set {LOCAL_ASSET_VAULT_ENV}=<absolute path to the \
             {LOCAL_ASSET_VAULT_DIR} directory> (it is a large gitignored directory that exists \
             only beside the PRIMARY checkout; linked git worktrees do not get a copy)",
            detail.as_ref()
        );
        Self {
            reason,
            searched,
            message,
        }
    }

    /// #1569: the vault path resolved to a real directory that is present but NOT materialized
    /// (0 asset files / none of [`EXPECTED_VAULT_ROOTS`]). ONE loud, actionable line — emitted in
    /// place of the confusing per-file `os error 3` cascade each downstream asset test used to
    /// produce when it independently failed to open a missing `.gltf`/`.glb`/`.png`.
    fn empty_vault(root: &Path) -> Self {
        let message = format!(
            "ASSET_VAULT_EMPTY: resolved {} but it contains 0 asset files (none of [{}] present) \
             - the vault directory is present but NOT materialized (data-loss incident #1569). \
             Re-materialize it from the asset-gen pipeline (#782), or set {LOCAL_ASSET_VAULT_ENV}=\
             <absolute path to a populated {LOCAL_ASSET_VAULT_DIR}>.",
            root.display(),
            EXPECTED_VAULT_ROOTS.join(", "),
        );
        Self {
            reason: ASSET_VAULT_EMPTY_REASON,
            searched: vec![root.to_path_buf()],
            message,
        }
    }
}

/// The vault root, resolved once per process and cached.
///
/// Use this from production/library code. Tests that need to vary the environment should call
/// [`resolve_local_asset_vault_root_from`], which takes the inputs explicitly and therefore
/// never races with other tests over process-global env state.
pub fn local_asset_vault_root() -> Result<PathBuf, AssetVaultRootError> {
    static CACHED: OnceLock<Result<PathBuf, AssetVaultRootError>> = OnceLock::new();
    CACHED.get_or_init(resolve_local_asset_vault_root).clone()
}

/// `local_asset_vault_root()` joined with a vault-relative path such as
/// `sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf`.
///
/// This is the asset-LOADING entry point, so a materialized vault is a precondition: if the vault
/// resolves to a present-but-empty directory (#1569) this fails ONCE with the loud
/// `ASSET_VAULT_EMPTY` diagnostic, instead of handing back a path that every caller then trips
/// over with an opaque `os error 3` on the missing file. Pure path resolution that does NOT need
/// materialized content stays on [`local_asset_vault_root`].
pub fn local_asset_vault_path(relative: impl AsRef<Path>) -> Result<PathBuf, AssetVaultRootError> {
    let root = local_asset_vault_root()?;
    ensure_materialized(&root)?;
    Ok(root.join(relative))
}

/// Resolve the vault root AND verify it is materialized, with every input injected (the testable
/// core behind the materialization check). Mirrors [`resolve_local_asset_vault_root_from`], then
/// applies the #1569 present-but-empty check.
pub fn resolve_materialized_asset_vault_root_from(
    override_value: Option<&OsStr>,
    start_dir: &Path,
) -> Result<PathBuf, AssetVaultRootError> {
    let root = resolve_local_asset_vault_root_from(override_value, start_dir)?;
    ensure_materialized(&root)?;
    Ok(root)
}

/// `Ok(())` when the resolved vault holds at least one [`EXPECTED_VAULT_ROOTS`] directory;
/// otherwise the loud [`AssetVaultRootError::empty_vault`] diagnostic.
fn ensure_materialized(root: &Path) -> Result<(), AssetVaultRootError> {
    if vault_is_materialized(root) {
        return Ok(());
    }
    Err(AssetVaultRootError::empty_vault(root))
}

/// A vault is materialized if it exists AND contains at least one of the expected fixture roots.
/// An empty directory (0 entries, the #1569 shape) has none, so this returns `false`.
pub fn vault_is_materialized(root: &Path) -> bool {
    EXPECTED_VAULT_ROOTS
        .iter()
        .any(|name| is_directory(&root.join(name)))
}

/// Uncached resolution against the live process environment.
pub fn resolve_local_asset_vault_root() -> Result<PathBuf, AssetVaultRootError> {
    resolve_local_asset_vault_root_from(
        env::var_os(LOCAL_ASSET_VAULT_ENV).as_deref(),
        start_dir().as_path(),
    )
}

/// The resolver core, with every input injected. This is what the tests drive.
///
/// `override_value` is the raw `LITD_LOCAL_ASSET_VAULT` value (`None` when unset).
/// `start_dir` is where the git query runs and where the ancestor walk begins.
pub fn resolve_local_asset_vault_root_from(
    override_value: Option<&OsStr>,
    start_dir: &Path,
) -> Result<PathBuf, AssetVaultRootError> {
    if let Some(configured) = override_value {
        return accept_override(configured);
    }

    let mut searched = Vec::new();
    for candidate in candidate_roots(start_dir) {
        if searched.contains(&candidate) {
            continue;
        }
        if is_directory(&candidate) {
            return Ok(normalize(&candidate));
        }
        searched.push(candidate);
    }

    Err(AssetVaultRootError::new(
        "no-candidate-exists",
        format!(
            "no {LOCAL_ASSET_VAULT_DIR} directory exists at any resolved location (start_dir={})",
            start_dir.display()
        ),
        searched,
    ))
}

/// The override is authoritative: honour it or fail. Never silently fall back to a guess —
/// a wrong-but-present vault is how you get false-green tests.
fn accept_override(configured: &OsStr) -> Result<PathBuf, AssetVaultRootError> {
    if configured.is_empty() {
        return Err(AssetVaultRootError::new(
            "override-empty",
            format!("{LOCAL_ASSET_VAULT_ENV} is set but empty"),
            Vec::new(),
        ));
    }
    let candidate = PathBuf::from(configured);
    if !is_directory(&candidate) {
        let detail = if candidate.exists() {
            format!(
                "{LOCAL_ASSET_VAULT_ENV} points at {} which exists but is not a directory",
                candidate.display()
            )
        } else {
            format!(
                "{LOCAL_ASSET_VAULT_ENV} points at {} which does not exist",
                candidate.display()
            )
        };
        return Err(AssetVaultRootError::new(
            "override-not-a-directory",
            detail,
            vec![candidate],
        ));
    }
    Ok(normalize(&candidate))
}

/// Candidate vault roots, best first. Every one is derived from RUNTIME state.
fn candidate_roots(start_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(primary) = primary_checkout_root(start_dir) {
        roots.push(primary.join(LOCAL_ASSET_VAULT_DIR));
    }
    if let Some(workspace) = cargo_workspace_root() {
        roots.push(workspace.join(LOCAL_ASSET_VAULT_DIR));
    }
    for ancestor in start_dir.ancestors() {
        roots.push(ancestor.join(LOCAL_ASSET_VAULT_DIR));
    }
    roots
}

/// The primary (main) working-tree root, even when called from a linked git worktree.
///
/// `git rev-parse --path-format=absolute --git-common-dir` returns `<primary>/.git` from BOTH
/// the main checkout and a linked worktree (a linked worktree's private `--git-dir` is
/// `<primary>/.git/worktrees/<name>`, but the COMMON dir is always the shared `<primary>/.git`).
/// Its parent is therefore the primary working-tree root. `--path-format=absolute` is required:
/// without it, git prints a bare relative `.git` from the main checkout.
fn primary_checkout_root(start_dir: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .current_dir(start_dir)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let common_dir = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    // A bare repository has no working tree to sit a vault beside; the common dir would not be
    // named `.git`. Only a normal repo's `<root>/.git` yields a usable parent.
    if common_dir.file_name() != Some(OsStr::new(".git")) {
        return None;
    }
    common_dir.parent().map(Path::to_path_buf)
}

/// `<workspace>/crates/<crate>` -> `<workspace>`, read at RUNTIME (cargo sets `CARGO_MANIFEST_DIR`
/// in the environment of the test/bin process it spawns, so this tracks the tree we are actually
/// running from — unlike `env!`, which freezes the tree we were COMPILED in).
fn cargo_workspace_root() -> Option<PathBuf> {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")?;
    Path::new(&manifest_dir)
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
}

fn start_dir() -> PathBuf {
    if let Some(manifest_dir) = env::var_os("CARGO_MANIFEST_DIR") {
        let manifest_dir = PathBuf::from(manifest_dir);
        if manifest_dir.is_dir() {
            return manifest_dir;
        }
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn is_directory(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.is_dir())
}

/// Canonicalize, then strip Windows' `\\?\` verbatim prefix so error text and FSV artifacts stay
/// readable. Falls back to the input when canonicalization is unavailable.
fn normalize(path: &Path) -> PathBuf {
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.as_os_str().to_string_lossy().to_string();
    match text.strip_prefix(r"\\?\") {
        Some(stripped) => PathBuf::from(stripped),
        None => resolved,
    }
}

/// Convenience for tests/examples that want an `OsString` override without touching process env.
pub fn override_value(path: impl AsRef<Path>) -> OsString {
    path.as_ref().as_os_str().to_os_string()
}

/// #1559: a fail-loud repo-root resolution error (the repo-root sibling of
/// [`AssetVaultRootError`]). Its `Display` is the whole failure line.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct RepoRootError(pub String);

/// The PRIMARY checkout (main working-tree) root, resolved at RUNTIME so it is correct from a
/// linked git worktree too.
///
/// #1559: the one worktree-aware repo-root helper. It reuses the exact same
/// `git rev-parse --git-common-dir` primary-checkout logic that resolves the asset vault, so
/// tests and tooling stop hardcoding a machine-specific checkout root for exe paths, scripts
/// dirs, and repo roots — the disease #1543 killed for the vault, one target further out.
pub fn resolve_repo_root() -> Result<PathBuf, RepoRootError> {
    resolve_repo_root_from(env::var_os(REPO_ROOT_ENV).as_deref(), start_dir().as_path())
}

/// The repo-root resolver core, with every input injected. `override_value` is the raw
/// `LITD_REPO_ROOT` value (`None` when unset); `start_dir` is where the git query runs.
pub fn resolve_repo_root_from(
    override_value: Option<&OsStr>,
    start_dir: &Path,
) -> Result<PathBuf, RepoRootError> {
    if let Some(configured) = override_value {
        if configured.is_empty() {
            return Err(RepoRootError(format!("{REPO_ROOT_ENV} is set but empty")));
        }
        let candidate = PathBuf::from(configured);
        if is_directory(&candidate) {
            return Ok(normalize(&candidate));
        }
        return Err(RepoRootError(format!(
            "{REPO_ROOT_ENV} points at {} which is not a directory",
            candidate.display()
        )));
    }

    let mut searched = Vec::new();
    for candidate in [primary_checkout_root(start_dir), cargo_workspace_root()]
        .into_iter()
        .flatten()
    {
        if is_directory(&candidate) {
            return Ok(normalize(&candidate));
        }
        searched.push(candidate);
    }

    let rendered = searched
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(RepoRootError(format!(
        "GATE_FAILED: step={REPO_ROOT_RESOLVE_STEP} reason=no-candidate-exists could not resolve \
         the primary checkout root (start_dir={}, searched=[{rendered}]) fix: set {REPO_ROOT_ENV}=\
         <absolute path to the GameEditor checkout root>",
        start_dir.display()
    )))
}

#[cfg(test)]
#[path = "asset_vault_root_tests.rs"]
mod tests;
