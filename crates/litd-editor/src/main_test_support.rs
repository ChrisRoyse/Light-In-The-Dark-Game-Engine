//! #1559: worktree-portable fixture paths for the binary crate's tests.
//!
//! The editor's `#[cfg(test)]` fixtures used to bake `C:\code\GameEditor\...` literals for the
//! executable path and repo root. Those pass only on a checkout at exactly that location and go
//! stale in a linked git worktree or any other clone — the same disease #1543 killed for the
//! asset vault, one target further out. These helpers resolve both at RUNTIME through the one
//! worktree-aware resolver in `litd_engine::asset_vault_root`, and FAIL LOUD if the checkout
//! root cannot be resolved rather than silently substituting a wrong guess.

use std::path::{Path, PathBuf};

/// The primary checkout (main working-tree) root, resolved at runtime. Fails loud.
pub(crate) fn repo_root_for_tests() -> PathBuf {
    litd_engine::asset_vault_root::resolve_repo_root()
        .expect("resolve primary checkout root for editor test fixture")
}

/// The editor executable path for fixtures. Prefers Cargo's per-target `CARGO_BIN_EXE_litd-editor`
/// when it is set (integration-test builds); otherwise derives it from the runtime-resolved
/// primary checkout. Never a hardcoded machine path.
pub(crate) fn editor_exe_for_tests(repo_root: &Path) -> PathBuf {
    option_env!("CARGO_BIN_EXE_litd-editor")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            repo_root
                .join("target")
                .join("debug")
                .join("litd-editor.exe")
        })
}
