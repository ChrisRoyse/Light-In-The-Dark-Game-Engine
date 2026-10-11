use calyx_core::{Anchor, AnchorKind, AnchorValue};
use serde_json::{Value, json};

use crate::{
    calyx_agent_runtime::{
        ENGINE_CALYX_AGENT_RUNTIME_HOT_PATH_POLICY, EngineCalyxAgentRuntimeError,
        EngineWorldSignal, EngineWorldSignalKind, activate_calyx_agents, read_calyx_agent_runtime,
        store_calyx_agent_runtime,
    },
    calyx_content::{
        EngineCalyxContentStore, EngineContentKind, EngineContentRecord, EngineContentSource,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    sim_order::SimOrderSourceKind,
};

#[test]
fn calyx_agent_runtime_activates_agents_and_lowers_orders_from_signals() {
    let (store, database, vault_dir) = content_store("happy");
    let guard = store
        .put_content(&agent(
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
        ))
        .unwrap();
    let quartermaster = store
        .put_content(&agent(
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
        ))
        .unwrap();
    let signals = vec![
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
    ];

    let runtime =
        activate_calyx_agents("issue1120-runtime", &[guard, quartermaster], &signals).unwrap();
    let stored = store_calyx_agent_runtime(&database, &runtime).unwrap();
    let readback = read_calyx_agent_runtime(&database, "issue1120-runtime").unwrap();

    assert_eq!(runtime.agent_count, 2);
    assert_eq!(runtime.signal_count, 2);
    assert_eq!(runtime.activation_count, 2);
    assert_eq!(runtime.order_count, 2);
    assert_eq!(
        runtime.hot_path_policy,
        ENGINE_CALYX_AGENT_RUNTIME_HOT_PATH_POLICY
    );
    assert!(!runtime.calyx_live_query_allowed_in_hot_path);
    assert_eq!(runtime.activations[0].execute_tick, 42);
    assert!(runtime.activations[0].prompt.contains("enemy squad"));
    assert_eq!(
        runtime.activations[0].lowered_order.source.kind,
        SimOrderSourceKind::LoweredAgent
    );
    assert_eq!(
        runtime.activations[0].order_fingerprint,
        runtime.activations[0].lowered_order.fingerprint()
    );
    assert_eq!(
        stored.runtime.runtime_fingerprint,
        runtime.runtime_fingerprint
    );
    assert_eq!(readback.runtime.activation_count, 2);

    println!(
        "issue1120_agent_runtime agents={} signals={} activations={} first_order={} runtime_sha={} vault={}",
        runtime.agent_count,
        runtime.signal_count,
        runtime.activation_count,
        runtime.activations[0].order_fingerprint,
        readback.database.sha256,
        vault_dir.display()
    );
}

#[test]
fn calyx_agent_runtime_returns_zero_activations_for_unmatched_signals() {
    let (store, _database, _vault_dir) = content_store("zero");
    let guard = store
        .put_content(&agent(
            "agent-guard-zero",
            "Gate Guard",
            json!({
                "agent_id": "guard-zero",
                "trigger_terms": ["enemy"],
                "prompt_template": "Agent {agent}: {signal}",
                "target_body": 3
            }),
        ))
        .unwrap();
    let runtime = activate_calyx_agents(
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
    )
    .unwrap();

    assert_eq!(runtime.activation_count, 0);
    assert!(runtime.activations.is_empty());
    assert_eq!(runtime.order_count, 0);
    assert!(!runtime.runtime_fingerprint.is_empty());
}

#[test]
fn calyx_agent_runtime_invalid_inputs_fail_loud() {
    let (store, _database, _vault_dir) = content_store("invalid");
    let non_agent = store
        .put_content(&content(
            EngineContentKind::Unit,
            "unit-not-agent",
            "Not Agent",
            json!({"unit_id": "not-agent"}),
        ))
        .unwrap();
    match activate_calyx_agents("bad-kind", &[non_agent], &[]) {
        Err(EngineCalyxAgentRuntimeError::InvalidInput { field, reason }) => {
            assert_eq!(field, "readback.kind");
            assert!(reason.contains("agent content"));
        }
        other => panic!("expected kind rejection, got {other:?}"),
    }

    let bad_agent = store
        .put_content(&agent(
            "agent-bad-template",
            "Bad Template",
            json!({
                "agent_id": "bad-template",
                "trigger_terms": ["enemy"],
                "prompt_template": "",
                "target_body": 1
            }),
        ))
        .unwrap();
    match activate_calyx_agents("bad-template", &[bad_agent], &[]) {
        Err(EngineCalyxAgentRuntimeError::InvalidInput { field, reason }) => {
            assert_eq!(field, "prompt_template");
            assert!(reason.contains("non-empty"));
        }
        other => panic!("expected template rejection, got {other:?}"),
    }

    let valid_agent = store
        .put_content(&agent(
            "agent-valid",
            "Valid Agent",
            json!({
                "agent_id": "valid",
                "trigger_terms": ["enemy"],
                "prompt_template": "Agent {agent}: {signal}",
                "target_body": 1
            }),
        ))
        .unwrap();
    let mut bad_signal = signal(
        "signal-empty-terms",
        1,
        EngineWorldSignalKind::Combat,
        500,
        &["enemy"],
        "enemy is near",
    );
    bad_signal.terms.clear();
    match activate_calyx_agents("bad-signal", &[valid_agent], &[bad_signal]) {
        Err(EngineCalyxAgentRuntimeError::InvalidInput { field, reason }) => {
            assert_eq!(field, "terms");
            assert!(reason.contains("at least one"));
        }
        other => panic!("expected signal rejection, got {other:?}"),
    }

    // #1464: a missing agent_id must fail loud, not silently remap to the
    // content id (which would activate the agent under a different identity).
    let missing_id_agent = store
        .put_content(&agent(
            "agent-missing-id",
            "Missing Id",
            json!({
                "trigger_terms": ["enemy"],
                "prompt_template": "Agent {agent}: {signal}",
                "target_body": 1
            }),
        ))
        .unwrap();
    match activate_calyx_agents("missing-id", &[missing_id_agent], &[]) {
        Err(EngineCalyxAgentRuntimeError::InvalidInput { field, reason }) => {
            assert_eq!(field, "agent_id");
            assert!(reason.contains("non-empty"));
        }
        other => panic!("expected agent_id rejection, got {other:?}"),
    }

    // A malformed (non-string) agent_id must also fail loud.
    let malformed_id_agent = store
        .put_content(&agent(
            "agent-malformed-id",
            "Malformed Id",
            json!({
                "agent_id": 42,
                "trigger_terms": ["enemy"],
                "prompt_template": "Agent {agent}: {signal}",
                "target_body": 1
            }),
        ))
        .unwrap();
    match activate_calyx_agents("malformed-id", &[malformed_id_agent], &[]) {
        Err(EngineCalyxAgentRuntimeError::InvalidInput { field, .. }) => {
            assert_eq!(field, "agent_id");
        }
        other => panic!("expected agent_id rejection, got {other:?}"),
    }
}

fn content_store(
    label: &str,
) -> (
    EngineCalyxContentStore,
    EngineCalyxDatabase,
    crate::test_temp::TestTempDir,
) {
    let vault_dir = unique_vault_dir(label);
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    (
        crate::calyx_content_tests::default_fixture_content_store(database.clone(), label),
        database,
        vault_dir,
    )
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
            author: "issue1120-test".to_string(),
            source_hash: litd_hash::sha256_hex(format!("issue1120:{id}").as_bytes()),
        },
        anchors: vec![Anchor {
            kind: AnchorKind::TestPass,
            value: AnchorValue::Bool(true),
            source: "issue1120_agent_runtime".to_string(),
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

fn unique_vault_dir(label: &str) -> crate::test_temp::TestTempDir {
    crate::test_temp::TestTempDir::new(&format!("issue1120-{label}"))
}
