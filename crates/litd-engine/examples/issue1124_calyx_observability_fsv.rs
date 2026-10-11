use std::{collections::BTreeSet, fs, io, path::Path};

use litd_engine::{
    calyx_observability::{
        EngineCalyxObservabilityError, EngineCalyxObservabilityPolicy, EngineTelemetryEvent,
        EngineTelemetryEventKind, evaluate_calyx_observability, read_calyx_observability,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1124_calyx_observability")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;

    let alert_stored =
        evaluate_calyx_observability(&database, "issue1124-fsv-alerts", policy(), &alert_events())?;
    let alert_readback = read_calyx_observability(&database, "issue1124-fsv-alerts")?;
    let normal_stored = evaluate_calyx_observability(
        &database,
        "issue1124-fsv-normal",
        policy(),
        &normal_events(),
    )?;
    let normal_readback = read_calyx_observability(&database, "issue1124-fsv-normal")?;
    let edges = edge_readbacks(&database);
    let vault_files = vault_file_readbacks(&vault_dir)?;

    let alert_codes = alert_readback
        .observability
        .alerts
        .iter()
        .map(|alert| alert.code.clone())
        .collect::<BTreeSet<_>>();
    let required_codes = required_alert_codes();
    let crash_repeats = alert_readback
        .observability
        .metrics
        .crash_signature_counts
        .get("render-device-lost")
        .copied()
        .unwrap_or_default();
    let alert_acceptance = alert_stored.observability.observability_fingerprint
        == alert_readback.observability.observability_fingerprint
        && alert_stored.database.sha256 == alert_readback.database.sha256
        && alert_readback.database.kind.as_str() == "observability_ledger"
        && alert_readback.observability.event_count == 10
        && alert_readback.observability.alert_count == 7
        && alert_readback.observability.metrics.error_rate_milli == 500
        && alert_readback.observability.metrics.max_latency_ms == 250
        && alert_readback.observability.metrics.max_frame_time_ns == 40_000_000
        && alert_readback
            .observability
            .metrics
            .metrics_endpoint_failures
            == 1
        && crash_repeats == 2
        && required_codes.is_subset(&alert_codes);
    let normal_acceptance = normal_stored.observability.observability_fingerprint
        == normal_readback.observability.observability_fingerprint
        && normal_stored.database.sha256 == normal_readback.database.sha256
        && normal_readback.observability.event_count == 5
        && normal_readback.observability.alert_count == 0
        && normal_readback.observability.alerts.is_empty()
        && normal_readback.observability.metrics.error_rate_milli == 0
        && normal_readback
            .observability
            .metrics
            .metrics_endpoint_failures
            == 0;
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));

    ensure(alert_acceptance, "observability alert acceptance failed")?;
    ensure(normal_acceptance, "observability normal acceptance failed")?;
    ensure(all_edges_matched, "observability edge acceptance failed")?;

    let report = json!({
        "schema": "litd.engine.issue1124.calyx_observability_fsv.v1",
        "schema_version": 1,
        "issue": 1124,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "vault_dir": vault_dir.display().to_string(),
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON, manifest, Aster vault rows, observability ledgers, alert codes, metrics rollups, provenance refs, and hashes"
        },
        "happy_path": {
            "alert_database": alert_readback.database,
            "alert_observability": alert_readback.observability,
            "normal_database": normal_readback.database,
            "normal_observability": normal_readback.observability,
            "vault_files": vault_files
        },
        "acceptance": {
            "alert_acceptance": alert_acceptance,
            "normal_acceptance": normal_acceptance,
            "all_edges_matched": all_edges_matched,
            "alert_codes": alert_codes,
            "required_alert_codes": required_codes,
            "event_count": 10,
            "alert_count": 7,
            "normal_event_count": 5,
            "error_rate_milli": 500,
            "max_latency_ms": 250,
            "max_frame_time_ns": 40000000_u64,
            "crash_signature": "render-device-lost",
            "crash_repeats": crash_repeats,
            "alert_fingerprint": alert_stored.observability.observability_fingerprint,
            "normal_fingerprint": normal_readback.observability.observability_fingerprint
        },
        "edges": edges
    });
    let readback_report = run.write_report(&report)?;
    println!(
        "ISSUE1124_CALYX_OBSERVABILITY_FSV report={} sha256={} manifest={} run_id={} alerts={} codes={} normal_alerts={} edges={} alert_fingerprint={}",
        readback_report.path.display(),
        readback_report.sha256,
        readback_report.latest_manifest_path.display(),
        run.run_id(),
        7,
        required_codes.len(),
        0,
        edges.len(),
        alert_stored.observability.observability_fingerprint
    );
    Ok(())
}

fn alert_events() -> Vec<EngineTelemetryEvent> {
    let mut events = vec![
        request("req-ok", 40),
        request("req-slow", 250),
        error("err-a", "CALYX_REGISTRY_UNAVAILABLE"),
        error("err-b", "CALYX_ORACLE_INSUFFICIENT"),
        chain("chain-bad", false),
        crash("crash-a", "render-device-lost"),
        crash("crash-b", "render-device-lost"),
        agent("agent-ood", 950),
        perf("perf-slow", 40_000_000),
        metrics("metrics-down", false),
    ];
    for (index, event) in events.iter_mut().enumerate() {
        event.observed_at_ms = 1_786_000_000 + index as u64;
    }
    events
}

fn normal_events() -> Vec<EngineTelemetryEvent> {
    vec![
        request("req-ok-a", 30),
        request("req-ok-b", 35),
        chain("chain-ok", true),
        agent("agent-ok", 100),
        metrics("metrics-ok", true),
    ]
}

fn policy() -> EngineCalyxObservabilityPolicy {
    EngineCalyxObservabilityPolicy {
        policy_id: "issue1124-policy".to_string(),
        max_latency_ms: 100,
        max_error_rate_milli: 250,
        max_agent_ood_milli: 700,
        max_frame_time_ns: 16_666_667,
        max_crash_repeats: 2,
        require_chain_ok: true,
    }
}

fn required_alert_codes() -> BTreeSet<String> {
    [
        "LITD-CALYX-ALERT-LATENCY",
        "LITD-CALYX-ALERT-ERROR-RATE",
        "LITD-CALYX-ALERT-CHAIN-VERIFY",
        "LITD-CALYX-ALERT-CRASH-PATTERN",
        "LITD-CALYX-ALERT-AGENT-OOD",
        "LITD-CALYX-ALERT-PERF",
        "LITD-CALYX-ALERT-METRICS-DOWN",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn edge_readbacks(database: &EngineCalyxDatabase) -> Vec<Value> {
    let mut missing_event_provenance = request("missing-provenance", 1);
    missing_event_provenance.provenance_refs.clear();
    let mut missing_latency = event("missing-latency", EngineTelemetryEventKind::Request);
    missing_latency.latency_ms = None;
    let mut missing_error_code = event("missing-error", EngineTelemetryEventKind::Error);
    missing_error_code.error_code = None;
    let mut missing_chain_ok = event("missing-chain", EngineTelemetryEventKind::ChainVerify);
    missing_chain_ok.chain_ok = None;
    let mut bad_crash = event("blank-crash", EngineTelemetryEventKind::Crash);
    bad_crash.crash_signature = Some(" ".to_string());
    let mut bad_agent_ood = event("bad-agent-ood", EngineTelemetryEventKind::AgentOod);
    bad_agent_ood.agent_ood_score_milli = Some(1001);
    let mut missing_frame_time = event("missing-frame", EngineTelemetryEventKind::PerfFrame);
    missing_frame_time.frame_time_ns = None;
    let mut missing_endpoint = event(
        "missing-endpoint",
        EngineTelemetryEventKind::MetricsEndpoint,
    );
    missing_endpoint.endpoint_ok = None;
    let mut bad_policy = policy();
    bad_policy.max_error_rate_milli = 1001;

    vec![
        expect_invalid_input(
            "empty_events_rejected",
            evaluate_calyx_observability(database, "empty-events", policy(), &[]),
            "events",
            "at least one",
        ),
        expect_invalid_input(
            "blank_run_id_rejected",
            evaluate_calyx_observability(database, " ", policy(), &normal_events()),
            "id",
            "non-empty",
        ),
        expect_invalid_input(
            "bad_policy_rejected",
            evaluate_calyx_observability(database, "bad-policy", bad_policy, &normal_events()),
            "policy",
            "milli values",
        ),
        expect_invalid_input(
            "missing_event_provenance_rejected",
            evaluate_calyx_observability(
                database,
                "missing-provenance",
                policy(),
                &[missing_event_provenance],
            ),
            "event",
            "provenance",
        ),
        expect_invalid_input(
            "missing_latency_rejected",
            evaluate_calyx_observability(database, "missing-latency", policy(), &[missing_latency]),
            "latency_ms",
            "required",
        ),
        expect_invalid_input(
            "missing_error_code_rejected",
            evaluate_calyx_observability(
                database,
                "missing-error",
                policy(),
                &[missing_error_code],
            ),
            "error_code",
            "required",
        ),
        expect_invalid_input(
            "missing_chain_ok_rejected",
            evaluate_calyx_observability(database, "missing-chain", policy(), &[missing_chain_ok]),
            "chain_ok",
            "required",
        ),
        expect_invalid_input(
            "blank_crash_signature_rejected",
            evaluate_calyx_observability(database, "blank-crash", policy(), &[bad_crash]),
            "crash_signature",
            "required",
        ),
        expect_invalid_input(
            "bad_agent_ood_rejected",
            evaluate_calyx_observability(database, "bad-agent-ood", policy(), &[bad_agent_ood]),
            "agent_ood_score_milli",
            "required",
        ),
        expect_invalid_input(
            "missing_frame_time_rejected",
            evaluate_calyx_observability(
                database,
                "missing-frame",
                policy(),
                &[missing_frame_time],
            ),
            "frame_time_ns",
            "required",
        ),
        expect_invalid_input(
            "missing_endpoint_rejected",
            evaluate_calyx_observability(
                database,
                "missing-endpoint",
                policy(),
                &[missing_endpoint],
            ),
            "endpoint_ok",
            "required",
        ),
        missing_ledger_edge(database),
    ]
}

fn request(id: &str, latency_ms: u64) -> EngineTelemetryEvent {
    let mut event = event(id, EngineTelemetryEventKind::Request);
    event.latency_ms = Some(latency_ms);
    event
}

fn error(id: &str, code: &str) -> EngineTelemetryEvent {
    let mut event = event(id, EngineTelemetryEventKind::Error);
    event.error_code = Some(code.to_string());
    event
}

fn chain(id: &str, ok: bool) -> EngineTelemetryEvent {
    let mut event = event(id, EngineTelemetryEventKind::ChainVerify);
    event.chain_ok = Some(ok);
    event
}

fn crash(id: &str, signature: &str) -> EngineTelemetryEvent {
    let mut event = event(id, EngineTelemetryEventKind::Crash);
    event.crash_signature = Some(signature.to_string());
    event
}

fn agent(id: &str, score_milli: u32) -> EngineTelemetryEvent {
    let mut event = event(id, EngineTelemetryEventKind::AgentOod);
    event.agent_ood_score_milli = Some(score_milli);
    event
}

fn perf(id: &str, frame_time_ns: u64) -> EngineTelemetryEvent {
    let mut event = event(id, EngineTelemetryEventKind::PerfFrame);
    event.frame_time_ns = Some(frame_time_ns);
    event
}

fn metrics(id: &str, ok: bool) -> EngineTelemetryEvent {
    let mut event = event(id, EngineTelemetryEventKind::MetricsEndpoint);
    event.endpoint_ok = Some(ok);
    event
}

fn event(id: &str, kind: EngineTelemetryEventKind) -> EngineTelemetryEvent {
    EngineTelemetryEvent {
        event_id: id.to_string(),
        source: "calyx://metrics/issue1124-fsv".to_string(),
        kind,
        observed_at_ms: 1_786_000_000,
        latency_ms: None,
        error_code: None,
        chain_ok: None,
        crash_signature: None,
        agent_ood_score_milli: None,
        frame_time_ns: None,
        endpoint_ok: None,
        provenance_refs: vec![format!("metrics://issue1124-fsv/{id}")],
    }
}

fn expect_invalid_input<T>(
    label: &str,
    result: Result<T, EngineCalyxObservabilityError>,
    expected_field: &str,
    expected_reason: &str,
) -> Value {
    match result {
        Err(EngineCalyxObservabilityError::InvalidInput { field, reason }) => json!({
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
            "unexpected": "operation succeeded",
        }),
    }
}

fn missing_ledger_edge(database: &EngineCalyxDatabase) -> Value {
    match read_calyx_observability(database, "missing-observability-ledger") {
        Err(EngineCalyxObservabilityError::Persistence(error)) => json!({
            "label": "missing_observability_ledger_rejected",
            "matched": error.to_string().contains("missing-observability-ledger"),
            "error": error.to_string(),
        }),
        Err(error) => json!({
            "label": "missing_observability_ledger_rejected",
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": "missing_observability_ledger_rejected",
            "matched": false,
            "unexpected": "read succeeded",
        }),
    }
}

fn vault_file_readbacks(vault_dir: &Path) -> io::Result<Vec<Value>> {
    let mut rows = Vec::new();
    for entry in fs::read_dir(vault_dir)? {
        collect_file_readbacks(&entry?.path(), &mut rows)?;
    }
    rows.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(rows)
}

fn collect_file_readbacks(path: &Path, rows: &mut Vec<Value>) -> io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            collect_file_readbacks(&entry?.path(), rows)?;
        }
    } else {
        let bytes = fs::read(path)?;
        rows.push(json!({
            "path": path.display().to_string(),
            "bytes": bytes.len(),
            "sha256": litd_hash::sha256_hex(&bytes),
        }));
    }
    Ok(())
}

fn ensure(condition: bool, message: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message))
    }
}
