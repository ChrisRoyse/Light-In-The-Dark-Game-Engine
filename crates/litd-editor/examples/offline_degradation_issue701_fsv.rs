use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use litd_editor::{
    mcp_object_tools::{
        APPLY_OBJECT_DELTA_TOOL, COPY_BASE_OBJECT_TOOL, call_mcp_object_tool,
        read_mcp_object_document,
    },
    object_editor::{
        issue732_base_document, lower_unit_from_document_path, object_editor_base_templates,
        read_lowered_unit_toml, verify_lowered_unit_toml, write_lowered_unit_toml,
    },
    offline_degradation::{
        CALYX_ENDPOINT_HARD_DOWN, CALYX_ENDPOINT_TIMEOUT, CALYX_HOME_NOT_DIRECTORY, CALYX_ONLINE,
        CalyxAvailabilityProbe, CalyxEndpointProbe, build_calyx_feature_panel,
        call_calyx_mcp_tool_when_available, probe_calyx_availability,
    },
};
use litd_engine::calyx_paths::canonical_gameeditor_calyx_root;
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_OFFLINE_DEGRADATION_ISSUE701_FSV_ROOT",
        "issue701_offline_degradation",
        "manual",
    )?;
    reset_root(&root)?;
    let calyx_fsv_root =
        canonical_gameeditor_calyx_root()?.join("tmp/fsv/issue701-offline-degradation");
    if calyx_fsv_root.exists() {
        fs::remove_dir_all(&calyx_fsv_root)?;
    }
    fs::create_dir_all(&calyx_fsv_root)?;

    let missing_home = calyx_fsv_root.join("missing-calyx-home");
    let offline_probe = CalyxAvailabilityProbe::local_home("issue701-offline", &missing_home);
    let offline_panel = build_calyx_feature_panel("issue701.ai", "Issue 701 AI", &offline_probe)?;
    if offline_panel.enabled || offline_panel.reason_code != CALYX_HOME_NOT_DIRECTORY {
        return Err("missing Calyx home did not disable AI feature panel".into());
    }

    let authoring = author_and_save_unit_offline(&root)?;
    let before_ai = read_mcp_object_document(root.join("object-document-aster"))?;
    let ai_error = call_calyx_mcp_tool_when_available(
        "issue701.ai",
        &offline_probe,
        "calyx.abundance",
        json!({"vault": "issue701_must_not_exist"}),
    )
    .unwrap_err()
    .to_string();
    let after_ai = read_mcp_object_document(root.join("object-document-aster"))?;
    let ai_document_unchanged = before_ai.document_fingerprint == after_ai.document_fingerprint;
    if !ai_document_unchanged || !ai_error.contains(CALYX_HOME_NOT_DIRECTORY) {
        return Err("offline AI action did not fail closed without document mutation".into());
    }

    let flap = flap_edge(&calyx_fsv_root)?;
    let hard_down = hard_down_edge(&calyx_fsv_root)?;
    let timeout = timeout_edge(&calyx_fsv_root)?;
    let reconnect = reconnect_edge(&calyx_fsv_root)?;

    let report_path = root.join("issue701-offline-degradation-fsv.json");
    let report = json!({
        "schema": "litd.editor.issue701.offline_degradation_fsv.v1",
        "issue": 701,
        "source_of_truth": {
            "local_authoring": "real MCP object document Aster rows + lowered unit TOML bytes",
            "calyx_disabled": "Calyx availability readback over CALYX_HOME metadata and MCP tool registry",
            "endpoint_edges": "loopback TCP/MCP initialize probes",
            "reconnect": "real calyx-mcp create_vault call plus CALYX_HOME/vaults/index.json disk readback"
        },
        "readme_refs_inspected": [
            "docs/readme/ED05-ai-coauthor.png",
            "docs/readme/ED03-object-editor.png"
        ],
        "offline_panel": offline_panel,
        "happy_path": {
            "authoring": authoring,
            "ai_disabled_error": ai_error,
            "ai_document_unchanged": ai_document_unchanged,
            "missing_home_vault_index_exists": missing_home.join("vaults").join("index.json").exists()
        },
        "edges": {
            "endpoint_flap": flap,
            "hard_down": hard_down,
            "timeout": timeout,
            "reconnect": reconnect
        }
    });
    let written = fsv_run.write_json_file(&report_path, &report)?;
    let disk: Value = serde_json::from_slice(&fs::read(&report_path)?)?;
    if disk != report {
        return Err("issue701 report readback mismatch".into());
    }
    fs::remove_dir_all(&calyx_fsv_root)?;
    println!(
        "ISSUE701_OFFLINE_DEGRADATION_FSV report={} sha256={} unit_toml_sha256={} document_fingerprint={}",
        written.path.display(),
        written.sha256,
        authoring["unit_toml_sha256"].as_str().unwrap_or("missing"),
        authoring["document_fingerprint"]
            .as_str()
            .unwrap_or("missing")
    );
    Ok(())
}

fn author_and_save_unit_offline(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let document_path = root.join("object-document-aster");
    let copy_ledger = root.join("copy-ledger-aster");
    let delta_ledger = root.join("delta-ledger-aster");
    let unit_toml_path = root.join("issue701-offline-unit.toml");
    let unit_id = "unit.issue701.offline_guardian";
    let seed = issue732_base_document();
    let footman = object_editor_base_templates(&seed)?
        .into_iter()
        .find(|template| template.base_object_id == "base.unit.footman")
        .ok_or("missing footman base template")?;
    call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": copy_ledger,
            "ledger_id": "issue701-copy-ledger",
            "request_id": "issue701-copy",
            "requested_by": "litd-editor-issue701-fsv",
            "initial_document": seed,
            "new_object_id": unit_id,
            "rawcode": "u701",
            "base": footman,
            "overrides": {}
        }),
    )?;
    call_mcp_object_tool(
        APPLY_OBJECT_DELTA_TOOL,
        json!({
            "document_path": document_path,
            "ledger_path": delta_ledger,
            "ledger_id": "issue701-delta-ledger",
            "request_id": "issue701-delta",
            "requested_by": "litd-editor-issue701-fsv",
            "initial_document": null,
            "object_id": unit_id,
            "delta": {
                "set_kind": null,
                "upsert_properties": {
                    "stats.move_speed": "301",
                    "abilities.normal": "ability.issue732.defend,ability.issue732.guardian_leap"
                },
                "remove_properties": []
            }
        }),
    )?;
    let document_readback = read_mcp_object_document(&document_path)?;
    let lowered = lower_unit_from_document_path(&document_path, unit_id)?;
    let toml_write = write_lowered_unit_toml(&unit_toml_path, &lowered)?;
    let toml_read = read_lowered_unit_toml(&unit_toml_path, &lowered.fingerprint)?;
    let verified = verify_lowered_unit_toml(&lowered.canonical_toml, &lowered.fingerprint)?;
    if toml_read.document.unit.move_speed != 301
        || !toml_read
            .document
            .unit
            .abilities
            .contains(&"ability.issue732.guardian_leap".to_string())
    {
        return Err("offline unit TOML readback did not match expected authored state".into());
    }
    Ok(json!({
        "document_path": document_path,
        "document_sha256": document_readback.sha256,
        "document_fingerprint": document_readback.document_fingerprint,
        "document_unit_count": document_readback.unit_count,
        "unit_toml_path": unit_toml_path,
        "unit_toml_sha256": toml_write.sha256,
        "unit_toml_bytes": toml_write.bytes,
        "toml_roundtrip_same": toml_write == toml_read,
        "verified_move_speed": verified.unit.move_speed,
        "verified_abilities": verified.unit.abilities
    }))
}

fn flap_edge(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let home = root.join("flap-calyx-home");
    fs::create_dir_all(&home)?;
    let probe = CalyxAvailabilityProbe::local_home("issue701-flap", &home);
    let online_before = probe_calyx_availability(&probe)?;
    fs::remove_dir_all(&home)?;
    let offline_mid = probe_calyx_availability(&probe)?;
    fs::create_dir_all(&home)?;
    let online_after = probe_calyx_availability(&probe)?;
    if online_before.reason_code != CALYX_ONLINE
        || offline_mid.reason_code != CALYX_HOME_NOT_DIRECTORY
        || online_after.reason_code != CALYX_ONLINE
    {
        return Err("flap edge did not produce online/offline/online readback".into());
    }
    Ok(json!({
        "before": online_before,
        "during": offline_mid,
        "after": online_after
    }))
}

fn hard_down_edge(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let home = root.join("endpoint-calyx-home");
    fs::create_dir_all(&home)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let join = thread::spawn(move || {
        let _accepted = listener.accept();
    });
    let mut probe = CalyxAvailabilityProbe::local_home("issue701-hard-down", &home);
    probe.endpoint = Some(CalyxEndpointProbe {
        address: addr.to_string(),
        require_initialize: true,
    });
    probe.timeout_ms = 50;
    let readback = probe_calyx_availability(&probe)?;
    join.join().map_err(|_| "hard-down listener panicked")?;
    if readback.reason_code != CALYX_ENDPOINT_HARD_DOWN {
        return Err(format!("expected hard-down endpoint, got {}", readback.reason_code).into());
    }
    Ok(serde_json::to_value(readback)?)
}

fn timeout_edge(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let home = root.join("timeout-calyx-home");
    fs::create_dir_all(&home)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let join = thread::spawn(move || {
        let _accepted = listener.accept();
        thread::sleep(Duration::from_millis(250));
    });
    let mut probe = CalyxAvailabilityProbe::local_home("issue701-timeout", &home);
    probe.endpoint = Some(CalyxEndpointProbe {
        address: addr.to_string(),
        require_initialize: true,
    });
    probe.timeout_ms = 50;
    let readback = probe_calyx_availability(&probe)?;
    join.join().map_err(|_| "timeout listener panicked")?;
    if readback.reason_code != CALYX_ENDPOINT_TIMEOUT {
        return Err(format!("expected timeout endpoint, got {}", readback.reason_code).into());
    }
    Ok(serde_json::to_value(readback)?)
}

fn reconnect_edge(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let home = root.join("reconnect-calyx-home");
    fs::create_dir_all(&home)?;
    let _env = EnvGuard::set(&home);
    let probe = CalyxAvailabilityProbe::from_env("issue701-reconnect");
    let created = call_calyx_mcp_tool_when_available(
        "issue701.reconnect",
        &probe,
        "calyx.create_vault",
        json!({"name": "issue701_reconnect"}),
    )?;
    if created.response.error.is_some() {
        return Err(format!(
            "reconnect create_vault returned error: {:?}",
            created.response
        )
        .into());
    }
    let index_path = home.join("vaults").join("index.json");
    let index_bytes = fs::read(&index_path)?;
    let index: Value = serde_json::from_slice(&index_bytes)?;
    let contains = index["vaults"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .any(|entry| entry["name"] == "issue701_reconnect");
    if !contains {
        return Err("reconnect vault index did not contain issue701_reconnect".into());
    }
    Ok(json!({
        "availability": created.availability,
        "response": created.response,
        "index_path": index_path,
        "index_sha256": litd_hash::sha256_hex(&index_bytes),
        "index_contains_issue701_reconnect": contains
    }))
}

fn reset_root(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    fs::create_dir_all(root)?;
    Ok(())
}

struct EnvGuard {
    old: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set(path: &Path) -> Self {
        let old = std::env::var_os("CALYX_HOME");
        unsafe {
            std::env::set_var("CALYX_HOME", path);
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

#[allow(dead_code)]
fn nonce_path(root: &Path, name: &str) -> PathBuf {
    root.join(format!(
        "{name}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
