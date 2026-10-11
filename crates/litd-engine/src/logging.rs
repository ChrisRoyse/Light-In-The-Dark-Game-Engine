use crate::EngineError;
use std::{env, fs, fs::OpenOptions, path::PathBuf, sync::Mutex};

/// Initialize structured console logging for command-line FSV tools.
pub fn init_logging() {
    use tracing_subscriber::{EnvFilter, fmt};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info,litd_engine=debug,litd_editor=debug,litd_game=debug")
    });
    let _ = fmt().with_env_filter(filter).with_target(true).try_init();
}

/// Initialize structured file logging for GUI binaries that must not open a
/// console window. Returns the physical log path for FSV readback.
pub fn init_file_logging(app_name: &str) -> Result<PathBuf, EngineError> {
    use tracing_subscriber::{EnvFilter, fmt};

    let log_path = log_file_path(app_name)?;
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|error| EngineError::LoggingInvalidConfig {
            field: "log_file.parent".to_string(),
            reason: format!("failed to create {}: {error}", parent.display()),
        })?;
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| EngineError::LoggingInvalidConfig {
            field: "log_file".to_string(),
            reason: format!("failed to open {}: {error}", log_path.display()),
        })?;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,litd_engine=info,litd_editor=info,litd_game=info")
    });
    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_ansi(false)
        .with_writer(Mutex::new(file))
        .try_init()
        .map_err(|error| EngineError::LoggingInvalidConfig {
            field: "tracing_subscriber".to_string(),
            reason: format!("failed to install file subscriber: {error}"),
        })?;
    Ok(log_path)
}

fn log_file_path(app_name: &str) -> Result<PathBuf, EngineError> {
    resolve_log_file_path(
        app_name,
        env::var("LITD_LOG_FILE").ok(),
        env::var("LOCALAPPDATA").ok(),
    )
}

/// Resolve the log path without reading process state, for deterministic tests.
fn resolve_log_file_path(
    app_name: &str,
    litd_log_file: Option<String>,
    local_app_data: Option<String>,
) -> Result<PathBuf, EngineError> {
    if app_name.trim().is_empty() {
        return Err(EngineError::LoggingInvalidConfig {
            field: "app_name".to_string(),
            reason: "app_name must be non-empty".to_string(),
        });
    }
    if let Some(path) = litd_log_file {
        let path = PathBuf::from(path);
        if path.as_os_str().is_empty() {
            return Err(EngineError::LoggingInvalidConfig {
                field: "LITD_LOG_FILE".to_string(),
                reason: "path must be non-empty".to_string(),
            });
        }
        return Ok(path);
    }
    let local_app_data = local_app_data.ok_or_else(|| EngineError::LoggingInvalidConfig {
        field: "LOCALAPPDATA".to_string(),
        reason: "LOCALAPPDATA is not set; cannot resolve the per-user log directory. It is standard on Windows; set it, or set LITD_LOG_FILE to an explicit absolute log path.".to_string(),
    })?;
    if local_app_data.trim().is_empty() {
        return Err(EngineError::LoggingInvalidConfig {
            field: "LOCALAPPDATA".to_string(),
            reason: "LOCALAPPDATA is empty; cannot resolve the per-user log directory. Set it to a valid path, or set LITD_LOG_FILE to an explicit absolute log path.".to_string(),
        });
    }
    Ok(PathBuf::from(local_app_data)
        .join("LitD")
        .join("logs")
        .join(format!("{app_name}.log")))
}

#[cfg(test)]
mod tests {
    use super::resolve_log_file_path;
    use std::path::PathBuf;

    #[test]
    fn defaults_to_localappdata_never_cwd_relative() {
        let path = resolve_log_file_path(
            "litd-editor",
            None,
            Some(r"C:\Users\tester\AppData\Local".to_string()),
        )
        .expect("resolves under LOCALAPPDATA");
        assert_eq!(
            path,
            PathBuf::from(r"C:\Users\tester\AppData\Local\LitD\logs\litd-editor.log")
        );
        assert!(
            path.is_absolute(),
            "default log path must be absolute: {path:?}"
        );
        assert!(
            !path.starts_with("tmp"),
            "default must not be cwd-relative: {path:?}"
        );
    }

    #[test]
    fn litd_log_file_override_wins_over_localappdata() {
        let path = resolve_log_file_path(
            "litd-editor",
            Some(r"D:\explicit\my.log".to_string()),
            Some(r"C:\Users\tester\AppData\Local".to_string()),
        )
        .expect("explicit override is honored");
        assert_eq!(path, PathBuf::from(r"D:\explicit\my.log"));
    }

    #[test]
    fn fails_loud_when_no_override_and_no_localappdata() {
        let err = resolve_log_file_path("litd-editor", None, None).unwrap_err();
        assert!(format!("{err}").contains("LOCALAPPDATA"));
    }

    #[test]
    fn empty_localappdata_fails_loud() {
        resolve_log_file_path("litd-editor", None, Some("   ".to_string())).unwrap_err();
    }

    #[test]
    fn empty_litd_log_file_fails_loud() {
        resolve_log_file_path("litd-editor", Some(String::new()), None).unwrap_err();
    }

    #[test]
    fn empty_app_name_fails_loud() {
        resolve_log_file_path("  ", None, Some(r"C:\x".to_string())).unwrap_err();
    }
}
