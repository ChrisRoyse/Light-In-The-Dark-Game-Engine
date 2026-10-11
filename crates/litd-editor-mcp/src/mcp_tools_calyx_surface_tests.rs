use super::*;
#[cfg(feature = "calyx-live-calls")]
use serde_json::{Value, json};
#[cfg(feature = "calyx-live-calls")]
use std::sync::Mutex;

#[cfg(feature = "calyx-live-calls")]
static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn calyx_surface_manifest_matches_live_registry() {
    let manifest = default_engine_calyx_mcp_surface_manifest().unwrap();
    let report = build_engine_calyx_mcp_surface_report(&manifest).unwrap();
    let expected = expected_tool_vec().unwrap();
    let mutating_count = manifest
        .tools
        .iter()
        .filter(|tool| tool.requires_authn)
        .count();

    assert_eq!(report.parent_issue, 1201);
    assert_eq!(report.tool_count, expected.len());
    assert_eq!(report.expected_tool_count, expected.len());
    assert_eq!(report.mutating_tool_count, mutating_count);
    assert_eq!(report.readback_tool_count, expected.len() - mutating_count);
    assert_eq!(report.local_only_count, expected.len());
    assert_eq!(report.jsonrpc_error_preserving_count, expected.len());
    assert_eq!(manifest.expected_tools, expected);
    assert_eq!(
        manifest
            .tools
            .iter()
            .map(|tool| tool.tool_name.as_str())
            .collect::<Vec<_>>(),
        manifest
            .expected_tools
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    assert!(!report.source_tool_fingerprint.is_empty());
    assert!(!report.surface_fingerprint.is_empty());
}

#[test]
fn calyx_surface_validation_rejects_duplicate_tool_name() {
    let mut manifest = default_engine_calyx_mcp_surface_manifest().unwrap();
    println!(
        "EDGE duplicate before tool_count={} unique_expected={}",
        manifest.tools.len(),
        manifest.expected_tools.len()
    );
    manifest.tools[1].tool_name = manifest.tools[0].tool_name.clone();
    manifest.tools[1].source_tool_name = manifest.tools[0].source_tool_name.clone();
    let (field, reason) = invalid_parts(validate_engine_calyx_mcp_surface_manifest(&manifest));
    println!("EDGE duplicate after field={field} reason={reason}");
    assert_eq!(field, "editor.mcp_tools.calyx_surface.tool.name");
    assert!(reason.contains("duplicate"));
}

#[test]
fn calyx_surface_validation_rejects_missing_expected_tool() {
    let mut manifest = default_engine_calyx_mcp_surface_manifest().unwrap();
    let removed = manifest.tools.pop().unwrap();
    println!(
        "EDGE missing before removed={} remaining={}",
        removed.tool_name,
        manifest.tools.len()
    );
    let (field, reason) = invalid_parts(validate_engine_calyx_mcp_surface_manifest(&manifest));
    println!("EDGE missing after field={field} reason={reason}");
    assert_eq!(field, "editor.mcp_tools.calyx_surface.expected_tools");
    assert!(reason.contains(&removed.tool_name));
}

#[test]
fn calyx_surface_validation_rejects_incomplete_descriptor() {
    let mut manifest = default_engine_calyx_mcp_surface_manifest().unwrap();
    println!(
        "EDGE descriptor before tool={} description_len={}",
        manifest.tools[0].tool_name,
        manifest.tools[0].source_description.len()
    );
    manifest.tools[0].source_description.clear();
    let (field, reason) = invalid_parts(validate_engine_calyx_mcp_surface_manifest(&manifest));
    println!("EDGE descriptor after field={field} reason={reason}");
    assert_eq!(field, "editor.mcp_tools.calyx_surface.tool.descriptor");
    assert!(reason.contains(&manifest.tools[0].tool_name));
}

#[test]
fn calyx_surface_validation_rejects_authn_drift() {
    let mut manifest = default_engine_calyx_mcp_surface_manifest().unwrap();
    let index = manifest
        .tools
        .iter()
        .position(|tool| tool.requires_authn)
        .unwrap();
    println!(
        "EDGE authn before tool={} requires_authn={}",
        manifest.tools[index].tool_name, manifest.tools[index].requires_authn
    );
    manifest.tools[index].requires_authn = false;
    let (field, reason) = invalid_parts(validate_engine_calyx_mcp_surface_manifest(&manifest));
    println!("EDGE authn after field={field} reason={reason}");
    assert_eq!(field, "editor.mcp_tools.calyx_surface.tool.authn");
    assert!(reason.contains(&manifest.tools[index].tool_name));
}

#[test]
fn calyx_surface_authn_metadata_rejects_unknown_tool() {
    let unknown = "calyx.not_registered";
    println!(
        "EDGE unknown-authn before tool={} present_in_expected={}",
        unknown,
        expected_tool_vec().unwrap().contains(&unknown.to_string())
    );
    let (field, reason) = invalid_parts(requires_authn(unknown));
    println!("EDGE unknown-authn after field={field} reason={reason}");
    assert_eq!(field, "editor.mcp_tools.calyx_surface.tool.authn");
    assert!(reason.contains(unknown));
}

#[test]
#[cfg(feature = "calyx-live-calls")]
fn abundance_readback_returns_real_engine_response() {
    // This is a SINGLE in-process loopback surface, so there is no vacuous self-parity to
    // assert here (#1533). We prove the readback carries the genuine engine response and a
    // real fingerprint; cross-implementation parity is exercised against the independent
    // calyx-cli in the calyx_mcp_surface_fsv example, not here.
    let _guard = EnvGuard::set_calyx_home("abundance-parity");
    let created = call_engine_calyx_mcp_tool(
        "calyx.create_vault",
        json!({
            "name": "issue1201_abundance"
        }),
    )
    .unwrap();
    assert!(created.error.is_none());

    let readback = readback_engine_calyx_mcp_tool_call(
        "calyx.abundance",
        json!({
            "vault": "issue1201_abundance"
        }),
    )
    .unwrap();

    assert_eq!(readback.surface, "calyx-mcp-loopback");
    let payload = tool_payload(&readback.engine_response);
    assert_eq!(payload["n"], 0);
    assert_eq!(payload["materialized"], 0);
    assert!(payload["panel_size"].as_u64().unwrap_or(0) > 0);
    assert!(!readback.response_fingerprint.is_empty());
}

#[test]
#[cfg(feature = "calyx-live-calls")]
fn invalid_arguments_fail_loud_as_jsonrpc_invalid_params() {
    let response = call_engine_calyx_mcp_tool("calyx.profile_lens", json!({})).unwrap();
    let error = response.error.unwrap();

    assert_eq!(error.code, -32602);
    assert!(
        error
            .message
            .contains("calyx.profile_lens invalid arguments")
    );
    assert!(error.data.is_none());
}

#[test]
#[cfg(feature = "calyx-live-calls")]
fn missing_vault_returns_structured_calyx_error() {
    let _guard = EnvGuard::set_calyx_home("missing-vault");
    let response = call_engine_calyx_mcp_tool(
        "calyx.list_panel",
        json!({
            "vault": "missing_issue1201_vault"
        }),
    )
    .unwrap();
    let error = response.error.unwrap();
    let data = error.data.unwrap();

    assert_eq!(error.code, -32000);
    assert_eq!(data["calyx_code"], "CALYX_VAULT_ACCESS_DENIED");
    assert!(error.message.contains("missing_issue1201_vault"));
    assert!(!data["remediation"].as_str().unwrap().is_empty());
}

#[test]
#[cfg(feature = "calyx-live-calls")]
fn mutating_parity_readback_is_rejected_before_calling_tool() {
    let _guard = EnvGuard::set_calyx_home("mutating-replay");
    let err = readback_engine_calyx_mcp_tool_call(
        "calyx.create_vault",
        json!({
            "name": "should_not_be_created"
        }),
    )
    .unwrap_err();

    match err {
        EditorMcpError::InvalidConfig { field, reason } => {
            assert_eq!(field, "editor.mcp_tools.calyx_surface.replay");
            assert!(reason.contains("mutates or authenticates state"));
        }
        other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
    }
    let home = std::env::var_os("CALYX_HOME").unwrap();
    assert!(
        !std::path::PathBuf::from(home)
            .join("vaults")
            .join("index.json")
            .exists()
    );
}

#[cfg(feature = "calyx-live-calls")]
fn tool_payload(response: &JsonRpcResponse) -> Value {
    let text = response.result.as_ref().unwrap()["content"][0]["text"]
        .as_str()
        .unwrap();
    serde_json::from_str(text).unwrap()
}

#[cfg(feature = "calyx-live-calls")]
struct EnvGuard {
    old: Option<std::ffi::OsString>,
    home: std::path::PathBuf,
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(feature = "calyx-live-calls")]
impl EnvGuard {
    fn set_calyx_home(name: &str) -> Self {
        let lock = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let old = std::env::var_os("CALYX_HOME");
        let home = litd_engine::calyx_paths::canonical_gameeditor_calyx_root()
            .unwrap()
            .join("tmp")
            .join("tests")
            .join(format!("litd-calyx-surface-{name}-{}", std::process::id()));
        if home.exists() {
            std::fs::remove_dir_all(&home).unwrap();
        }
        std::fs::create_dir_all(&home).unwrap();
        unsafe {
            std::env::set_var("CALYX_HOME", &home);
        }
        Self {
            old,
            home,
            _lock: lock,
        }
    }
}

#[cfg(feature = "calyx-live-calls")]
impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.old {
                Some(value) => std::env::set_var("CALYX_HOME", value),
                None => std::env::remove_var("CALYX_HOME"),
            }
        }
        std::fs::remove_dir_all(&self.home).ok();
    }
}

fn invalid_parts<T>(result: Result<T, EditorMcpError>) -> (String, String) {
    let error = match result {
        Ok(_) => panic!("expected InvalidConfig error"),
        Err(error) => error,
    };
    match error {
        EditorMcpError::InvalidConfig { field, reason } => (field, reason),
        other => panic!("expected InvalidConfig, got {other:?}"),
    }
}

#[test]
fn issue1461_source_server_is_built_once_per_thread() {
    // The calyx-mcp source server used to be rebuilt (full register_all) on every
    // source_server() call — twice per parity readback. It is now a thread_local Rc cache
    // (#1461 item 5). Two calls on the same thread must hand back the SAME instance, proving
    // register_all ran once, not per call.
    let first = source_server().expect("source server builds");
    let second = source_server().expect("cached source server");
    assert!(
        std::rc::Rc::ptr_eq(&first, &second),
        "source_server() must return the cached instance, not a freshly registered server"
    );
    assert!(
        first.tool_count() > 0,
        "the cached server must have registered tools"
    );
}
