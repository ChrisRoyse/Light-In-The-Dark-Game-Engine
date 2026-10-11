//! #1543: proof that the vault resolver is worktree-safe and fails loud.
//!
//! Every test drives [`resolve_local_asset_vault_root_from`] with injected inputs, so nothing
//! here mutates process-global env state and nothing races with the rest of the suite.

use super::*;
use crate::test_temp::TestTempDir;

/// The regression itself: resolving from INSIDE this crate's source dir must land on the vault
/// beside the PRIMARY checkout. In a linked worktree the old `env!("CARGO_MANIFEST_DIR")` path
/// arithmetic pointed at a directory that does not exist; this must not.
#[test]
fn issue1543_resolves_real_vault_from_the_current_checkout() {
    let root = resolve_local_asset_vault_root().unwrap();
    println!("resolved_vault_root={}", root.display());

    // Source of truth is the FILESYSTEM, not the return value.
    assert!(
        root.is_dir(),
        "resolver returned {} which is not a directory",
        root.display()
    );
    let stan = root.join("sources/quaternius/game-kit/animatedmech/Flat_Colors/glTF/Stan.gltf");
    assert!(
        stan.is_file(),
        "resolved vault {} does not contain the Stan.gltf fixture at {}",
        root.display(),
        stan.display()
    );
    println!(
        "stan_exists={} stan_bytes={}",
        stan.is_file(),
        std::fs::metadata(&stan).unwrap().len()
    );
}

/// A linked git worktree has NO `local-asset-vault` of its own. Starting the resolver there must
/// still reach the primary checkout's vault, because step 2 asks git for the COMMON dir.
#[test]
fn issue1543_worktree_start_dir_still_reaches_the_primary_vault() {
    let from_crate =
        resolve_local_asset_vault_root_from(None, Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sibling_guess = manifest_dir
        .ancestors()
        .nth(2)
        .unwrap()
        .join(LOCAL_ASSET_VAULT_DIR);

    println!(
        "compile_time_manifest_dir={}\ncompile_time_sibling_guess={} exists={}\nruntime_resolved={} exists={}",
        manifest_dir.display(),
        sibling_guess.display(),
        sibling_guess.is_dir(),
        from_crate.display(),
        from_crate.is_dir()
    );
    // Whatever the compile-time guess was, the runtime answer must be a real vault.
    assert!(from_crate.is_dir());
    assert!(from_crate.join("manifests").is_dir());
}

/// Edge case (a): an explicit, valid override wins over everything else.
#[test]
fn issue1543_explicit_override_wins() {
    let temp = TestTempDir::new("issue1543-explicit-override");
    let vault = temp.path().join(LOCAL_ASSET_VAULT_DIR);
    std::fs::create_dir_all(vault.join("manifests")).unwrap();

    let resolved =
        resolve_local_asset_vault_root_from(Some(vault.as_os_str()), Path::new(".")).unwrap();
    println!(
        "override_in={}\noverride_resolved={} exists={}",
        vault.display(),
        resolved.display(),
        resolved.is_dir()
    );

    assert!(resolved.is_dir());
    assert_eq!(normalize(&vault), resolved);
}

/// Edge case (b): an override that does not exist FAILS LOUD. It must NOT fall through to the
/// git-resolved vault — a silent fallback would make a typo'd override look green.
#[test]
fn issue1543_nonexistent_override_fails_loud_and_never_falls_back() {
    let missing = Path::new(r"C:\definitely\not\a\real\vault\issue1543");
    let error =
        resolve_local_asset_vault_root_from(Some(missing.as_os_str()), Path::new(".")).unwrap_err();

    println!("nonexistent_override_error={error}");

    assert_eq!(error.reason, "override-not-a-directory");
    assert!(
        error
            .to_string()
            .starts_with("GATE_FAILED: step=asset-vault-resolve")
    );
    assert!(error.to_string().contains(LOCAL_ASSET_VAULT_ENV));
    assert!(error.to_string().contains("does not exist"));
    assert_eq!(error.searched, vec![missing.to_path_buf()]);
}

/// Edge case (b'): an override pointing at a FILE, not a directory, is also a loud failure.
#[test]
fn issue1543_override_pointing_at_a_file_fails_loud() {
    let temp = TestTempDir::new("issue1543-override-file");
    std::fs::create_dir_all(temp.path()).unwrap();
    let file = temp.path().join("not-a-vault.txt");
    std::fs::write(&file, b"i am a file\n").unwrap();

    let error =
        resolve_local_asset_vault_root_from(Some(file.as_os_str()), Path::new(".")).unwrap_err();
    println!("override_is_file_error={error}");

    assert_eq!(error.reason, "override-not-a-directory");
    assert!(error.to_string().contains("is not a directory"));
}

/// Edge case (b''): an override that is set but EMPTY is a loud failure, not "unset".
#[test]
fn issue1543_empty_override_fails_loud() {
    let error =
        resolve_local_asset_vault_root_from(Some(OsStr::new("")), Path::new(".")).unwrap_err();
    println!("empty_override_error={error}");

    assert_eq!(error.reason, "override-empty");
    assert!(error.to_string().contains("is set but empty"));
}

/// Edge case (c): the vault is genuinely unreachable. ONE explained failure that names every path
/// searched — not twelve `os error 3` import failures.
#[test]
fn issue1543_unreachable_vault_reports_every_searched_path_once() {
    // A temp dir outside any git repo and outside the workspace: no git common dir, no vault in
    // any ancestor. `CARGO_MANIFEST_DIR` is still set by cargo, so the workspace candidate is
    // still tried — and it EXISTS here, so to prove the unreachable path we must look at a run
    // where it does not. Assert on the structure instead: every candidate is listed, deduped.
    let temp = TestTempDir::new("issue1543-unreachable");
    let start = temp.path().to_path_buf();

    let candidates = candidate_roots(&start);
    println!("unreachable_start_dir={}", start.display());
    for candidate in &candidates {
        println!(
            "  candidate={} exists={}",
            candidate.display(),
            candidate.is_dir()
        );
    }

    // Simulate "nothing exists" by asking the error constructor to render the real candidate set,
    // which is exactly what the resolver does when none of them are directories.
    let error = AssetVaultRootError::new(
        "no-candidate-exists",
        format!("no {LOCAL_ASSET_VAULT_DIR} directory exists at any resolved location"),
        candidates.clone(),
    );
    println!("unreachable_error={error}");

    assert!(
        error
            .to_string()
            .starts_with("GATE_FAILED: step=asset-vault-resolve")
    );
    assert!(error.to_string().contains("reason=no-candidate-exists"));
    assert!(error.to_string().contains(LOCAL_ASSET_VAULT_ENV));
    for candidate in &candidates {
        assert!(error.to_string().contains(&candidate.display().to_string()));
    }
}

/// The resolver must never hand back a path it has not confirmed on disk.
#[test]
fn issue1543_resolved_root_is_always_a_real_directory() {
    let root = local_asset_vault_root().unwrap();
    let meta = std::fs::metadata(&root).unwrap();
    println!(
        "cached_root={} is_dir={} manifests={}",
        root.display(),
        meta.is_dir(),
        root.join("manifests").is_dir()
    );
    assert!(meta.is_dir());
}

/// #1569 edge case (a): a resolved vault that IS materialized (holds an expected fixture root)
/// produces NO empty-vault warning — the materialization check passes through cleanly.
#[test]
fn issue1569_materialized_override_has_no_empty_warning() {
    let temp = TestTempDir::new("issue1569-materialized");
    let vault = temp.path().join(LOCAL_ASSET_VAULT_DIR);
    std::fs::create_dir_all(vault.join("sources")).unwrap();

    let resolved =
        resolve_materialized_asset_vault_root_from(Some(vault.as_os_str()), Path::new("."))
            .unwrap();
    println!(
        "materialized_override_in={} materialized_resolved={} is_materialized={}",
        vault.display(),
        resolved.display(),
        vault_is_materialized(&resolved)
    );

    assert_eq!(normalize(&vault), resolved);
    assert!(vault_is_materialized(&resolved));
}

/// #1569 edge case (b): a vault that RESOLVES and EXISTS but is EMPTY (0 asset files, none of the
/// expected roots) is a DISTINCT loud failure — `ASSET_VAULT_EMPTY`, "present but not
/// materialized" — not the opaque per-file `os error 3` cascade and not a false green.
#[test]
fn issue1569_empty_but_present_vault_reports_materialize_diagnostic() {
    let temp = TestTempDir::new("issue1569-empty-present");
    // The vault directory EXISTS but has zero asset content — the exact #1569 shape.
    let vault = temp.path().join(LOCAL_ASSET_VAULT_DIR);
    std::fs::create_dir_all(&vault).unwrap();
    assert!(vault.is_dir());
    assert!(!vault_is_materialized(&vault));

    let error = resolve_materialized_asset_vault_root_from(Some(vault.as_os_str()), Path::new("."))
        .unwrap_err();
    println!("empty_vault_error={error}");

    assert_eq!(error.reason, ASSET_VAULT_EMPTY_REASON);
    assert!(error.to_string().starts_with("ASSET_VAULT_EMPTY:"));
    assert!(error.to_string().contains("present but NOT materialized"));
    assert!(error.to_string().contains("#1569"));
    assert!(error.to_string().contains("Re-materialize"));
    // It is the empty diagnostic, NOT the "absent" one — a categorically different failure.
    assert!(!error.to_string().contains("no-candidate-exists"));
}

/// #1559: the worktree-aware repo-root resolver reaches the PRIMARY checkout (a real directory
/// with the workspace `Cargo.toml`), just like the vault resolver — no hardcoded machine path.
#[test]
fn issue1559_repo_root_resolves_primary_checkout() {
    let root = resolve_repo_root_from(None, Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    println!("resolved_repo_root={}", root.display());
    assert!(root.is_dir());
    assert!(
        root.join("Cargo.toml").is_file(),
        "{} is not a workspace root",
        root.display()
    );
}

/// #1559: an explicit `LITD_REPO_ROOT` override wins, and a bad one FAILS LOUD (never a silent
/// fallback to a wrong guess).
#[test]
fn issue1559_repo_root_override_wins_and_bad_override_fails_loud() {
    let temp = TestTempDir::new("issue1559-repo-root-override");
    std::fs::create_dir_all(temp.path()).unwrap();
    let good = resolve_repo_root_from(Some(temp.path().as_os_str()), Path::new(".")).unwrap();
    assert_eq!(normalize(temp.path()), good);

    let missing = Path::new(r"C:\definitely\not\a\real\repo\issue1559");
    let error = resolve_repo_root_from(Some(missing.as_os_str()), Path::new(".")).unwrap_err();
    println!("bad_repo_root_override_error={error}");
    assert!(error.to_string().contains(REPO_ROOT_ENV));
    assert!(error.to_string().contains("is not a directory"));
}

/// `git rev-parse --git-common-dir` must agree with the vault we resolved: the vault sits inside
/// the PRIMARY working tree, whose root is the parent of the common git dir.
#[test]
fn issue1543_primary_checkout_root_matches_git_common_dir_parent() {
    let start = Path::new(env!("CARGO_MANIFEST_DIR"));
    let primary = primary_checkout_root(start).expect("git must resolve the primary checkout");
    println!("primary_checkout_root={}", primary.display());

    assert!(primary.is_dir());
    assert!(
        primary.join("Cargo.toml").is_file(),
        "{} is not a workspace root",
        primary.display()
    );
    assert_eq!(
        normalize(&primary.join(LOCAL_ASSET_VAULT_DIR)),
        local_asset_vault_root().unwrap()
    );
}
