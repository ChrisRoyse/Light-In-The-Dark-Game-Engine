use std::{fs, io, path::Path};

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
    let run = begin_issue_run("issue787_agent_anomaly_quarantine")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;

    let anomalous = evaluate_calyx_observability(
        &database,
        "issue787-anomalous-agent",
        policy(),
        &anomalous_events(),
    )?;
    let anomalous_readback = read_calyx_observability(&database, "issue787-anomalous-agent")?;
    let normal = evaluate_calyx_observability(
        &database,
        "issue787-normal-agents",
        policy(),
        &normal_events(),
    )?;
    let normal_readback = read_calyx_observability(&database, "issue787-normal-agents")?;
    let edges = edge_readbacks(&database);
    let vault_files = vault_file_readbacks(&vault_dir)?;

    let Some(ood_alert) = anomalous_readback
        .observability
        .alerts
        .iter()
        .find(|alert| alert.code == "LITD-CALYX-ALERT-AGENT-OOD")
    else {
        return Err("missing agent OOD alert".into());
    };
    let expected_quarantine = "agent://npc.psychopath".to_string();
    let anomalous_detected = anomalous_readback.observability.alert_count == 1
        && anomalous_readback.observability.metrics.agent_ood_max_milli == 950
        && ood_alert.event_ids == vec!["event.agent.psychopath.ood".to_string()]
        && ood_alert.quarantine_refs == vec![expected_quarantine.clone()];
    let quarantine_persisted = anomalous_readback.database.payload
        == serde_json::to_value(&anomalous_readback.observability)?
        && anomalous_readback.database.provenance["source_schema"]
            == json!("litd.engine.calyx_observability.v1")
        && anomalous_readback.database.provenance["run_id"] == json!("issue787-anomalous-agent")
        && anomalous.observability.observability_fingerprint
            == anomalous_readback.observability.observability_fingerprint;
    let normal_not_alerted = normal_readback.observability.alert_count == 0
        && normal_readback.observability.alerts.is_empty()
        && normal_readback.observability.metrics.agent_ood_max_milli == 250
        && normal.observability.observability_fingerprint
            == normal_readback.observability.observability_fingerprint;
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));
    let acceptance =
        anomalous_detected && quarantine_persisted && normal_not_alerted && all_edges_matched;
    ensure(acceptance, "issue787 anomaly quarantine acceptance failed")?;

    let report = json!({
        "schema": "litd.engine.issue787.agent_anomaly_quarantine_fsv.v1",
        "schema_version": 1,
        "issue": 787,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "vault_dir": vault_dir.display().to_string(),
            "manual_gate": "supporting generated report; manual FSV must inspect this JSON, manifest, Aster ObservabilityLedger rows, alert/quarantine refs, normal quiet run, edge readbacks, and Synapse bitmap"
        },
        "research_sources": [
            "https://www.nist.gov/itl/ai-risk-management-framework",
            "https://csrc.nist.gov/pubs/sp/800/61/r3/final",
            "https://cheatsheetseries.owasp.org/cheatsheets/Logging_Cheat_Sheet.html",
            "https://owasp.org/Top10/2021/A09_2021-Security_Logging_and_Monitoring_Failures/"
        ],
        "happy_path": {
            "anomalous_database": anomalous_readback.database,
            "anomalous_observability": anomalous_readback.observability,
            "normal_database": normal_readback.database,
            "normal_observability": normal_readback.observability,
            "vault_files": vault_files
        },
        "acceptance": {
            "anomalous_detected": anomalous_detected,
            "quarantine_persisted": quarantine_persisted,
            "normal_not_alerted": normal_not_alerted,
            "all_edges_matched": all_edges_matched,
            "anomalous_event_count": anomalous_readback.observability.event_count,
            "anomalous_alert_count": anomalous_readback.observability.alert_count,
            "normal_event_count": normal_readback.observability.event_count,
            "normal_alert_count": normal_readback.observability.alert_count,
            "agent_ood_max_milli": anomalous_readback.observability.metrics.agent_ood_max_milli,
            "normal_agent_ood_max_milli": normal_readback.observability.metrics.agent_ood_max_milli,
            "alert_code": ood_alert.code,
            "alert_id": ood_alert.alert_id,
            "quarantine_refs": ood_alert.quarantine_refs,
            "expected_quarantine_ref": expected_quarantine,
            "anomalous_fingerprint": anomalous_readback.observability.observability_fingerprint,
            "normal_fingerprint": normal_readback.observability.observability_fingerprint,
            "anomalous_database_sha256": anomalous_readback.database.sha256,
            "normal_database_sha256": normal_readback.database.sha256
        },
        "edges": edges
    });
    let readback_report = run.write_report(&report)?;
    println!(
        "ISSUE787_AGENT_ANOMALY_QUARANTINE_FSV report={} sha256={} manifest={} run_id={} anomaly_alerts={} quarantine={} normal_alerts={} edges={} anomaly_fingerprint={}",
        readback_report.path.display(),
        readback_report.sha256,
        readback_report.latest_manifest_path.display(),
        run.run_id(),
        anomalous_readback.observability.alert_count,
        expected_quarantine,
        normal_readback.observability.alert_count,
        edges.len(),
        anomalous_readback.observability.observability_fingerprint
    );
    Ok(())
}

fn policy() -> EngineCalyxObservabilityPolicy {
    EngineCalyxObservabilityPolicy {
        policy_id: "issue787-agent-anomaly".to_string(),
        max_latency_ms: 100,
        max_error_rate_milli: 500,
        max_agent_ood_milli: 700,
        max_frame_time_ns: 16_666_667,
        max_crash_repeats: 2,
        require_chain_ok: true,
    }
}

fn anomalous_events() -> Vec<EngineTelemetryEvent> {
    vec![
        agent_ood(
            "event.agent.normal.watch",
            "agent://npc.watchful-guard",
            150,
            1,
        ),
        agent_ood(
            "event.agent.psychopath.ood",
            "agent://npc.psychopath",
            950,
            2,
        ),
    ]
}

fn normal_events() -> Vec<EngineTelemetryEvent> {
    vec![
        agent_ood("event.agent.normal.a", "agent://npc.watchful-guard", 100, 3),
        agent_ood("event.agent.normal.b", "agent://npc.barkeep", 250, 4),
    ]
}

fn edge_readbacks(database: &EngineCalyxDatabase) -> Vec<Value> {
    let mut missing_provenance = agent_ood("edge.missing.provenance", "agent://npc.edge", 100, 10);
    missing_provenance.provenance_refs.clear();
    let mut missing_score = event("edge.missing.score", "agent://npc.edge", 11);
    missing_score.agent_ood_score_milli = None;
    let bad_score = agent_ood("edge.bad.score", "agent://npc.edge", 1001, 12);
    let mut bad_policy = policy();
    bad_policy.max_agent_ood_milli = 1001;
    vec![
        expect_invalid_input(
            "empty_events_rejected",
            evaluate_calyx_observability(database, "edge-empty", policy(), &[]),
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
            evaluate_calyx_observability(database, "edge-bad-policy", bad_policy, &normal_events()),
            "policy",
            "milli values",
        ),
        expect_invalid_input(
            "missing_provenance_rejected",
            evaluate_calyx_observability(
                database,
                "edge-missing-provenance",
                policy(),
                &[missing_provenance],
            ),
            "event",
            "provenance",
        ),
        expect_invalid_input(
            "missing_agent_ood_score_rejected",
            evaluate_calyx_observability(
                database,
                "edge-missing-score",
                policy(),
                &[missing_score],
            ),
            "agent_ood_score_milli",
            "required",
        ),
        expect_invalid_input(
            "bad_agent_ood_score_rejected",
            evaluate_calyx_observability(database, "edge-bad-score", policy(), &[bad_score]),
            "agent_ood_score_milli",
            "required",
        ),
        missing_ledger_edge(database),
    ]
}

fn agent_ood(
    id: &str,
    source: &str,
    score_milli: u32,
    observed_at_ms: u64,
) -> EngineTelemetryEvent {
    let mut event = event(id, source, observed_at_ms);
    event.agent_ood_score_milli = Some(score_milli);
    event
}

fn event(id: &str, source: &str, observed_at_ms: u64) -> EngineTelemetryEvent {
    EngineTelemetryEvent {
        event_id: id.to_string(),
        source: source.to_string(),
        kind: EngineTelemetryEventKind::AgentOod,
        observed_at_ms,
        latency_ms: None,
        error_code: None,
        chain_ok: None,
        crash_signature: None,
        agent_ood_score_milli: None,
        frame_time_ns: None,
        endpoint_ok: None,
        provenance_refs: vec![format!("telemetry://issue787/{id}")],
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
            "reason": reason
        }),
        Err(error) => json!({
            "label": label,
            "matched": false,
            "unexpected_error": error.to_string()
        }),
        Ok(_) => json!({
            "label": label,
            "matched": false,
            "unexpected": "operation succeeded"
        }),
    }
}

fn missing_ledger_edge(database: &EngineCalyxDatabase) -> Value {
    match read_calyx_observability(database, "missing-issue787-ledger") {
        Err(EngineCalyxObservabilityError::Persistence(error)) => json!({
            "label": "missing_observability_ledger_rejected",
            "matched": error.to_string().contains("missing-issue787-ledger"),
            "error": error.to_string()
        }),
        Err(error) => json!({
            "label": "missing_observability_ledger_rejected",
            "matched": false,
            "unexpected_error": error.to_string()
        }),
        Ok(_) => json!({
            "label": "missing_observability_ledger_rejected",
            "matched": false,
            "unexpected": "read succeeded"
        }),
    }
}

fn vault_file_readbacks(vault_dir: &Path) -> io::Result<Vec<Value>> {
    let mut rows = Vec::new();
    if vault_dir.exists() {
        for entry in fs::read_dir(vault_dir)? {
            collect_file_readbacks(&entry?.path(), &mut rows)?;
        }
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
            "sha256": litd_hash::sha256_hex(&bytes)
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
