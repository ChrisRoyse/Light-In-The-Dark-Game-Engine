use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn main() {
    require_windows_target("litd-editor");
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("litd-editor must live under <repo>/crates/litd-editor");

    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src");
    println!(
        "cargo::rerun-if-changed={}",
        repo_root.join(".git/HEAD").display()
    );
    if let Some(head_ref) = git_head_ref(repo_root) {
        println!(
            "cargo::rerun-if-changed={}",
            repo_root.join(".git").join(head_ref).display()
        );
    }

    let git_commit = git_output(repo_root, &["rev-parse", "HEAD"]);
    validate_commit(&git_commit);
    let git_dirty = !git_output(
        repo_root,
        &["status", "--porcelain", "--untracked-files=no"],
    )
    .trim()
    .is_empty();
    let profile = env::var("PROFILE").expect("PROFILE");
    let target = env::var("TARGET").expect("TARGET");
    let timestamp = build_timestamp_unix_seconds();

    println!("cargo::rustc-env=LITD_BUILD_GIT_COMMIT={git_commit}");
    println!("cargo::rustc-env=LITD_BUILD_GIT_DIRTY={git_dirty}");
    println!("cargo::rustc-env=LITD_BUILD_PROFILE={profile}");
    println!("cargo::rustc-env=LITD_BUILD_TARGET={target}");
    println!("cargo::rustc-env=LITD_BUILD_TIMESTAMP_UNIX_SECONDS={timestamp}");
}

fn require_windows_target(package: &str) {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        panic!(
            "LITD_WINDOWS_PLATFORM_REQUIRED: {package} supports native Windows only; requested target_os={target_os}"
        );
    }
}

fn git_head_ref(repo_root: &Path) -> Option<String> {
    let head = std::fs::read_to_string(repo_root.join(".git/HEAD")).ok()?;
    head.strip_prefix("ref: ")
        .map(|value| value.trim().replace('\\', "/"))
}

fn git_output(repo_root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .unwrap_or_else(|error| panic!("failed to run git {}: {error}", args.join(" ")));
    if !output.status.success() {
        panic!(
            "git {} failed with status {}: {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8(output.stdout)
        .expect("git output must be utf-8")
        .trim()
        .to_string()
}

fn validate_commit(commit: &str) {
    assert!(
        commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "git commit must be a full 40-character SHA-1, got '{commit}'"
    );
}

fn build_timestamp_unix_seconds() -> u64 {
    if let Ok(value) = env::var("SOURCE_DATE_EPOCH") {
        return value
            .parse::<u64>()
            .expect("SOURCE_DATE_EPOCH must be an unsigned unix timestamp");
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_secs()
}
