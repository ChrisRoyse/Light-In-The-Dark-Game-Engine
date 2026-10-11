use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};

use litd_engine::{
    EngineError,
    app::{AppModule, AppRuntime, ModuleRegistry},
    diagnostics::{ErrorReport, log_engine_error},
};

const PROBE_ENV: &str = "LITD_FAIL_LOUD_PROBE";
const STATS_ENV: &str = "LITD_FAIL_LOUD_STATS";

pub fn run_from_env() -> Option<ExitCode> {
    let probe = env::var(PROBE_ENV).ok()?;
    let stats_path = env::var(STATS_ENV).ok().map(PathBuf::from);
    Some(match run_probe(&probe, stats_path.as_deref()) {
        Ok(report) => {
            tracing::error!(
                probe,
                error_code = report.code,
                "fail-loud probe exited with expected engine failure"
            );
            ExitCode::FAILURE
        }
        Err(err) => {
            tracing::error!(probe, error = %err, "fail-loud probe setup failed");
            ExitCode::FAILURE
        }
    })
}

fn run_probe(probe: &str, stats_path: Option<&Path>) -> io::Result<ErrorReport> {
    let error = match probe {
        "frame-before-start" => frame_before_start_error(),
        "duplicate-module" => duplicate_module_error(),
        "registration-after-start" => registration_after_start_error(),
        _ => EngineError::UnknownFailLoudProbe(probe.to_string()),
    };
    let report = log_engine_error("fail_loud_probe", &error);
    if let Some(path) = stats_path {
        write_probe_stats(path, probe, &report)?;
    }
    Ok(report)
}

fn frame_before_start_error() -> EngineError {
    let mut runtime = AppRuntime::default();
    runtime
        .frame(Instant::now())
        .expect_err("frame before AppRuntime::start must fail loud")
}

fn duplicate_module_error() -> EngineError {
    let mut registry = ModuleRegistry::default();
    registry
        .register(ProbeModule("probe.runtime"))
        .expect("first module registration should succeed");
    registry
        .register(ProbeModule("probe.runtime"))
        .expect_err("duplicate module registration must fail loud")
}

fn registration_after_start_error() -> EngineError {
    let mut runtime = AppRuntime::default();
    runtime
        .modules_mut()
        .register(ProbeModule("probe.runtime"))
        .expect("initial module registration should succeed");
    runtime
        .start()
        .expect("runtime should start for registration probe");
    runtime
        .modules_mut()
        .register(ProbeModule("late.runtime"))
        .expect_err("module registration after startup must fail loud")
}

fn write_probe_stats(path: &Path, probe: &str, report: &ErrorReport) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        format!("probe={probe}\nexit_code=1\n{}", report.as_key_values()),
    )
}

struct ProbeModule(&'static str);

impl AppModule for ProbeModule {
    fn name(&self) -> &'static str {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_before_start_probe_reports_expected_code() {
        let report = run_probe("frame-before-start", None).unwrap();
        assert_eq!(report.code, "LITD-APP-FRAME-BEFORE-START");
    }

    #[test]
    fn duplicate_module_probe_reports_expected_code() {
        let report = run_probe("duplicate-module", None).unwrap();
        assert_eq!(report.code, "LITD-APP-DUPLICATE-MODULE");
    }

    #[test]
    fn registration_after_start_probe_reports_expected_code() {
        let report = run_probe("registration-after-start", None).unwrap();
        assert_eq!(report.code, "LITD-APP-REGISTRATION-CLOSED");
    }

    #[test]
    fn unknown_probe_reports_expected_code() {
        let report = run_probe("bogus", None).unwrap();
        assert_eq!(report.code, "LITD-FOUNDATION-UNKNOWN-FAIL-LOUD-PROBE");
        assert!(report.message.contains("bogus"));
    }
}
