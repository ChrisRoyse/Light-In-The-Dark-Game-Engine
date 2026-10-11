//! Calyx-backed engine/editor observability and alerting.

use std::collections::BTreeMap;

use litd_hash::sha256_hex;
use serde_json::json;

pub use crate::calyx_observability_types::*;
use crate::calyx_persistence::{
    EngineCalyxDatabase, EngineCalyxDatabaseReadback, EngineCalyxRecord, EngineCalyxRecordKind,
};

pub fn evaluate_calyx_observability(
    database: &EngineCalyxDatabase,
    run_id: impl Into<String>,
    policy: EngineCalyxObservabilityPolicy,
    events: &[EngineTelemetryEvent],
) -> Result<EngineCalyxObservabilityStoredReadback, EngineCalyxObservabilityError> {
    let run_id = run_id.into();
    validate_run(&run_id, &policy, events)?;
    let metrics = observability_metrics(events);
    let mut alerts = alerts_for_events(&policy, events, &metrics);
    alerts.sort_by(|left, right| left.alert_id.cmp(&right.alert_id));
    let mut readback = EngineCalyxObservabilityReadback {
        schema: ENGINE_CALYX_OBSERVABILITY_SCHEMA.to_string(),
        run_id,
        policy,
        event_count: events.len(),
        alert_count: alerts.len(),
        normal_count: events.len().saturating_sub(alerts.len()),
        metrics,
        alerts,
        observability_fingerprint: String::new(),
    };
    readback.observability_fingerprint = observability_fingerprint(&readback)?;
    validate_readback(&readback)?;
    let stored = database.put_record(&EngineCalyxRecord::new(
        EngineCalyxRecordKind::ObservabilityLedger,
        readback.run_id.clone(),
        serde_json::to_value(&readback).map_err(codec_failed)?,
        json!({
            "source_schema": ENGINE_CALYX_OBSERVABILITY_SCHEMA,
            "run_id": readback.run_id,
            "event_count": readback.event_count,
            "alert_count": readback.alert_count,
            "observability_fingerprint": readback.observability_fingerprint
        }),
    ))?;
    parse_stored_observability(stored)
}

pub fn read_calyx_observability(
    database: &EngineCalyxDatabase,
    run_id: &str,
) -> Result<EngineCalyxObservabilityStoredReadback, EngineCalyxObservabilityError> {
    if run_id.trim().is_empty() {
        return invalid("run_id", "run id must be non-empty");
    }
    parse_stored_observability(
        database.get_record(EngineCalyxRecordKind::ObservabilityLedger, run_id)?,
    )
}

fn observability_metrics(events: &[EngineTelemetryEvent]) -> EngineCalyxObservabilityMetrics {
    let request_count = events
        .iter()
        .filter(|event| matches!(event.kind, EngineTelemetryEventKind::Request))
        .count();
    let error_count = events
        .iter()
        .filter(|event| matches!(event.kind, EngineTelemetryEventKind::Error))
        .count();
    let denominator = request_count.saturating_add(error_count).max(1);
    let error_rate_milli = ((error_count * 1000) / denominator) as u32;
    let max_latency_ms = events
        .iter()
        .filter_map(|event| event.latency_ms)
        .max()
        .unwrap_or_default();
    let max_frame_time_ns = events
        .iter()
        .filter_map(|event| event.frame_time_ns)
        .max()
        .unwrap_or_default();
    let mut crash_signature_counts = BTreeMap::new();
    for signature in events
        .iter()
        .filter_map(|event| event.crash_signature.as_ref())
    {
        *crash_signature_counts.entry(signature.clone()).or_default() += 1;
    }
    let agent_ood_max_milli = events
        .iter()
        .filter_map(|event| event.agent_ood_score_milli)
        .max()
        .unwrap_or_default();
    let metrics_endpoint_failures = events
        .iter()
        .filter(|event| {
            matches!(event.kind, EngineTelemetryEventKind::MetricsEndpoint)
                && event.endpoint_ok == Some(false)
        })
        .count();
    EngineCalyxObservabilityMetrics {
        request_count,
        error_count,
        error_rate_milli,
        max_latency_ms,
        max_frame_time_ns,
        crash_signature_counts,
        agent_ood_max_milli,
        metrics_endpoint_failures,
    }
}

fn alerts_for_events(
    policy: &EngineCalyxObservabilityPolicy,
    events: &[EngineTelemetryEvent],
    metrics: &EngineCalyxObservabilityMetrics,
) -> Vec<EngineCalyxAlert> {
    let mut alerts = Vec::new();
    alerts.extend(
        events
            .iter()
            .filter_map(|event| latency_alert(policy, event)),
    );
    alerts.extend(events.iter().filter_map(|event| chain_alert(policy, event)));
    alerts.extend(
        events
            .iter()
            .filter_map(|event| agent_ood_alert(policy, event)),
    );
    alerts.extend(events.iter().filter_map(|event| perf_alert(policy, event)));
    alerts.extend(events.iter().filter_map(metrics_endpoint_alert));
    if metrics.error_rate_milli > policy.max_error_rate_milli {
        alerts.push(alert(
            "error-rate",
            "LITD-CALYX-ALERT-ERROR-RATE",
            "critical",
            format!(
                "Calyx bridge error rate {} milli exceeds {}",
                metrics.error_rate_milli, policy.max_error_rate_milli
            ),
            events
                .iter()
                .filter(|event| matches!(event.kind, EngineTelemetryEventKind::Error))
                .map(|event| event.event_id.clone())
                .collect(),
            "inspect structured CALYX_* errors and restore the failing bridge operation",
        ));
    }
    for (signature, count) in &metrics.crash_signature_counts {
        if *count >= policy.max_crash_repeats {
            alerts.push(alert(
                format!("crash-{signature}"),
                "LITD-CALYX-ALERT-CRASH-PATTERN",
                "critical",
                format!("crash signature {signature} repeated {count} times"),
                events
                    .iter()
                    .filter(|event| event.crash_signature.as_deref() == Some(signature.as_str()))
                    .map(|event| event.event_id.clone())
                    .collect(),
                "bucket crashes by signature and fix the repeated source before release",
            ));
        }
    }
    alerts
}

fn latency_alert(
    policy: &EngineCalyxObservabilityPolicy,
    event: &EngineTelemetryEvent,
) -> Option<EngineCalyxAlert> {
    let latency = event.latency_ms?;
    (latency > policy.max_latency_ms).then(|| {
        alert(
            format!("latency-{}", event.event_id),
            "LITD-CALYX-ALERT-LATENCY",
            "warning",
            format!(
                "{} latency {latency}ms exceeds {}",
                event.source, policy.max_latency_ms
            ),
            vec![event.event_id.clone()],
            "profile the operation and reduce Calyx bridge latency before shipping",
        )
    })
}

fn chain_alert(
    policy: &EngineCalyxObservabilityPolicy,
    event: &EngineTelemetryEvent,
) -> Option<EngineCalyxAlert> {
    (policy.require_chain_ok
        && matches!(event.kind, EngineTelemetryEventKind::ChainVerify)
        && event.chain_ok == Some(false))
    .then(|| {
        alert(
            format!("chain-{}", event.event_id),
            "LITD-CALYX-ALERT-CHAIN-VERIFY",
            "critical",
            "Calyx ledger chain verification returned ok=0",
            vec![event.event_id.clone()],
            "stop trusting the vault and inspect ledger chain verification immediately",
        )
    })
}

fn agent_ood_alert(
    policy: &EngineCalyxObservabilityPolicy,
    event: &EngineTelemetryEvent,
) -> Option<EngineCalyxAlert> {
    let score = event.agent_ood_score_milli?;
    (score > policy.max_agent_ood_milli).then(|| {
        let mut alert = alert(
            format!("agent-ood-{}", event.event_id),
            "LITD-CALYX-ALERT-AGENT-OOD",
            "critical",
            format!(
                "agent OOD score {score} exceeds {}",
                policy.max_agent_ood_milli
            ),
            vec![event.event_id.clone()],
            "route the agent output through Ward review and quarantine the activation",
        );
        alert.quarantine_refs.push(quarantine_ref(event));
        alert
    })
}

fn perf_alert(
    policy: &EngineCalyxObservabilityPolicy,
    event: &EngineTelemetryEvent,
) -> Option<EngineCalyxAlert> {
    let frame_time = event.frame_time_ns?;
    (frame_time > policy.max_frame_time_ns).then(|| {
        alert(
            format!("perf-{}", event.event_id),
            "LITD-CALYX-ALERT-PERF",
            "warning",
            format!(
                "frame time {frame_time}ns exceeds {}",
                policy.max_frame_time_ns
            ),
            vec![event.event_id.clone()],
            "open the frame profile and reduce the slow system before release",
        )
    })
}

fn metrics_endpoint_alert(event: &EngineTelemetryEvent) -> Option<EngineCalyxAlert> {
    (matches!(event.kind, EngineTelemetryEventKind::MetricsEndpoint)
        && event.endpoint_ok == Some(false))
    .then(|| {
        alert(
            format!("metrics-{}", event.event_id),
            "LITD-CALYX-ALERT-METRICS-DOWN",
            "critical",
            "Calyx metrics endpoint is down",
            vec![event.event_id.clone()],
            "restore /metrics scraping before trusting observability",
        )
    })
}

fn alert(
    alert_id: impl Into<String>,
    code: &str,
    severity: &str,
    message: impl Into<String>,
    mut event_ids: Vec<String>,
    remediation: &str,
) -> EngineCalyxAlert {
    event_ids.sort();
    event_ids.dedup();
    EngineCalyxAlert {
        alert_id: alert_id.into(),
        code: code.to_string(),
        severity: severity.to_string(),
        message: message.into(),
        event_ids,
        quarantine_refs: Vec::new(),
        remediation: remediation.to_string(),
    }
}

fn quarantine_ref(event: &EngineTelemetryEvent) -> String {
    if event.source.starts_with("agent://") {
        event.source.clone()
    } else {
        format!("{}#{}", event.source, event.event_id)
    }
}

fn validate_run(
    run_id: &str,
    policy: &EngineCalyxObservabilityPolicy,
    events: &[EngineTelemetryEvent],
) -> Result<(), EngineCalyxObservabilityError> {
    if run_id.trim().is_empty() || policy.policy_id.trim().is_empty() {
        return invalid("id", "run id and policy id must be non-empty");
    }
    if policy.max_latency_ms == 0
        || policy.max_error_rate_milli > 1000
        || policy.max_agent_ood_milli > 1000
        || policy.max_frame_time_ns == 0
        || policy.max_crash_repeats == 0
    {
        return invalid(
            "policy",
            "thresholds must be positive and milli values <= 1000",
        );
    }
    if events.is_empty() {
        return invalid(
            "events",
            "observability evaluation requires at least one event",
        );
    }
    for event in events {
        validate_event(event)?;
    }
    Ok(())
}

fn validate_event(event: &EngineTelemetryEvent) -> Result<(), EngineCalyxObservabilityError> {
    if event.event_id.trim().is_empty()
        || event.source.trim().is_empty()
        || event
            .provenance_refs
            .iter()
            .all(|reference| reference.trim().is_empty())
    {
        return invalid(
            "event",
            "event id, source, and provenance refs are required",
        );
    }
    match event.kind {
        EngineTelemetryEventKind::Request => require(event.latency_ms.is_some(), "latency_ms"),
        EngineTelemetryEventKind::Error => require(event.error_code.is_some(), "error_code"),
        EngineTelemetryEventKind::ChainVerify => require(event.chain_ok.is_some(), "chain_ok"),
        EngineTelemetryEventKind::Crash => require(
            event
                .crash_signature
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            "crash_signature",
        ),
        EngineTelemetryEventKind::AgentOod => require(
            event
                .agent_ood_score_milli
                .is_some_and(|value| value <= 1000),
            "agent_ood_score_milli",
        ),
        EngineTelemetryEventKind::PerfFrame => {
            require(event.frame_time_ns.is_some(), "frame_time_ns")
        }
        EngineTelemetryEventKind::MetricsEndpoint => {
            require(event.endpoint_ok.is_some(), "endpoint_ok")
        }
    }
}

fn require(ok: bool, field: &str) -> Result<(), EngineCalyxObservabilityError> {
    ok.then_some(())
        .ok_or_else(|| EngineCalyxObservabilityError::InvalidInput {
            field: field.to_string(),
            reason: "required for telemetry event kind".to_string(),
        })
}

fn validate_readback(
    readback: &EngineCalyxObservabilityReadback,
) -> Result<(), EngineCalyxObservabilityError> {
    if readback.schema != ENGINE_CALYX_OBSERVABILITY_SCHEMA {
        return invalid(
            "schema",
            format!("expected {ENGINE_CALYX_OBSERVABILITY_SCHEMA}"),
        );
    }
    if readback.event_count == 0
        || readback.alert_count != readback.alerts.len()
        || readback.observability_fingerprint != observability_fingerprint(readback)?
    {
        return invalid(
            "readback",
            "counts and fingerprint must match observability payload",
        );
    }
    Ok(())
}

fn observability_fingerprint(
    readback: &EngineCalyxObservabilityReadback,
) -> Result<String, EngineCalyxObservabilityError> {
    serde_json::to_vec(&json!({
        "schema": readback.schema,
        "run_id": readback.run_id,
        "policy": readback.policy,
        "event_count": readback.event_count,
        "metrics": readback.metrics,
        "alerts": readback.alerts,
    }))
    .map(|bytes| sha256_hex(&bytes))
    .map_err(codec_failed)
}

fn parse_stored_observability(
    database: EngineCalyxDatabaseReadback,
) -> Result<EngineCalyxObservabilityStoredReadback, EngineCalyxObservabilityError> {
    let observability =
        serde_json::from_value::<EngineCalyxObservabilityReadback>(database.payload.clone())
            .map_err(codec_failed)?;
    validate_readback(&observability)?;
    Ok(EngineCalyxObservabilityStoredReadback {
        schema: ENGINE_CALYX_OBSERVABILITY_READBACK_SCHEMA.to_string(),
        database,
        observability,
    })
}

fn invalid<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineCalyxObservabilityError> {
    Err(EngineCalyxObservabilityError::InvalidInput {
        field: field.into(),
        reason: reason.into(),
    })
}

fn codec_failed(error: impl std::fmt::Display) -> EngineCalyxObservabilityError {
    EngineCalyxObservabilityError::Codec(error.to_string())
}
