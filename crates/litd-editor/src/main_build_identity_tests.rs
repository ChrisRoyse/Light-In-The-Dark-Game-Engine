//! Tests for the build-identity git-ref reader (#1770).
//!
//! The commondir-indirection tests build a REAL temporary git repository plus a REAL
//! linked worktree with the `git` CLI (no hand-mocked `.git` layouts): the source of
//! truth (`git rev-parse HEAD`) is read independently of the reader under test, then the
//! reader must return the identical commit from both the main checkout and the linked
//! worktree, including the packed-refs and detached-HEAD edge cases.

use super::*;
use std::io::Write;
use std::process::Command;

/// Run `git ARGS` in `dir`, panicking loudly (with stderr) on any failure.
fn run_git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|error| panic!("failed to spawn `git {}`: {error}", args.join(" ")));
    assert!(
        output.status.success(),
        "`git {}` failed ({}): {}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Read the commit `git` itself resolves for HEAD in `dir` -- the source of truth.
fn git_rev_parse_head(dir: &Path) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap_or_else(|error| panic!("failed to spawn `git rev-parse HEAD`: {error}"));
    assert!(
        output.status.success(),
        "`git rev-parse HEAD` failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git rev-parse output is utf8")
        .trim()
        .to_string()
}

/// Initialize a real repo at `dir` with one commit; returns that commit.
fn init_repo_with_commit(dir: &Path) -> String {
    fs::create_dir_all(dir).unwrap();
    run_git(dir, &["init", "-q"]);
    run_git(dir, &["config", "user.email", "fsv@litd.test"]);
    run_git(dir, &["config", "user.name", "FSV"]);
    run_git(dir, &["config", "commit.gpgsign", "false"]);
    fs::write(dir.join("seed.txt"), "seed").unwrap();
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-q", "-m", "seed"]);
    git_rev_parse_head(dir)
}

#[test]
fn sha256_file_reads_physical_file_bytes() {
    // #1535: TestTempDir auto-cleans on Drop.
    let _temp_root = litd_engine::test_temp::TestTempDir::new("editor-build-identity");
    let dir = _temp_root.path().to_path_buf();
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hash.txt");
    fs::write(&path, b"abc").unwrap();

    assert_eq!(
        sha256_file(&path).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn repo_head_reader_resolves_loose_ref() {
    // #1535: TestTempDir auto-cleans on Drop.
    let _temp_root = litd_engine::test_temp::TestTempDir::new("editor-build-identity-loose");
    let root = _temp_root.path().to_path_buf();
    let git = root.join(".git");
    fs::create_dir_all(git.join("refs/heads")).unwrap();
    fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(
        git.join("refs/heads/main"),
        "0123456789abcdef0123456789abcdef01234567\n",
    )
    .unwrap();

    assert_eq!(
        read_repo_head_commit(&root).unwrap(),
        "0123456789abcdef0123456789abcdef01234567"
    );
}

#[test]
fn repo_head_reader_resolves_packed_ref() {
    // #1535: TestTempDir auto-cleans on Drop.
    let _temp_root = litd_engine::test_temp::TestTempDir::new("editor-build-identity-packed");
    let root = _temp_root.path().to_path_buf();
    let git = root.join(".git");
    fs::create_dir_all(&git).unwrap();
    fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    let mut packed = fs::File::create(git.join("packed-refs")).unwrap();
    writeln!(packed, "# pack-refs with: peeled fully-peeled sorted").unwrap();
    writeln!(
        packed,
        "abcdef0123456789abcdef0123456789abcdef01 refs/heads/main"
    )
    .unwrap();

    assert_eq!(
        read_repo_head_commit(&root).unwrap(),
        "abcdef0123456789abcdef0123456789abcdef01"
    );
}

#[test]
fn resolve_common_dir_handles_absent_relative_and_absolute() {
    // #1770: git only ever writes a relative `../..` commondir, but the reader must also
    // honor an absolute commondir and the no-commondir (normal `.git`) case. `git` will
    // not produce those two branches, so exercise them directly here.
    let _temp_root = litd_engine::test_temp::TestTempDir::new("editor-build-identity-commondir");
    let base = _temp_root.path().to_path_buf();

    // No commondir file -> the git dir is its own common dir.
    let plain = base.join("plain.git");
    fs::create_dir_all(&plain).unwrap();
    assert_eq!(resolve_common_dir(&plain), plain);

    let shared = base.join("shared.git");
    fs::create_dir_all(&shared).unwrap();
    let shared_canon = fs::canonicalize(&shared).unwrap();

    // Relative commondir -> joined onto the gitdir and canonicalized to the shared dir.
    let wt_rel = shared.join("worktrees/rel");
    fs::create_dir_all(&wt_rel).unwrap();
    fs::write(wt_rel.join("commondir"), "../..\n").unwrap();
    assert_eq!(resolve_common_dir(&wt_rel), shared_canon);

    // Absolute commondir -> used verbatim (canonicalized).
    let wt_abs = shared.join("worktrees/abs");
    fs::create_dir_all(&wt_abs).unwrap();
    fs::write(wt_abs.join("commondir"), format!("{}\n", shared.display())).unwrap();
    assert_eq!(resolve_common_dir(&wt_abs), shared_canon);
}

#[test]
fn reader_matches_git_cli_in_main_and_linked_worktree() {
    // #1770: a REAL repo + REAL linked worktree. The worktree's branch ref lives only in
    // the shared (common) git dir; the reader must resolve the same commit git reports
    // from BOTH checkouts. The worktree HEAD is advanced so it DIVERGES from main --
    // proving the reader reads the per-worktree HEAD, not the main one.
    let _temp = litd_engine::test_temp::TestTempDir::new("editor-build-identity-realgit");
    let base = _temp.path().to_path_buf();
    let main = base.join("main");
    init_repo_with_commit(&main);

    let wt = base.join("linked");
    run_git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "wt-branch",
            wt.to_str().unwrap(),
        ],
    );
    fs::write(wt.join("wt.txt"), "wt").unwrap();
    run_git(&wt, &["add", "."]);
    run_git(&wt, &["commit", "-q", "-m", "worktree advance"]);

    let main_truth = git_rev_parse_head(&main);
    let wt_truth = git_rev_parse_head(&wt);
    assert_ne!(
        main_truth, wt_truth,
        "worktree HEAD must diverge from main for a meaningful test"
    );

    assert_eq!(read_repo_head_commit(&main).unwrap(), main_truth);
    assert_eq!(read_repo_head_commit(&wt).unwrap(), wt_truth);
}

#[test]
fn reader_matches_git_cli_when_worktree_refs_are_packed() {
    // #1770 edge: `git pack-refs --all` moves the worktree branch head out of a loose
    // file into the common packed-refs; the reader must still resolve it via commondir.
    let _temp = litd_engine::test_temp::TestTempDir::new("editor-build-identity-realgit-packed");
    let base = _temp.path().to_path_buf();
    let main = base.join("main");
    init_repo_with_commit(&main);

    let wt = base.join("linked");
    run_git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "wt-branch",
            wt.to_str().unwrap(),
        ],
    );
    fs::write(wt.join("wt.txt"), "wt").unwrap();
    run_git(&wt, &["add", "."]);
    run_git(&wt, &["commit", "-q", "-m", "worktree advance"]);
    // Collapse every loose ref (including refs/heads/wt-branch) into common packed-refs.
    run_git(&main, &["pack-refs", "--all"]);

    let wt_truth = git_rev_parse_head(&wt);
    assert_eq!(read_repo_head_commit(&wt).unwrap(), wt_truth);
}

#[test]
fn reader_matches_git_cli_for_detached_head_worktree() {
    // #1770 edge: a detached-HEAD worktree stores the raw 40-hex commit in per-worktree
    // HEAD with no ref indirection; the reader must return it verbatim (and it must equal
    // what git reports).
    let _temp = litd_engine::test_temp::TestTempDir::new("editor-build-identity-realgit-detached");
    let base = _temp.path().to_path_buf();
    let main = base.join("main");
    let commit = init_repo_with_commit(&main);

    let wt = base.join("detached");
    run_git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            wt.to_str().unwrap(),
            &commit,
        ],
    );

    let wt_truth = git_rev_parse_head(&wt);
    assert_eq!(wt_truth, commit);
    assert_eq!(read_repo_head_commit(&wt).unwrap(), wt_truth);
}
