use std::{fs, io, path::Path};

use calyx_core::{Anchor, AnchorKind, AnchorValue};
use litd_engine::{
    calyx_agent_runtime::{
        ENGINE_CALYX_AGENT_RUNTIME_HOT_PATH_POLICY, EngineCalyxAgentRuntimeError,
        EngineCalyxAgentRuntimeReadback, EngineWorldSignal, EngineWorldSignalKind,
        activate_calyx_agents, read_calyx_agent_runtime, store_calyx_agent_runtime,
        validate_runtime_readback,
    },
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    sim_order::SimOrderSourceKind,
};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue1120_calyx_agent_runtime")?;
    let vault_dir = run.run_dir().join("aster-vault");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::new(database.clone());

    let agents = agent_records();
    let agent_readbacks = agents
        .iter()
        .map(|agent| store.put_content(agent))
        .collect::<Result<Vec<_>, _>>()?;
    let signals = matching_signals();
    let runtime = activate_calyx_agents("issue1120-runtime", &agent_readbacks, &signals)?;
    let persisted = store_calyx_agent_runtime(&database, &runtime)?;
    let stored = read_calyx_agent_runtime(&database, "issue1120-runtime")?;
    let zero_activation = zero_activation_readback(&store)?;
    let edges = edge_readbacks(&store, &database, &agent_readbacks, &runtime)?;
    let vault_files = vault_file_readbacks(&vault_dir)?;

    let all_order_fingerprints_match = runtime
        .activations
        .iter()
        .all(|activation| activation.order_fingerprint == activation.lowered_order.fingerprint());
    let all_orders_lowered_agent = runtime
        .activations
        .iter()
        .all(|activation| activation.lowered_order.source.kind == SimOrderSourceKind::LoweredAgent);
    let prompts_rendered = runtime
        .activations
        .iter()
        .all(|activation| activation.prompt.contains("Agent ") && !activation.prompt.is_empty());
    let persisted_runtime_matches = persisted.runtime.runtime_fingerprint
        == runtime.runtime_fingerprint
        && stored.runtime.runtime_fingerprint == runtime.runtime_fingerprint
        && stored.database.sha256 == persisted.database.sha256;
    let runtime_acceptance = runtime.agent_count == 2
        && runtime.signal_count == 2
        && runtime.activation_count == 2
        && runtime.order_count == 2
        && runtime.hot_path_policy == ENGINE_CALYX_AGENT_RUNTIME_HOT_PATH_POLICY
        && !runtime.calyx_live_query_allowed_in_hot_path
        && all_order_fingerprints_match
        && all_orders_lowered_agent
        && prompts_rendered
        && runtime.activations[0].execute_tick == 42;
    let zero_acceptance = zero_activation.activation_count == 0
        && zero_activation.order_count == 0
        && zero_activation.activations.is_empty()
        && !zero_activation.runtime_fingerprint.is_empty();
    let all_edges_matched = edges
        .iter()
        .all(|edge| edge["matched"].as_bool().unwrap_or(false));

    ensure(
        runtime_acceptance,
        "agent runtime activation acceptance failed",
    )?;
    ensure(persisted_runtime_matches, "persisted runtime did not match")?;
    ensure(zero_acceptance, "zero-activation acceptance failed")?;
    ensure(
        all_edges_matched,
        "agent runtime edge readback acceptance failed",
    )?;

    let report = json!({
        "schema": "litd.engine.issue1120.calyx_agent_runtime_fsv.v1",
        "schema_version": 1,
        "issue": 1120,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "latest_manifest_path": run.latest_manifest_path().display().to_string(),
            "vault_dir": vault_dir.display().to_string(),
            "manual_gate": "supporting generated report; manual readback must inspect this persisted JSON, manifest, Aster vault rows, agent content readbacks, runtime ledger, prompts, lowered orders, and hashes"
        },
        "happy_path": {
            "agent_readbacks": agent_readbacks.iter().map(|readback| json!({
                "kind": readback.kind,
                "id": readback.id,
                "cx_id": readback.cx_id,
                "input_sha256": readback.input_sha256,
                "source_path": readback.source_path,
                "database_seq": readback.database.seq,
                "database_sha256": readback.database.sha256,
                "row_key": readback.database.row_key,
                "graph_rows": readback.database.graph_rows
            })).collect::<Vec<_>>(),
            "signals": signals,
            "runtime": runtime,
            "persisted_database": persisted.database,
            "stored_database": stored.database,
            "zero_activation": zero_activation,
            "vault_files": vault_files
        },
        "acceptance": {
            "runtime_acceptance": runtime_acceptance,
            "persisted_runtime_matches": persisted_runtime_matches,
            "agent_count": runtime.agent_count,
            "signal_count": runtime.signal_count,
            "activation_count": runtime.activation_count,
            "order_count": runtime.order_count,
            "hot_path_policy": runtime.hot_path_policy,
            "calyx_live_query_allowed_in_hot_path": runtime.calyx_live_query_allowed_in_hot_path,
            "all_orders_lowered_agent": all_orders_lowered_agent,
            "all_order_fingerprints_match": all_order_fingerprints_match,
            "prompts_rendered": prompts_rendered,
            "first_activation_id": runtime.activations[0].activation_id,
            "first_order_fingerprint": runtime.activations[0].order_fingerprint,
            "first_execute_tick": runtime.activations[0].execute_tick,
            "runtime_fingerprint": runtime.runtime_fingerprint,
            "stored_database_sha256": stored.database.sha256,
            "zero_activation_count": zero_activation.activation_count,
            "zero_runtime_fingerprint": zero_activation.runtime_fingerprint,
            "all_edges_matched": all_edges_matched
        },
        "edges": edges
    });
    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1120_CALYX_AGENT_RUNTIME_FSV report={} sha256={} manifest={} run_id={} agents={} signals={} activations={} first_order={} edges={} runtime_fingerprint={}",
        readback.path.display(),
        readback.sha256,
        readback.latest_manifest_path.display(),
        run.run_id(),
        runtime.agent_count,
        runtime.signal_count,
        runtime.activation_count,
        runtime.activations[0].order_fingerprint,
        edges.len(),
        runtime.runtime_fingerprint
    );
    Ok(())
}

fn agent_records() -> Vec<EngineContentRecord> {
    vec![
        agent(
            "agent-gate-guard",
            "Gate Guard",
            json!({
                "agent_id": "gate-guard",
                "trigger_terms": ["enemy", "gate", "threat"],
                "min_strength_milli": 600,
                "prompt_template": "Agent {agent}: respond to {signal} in {world}; terms={terms}",
                "target_body": 7,
                "order_velocity": [1, 0, 0]
            }),
        ),
        agent(
            "agent-quartermaster",
            "Quartermaster",
            json!({
                "agent_id": "quartermaster",
                "trigger_terms": ["supply", "stockpile"],
                "min_strength_milli": 500,
                "prompt_template": "Agent {agent}: handle {signal} in {world}",
                "target_body": 9,
                "order_velocity": [0, 0, 1]
            }),
        ),
    ]
}

fn matching_signals() -> Vec<EngineWorldSignal> {
    vec![
        signal(
            "signal-enemy-gate",
            41,
            EngineWorldSignalKind::Combat,
            900,
            &["enemy", "gate", "threat"],
            "enemy squad entered the north gate range",
        ),
        signal(
            "signal-supply-low",
            77,
            EngineWorldSignalKind::Economy,
            800,
            &["supply", "stockpile"],
            "stockpile is below patrol threshold",
        ),
    ]
}

fn zero_activation_readback(
    store: &EngineCalyxContentStore,
) -> Result<EngineCalyxAgentRuntimeReadback, Box<dyn std::error::Error>> {
    let guard = store.put_content(&agent(
        "agent-zero-guard",
        "Gate Guard",
        json!({
            "agent_id": "zero-guard",
            "trigger_terms": ["enemy"],
            "prompt_template": "Agent {agent}: {signal}",
            "target_body": 3
        }),
    ))?;
    Ok(activate_calyx_agents(
        "issue1120-zero",
        &[guard],
        &[signal(
            "signal-weather",
            5,
            EngineWorldSignalKind::Story,
            900,
            &["rain", "wind"],
            "rain starts over the road",
        )],
    )?)
}

fn edge_readbacks(
    store: &EngineCalyxContentStore,
    database: &EngineCalyxDatabase,
    agent_readbacks: &[litd_engine::calyx_content::EngineContentConstellationReadback],
    runtime: &EngineCalyxAgentRuntimeReadback,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let non_agent = store.put_content(&content(
        EngineContentKind::Unit,
        "unit-not-agent",
        "Not Agent",
        json!({"unit_id": "not-agent"}),
    ))?;
    let bad_template = store.put_content(&agent(
        "agent-bad-template",
        "Bad Template",
        json!({
            "agent_id": "bad-template",
            "trigger_terms": ["enemy"],
            "prompt_template": "",
            "target_body": 1
        }),
    ))?;
    let valid_agent = store.put_content(&agent(
        "agent-valid-edge",
        "Valid Agent",
        json!({
            "agent_id": "valid-edge",
            "trigger_terms": ["enemy"],
            "prompt_template": "Agent {agent}: {signal}",
            "target_body": 1
        }),
    ))?;
    let mut empty_terms_signal = signal(
        "signal-empty-terms",
        1,
        EngineWorldSignalKind::Combat,
        500,
        &["enemy"],
        "enemy is near",
    );
    empty_terms_signal.terms.clear();
    let bad_strength_signal = signal(
        "signal-bad-strength",
        1,
        EngineWorldSignalKind::Combat,
        0,
        &["enemy"],
        "enemy is near",
    );
    let mut bad_counts = runtime.clone();
    bad_counts.activation_count += 1;
    let mut live_query = runtime.clone();
    live_query.calyx_live_query_allowed_in_hot_path = true;
    let mut bad_fingerprint = runtime.clone();
    bad_fingerprint.runtime_fingerprint = "00000000".to_string();

    Ok(vec![
        expect_invalid_input(
            "empty_runtime_id_rejected",
            activate_calyx_agents("", agent_readbacks, &[]),
            "runtime_id",
            "non-empty",
        ),
        expect_invalid_input(
            "empty_agent_readbacks_rejected",
            activate_calyx_agents("empty-agents", &[], &[]),
            "agent_readbacks",
            "at least one",
        ),
        expect_invalid_input(
            "non_agent_content_rejected",
            activate_calyx_agents("bad-kind", &[non_agent], &[]),
            "readback.kind",
            "agent content",
        ),
        expect_invalid_input(
            "bad_prompt_template_rejected",
            activate_calyx_agents("bad-template", &[bad_template], &[]),
            "prompt_template",
            "non-empty",
        ),
        expect_invalid_input(
            "empty_signal_terms_rejected",
            activate_calyx_agents(
                "bad-signal",
                std::slice::from_ref(&valid_agent),
                &[empty_terms_signal],
            ),
            "terms",
            "at least one",
        ),
        expect_invalid_input(
            "zero_signal_strength_rejected",
            activate_calyx_agents("bad-strength", &[valid_agent], &[bad_strength_signal]),
            "signal.strength_milli",
            "1..=1000",
        ),
        expect_invalid_input(
            "runtime_count_tamper_rejected",
            validate_runtime_readback(&bad_counts),
            "counts",
            "must match",
        ),
        expect_invalid_input(
            "live_query_runtime_rejected",
            validate_runtime_readback(&live_query),
            "calyx_live_query_allowed_in_hot_path",
            "forbidden",
        ),
        expect_invalid_input(
            "runtime_fingerprint_tamper_rejected",
            validate_runtime_readback(&bad_fingerprint),
            "runtime_fingerprint",
            "fingerprint mismatch",
        ),
        missing_runtime_edge(database),
    ])
}

fn agent(id: &str, display_name: &str, payload: Value) -> EngineContentRecord {
    content(EngineContentKind::Agent, id, display_name, payload)
}

fn content(
    kind: EngineContentKind,
    id: &str,
    display_name: &str,
    payload: Value,
) -> EngineContentRecord {
    EngineContentRecord {
        kind,
        id: id.to_string(),
        display_name: display_name.to_string(),
        payload,
        source: EngineContentSource {
            path: format!("synthetic://issue1120/{id}"),
            author: "issue1120-fsv".to_string(),
            source_hash: litd_hash::sha256_hex(format!("issue1120:{id}").as_bytes()),
        },
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue1120_calyx_agent_runtime_fsv".to_string(),
            observed_at: 1_786_000_000,
            confidence: 1.0,
        }],
    }
}

fn signal(
    signal_id: &str,
    tick: u64,
    kind: EngineWorldSignalKind,
    strength_milli: u16,
    terms: &[&str],
    summary: &str,
) -> EngineWorldSignal {
    EngineWorldSignal {
        signal_id: signal_id.to_string(),
        world_id: "world-ashen-reach".to_string(),
        tick,
        kind,
        strength_milli,
        terms: terms.iter().map(|term| term.to_string()).collect(),
        summary: summary.to_string(),
        provenance_refs: vec![format!("signal://issue1120/{signal_id}")],
    }
}

fn expect_invalid_input<T>(
    label: &str,
    result: Result<T, EngineCalyxAgentRuntimeError>,
    expected_field: &str,
    expected_reason: &str,
) -> Value {
    match result {
        Err(EngineCalyxAgentRuntimeError::InvalidInput { field, reason }) => json!({
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

fn missing_runtime_edge(database: &EngineCalyxDatabase) -> Value {
    match read_calyx_agent_runtime(database, "missing-agent-runtime") {
        Err(EngineCalyxAgentRuntimeError::Persistence(error)) => json!({
            "label": "missing_persisted_runtime_rejected",
            "matched": error.to_string().contains("missing-agent-runtime"),
            "error": error.to_string(),
        }),
        Err(error) => json!({
            "label": "missing_persisted_runtime_rejected",
            "matched": false,
            "unexpected_error": error.to_string(),
        }),
        Ok(_) => json!({
            "label": "missing_persisted_runtime_rejected",
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
