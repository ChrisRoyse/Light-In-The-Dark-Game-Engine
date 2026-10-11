use std::{
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use litd_engine::EngineError;
use sha2::{Digest, Sha256};
use tracing::{info, warn};

const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD_GIT_COMMIT: &str = env!("LITD_BUILD_GIT_COMMIT");
const BUILD_GIT_DIRTY: &str = env!("LITD_BUILD_GIT_DIRTY");
const BUILD_PROFILE: &str = env!("LITD_BUILD_PROFILE");
const BUILD_TARGET: &str = env!("LITD_BUILD_TARGET");
const BUILD_TIMESTAMP_UNIX_SECONDS: &str = env!("LITD_BUILD_TIMESTAMP_UNIX_SECONDS");

#[derive(Debug, Clone)]
pub struct BuildIdentity {
    pub package_version: &'static str,
    pub build_git_commit: &'static str,
    pub build_git_dirty: bool,
    pub build_profile: &'static str,
    pub build_target: &'static str,
    pub build_timestamp_unix_seconds: &'static str,
    pub executable_path: PathBuf,
    pub executable_sha256: String,
    pub repo_root: Option<PathBuf>,
    pub repo_head_commit: Option<String>,
    pub repo_head_matches_build: Option<bool>,
}

impl BuildIdentity {
    pub fn current() -> Result<Self, EngineError> {
        validate_embedded_identity()?;
        let executable_path = env::current_exe()
            .map_err(|error| identity_error("executable_path", error.to_string()))
            .and_then(|path| {
                fs::canonicalize(&path).map_err(|error| {
                    identity_error(
                        "executable_path",
                        format!("failed to canonicalize {}: {error}", path.display()),
                    )
                })
            })?;
        let executable_sha256 = sha256_file(&executable_path).map_err(|error| {
            identity_error(
                "executable_sha256",
                format!("failed to hash {}: {error}", executable_path.display()),
            )
        })?;
        let repo_root = find_repo_root(&executable_path);
        let repo_head_commit = repo_root
            .as_deref()
            .map(read_repo_head_commit)
            .transpose()?;
        let repo_head_matches_build = repo_head_commit
            .as_ref()
            .map(|commit| commit.eq_ignore_ascii_case(BUILD_GIT_COMMIT));

        Ok(Self {
            package_version: PACKAGE_VERSION,
            build_git_commit: BUILD_GIT_COMMIT,
            build_git_dirty: BUILD_GIT_DIRTY == "true",
            build_profile: BUILD_PROFILE,
            build_target: BUILD_TARGET,
            build_timestamp_unix_seconds: BUILD_TIMESTAMP_UNIX_SECONDS,
            executable_path,
            executable_sha256,
            repo_root,
            repo_head_commit,
            repo_head_matches_build,
        })
    }

    pub fn log_startup(&self, log_path: &Path) {
        info!(
            version = self.package_version,
            build_git_commit = self.build_git_commit,
            build_git_dirty = self.build_git_dirty,
            build_profile = self.build_profile,
            build_target = self.build_target,
            build_timestamp_unix_seconds = self.build_timestamp_unix_seconds,
            executable_path = %self.executable_path.display(),
            executable_sha256 = %self.executable_sha256,
            repo_root = self.repo_root.as_ref().map(|path| path.display().to_string()).unwrap_or_else(|| "not_found".to_string()),
            repo_head_commit = self.repo_head_commit.as_deref().unwrap_or("not_found"),
            repo_head_matches_build = self.repo_head_matches_build.map(|value| value.to_string()).unwrap_or_else(|| "unknown".to_string()),
            log_path = %log_path.display(),
            "Light in the Dark editor starting"
        );
        if self.repo_head_matches_build == Some(false) {
            warn!(
                build_git_commit = self.build_git_commit,
                repo_head_commit = self.repo_head_commit.as_deref().unwrap_or("not_found"),
                executable_path = %self.executable_path.display(),
                executable_sha256 = %self.executable_sha256,
                "editor executable build identity differs from checked-out HEAD; rebuild this binary before FSV"
            );
        }
    }
}

fn validate_embedded_identity() -> Result<(), EngineError> {
    if !is_full_git_commit(BUILD_GIT_COMMIT) {
        return Err(identity_error(
            "build_git_commit",
            format!("missing or invalid embedded git commit '{BUILD_GIT_COMMIT}'"),
        ));
    }
    if BUILD_PROFILE.trim().is_empty() {
        return Err(identity_error(
            "build_profile",
            "missing embedded Cargo profile".to_string(),
        ));
    }
    if BUILD_TARGET.trim().is_empty() {
        return Err(identity_error(
            "build_target",
            "missing embedded target triple".to_string(),
        ));
    }
    if BUILD_TIMESTAMP_UNIX_SECONDS.parse::<u64>().is_err() {
        return Err(identity_error(
            "build_timestamp_unix_seconds",
            format!("invalid embedded timestamp '{BUILD_TIMESTAMP_UNIX_SECONDS}'"),
        ));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn find_repo_root(executable_path: &Path) -> Option<PathBuf> {
    executable_path
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .map(Path::to_path_buf)
}

fn read_repo_head_commit(repo_root: &Path) -> Result<String, EngineError> {
    // In a linked git worktree the per-worktree gitdir holds HEAD/index but NOT the
    // shared refs. `refs/heads/*`, `refs/tags/*`, and `packed-refs` live in the common
    // git dir, located via the `commondir` file (git gitrepository-layout(5)). HEAD is
    // read from the per-worktree gitdir; refs resolve against the common dir.
    let git_dir = resolve_git_dir(repo_root)?;
    let common_dir = resolve_common_dir(&git_dir);
    let head_path = git_dir.join("HEAD");
    let head = fs::read_to_string(&head_path).map_err(|error| {
        identity_error(
            "repo_head",
            format!("failed to read {}: {error}", head_path.display()),
        )
    })?;
    let head = head.trim();
    let commit = if let Some(reference) = head.strip_prefix("ref: ") {
        read_ref_commit(&git_dir, &common_dir, reference.trim())?
    } else {
        head.to_string()
    };
    if !is_full_git_commit(&commit) {
        return Err(identity_error(
            "repo_head",
            format!("repo HEAD did not resolve to a full commit: '{commit}'"),
        ));
    }
    Ok(commit)
}

fn resolve_git_dir(repo_root: &Path) -> Result<PathBuf, EngineError> {
    let git_path = repo_root.join(".git");
    if git_path.is_dir() {
        return Ok(git_path);
    }
    let text = fs::read_to_string(&git_path).map_err(|error| {
        identity_error(
            "repo_git_dir",
            format!("failed to read {}: {error}", git_path.display()),
        )
    })?;
    let Some(raw_path) = text.trim().strip_prefix("gitdir:") else {
        return Err(identity_error(
            "repo_git_dir",
            format!("{} is not a gitdir pointer", git_path.display()),
        ));
    };
    let raw_path = raw_path.trim();
    let git_dir = PathBuf::from(raw_path);
    if git_dir.is_absolute() {
        Ok(git_dir)
    } else {
        Ok(repo_root.join(git_dir))
    }
}

/// Resolve the shared (common) git dir for `git_dir`.
///
/// A linked worktree's gitdir contains a `commondir` file whose content is the path to
/// the shared git dir -- absolute, or relative to the gitdir (typically `../..`). When
/// no `commondir` file exists (a normal, non-worktree `.git`), the git dir is its own
/// common dir. The joined relative path is canonicalized when possible to collapse the
/// `..` segments; canonicalization is best-effort so a not-yet-existing path still works.
fn resolve_common_dir(git_dir: &Path) -> PathBuf {
    let commondir_file = git_dir.join("commondir");
    let Ok(text) = fs::read_to_string(&commondir_file) else {
        return git_dir.to_path_buf();
    };
    let raw = text.trim();
    if raw.is_empty() {
        return git_dir.to_path_buf();
    }
    let candidate = PathBuf::from(raw);
    let common_dir = if candidate.is_absolute() {
        candidate
    } else {
        git_dir.join(candidate)
    };
    fs::canonicalize(&common_dir).unwrap_or(common_dir)
}

fn read_ref_commit(
    git_dir: &Path,
    common_dir: &Path,
    reference: &str,
) -> Result<String, EngineError> {
    // Loose ref precedence: per-worktree gitdir first (covers per-worktree refs such as
    // refs/bisect/*, refs/worktree/*), then the shared common dir (refs/heads, refs/tags).
    // A loose ref beats packed-refs, which lives only in the common dir.
    let worktree_ref_path = git_dir.join(reference);
    let common_ref_path = common_dir.join(reference);
    for loose_ref_path in [&worktree_ref_path, &common_ref_path] {
        if loose_ref_path.exists() {
            return fs::read_to_string(loose_ref_path)
                .map(|value| value.trim().to_string())
                .map_err(|error| {
                    identity_error(
                        "repo_head_ref",
                        format!("failed to read {}: {error}", loose_ref_path.display()),
                    )
                });
        }
    }
    let packed_refs_path = common_dir.join("packed-refs");
    let packed_refs = fs::read_to_string(&packed_refs_path).map_err(|error| {
        identity_error(
            "repo_head_ref",
            format!(
                "failed to read loose ref (worktree {} or common {}) and packed refs {}: {error}",
                worktree_ref_path.display(),
                common_ref_path.display(),
                packed_refs_path.display()
            ),
        )
    })?;
    for line in packed_refs.lines() {
        if line.starts_with('#') || line.starts_with('^') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some(commit) = fields.next() else {
            continue;
        };
        if fields.next() == Some(reference) {
            return Ok(commit.to_string());
        }
    }
    Err(identity_error(
        "repo_head_ref",
        format!("HEAD reference '{reference}' was not found"),
    ))
}

fn identity_error(field: &'static str, reason: String) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.build_identity.{field}"),
        reason,
    }
}

fn is_full_git_commit(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
#[path = "main_build_identity_tests.rs"]
mod tests;
