use std::{
    net::TcpListener,
    sync::Mutex,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::json;

use super::*;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn missing_calyx_home_disables_feature_before_tool_dispatch() {
    let _lock = ENV_LOCK.lock().unwrap();
    let _env = EnvGuard::unset();
    let probe = CalyxAvailabilityProbe::from_env("issue701-missing-home");
    let panel = build_calyx_feature_panel("issue701.ai", "Issue 701 AI", &probe).unwrap();

    assert!(!panel.enabled);
    assert_eq!(panel.reason_code, CALYX_MISSING_HOME);
    assert!(panel.disabled_reason.contains("AI features offline"));

    let error = call_calyx_mcp_tool_when_available(
        "issue701.ai",
        &probe,
        "calyx.create_vault",
        json!({"name": "must_not_exist"}),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains(CALYX_MISSING_HOME));
}

#[test]
fn readable_local_home_reenables_calyx_features() {
    let home = temp_path("issue701-online-home");
    std::fs::create_dir_all(&home).unwrap();
    let probe = CalyxAvailabilityProbe::local_home("issue701-online-home", &home);

    let readback = probe_calyx_availability(&probe).unwrap();

    assert!(readback.available);
    assert_eq!(readback.reason_code, CALYX_ONLINE);
    assert!(readback.home_exists);
    assert!(readback.home_is_dir);
    assert!(readback.home_readable);
    assert!(readback.tool_count >= 3);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn outside_calyx_home_fails_closed_before_tool_dispatch() {
    // #1559: a real absolute path OUTSIDE the canonical Calyx root, resolved from the primary
    // checkout at runtime (was a hardcoded machine-specific `scripts` literal). `scripts/` sits
    // beside `Calyx/`, so it is guaranteed outside the canonical root regardless of checkout path.
    let outside = litd_engine::asset_vault_root::resolve_repo_root()
        .expect("resolve primary checkout root for outside-Calyx-home probe")
        .join("scripts");
    let probe = CalyxAvailabilityProbe::local_home("issue1503-outside-home", &outside);

    let readback = probe_calyx_availability(&probe).unwrap();

    assert!(!readback.available);
    assert_eq!(
        readback.reason_code,
        litd_engine::calyx_paths::CALYX_PATH_OUTSIDE_ROOT
    );
    assert!(
        readback
            .reason
            .contains("outside canonical GameEditor Calyx root")
    );
}

#[test]
fn endpoint_probe_distinguishes_hard_down_from_timeout() {
    let home = temp_path("issue701-endpoints-home");
    std::fs::create_dir_all(&home).unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let hard_down_addr = listener.local_addr().unwrap();
    let hard_down_join = thread::spawn(move || {
        let _accepted = listener.accept();
    });
    let mut hard_down = CalyxAvailabilityProbe::local_home("issue701-hard-down", &home);
    hard_down.endpoint = Some(CalyxEndpointProbe {
        address: hard_down_addr.to_string(),
        require_initialize: true,
    });
    hard_down.timeout_ms = 50;
    let hard_down_readback = probe_calyx_availability(&hard_down).unwrap();
    hard_down_join.join().unwrap();
    assert!(!hard_down_readback.available);
    assert_eq!(hard_down_readback.reason_code, CALYX_ENDPOINT_HARD_DOWN);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let timeout_addr = listener.local_addr().unwrap();
    let join = thread::spawn(move || {
        let _accepted = listener.accept();
        thread::sleep(Duration::from_millis(250));
    });
    let mut timeout = CalyxAvailabilityProbe::local_home("issue701-timeout", &home);
    timeout.endpoint = Some(CalyxEndpointProbe {
        address: timeout_addr.to_string(),
        require_initialize: true,
    });
    timeout.timeout_ms = 50;
    let timeout_readback = probe_calyx_availability(&timeout).unwrap();
    join.join().unwrap();

    assert!(!timeout_readback.available);
    assert_eq!(timeout_readback.reason_code, CALYX_ENDPOINT_TIMEOUT);
    assert!(timeout_readback.endpoint.as_ref().unwrap().connected);
    std::fs::remove_dir_all(home).unwrap();
}

fn temp_path(name: &str) -> std::path::PathBuf {
    litd_engine::calyx_paths::canonical_gameeditor_calyx_root()
        .unwrap()
        .join("tmp")
        .join(format!(
            "litd-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
}

struct EnvGuard {
    old: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn unset() -> Self {
        let old = std::env::var_os("CALYX_HOME");
        unsafe {
            std::env::remove_var("CALYX_HOME");
        }
        Self { old }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.old {
                Some(value) => std::env::set_var("CALYX_HOME", value),
                None => std::env::remove_var("CALYX_HOME"),
            }
        }
    }
}
