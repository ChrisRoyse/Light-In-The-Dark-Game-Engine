use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use litd_editor::{
    history::stable_fingerprint,
    mcp_tools::{
        EngineCalyxMcpCallReadback, EngineCalyxMcpSurfaceManifest, EngineCalyxMcpSurfaceReport,
        build_engine_calyx_mcp_surface_report, call_engine_calyx_mcp_tool,
        default_engine_calyx_mcp_surface_manifest, readback_engine_calyx_mcp_tool_call,
    },
};
use litd_engine::calyx_paths::canonical_gameeditor_calyx_root;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CalyxMcpSurfaceFsv {
    issue: u32,
    source_of_truth: String,
    manifest: EngineCalyxMcpSurfaceManifest,
    report: EngineCalyxMcpSurfaceReport,
    happy_path: HappyPath,
    edge_cases: Vec<EdgeCase>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct HappyPath {
    tool_name: String,
    arguments: Value,
    before_state: Value,
    create_response: Value,
    after_create_state: Value,
    engine_source_readback: EngineCalyxMcpCallReadback,
    engine_payload: Value,
    cli_payload: Value,
    cli_parity: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct EdgeCase {
    name: String,
    trigger: Value,
    before_state: Value,
    after_state: Value,
    response: Value,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, _fsv_root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_CALYX_MCP_SURFACE_FSV_ROOT",
        "issue1201_calyx_mcp_surface",
        "manual",
    )?;
    let artifact_relative_path =
        env::var("LITD_CALYX_MCP_SURFACE_FSV").unwrap_or_else(|_| "manual-fsv.json".to_string());
    let artifact_path = fsv_run.artifact_path(&artifact_relative_path)?;
    if let Some(parent) = artifact_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let manifest = default_engine_calyx_mcp_surface_manifest()?;
    let report = build_engine_calyx_mcp_surface_report(&manifest)?;
    let calyx_fsv_root =
        canonical_gameeditor_calyx_root()?.join("tmp/fsv/issue1201-calyx-mcp-surface");
    if calyx_fsv_root.exists() {
        fs::remove_dir_all(&calyx_fsv_root)?;
    }
    fs::create_dir_all(&calyx_fsv_root)?;
    let happy_home = calyx_fsv_root.join("happy-calyx-home");
    if happy_home.exists() {
        fs::remove_dir_all(&happy_home)?;
    }
    fs::create_dir_all(&happy_home)?;
    let _happy_env = CalyxHomeScope::set(&happy_home);
    let before_state = capture_state(Some(&happy_home))?;
    let create_response = call_engine_calyx_mcp_tool(
        "calyx.create_vault",
        json!({
            "name": "issue1201_happy"
        }),
    )?;
    let create_response_value = serde_json::to_value(&create_response)?;
    if create_response.error.is_some() {
        return Err(format!("create happy-path vault failed: {create_response_value}").into());
    }
    let after_create_state = capture_state(Some(&happy_home))?;
    let arguments = json!({
        "vault": "issue1201_happy"
    });
    let engine_source_readback =
        readback_engine_calyx_mcp_tool_call("calyx.abundance", arguments.clone())?;
    let engine_payload = tool_payload(&engine_source_readback.engine_response)?;
    let cli_payload = run_calyx_cli_abundance(&happy_home, "issue1201_happy")?;
    let cli_parity = engine_payload == cli_payload;
    if !cli_parity {
        return Err("calyx.abundance engine payload did not match calyx CLI payload".into());
    }

    let edge_cases = vec![
        invalid_arguments_edge()?,
        missing_vault_edge(&calyx_fsv_root)?,
        unknown_tool_edge()?,
    ];
    let fsv = CalyxMcpSurfaceFsv {
        issue: 1201,
        source_of_truth:
            "real calyx-mcp tools/list, engine loopback tools/call, persisted Calyx vault files, and independent calyx CLI abundance stdout"
                .to_string(),
        manifest,
        report,
        happy_path: HappyPath {
            tool_name: "calyx.abundance".to_string(),
            arguments,
            before_state,
            create_response: create_response_value,
            after_create_state,
            engine_source_readback,
            engine_payload,
            cli_payload,
            cli_parity,
        },
        edge_cases,
    };

    write_json(&artifact_path, &fsv)?;
    let readback = read_json::<CalyxMcpSurfaceFsv>(&artifact_path)?;
    if readback != fsv {
        return Err(format!("artifact readback mismatch at {}", artifact_path.display()).into());
    }
    let report_readback = fsv_run.record_json_file(&artifact_path)?;
    drop(_happy_env);
    fs::remove_dir_all(&calyx_fsv_root)?;
    println!(
        "{}",
        json!({
            "issue": 1201,
            "artifact_path": artifact_path,
            "artifact_sha256": report_readback.sha256,
            "tool_count": fsv.report.tool_count,
            "mutating_tool_count": fsv.report.mutating_tool_count,
            "readback_tool_count": fsv.report.readback_tool_count,
            "happy_tool": fsv.happy_path.tool_name,
            "loopback_surface": fsv.happy_path.engine_source_readback.surface,
            "cli_parity": fsv.happy_path.cli_parity,
            "edge_case_count": fsv.edge_cases.len(),
            "surface_fingerprint": fsv.report.surface_fingerprint,
            "artifact_readback_matches": true
        })
    );
    Ok(())
}

fn invalid_arguments_edge() -> Result<EdgeCase, Box<dyn std::error::Error>> {
    let before = capture_state(None)?;
    let response = call_engine_calyx_mcp_tool("calyx.profile_lens", json!({}))?;
    let after = capture_state(None)?;
    let response_value = serde_json::to_value(response)?;
    assert_error_code(&response_value, -32602)?;
    Ok(EdgeCase {
        name: "invalid_arguments".to_string(),
        trigger: json!({
            "tool": "calyx.profile_lens",
            "arguments": {}
        }),
        before_state: before,
        after_state: after,
        response: response_value,
    })
}

fn missing_vault_edge(fsv_root: &Path) -> Result<EdgeCase, Box<dyn std::error::Error>> {
    let home = fsv_root.join("missing-vault-calyx-home");
    if home.exists() {
        fs::remove_dir_all(&home)?;
    }
    fs::create_dir_all(&home)?;
    let _env = CalyxHomeScope::set(&home);
    let before = capture_state(Some(&home))?;
    let response = call_engine_calyx_mcp_tool(
        "calyx.list_panel",
        json!({
            "vault": "issue1201_missing_vault"
        }),
    )?;
    let after = capture_state(Some(&home))?;
    let response_value = serde_json::to_value(response)?;
    assert_error_code(&response_value, -32000)?;
    if response_value["error"]["data"]["calyx_code"] != "CALYX_VAULT_ACCESS_DENIED" {
        return Err("missing vault did not return CALYX_VAULT_ACCESS_DENIED".into());
    }
    Ok(EdgeCase {
        name: "vault_not_found".to_string(),
        trigger: json!({
            "tool": "calyx.list_panel",
            "arguments": {
                "vault": "issue1201_missing_vault"
            }
        }),
        before_state: before,
        after_state: after,
        response: response_value,
    })
}

fn unknown_tool_edge() -> Result<EdgeCase, Box<dyn std::error::Error>> {
    let before = capture_state(None)?;
    let response = call_engine_calyx_mcp_tool("calyx.not_registered", json!({}))?;
    let after = capture_state(None)?;
    let response_value = serde_json::to_value(response)?;
    assert_error_code(&response_value, -32601)?;
    Ok(EdgeCase {
        name: "unknown_tool".to_string(),
        trigger: json!({
            "tool": "calyx.not_registered",
            "arguments": {}
        }),
        before_state: before,
        after_state: after,
        response: response_value,
    })
}

fn capture_state(calyx_home: Option<&Path>) -> Result<Value, Box<dyn std::error::Error>> {
    let manifest = default_engine_calyx_mcp_surface_manifest()?;
    let tool_names = manifest
        .tools
        .iter()
        .map(|tool| tool.tool_name.as_str())
        .collect::<Vec<_>>();
    let home_state = calyx_home.map(|home| {
        let index = home.join("vaults").join("index.json");
        let vault_root = home.join("vaults");
        json!({
            "calyx_home": home,
            "home_exists": home.exists(),
            "vault_root_exists": vault_root.exists(),
            "vault_index_exists": index.exists(),
            "vault_index_bytes": fs::metadata(&index).map(|metadata| metadata.len()).ok(),
            "vault_dir_count": fs::read_dir(&vault_root)
                .map(|entries| entries.filter_map(Result::ok).count())
                .unwrap_or(0)
        })
    });
    Ok(json!({
        "registry_tool_count": manifest.tools.len(),
        "registry_fingerprint": stable_fingerprint(&tool_names)?,
        "calyx_home_state": home_state
    }))
}

fn run_calyx_cli_abundance(
    calyx_home: &Path,
    vault: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let root = workspace_root();
    let output = Command::new("cargo")
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(root.join("Calyx").join("Cargo.toml"))
        .arg("-p")
        .arg("calyx-cli")
        .arg("--")
        .arg("abundance")
        .arg(vault)
        .env("CALYX_HOME", calyx_home)
        .current_dir(&root)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "calyx CLI abundance failed: status={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "decode calyx CLI abundance stdout as JSON: {error}; stdout={}",
            String::from_utf8_lossy(&output.stdout)
        )
        .into()
    })
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("litd-editor is under crates/litd-editor")
        .to_path_buf()
}

fn tool_payload(
    response: &calyx_mcp::JsonRpcResponse,
) -> Result<Value, Box<dyn std::error::Error>> {
    let text = response
        .result
        .as_ref()
        .and_then(|result| result["content"][0]["text"].as_str())
        .ok_or("tool response did not contain text content")?;
    Ok(serde_json::from_str(text)?)
}

fn assert_error_code(response: &Value, expected: i64) -> Result<(), Box<dyn std::error::Error>> {
    let actual = response["error"]["code"]
        .as_i64()
        .ok_or("response did not contain JSON-RPC error.code")?;
    if actual != expected {
        return Err(format!("expected JSON-RPC error code {expected}, got {actual}").into());
    }
    if !response["result"].is_null() {
        return Err("error response unexpectedly contained result".into());
    }
    Ok(())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

struct CalyxHomeScope {
    old: Option<std::ffi::OsString>,
}

impl CalyxHomeScope {
    fn set(home: &Path) -> Self {
        let old = env::var_os("CALYX_HOME");
        unsafe {
            env::set_var("CALYX_HOME", home);
        }
        Self { old }
    }
}

impl Drop for CalyxHomeScope {
    fn drop(&mut self) {
        unsafe {
            match &self.old {
                Some(value) => env::set_var("CALYX_HOME", value),
                None => env::remove_var("CALYX_HOME"),
            }
        }
    }
}
