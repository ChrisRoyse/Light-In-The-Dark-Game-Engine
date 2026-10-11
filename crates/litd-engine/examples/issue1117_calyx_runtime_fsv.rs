use std::{fs, io, path::PathBuf, process};

use litd_engine::{
    calyx_runtime::{
        CALYX_RUNTIME_VRAM_BUDGET_CEILING_MIB, ENGINE_CALYX_RUNTIME_READBACK_SCHEMA,
        ENGINE_CALYX_RUNTIME_SCHEMA, EngineCalyxRuntime, EngineCalyxRuntimeConfig,
        EngineCalyxRuntimeError,
    },
    fsv_artifacts::begin_issue_run,
};
use litd_hash::sha256_hex;
use serde_json::{Value, json};

const EXPECTED_CRATES: [&str; 8] = [
    "calyx-core",
    "calyx-aster",
    "calyx-ledger",
    "calyx-assay",
    "calyx-loom",
    "calyx-oracle",
    "calyx-anneal",
    "calyx-forge",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1117_calyx_runtime")?;

    let mut in_process = EngineCalyxRuntime::configure(EngineCalyxRuntimeConfig::in_process(
        "constellation-24",
        50,
        16_384,
        13_500,
    ))?;
    let configured = in_process.readback();
    let started = in_process.start()?;
    let warm_loaded = in_process.warm_load()?;
    let stopped = in_process.stop();
    let in_process_states = [
        configured.lifecycle_state.as_str(),
        started.lifecycle_state.as_str(),
        warm_loaded.lifecycle_state.as_str(),
        stopped.lifecycle_state.as_str(),
    ];

    let loopback_ipv4 = loopback_start("http://127.0.0.1:7766")?;
    let loopback_ipv6 = loopback_start("https://[::1]:7766")?;
    let edges = edge_readbacks();
    let source_files = source_file_readbacks()?;

    let expected_crates_present = EXPECTED_CRATES.iter().all(|name| {
        warm_loaded
            .linked_calyx_crates
            .iter()
            .any(|item| item == name)
    });
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let all_lifecycle_states_match =
        in_process_states == ["configured", "started", "warm_loaded", "stopped"];
    let in_process_acceptance = all_lifecycle_states_match
        && warm_loaded.transport == "in_process"
        && warm_loaded.process_boundary == "rust_crate_in_process"
        && !warm_loaded.per_call_process_allowed
        && warm_loaded.warm_load_lenses == 50
        && warm_loaded.warm_load_vram_mib == 13_500
        && warm_loaded.vram_budget_mib == 16_384
        && expected_crates_present;
    let loopback_acceptance = loopback_ipv4.transport == "loopback"
        && loopback_ipv4.process_boundary == "long_lived_loopback_service"
        && !loopback_ipv4.per_call_process_allowed
        && loopback_ipv6.transport == "loopback"
        && loopback_ipv6.process_boundary == "long_lived_loopback_service"
        && !loopback_ipv6.per_call_process_allowed;

    ensure(
        in_process_acceptance,
        "in-process runtime acceptance failed",
    )?;
    ensure(loopback_acceptance, "loopback runtime acceptance failed")?;
    ensure(all_edges_matched, "runtime edge readback acceptance failed")?;

    let report = json!({
        "schema": "litd.engine.issue1117.calyx_runtime_fsv.v1",
        "schema_version": 1,
        "issue": 1117,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "current_process_id": process::id(),
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON, manifest, source-file hashes, and Synapse health"
        },
        "runtime_contract": {
            "runtime_schema": ENGINE_CALYX_RUNTIME_SCHEMA,
            "readback_schema": ENGINE_CALYX_RUNTIME_READBACK_SCHEMA,
            "vram_ceiling_mib": CALYX_RUNTIME_VRAM_BUDGET_CEILING_MIB,
            "expected_linked_calyx_crates": EXPECTED_CRATES,
            "per_call_process_policy": "forbidden",
            "loopback_policy": "literal 127.0.0.1 or [::1] only; long-lived service boundary"
        },
        "happy_path": {
            "in_process_lifecycle": [
                {"step": "configure", "readback": configured},
                {"step": "start", "readback": started},
                {"step": "warm_load", "readback": warm_loaded},
                {"step": "stop", "readback": stopped}
            ],
            "loopback_ipv4_start": loopback_ipv4,
            "loopback_ipv6_start": loopback_ipv6,
            "source_files": source_files
        },
        "acceptance": {
            "in_process_states": in_process_states,
            "all_lifecycle_states_match": all_lifecycle_states_match,
            "expected_crates_present": expected_crates_present,
            "linked_crate_count": warm_loaded.linked_calyx_crates.len(),
            "in_process_boundary": warm_loaded.process_boundary,
            "in_process_per_call_process_allowed": warm_loaded.per_call_process_allowed,
            "warm_load_lenses": warm_loaded.warm_load_lenses,
            "warm_load_vram_mib": warm_loaded.warm_load_vram_mib,
            "vram_budget_mib": warm_loaded.vram_budget_mib,
            "loopback_ipv4_boundary": loopback_ipv4.process_boundary,
            "loopback_ipv6_boundary": loopback_ipv6.process_boundary,
            "loopback_acceptance": loopback_acceptance,
            "all_edges_matched": all_edges_matched
        },
        "edges": edges
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1117_CALYX_RUNTIME_FSV report={} sha256={} manifest={} run_id={} states={} edges={}",
        readback.path.display(),
        readback.sha256,
        readback.latest_manifest_path.display(),
        run.run_id(),
        in_process_states.join(","),
        edges.len()
    );
    Ok(())
}

fn loopback_start(
    endpoint: &str,
) -> Result<litd_engine::calyx_runtime::EngineCalyxRuntimeReadback, EngineCalyxRuntimeError> {
    let mut runtime = EngineCalyxRuntime::configure(EngineCalyxRuntimeConfig::loopback(
        endpoint,
        "constellation-24",
        50,
        8192,
        4096,
    ))?;
    runtime.start()
}

fn edge_readbacks() -> Vec<Value> {
    let mut per_call = EngineCalyxRuntimeConfig::in_process("constellation-24", 50, 8192, 4096);
    per_call.per_call_process_allowed = true;
    vec![
        invalid_config_edge(
            "per_call_process_rejected",
            per_call,
            "per_call_process_allowed",
            "forbidden",
        ),
        invalid_config_edge(
            "non_loopback_endpoint_rejected",
            EngineCalyxRuntimeConfig::loopback(
                "http://192.168.1.5:7766",
                "constellation-24",
                50,
                8192,
                4096,
            ),
            "transport.endpoint",
            "127.0.0.1",
        ),
        invalid_config_edge(
            "empty_loopback_endpoint_rejected",
            EngineCalyxRuntimeConfig::loopback("", "constellation-24", 50, 8192, 4096),
            "transport.endpoint",
            "non-empty",
        ),
        invalid_config_edge(
            "empty_panel_template_rejected",
            EngineCalyxRuntimeConfig::in_process(" ", 50, 8192, 4096),
            "panel_template",
            "non-empty",
        ),
        invalid_config_edge(
            "zero_vram_budget_rejected",
            EngineCalyxRuntimeConfig::in_process("constellation-24", 50, 0, 0),
            "vram_budget_mib",
            "1..=30000",
        ),
        invalid_config_edge(
            "over_ceiling_vram_budget_rejected",
            EngineCalyxRuntimeConfig::in_process("constellation-24", 50, 30_001, 4096),
            "vram_budget_mib",
            "1..=30000",
        ),
        invalid_config_edge(
            "warm_load_vram_over_budget_rejected",
            EngineCalyxRuntimeConfig::in_process("constellation-24", 50, 8192, 9000),
            "warm_load_vram_mib",
            "must not exceed",
        ),
        warm_load_before_start_edge(),
        start_twice_edge(),
        zero_lens_warm_load_edge(),
    ]
}

fn invalid_config_edge(
    label: &str,
    config: EngineCalyxRuntimeConfig,
    expected_field: &str,
    expected_reason: &str,
) -> Value {
    match EngineCalyxRuntime::configure(config) {
        Err(EngineCalyxRuntimeError::InvalidConfig { field, reason }) => json!({
            "label": label,
            "matched": field == expected_field && reason.contains(expected_reason),
            "field": field,
            "reason": reason,
        }),
        Err(error) => json!({
            "label": label,
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": label,
            "matched": false,
            "unexpected": "configuration succeeded",
        }),
    }
}

fn warm_load_before_start_edge() -> Value {
    let mut runtime = EngineCalyxRuntime::configure(EngineCalyxRuntimeConfig::in_process(
        "constellation-24",
        50,
        8192,
        4096,
    ))
    .expect("valid runtime config");
    match runtime.warm_load() {
        Err(EngineCalyxRuntimeError::InvalidState {
            operation,
            actual,
            expected,
        }) => json!({
            "label": "warm_load_before_start_rejected",
            "matched": operation == "warm_load" && actual == "configured" && expected == "started",
            "operation": operation,
            "actual": actual,
            "expected": expected,
        }),
        other => json!({
            "label": "warm_load_before_start_rejected",
            "matched": false,
            "unexpected": format!("{other:?}"),
        }),
    }
}

fn start_twice_edge() -> Value {
    let mut runtime = EngineCalyxRuntime::configure(EngineCalyxRuntimeConfig::in_process(
        "constellation-24",
        50,
        8192,
        4096,
    ))
    .expect("valid runtime config");
    runtime.start().expect("first start should succeed");
    match runtime.start() {
        Err(EngineCalyxRuntimeError::InvalidState {
            operation,
            actual,
            expected,
        }) => json!({
            "label": "start_twice_rejected",
            "matched": operation == "start" && actual == "started" && expected == "configured",
            "operation": operation,
            "actual": actual,
            "expected": expected,
        }),
        other => json!({
            "label": "start_twice_rejected",
            "matched": false,
            "unexpected": format!("{other:?}"),
        }),
    }
}

fn zero_lens_warm_load_edge() -> Value {
    let mut runtime = EngineCalyxRuntime::configure(EngineCalyxRuntimeConfig::in_process(
        "constellation-24",
        0,
        8192,
        4096,
    ))
    .expect("zero lenses are rejected at warm-load time");
    runtime.start().expect("start should succeed");
    match runtime.warm_load() {
        Err(EngineCalyxRuntimeError::InvalidConfig { field, reason }) => json!({
            "label": "zero_lens_warm_load_rejected",
            "matched": field == "warm_load_lenses" && reason.contains("at least one lens"),
            "field": field,
            "reason": reason,
        }),
        other => json!({
            "label": "zero_lens_warm_load_rejected",
            "matched": false,
            "unexpected": format!("{other:?}"),
        }),
    }
}

fn source_file_readbacks() -> io::Result<Vec<Value>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    [
        "crates/litd-engine/src/calyx_runtime.rs",
        "crates/litd-engine/src/calyx_runtime_tests.rs",
        "crates/litd-engine/examples/issue1117_calyx_runtime_fsv.rs",
        "crates/litd-engine/src/lib.rs",
    ]
    .iter()
    .map(|relative| {
        let path = root.join(relative);
        let bytes = fs::read(&path)?;
        Ok(json!({
            "path": relative,
            "absolute_path": path.display().to_string(),
            "bytes": bytes.len(),
            "sha256": sha256_hex(&bytes),
        }))
    })
    .collect()
}

fn ensure(condition: bool, message: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}
