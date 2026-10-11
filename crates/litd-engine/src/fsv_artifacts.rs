use std::{
    ffi::OsStr,
    fs,
    fs::OpenOptions,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use litd_hash::sha256_hex;
use serde_json::{Value, json};

static RUN_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct FsvRun {
    issue_slug: String,
    run_id: String,
    run_dir: PathBuf,
    latest_manifest_path: PathBuf,
}

#[derive(Debug)]
pub struct FsvReportReadback {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub json: Value,
    pub latest_manifest_path: PathBuf,
    pub manifest: Value,
}

pub fn begin_issue_run(issue_slug: &str) -> io::Result<FsvRun> {
    let run_id = format!(
        "run-{}-{}-{}",
        unix_nanos(),
        process::id(),
        RUN_COUNTER.fetch_add(1, Ordering::SeqCst)
    );
    begin_issue_run_with_id(issue_slug, &run_id)
}

pub fn begin_issue_run_with_id(issue_slug: &str, run_id: &str) -> io::Result<FsvRun> {
    validate_id("issue_slug", issue_slug)?;
    validate_id("run_id", run_id)?;
    let root = issue_root(issue_slug);
    fs::create_dir_all(&root)?;
    let root = fs::canonicalize(root)?;
    let runs_dir = root.join("runs");
    fs::create_dir_all(&runs_dir)?;
    let run_dir = runs_dir.join(run_id);
    fs::create_dir(&run_dir)?;
    let latest_manifest_path = root.join("latest-manifest.json");
    Ok(FsvRun {
        issue_slug: issue_slug.to_string(),
        run_id: run_id.to_string(),
        run_dir,
        latest_manifest_path,
    })
}

pub fn issue_root(issue_slug: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("fsv")
        .join(issue_slug)
}

/// Resolve a workspace-root-relative path (e.g. `docs/lore/…`, `crates/litd-engine/src/…`)
/// to an absolute path that is independent of the process working directory.
///
/// This matters because the two ways FSV examples are launched set different working
/// directories: `cargo test --example` (the run gate in `test-fsv-examples.ps1`) runs the
/// binary with CWD = the package root (`crates/litd-engine`), while `cargo run --example`
/// inherits the caller's CWD (typically the workspace root). Any example that reads
/// repository sources with a plain relative path therefore fails with a bare `NotFound`
/// under the gate. Anchoring to `CARGO_MANIFEST_DIR` (always `<workspace>/crates/litd-engine`)
/// makes source readbacks run identically under both launch modes (#1527). Mirrors the
/// anchor used by [`issue_root`].
pub fn workspace_path(relative: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(relative)
}

/// Resolve a filesystem path from environment variable `name`, falling back to
/// `default` when the variable is unset. Shared by the FSV example harnesses (#1515),
/// which each copy-pasted this helper. Uses `var_os` so a set-but-non-UTF-8 path is
/// honored rather than silently dropped in favor of the default.
pub fn env_path(name: &str, default: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

impl FsvRun {
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    pub fn report_path(&self) -> PathBuf {
        self.run_dir.join("manual-fsv-report.json")
    }

    pub fn latest_manifest_path(&self) -> &Path {
        &self.latest_manifest_path
    }

    pub fn artifact_path(&self, relative_path: &str) -> io::Result<PathBuf> {
        validate_relative_artifact_path(relative_path)?;
        Ok(self.run_dir.join(relative_path))
    }

    pub fn artifact_dir(&self, relative_path: &str) -> io::Result<PathBuf> {
        validate_relative_artifact_path(relative_path)?;
        let path = self.run_dir.join(relative_path);
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    pub fn write_report(&self, report: &Value) -> io::Result<FsvReportReadback> {
        self.write_json_artifact("manual-fsv-report.json", report)
    }

    pub fn write_json_artifact(
        &self,
        relative_path: &str,
        report: &Value,
    ) -> io::Result<FsvReportReadback> {
        let report_path = self.artifact_path(relative_path)?;
        self.write_json_file(&report_path, report)
    }

    pub fn write_json_file(&self, path: &Path, report: &Value) -> io::Result<FsvReportReadback> {
        let relative_path = self.relative_artifact_path(path)?;
        let bytes = serde_json::to_vec_pretty(report).map_err(invalid_data)?;
        let report_path = self.artifact_path(&relative_path)?;
        if let Some(parent) = report_path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_create_new(&report_path, &bytes)?;
        self.record_json_file(&report_path)
    }

    pub fn record_json_file(&self, path: &Path) -> io::Result<FsvReportReadback> {
        let relative_path = self.relative_artifact_path(path)?;
        let report_path = self.artifact_path(&relative_path)?;
        let report_bytes = fs::read(&report_path)?;
        let report_json: Value = serde_json::from_slice(&report_bytes).map_err(invalid_data)?;
        let report_sha256 = sha256_hex(&report_bytes);
        let manifest = json!({
            "schema_version": 1,
            "issue_slug": self.issue_slug,
            "run_id": self.run_id,
            "run_dir": self.run_dir.display().to_string(),
            "report_path": report_path.display().to_string(),
            "report_bytes": report_bytes.len(),
            "report_sha256": report_sha256,
            "written_at_unix_nanos": unix_nanos(),
            "source_of_truth": "immutable run report; this manifest is only the latest pointer",
        });
        let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(invalid_data)?;
        fs::write(&self.latest_manifest_path, manifest_bytes)?;
        let manifest_readback = fs::read(&self.latest_manifest_path)?;
        let manifest_json: Value =
            serde_json::from_slice(&manifest_readback).map_err(invalid_data)?;
        if manifest_json["run_id"] != self.run_id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "latest FSV manifest readback points at the wrong run_id",
            ));
        }
        if manifest_json["report_sha256"] != report_sha256 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "latest FSV manifest readback hash does not match report bytes",
            ));
        }
        if manifest_json["report_path"] != report_path.display().to_string() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "latest FSV manifest readback points at the wrong report_path",
            ));
        }
        Ok(FsvReportReadback {
            path: report_path,
            bytes: report_bytes,
            sha256: report_sha256,
            json: report_json,
            latest_manifest_path: self.latest_manifest_path.clone(),
            manifest: manifest_json,
        })
    }

    fn relative_artifact_path(&self, path: &Path) -> io::Result<String> {
        let relative_path = path.strip_prefix(&self.run_dir).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "FSV artifact path is outside the immutable run directory",
            )
        })?;
        normalize_relative_path(relative_path)
    }
}

fn validate_id(field: &str, value: &str) -> io::Result<()> {
    if value.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{field} must be non-empty"),
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{field} contains unsafe path characters"),
        ));
    }
    Ok(())
}

fn normalize_relative_path(path: &Path) -> io::Result<String> {
    let mut segments = Vec::new();
    for component in path.components() {
        let Component::Normal(segment) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "artifact path contains unsafe path components",
            ));
        };
        validate_path_segment(segment)?;
        let segment = segment.to_str().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "artifact path segment must be valid UTF-8",
            )
        })?;
        segments.push(segment.to_string());
    }
    if segments.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "artifact path must contain a file or directory name",
        ));
    }
    Ok(segments.join("/"))
}

fn validate_relative_artifact_path(value: &str) -> io::Result<()> {
    if value.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "artifact path must be non-empty",
        ));
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "artifact path must be relative",
        ));
    }
    let mut segment_count = 0;
    for component in path.components() {
        let Component::Normal(segment) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "artifact path contains unsafe path components",
            ));
        };
        validate_path_segment(segment)?;
        segment_count += 1;
    }
    if segment_count == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "artifact path must contain a file or directory name",
        ));
    }
    Ok(())
}

fn validate_path_segment(segment: &OsStr) -> io::Result<()> {
    let Some(segment) = segment.to_str() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "artifact path segment must be valid UTF-8",
        ));
    };
    if segment.is_empty() || segment == "." || segment == ".." {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "artifact path contains unsafe path segment",
        ));
    }
    if !segment
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "artifact path segment contains unsafe characters",
        ));
    }
    Ok(())
}

fn write_create_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn invalid_data(error: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[test]
fn issue1515_env_path_falls_back_to_default_when_unset() {
    // Source of truth: an env var that is definitely not set -> the default is used.
    let resolved = env_path(
        "LITD_ISSUE1515_ENV_PATH_DEFINITELY_UNSET_9x7z",
        "some/fallback/dir",
    );
    assert_eq!(resolved, PathBuf::from("some/fallback/dir"));
}

#[test]
fn issue1515_env_path_uses_env_override_when_set() {
    let name = "LITD_ISSUE1515_ENV_PATH_OVERRIDE_UNIQUE_42";
    // SAFETY: a unique test-only variable name; this crate's tests already manipulate
    // process env. Set an override, resolve, then restore.
    unsafe {
        std::env::set_var(name, "custom/override/dir");
    }
    let resolved = env_path(name, "the/default/dir");
    unsafe {
        std::env::remove_var(name);
    }
    assert_eq!(resolved, PathBuf::from("custom/override/dir"));
}

#[test]
fn issue1344_immutable_fsv_runs_preserve_historical_reports() {
    let slug = "issue1344_immutable_fsv_artifacts";
    let before = artifact_state(slug);
    let first = begin_issue_run(slug).unwrap();
    let first_report = json!({
        "schema_version": 1,
        "issue": 1344,
        "sequence": "first",
        "expected": "this report must remain byte-stable after the second run",
    });
    let first_readback = first.write_report(&first_report).unwrap();
    let first_bytes_after_first = fs::read(&first_readback.path).unwrap();
    assert_eq!(sha256_hex(&first_bytes_after_first), first_readback.sha256);

    let second = begin_issue_run(slug).unwrap();
    let second_report = json!({
        "schema_version": 1,
        "issue": 1344,
        "sequence": "second",
        "expected": "latest manifest must point here without touching the first report",
    });
    let second_readback = second.write_report(&second_report).unwrap();
    let first_bytes_after_second = fs::read(&first_readback.path).unwrap();
    assert_eq!(first_bytes_after_second, first_bytes_after_first);
    assert_ne!(first_readback.path, second_readback.path);
    assert_eq!(first_readback.json["sequence"], "first");
    assert_eq!(second_readback.json["sequence"], "second");

    let latest_manifest_bytes = fs::read(&second_readback.latest_manifest_path).unwrap();
    let latest_manifest: Value = serde_json::from_slice(&latest_manifest_bytes).unwrap();
    assert_eq!(latest_manifest["run_id"], second.run_id());
    assert_eq!(latest_manifest["report_sha256"], second_readback.sha256);
    assert_eq!(
        latest_manifest["report_path"],
        second_readback.path.display().to_string()
    );

    let unsafe_slug = begin_issue_run_with_id("../bad", "run-edge").unwrap_err();
    let empty_run = begin_issue_run_with_id(slug, "").unwrap_err();
    let duplicate_run = begin_issue_run_with_id(slug, first.run_id()).unwrap_err();
    let unsafe_artifact = first.artifact_path("../bad.json").unwrap_err();
    let unsafe_nested = first
        .write_json_artifact("manual/../bad.json", &json!({"bad": true}))
        .unwrap_err();
    assert_eq!(unsafe_slug.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(empty_run.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(duplicate_run.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(unsafe_artifact.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(unsafe_nested.kind(), io::ErrorKind::InvalidInput);

    let custom = begin_issue_run(slug).unwrap();
    let custom_readback = custom
        .write_json_artifact("manual/custom-fsv.json", &json!({"sequence": "custom"}))
        .unwrap();
    assert_eq!(custom_readback.json["sequence"], "custom");
    assert!(
        custom_readback
            .path
            .ends_with(Path::new("manual").join("custom-fsv.json"))
    );

    let after = artifact_state(slug);
    println!(
        "ISSUE1344_IMMUTABLE_FSV_READBACK before={} after={} first_path={} first_sha256={} second_path={} second_sha256={} custom_path={} custom_sha256={} latest_manifest={} latest_run_id={} unsafe_slug_error={} empty_run_error={} duplicate_run_error={} unsafe_artifact_error={} unsafe_nested_error={}",
        before,
        after,
        first_readback.path.display(),
        first_readback.sha256,
        second_readback.path.display(),
        second_readback.sha256,
        custom_readback.path.display(),
        custom_readback.sha256,
        second_readback.latest_manifest_path.display(),
        custom_readback.manifest["run_id"],
        unsafe_slug.kind(),
        empty_run.kind(),
        duplicate_run.kind(),
        unsafe_artifact.kind(),
        unsafe_nested.kind(),
    );
}

#[cfg(test)]
fn artifact_state(slug: &str) -> Value {
    let root = issue_root(slug);
    let runs_dir = root.join("runs");
    let run_count = fs::read_dir(&runs_dir)
        .map(|entries| entries.filter_map(Result::ok).count())
        .unwrap_or(0);
    json!({
        "root": root.display().to_string(),
        "root_exists": root.exists(),
        "runs_dir_exists": runs_dir.exists(),
        "run_count": run_count,
        "latest_manifest_exists": root.join("latest-manifest.json").exists(),
    })
}
